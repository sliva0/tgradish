//! Pictures of inputs and results: decoded on worker threads and kept as
//! frames with the time each starts.

use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use eframe::egui;
use tgradish_core::backend::{Backend, Frames, FramesRequest};
use tgradish_core::ffmpeg::{CancelToken, Probe};
use tgradish_core::tgs::{self, Preview, TgsOptions};

/// Frames of a video or animation, as straight RGBA.
pub struct Clip {
    /// Tells clips apart, so a texture showing one knows when it changes.
    pub id: u64,
    pub width: u32,
    pub height: u32,
    pub frames: Vec<Vec<u8>>,
    /// Seconds into the input where each frame starts, then where the last
    /// one ends.
    pub times: Vec<f64>,
    /// Art pixels, kept sharp when scaled up.
    pub pixelated: bool,
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
    pub fn from_preview(preview: Preview, pixelated: bool) -> Clip {
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

    pub fn start(&self) -> f64 {
        self.times[0]
    }

    pub fn end(&self) -> f64 {
        *self.times.last().expect("clips have times")
    }

    pub fn is_animated(&self) -> bool {
        self.frames.len() > 1
    }

    /// The frame shown at `time`.
    pub fn index_at(&self, time: f64) -> usize {
        self.times
            .partition_point(|&start| start <= time)
            .saturating_sub(1)
            .min(self.frames.len() - 1)
    }

    pub fn image(&self, index: usize) -> egui::ColorImage {
        egui::ColorImage::from_rgba_unmultiplied(
            [self.width as usize, self.height as usize],
            &self.frames[index],
        )
    }

    /// A frame from a third of the way in, which is rarely a black or
    /// empty first frame, scaled to at most `side` pixels.
    pub fn thumbnail(&self, side: u32) -> egui::ColorImage {
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

    /// Whether this looks like pixel art rather than a photo or video: few
    /// colours.
    pub fn looks_like_pixel_art(&self) -> bool {
        let mut colours = std::collections::HashSet::new();
        for rgba in self.frames.iter().take(8) {
            for pixel in rgba.as_chunks::<4>().0.iter().filter(|pixel| pixel[3] > 0) {
                colours.insert(*pixel);
                if colours.len() > PIXEL_ART_COLOURS {
                    return false;
                }
            }
        }
        true
    }
}

fn next_id() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// More colours than this in the first frames is not pixel art.
const PIXEL_ART_COLOURS: usize = 128;

/// Work on another thread, cancelled when dropped.
pub struct Task<T> {
    receiver: Receiver<Result<T, String>>,
    cancel: CancelToken,
}

impl<T: Send + 'static> Task<T> {
    pub fn spawn(
        ctx: &egui::Context,
        work: impl FnOnce(&CancelToken) -> Result<T, String> + Send + 'static,
    ) -> Task<T> {
        let (sender, receiver) = channel();
        let cancel = CancelToken::new();
        let token = cancel.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| work(&token)))
                .unwrap_or_else(|_| Err("tgradish crashed while reading this".into()));
            let _ = sender.send(result);
            ctx.request_repaint();
        });
        Task { receiver, cancel }
    }
}

impl<T> Task<T> {
    /// The result, once there is one.
    pub fn poll(&self) -> Option<Result<T, String>> {
        match self.receiver.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err("stopped".into())),
        }
    }
}

impl<T> Drop for Task<T> {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

/// Something loaded on another thread.
pub enum Load<T> {
    Idle,
    Loading(Task<T>),
    Ready(T),
    Failed(String),
}

impl<T> Load<T> {
    /// Takes in a finished load; true if it just finished.
    pub fn poll(&mut self) -> bool {
        let Load::Loading(task) = self else { return false };
        match task.poll() {
            Some(Ok(value)) => *self = Load::Ready(value),
            Some(Err(message)) => *self = Load::Failed(message),
            None => return false,
        }
        true
    }

    pub fn ready(&self) -> Option<&T> {
        match self {
            Load::Ready(value) => Some(value),
            _ => None,
        }
    }

