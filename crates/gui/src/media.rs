//! Pictures of inputs and results: decoded on worker threads and kept as
//! frames with the time each starts.

use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use eframe::egui;
use tgradish_core::CancelToken;
use tgradish_core::backend::{Backend, Frames, FramesRequest};
use tgradish_core::drawn::{self, Readable};
use tgradish_core::ffmpeg::Probe;
use tgradish_core::input::Input;
use tgradish_core::preview::Preview;
use tgradish_core::tgradish_tgs::frames::aseprite::{LayerInfo, Tag};
use tgradish_core::tgs;
use tgradish_core::webm::Options;

use crate::cuts::{self, Cut};
use crate::jobs::failure;
use crate::l10n::fl;

/// Frames of a video or animation, as straight RGBA.
pub(crate) struct Clip {
    /// Tells clips apart, so a texture showing one knows when it changes.
    pub(crate) id: u64,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) frames: Vec<Vec<u8>>,
    /// Seconds into the input where each frame starts, then where the last
    /// one ends.
    pub(crate) times: Vec<f64>,
    /// Art pixels, kept sharp when scaled up.
    pub(crate) pixelated: bool,
}

impl Clip {
    fn from_frames(frames: Frames) -> Clip {
        let count = frames.frames.len();
        let times = (0..=count).map(|i| frames.start + i as f64 / frames.fps).collect();
        Clip {
            id: next_id(),
            width: frames.width,
            height: frames.height,
            frames: frames.frames,
            times,
            pixelated: false,
        }
    }

    /// A sticker's frames, which last whole 60 fps frames.
    pub(crate) fn from_preview(preview: Preview, pixelated: bool) -> Clip {
        let mut times = vec![0.0];
        let mut ticks = 0;
        let mut frames = Vec::with_capacity(preview.frames.len());
        for (rgba, length) in preview.frames {
            ticks += length;
            times.push(f64::from(ticks) / 60.0);
            frames.push(rgba);
        }
        Clip {
            id: next_id(),
            width: preview.width,
            height: preview.height,
            frames,
            times,
            pixelated,
        }
    }

    pub(crate) fn start(&self) -> f64 {
        self.times[0]
    }

    pub(crate) fn end(&self) -> f64 {
        *self.times.last().expect("clips have times")
    }

    pub(crate) fn is_animated(&self) -> bool {
        self.frames.len() > 1
    }

    /// The frame shown at `time`.
    pub(crate) fn index_at(&self, time: f64) -> usize {
        self.times
            .partition_point(|&start| start <= time)
            .saturating_sub(1)
            .min(self.frames.len() - 1)
    }

    pub(crate) fn image(&self, index: usize) -> egui::ColorImage {
        egui::ColorImage::from_rgba_unmultiplied(
            [self.width as usize, self.height as usize],
            &self.frames[index],
        )
    }

    /// A frame from a third of the way in, which is rarely a black or
    /// empty first frame, scaled to at most `side` pixels.
    pub(crate) fn thumbnail(&self, side: u32) -> egui::ColorImage {
        let rgba = &self.frames[self.frames.len() / 3];
        let scale = (f64::from(side) / f64::from(self.width.max(self.height))).min(1.0);
        let width = ((f64::from(self.width) * scale).round() as u32).max(1);
        let height = ((f64::from(self.height) * scale).round() as u32).max(1);
        let mut out = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                // the middle of the source pixels this one covers
                let sx = (((f64::from(x) + 0.5) / scale) as u32).min(self.width - 1);
                let sy = (((f64::from(y) + 0.5) / scale) as u32).min(self.height - 1);
                let at = ((sy * self.width + sx) * 4) as usize;
                out.extend_from_slice(&rgba[at..at + 4]);
            }
        }
        egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &out)
    }

    /// Whether this looks like pixel art to make a `.tgs` of, rather than
    /// a photo or video: few colours in its first frames.
    pub(crate) fn looks_like_pixel_art(&self) -> bool {
        let frames = &self.frames[..self.frames.len().min(8)];
        tgradish_core::picture::has_few_colours(frames, self.width, None, TGS_COLOURS)
    }
}

fn next_id() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// More colours than this in the first frames is not pixel art to make a
/// `.tgs` of. Stricter than auto scaling's
/// [`PIXEL_ART_COLOURS`](tgradish_core::picture::PIXEL_ART_COLOURS): each
/// colour of a `.tgs` costs bytes of its own, while sharp scaling takes any
/// number.
const TGS_COLOURS: usize = 128;

/// Work on another thread, cancelled when dropped.
pub(crate) struct Task<T> {
    receiver: Receiver<Result<T, String>>,
    cancel: CancelToken,
}

impl<T: Send + 'static> Task<T> {
    pub(crate) fn spawn(
        ctx: &egui::Context,
        work: impl FnOnce(&CancelToken) -> Result<T, String> + Send + 'static,
    ) -> Task<T> {
        let (sender, receiver) = channel();
        let cancel = CancelToken::new();
        let token = cancel.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| work(&token)))
                .unwrap_or_else(|_| Err(fl!("reading-crashed")));
            let _ = sender.send(result);
            ctx.request_repaint();
        });
        Task { receiver, cancel }
    }
}

impl<T> Task<T> {
    /// The result, once there is one.
    pub(crate) fn poll(&self) -> Option<Result<T, String>> {
        match self.receiver.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err(fl!("reading-stopped"))),
        }
    }
}

impl<T> Drop for Task<T> {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

/// Something loaded on another thread.
pub(crate) enum Load<T> {
    Idle,
    Loading(Task<T>),
    /// What was loaded last, shown until what is loading again is there.
    Reloading(T, Task<T>),
    Ready(T),
    Failed(String),
}

impl<T> Load<T> {
    /// Loads with `task`, keeping what is loaded meanwhile.
    pub(crate) fn reload(&mut self, task: Task<T>) {
        *self = match std::mem::replace(self, Load::Idle) {
            Load::Ready(value) | Load::Reloading(value, _) => Load::Reloading(value, task),
            _ => Load::Loading(task),
        };
    }

    /// Takes in a finished load; true if it just finished.
    pub(crate) fn poll(&mut self) -> bool {
        let (Load::Loading(task) | Load::Reloading(_, task)) = self else { return false };
        match task.poll() {
            Some(Ok(value)) => *self = Load::Ready(value),
            Some(Err(message)) => *self = Load::Failed(message),
            None => return false,
        }
        true
    }

    /// Idle again unless it is loaded, to be loaded once more: what
    /// failed, and what is still loading, which stops.
    pub(crate) fn restart(&mut self) {
        if !self.is_ready() {
            *self = Load::Idle;
        }
    }

    /// Whether it is loaded, and not loading again.
    pub(crate) fn is_ready(&self) -> bool {
        matches!(self, Load::Ready(_))
    }

    pub(crate) fn ready(&self) -> Option<&T> {
        match self {
            Load::Ready(value) | Load::Reloading(value, _) => Some(value),
            _ => None,
        }
    }

    pub(crate) fn is_idle(&self) -> bool {
        matches!(self, Load::Idle)
    }