    pub fn is_idle(&self) -> bool {
        matches!(self, Load::Idle)
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, Load::Loading(_))
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
pub const RESULT_SIDE: u32 = 512;

/// A video or image read through ffmpeg, the way WebM conversions read it.
pub struct Video {
    pub probe: Probe,
    /// All of it, a few frames a second.
    pub overview: Clip,
}

impl Video {
    pub fn load(ctx: &egui::Context, backend: Backend, path: PathBuf) -> Task<Video> {
        Task::spawn(ctx, move |cancel| {
            let probe = backend.probe(&path, cancel).map_err(|err| err.to_string())?;
            let request = FramesRequest {
                start: 0.0,
                length: None,
                fps: 30.0,
                max_side: OVERVIEW_SIDE,
                max_frames: OVERVIEW_FRAMES,
            };
            let frames =
                backend.frames(&path, &probe, &request, cancel).map_err(|err| err.to_string())?;
            if frames.frames.is_empty() {
                return Err("ffmpeg found no frames in it".into());
            }
            Ok(Video { probe, overview: Clip::from_frames(frames) })
        })
    }

    /// `length` seconds from `start` at up to 30 frames a second, for
    /// playing.
    pub fn load_part(
        &self,
        ctx: &egui::Context,
        backend: Backend,
        path: PathBuf,
        (start, length): (f64, f64),
    ) -> Task<Clip> {
        let probe = self.probe.clone();
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
            let frames =
                backend.frames(&path, &probe, &request, cancel).map_err(|err| err.to_string())?;
            if frames.frames.is_empty() {
                return Err("ffmpeg found no frames there".into());
            }
            Ok(Clip::from_frames(frames))
        })
    }
}

impl Video {
    /// The frame at `time`, at full size.
    pub fn load_still(
        &self,
        ctx: &egui::Context,
        backend: Backend,
        path: PathBuf,
        time: f64,
    ) -> Task<Clip> {
        let probe = self.probe.clone();
        Task::spawn(ctx, move |cancel| {
            let fps = probe.fps.unwrap_or(25.0);
            let request = FramesRequest {
                start: time,
                length: Some(1.5 / fps),
                fps,
                max_side: probe.width.max(probe.height),
                max_frames: 1,
            };
            let frames =
                backend.frames(&path, &probe, &request, cancel).map_err(|err| err.to_string())?;
            if frames.frames.is_empty() {
                return Err("ffmpeg found no frame there".into());
            }
            Ok(Clip::from_frames(frames))
        })
    }
}

/// Pixel art read the way `.tgs` conversions read it.
pub struct Art {
    pub clip: Clip,
    /// Aseprite tags.
    pub tags: Vec<String>,
}

impl Art {
    /// `options` choose what is read: an Aseprite tag, a sprite sheet's
    /// layout and the frame rate of sheets and sequences.
    pub fn load(
        ctx: &egui::Context,
        inputs: Vec<PathBuf>,
        sequence: bool,
        options: TgsOptions,
    ) -> Task<Art> {
        Task::spawn(ctx, move |_| {
            let source =
                tgs::read_source(&inputs, sequence, &options).map_err(|e| e.to_string())?;
            if source.frames.is_empty() {
                return Err("no frames".into());
            }
            let mut times = vec![0.0];
            let mut frames = Vec::with_capacity(source.frames.len());
            for (rgba, seconds) in source.frames {
                times.push(times.last().unwrap() + seconds);
                frames.push(rgba);
            }
            let (width, height) = (source.width, source.height);
            let clip = Clip { id: next_id(), width, height, frames, times, pixelated: true };
            Ok(Art { clip, tags: source.tags })
        })
    }
}

/// A finished WebM sticker, decoded.
pub fn load_result(ctx: &egui::Context, backend: Backend, path: &Path) -> Task<Clip> {
    let path = path.to_path_buf();
    Task::spawn(ctx, move |cancel| {
        let preview = backend.preview(&path, RESULT_SIDE, cancel).map_err(|err| err.to_string())?;
        if preview.frames.is_empty() {
            return Err("no frames".into());
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
}