    pub(crate) fn is_loading(&self) -> bool {
        matches!(self, Load::Loading(_) | Load::Reloading(..))
    }
}

/// Longer side of the frames that show all of a video.
const OVERVIEW_SIDE: u32 = 640;
/// How many frames show all of a video.
const OVERVIEW_FRAMES: usize = 90;
/// Longer side of the frames of the part that is played.
const DETAIL_SIDE: u32 = 960;
/// Memory for the frames of the part that is played.
const DETAIL_BYTES: usize = 192 << 20;
/// Longer side of finished WebM stickers' previews: their full size.
pub(crate) const RESULT_SIDE: u32 = 512;

/// A video or image read through ffmpeg, the way WebM conversions read it.
pub(crate) struct Video {
    pub(crate) probe: Probe,
    /// All of it, a few frames a second.
    pub(crate) overview: Clip,
    /// Where its frames start, in order: empty for a picture, or when they
    /// carry no times.
    pub(crate) starts: Vec<f64>,
    /// What ffmpeg reads: the input, or an Aseprite file drawn for it.
    pub(crate) readable: Arc<Readable>,
    /// The overview is every frame at its full size, as pixel art drawn
    /// for ffmpeg is: nothing more is read to play or pause it.
    pub(crate) complete: bool,
    /// Where its frames start, which cuts land between: as its packets
    /// time them, or else at its frame rate. Worked out once, as the
    /// timeline and keys ask for it every frame.
    pub(crate) frames: Option<Arc<cuts::Frames>>,
}

/// Most bytes of drawn frames shown as they are, rather than read again
/// through ffmpeg like a video.
const DRAWN_BYTES: usize = 256 << 20;

impl Video {
    /// `input` as ffmpeg reads it; `drawing` says how pixel art ffmpeg
    /// can't read is drawn for it.
    pub(crate) fn load(
        ctx: &egui::Context,
        backend: Backend,
        input: Input,
        drawing: Options,
    ) -> Task<Video> {
        Task::spawn(ctx, move |cancel| {
            let mut readable =
                drawn::readable(&input, &drawing, cancel).map_err(|err| failure(&err))?;
            // pixel art drawn here, shown as it was drawn, without ffmpeg
            if let Some((animation, still)) = readable.take_drawn() {
                let bytes = animation.frames().iter().map(|frame| frame.rgba.len()).sum::<usize>();
                if bytes <= DRAWN_BYTES {
                    let probe = readable.probe_drawn(&animation, still);
                    return Ok(Video::of_drawn(animation, probe, readable));
                }
            }
            let path = readable.path();
            let probe = readable.probe(&backend, cancel).map_err(|err| failure(&err))?;
            let request = FramesRequest {
                start: 0.0,
                length: None,
                fps: 30.0,
                max_side: OVERVIEW_SIDE,
                max_frames: OVERVIEW_FRAMES,
            };
            let frames =
                backend.frames(path, &probe, &request, cancel).map_err(|err| failure(&err))?;
            if frames.frames.is_empty() {
                return Err(fl!("no-frames"));
            }
            let starts = if probe.still_image {
                Vec::new()
            } else {
                match backend.frame_starts(path, &probe, cancel) {
                    Err(err @ tgradish_core::Error::Cancelled) => return Err(err.to_string()),
                    starts => starts.unwrap_or_default(),
                }
            };
            let overview = Clip::from_frames(frames);
            let readable = Arc::new(readable);
            Ok(Video { probe, overview, starts, readable, complete: false, frames: None }.timed())
        })
    }

    /// Frames drawn for ffmpeg, as ffmpeg would read them from `readable`,
    /// as core's `probe` of them says: a still picture, or each frame
    /// lasting its own time.
    fn of_drawn(
        animation: tgradish_core::tgradish_tgs::frames::Animation,
        probe: Probe,
        readable: Readable,
    ) -> Video {
        let (width, height) = (animation.width(), animation.height());
        let still = probe.still_image;
        let mut times = vec![0.0];
        let frames: Vec<Vec<u8>> = animation
            .into_frames()
            .into_iter()
            .map(|frame| {
                times.push(times.last().unwrap() + frame.duration.as_secs_f64());
                frame.rgba
            })
            .collect();
        let frames = if still { frames[..1].to_vec() } else { frames };
        let starts = if still { Vec::new() } else { times[..times.len() - 1].to_vec() };
        // a picture as ffmpeg times one, a frame at its default rate
        let times = if still { vec![0.0, 1.0 / probe.fps_or_default()] } else { times };
        let overview = Clip { id: next_id(), width, height, frames, times, pixelated: true };
        let readable = Arc::new(readable);
        Video { probe, overview, starts, readable, complete: true, frames: None }.timed()
    }

    /// With [`Video::frames`] worked out.
    fn timed(mut self) -> Video {
        let length = self.probe.duration.filter(|_| !self.probe.still_image);
        let frames = length.and_then(|length| {
            cuts::Frames::starting(&self.starts, length, Cut::Video).or_else(|| {
                let fps = self.probe.fps.or(self.probe.base_fps)?;
                cuts::Frames::even(length, fps, Cut::Video)
            })
        });
        self.frames = frames.map(Arc::new);
        self
    }

    /// `length` seconds from `start` at up to 30 frames a second, for
    /// playing.
    pub(crate) fn load_part(
        &self,
        ctx: &egui::Context,
        backend: Backend,
        (start, length): (f64, f64),
    ) -> Task<Clip> {
        let (probe, readable) = (self.probe.clone(), self.readable.clone());
        Task::spawn(ctx, move |cancel| {
            let longer = probe.width.max(probe.height).max(1);
            let scale = (f64::from(DETAIL_SIDE) / f64::from(longer)).min(1.0);
            let bytes = (f64::from(probe.width) * scale * f64::from(probe.height) * scale * 4.0)
                .max(1.0) as usize;
            let request = FramesRequest {
                start,
                length: Some(length),
                fps: 30.0,
                max_side: DETAIL_SIDE,
                max_frames: (DETAIL_BYTES / bytes).clamp(1, 600),
            };
            let path = readable.path();
            let frames =
                backend.frames(path, &probe, &request, cancel).map_err(|err| failure(&err))?;
            if frames.frames.is_empty() {
                return Err(fl!("no-frames-there"));
            }
            Ok(Clip::from_frames(frames))
        })
    }

    /// The frame shown at `time`, at full size.
    pub(crate) fn load_still(
        &self,
        ctx: &egui::Context,
        backend: Backend,
        time: f64,
    ) -> Task<Clip> {
        let (probe, starts) = (self.probe.clone(), self.starts.clone());
        let readable = self.readable.clone();
        Task::spawn(ctx, move |cancel| {
            let fps = probe.fps_or_default();
            let start = tgradish_core::webp::frame_start(time, &starts, fps);
            let request = FramesRequest {
                start,
                length: Some(1.0 / fps),
                fps,
                max_side: probe.width.max(probe.height),
                max_frames: 1,
            };
            let path = readable.path();
            let frames =
                backend.frames(path, &probe, &request, cancel).map_err(|err| failure(&err))?;
            if frames.frames.is_empty() {
                return Err(fl!("no-frame-there"));
            }
            Ok(Clip::from_frames(frames))
        })
    }
}

/// Pixel art read the way `.tgs` conversions read it.
pub(crate) struct Art {
    pub(crate) clip: Clip,
    /// Aseprite tags and layers.
    pub(crate) tags: Vec<Tag>,
    pub(crate) layers: Vec<LayerInfo>,
    /// How long each frame of an Aseprite file shows, in seconds, its
    /// tags' or not.
    pub(crate) durations: Vec<f64>,
    /// Where its frames start, when it is animated.
    pub(crate) frames: Option<Arc<cuts::Frames>>,
    /// How many frames the file has where it was asked to be read as a
    /// sprite sheet, which is one picture: read as it is instead.
    pub(crate) sheet_refused: Option<usize>,
}

impl Art {
    /// `options` choose what is read: an Aseprite tag and layers, a sprite
    /// sheet's layout and the frame rate of sheets and sequences. Videos
    /// are read through `backend`.
    pub(crate) fn load(
        ctx: &egui::Context,
        input: Input,
        options: tgs::Options,
        backend: Option<Backend>,
    ) -> Task<Art> {
        Task::spawn(ctx, move |cancel| {
            // a sheet of a file of frames of its own, as a session may
            // keep: read as it is, and said so, rather than not at all
            let (art, sheet_refused) =
                match tgs::read_art(backend.as_ref(), &input, &options, cancel) {
                    Ok(art) => (art, None),
                    Err(err) => match animated(backend.as_ref(), &input, &options, cancel) {
                        Some((art, frames)) => (art, Some(frames)),
                        None => return Err(failure(&err)),
                    },
                };
            if art.frames.is_empty() {
                return Err(fl!("art-no-frames"));
            }
            let mut times = vec![0.0];
            let mut frames = Vec::with_capacity(art.frames.len());
            for (rgba, seconds) in art.frames {
                times.push(times.last().unwrap() + seconds);
                frames.push(rgba);
            }
            let (width, height) = (art.width, art.height);
            let clip = Clip { id: next_id(), width, height, frames, times, pixelated: true };
            let (tags, layers, durations) = (art.tags, art.layers, art.durations);
            let frames = clip
                .is_animated()
                .then(|| Arc::new(cuts::Frames { times: clip.times.clone(), cut: Cut::Art }));
            Ok(Art { clip, tags, layers, durations, frames, sheet_refused })
        })
    }

    /// How many frames the file read has, where that is known: as many as
    /// read as it is, or as it has where it was refused as a sheet; one,
    /// as a sheet, which is one picture. `reading` is what it was read
    /// with.
    pub(crate) fn file_frames(&self, reading: &tgs::Options) -> usize {
        match (self.sheet_refused, reading.sheet) {
            (Some(frames), _) => frames,
            (None, Some(_)) => 1,
            // all of an Aseprite file's, whichever its tag uses
            (None, None) => self.clip.frames.len().max(self.durations.len()),
        }
    }
}

/// `input` read as it is, with the rest of `options`, as its tag, where
/// reading it as the sprite sheet they ask for can't be, as the file has
/// frames of its own, and how many: `None` where they ask for no sheet, or
/// it has one frame. An Aseprite file's are all of its frames, whichever
/// its tag, which may have only one, uses.
pub(crate) fn animated(
    backend: Option<&Backend>,
    input: &Input,
    options: &tgs::Options,
    cancel: &CancelToken,
) -> Option<(tgs::Art, usize)> {
    options.sheet?;
    let whole = tgs::Options { sheet: None, sheet_frames: None, ..options.clone() };
    let art = tgs::read_art(backend, input, &whole, cancel).ok()?;
    let frames = art.frames.len().max(art.durations.len());
    (frames > 1).then_some((art, frames))
}

/// A finished WebM sticker, decoded.
pub(crate) fn load_result(ctx: &egui::Context, backend: Backend, path: &Path) -> Task<Clip> {
    let path = path.to_path_buf();
    Task::spawn(ctx, move |cancel| {
        let preview = backend.preview(&path, RESULT_SIDE, cancel).map_err(|err| failure(&err))?;
        if preview.frames.is_empty() {
            return Err(fl!("result-no-frames"));
        }
        Ok(Clip::from_preview(preview, false))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(times: Vec<f64>) -> Clip {
        let frames = vec![vec![0; 4]; times.len() - 1];
        Clip { id: 0, width: 1, height: 1, frames, times, pixelated: false }
    }

    #[test]
    fn finds_frames_by_time() {
        let clip = clip(vec![1.0, 1.5, 2.0, 3.0]);
        assert_eq!(clip.index_at(0.0), 0);
        assert_eq!(clip.index_at(1.49), 0);
        assert_eq!(clip.index_at(1.5), 1);
        assert_eq!(clip.index_at(2.5), 2);
        assert_eq!(clip.index_at(9.0), 2);
    }

    #[test]
    fn times_sticker_frames_in_60ths() {
        let preview =
            Preview { width: 1, height: 1, frames: vec![(vec![0; 4], 6), (vec![0; 4], 3)] };
        let clip = Clip::from_preview(preview, true);
        assert_eq!(clip.times, [0.0, 0.1, 0.15]);
    }

    #[test]
    fn reads_a_file_of_frames_kept_as_a_sheet_as_it_is_whatever_its_tag() {
        // an Aseprite file of frames, as a session may keep it ticked as a
        // sheet, its tag one of a single frame
        let dir = tempfile::tempdir().unwrap();
        let walker = tgradish_corpus::sprite("walker_2x");
        let tags = [("still", 0, 0), ("walk", 1, 7)];
        let path = dir.path().join("walker.aseprite");
        std::fs::write(&path, tgradish_corpus::encode::aseprite_with(&walker, &tags, &[])).unwrap();
        let input = Input::File(path);
        let options = tgs::Options {
            sheet: "2x1".parse().ok(),
            tag: Some("still".into()),
            ..tgs::Options::default()
        };
        let cancel = CancelToken::new();
        assert!(tgs::read_art(None, &input, &options, &cancel).is_err());
        // read as it is, its tag kept: one frame, of a file of eight
        let (art, frames) = animated(None, &input, &options, &cancel).expect("read as it is");
        assert_eq!((art.frames.len(), frames), (1, walker.frames.len()));
        // and no sheet asked for, nothing to read again
        let plain = tgs::Options { sheet: None, ..options };
        assert!(animated(None, &input, &plain, &cancel).is_none());
    }
}
