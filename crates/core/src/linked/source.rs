//! Decoding the input the way the ffmpeg command line does: first video
//! stream, timestamps counted from the file's start, seek to the start and
//! drop frames before it, stop after the read window, repeat still images.

use std::path::Path;

use ff::{Packet, Rational, Rescale, codec, format, frame};
use ffmpeg_next as ff;

use super::libav;
use crate::convert::Plan;
use crate::error::{Error, Result};
use crate::ffmpeg::{CancelToken, Probe};

/// Frame rate still images are repeated at, like ffmpeg's image demuxer.
const STILL_FPS: i32 = 25;

/// Demuxer and decoder of one stream.
struct Decoding {
    input: format::context::Input,
    stream: usize,
    decoder: ff::decoder::Video,
    input_done: bool,
    cancel: CancelToken,
}

impl Decoding {
    /// Decodes the next frame of the stream, reading packets as needed.
    fn next(&mut self, frame: &mut frame::Video) -> Result<bool> {
        loop {
            self.cancel.check()?;
            match self.decoder.receive_frame(frame) {
                Ok(()) => return Ok(true),
                Err(ff::Error::Eof) => return Ok(false),
                Err(ff::Error::Other { errno }) if errno == ff::util::error::EAGAIN => {}
                Err(err) => return Err(libav("decoding")(err)),
            }
            if self.input_done {
                return Ok(false);
            }
            let mut packet = Packet::empty();
            match packet.read(&mut self.input) {
                Ok(()) if packet.stream() == self.stream => {
                    self.decoder.send_packet(&packet).map_err(libav("decoding"))?;
                }
                Ok(()) | Err(ff::Error::InvalidData) => {}
                Err(ff::Error::Eof) => {
                    self.input_done = true;
                    self.decoder.send_eof().map_err(libav("decoding"))?;
                }
                // the interrupt callback makes reads fail when cancelled
                Err(_) if self.cancel.is_cancelled() => return Err(Error::Cancelled),
                Err(err) => return Err(libav("reading the input")(err)),
            }
        }
    }
}

pub(super) struct Source {
    decoding: Decoding,
    time_base: Rational,
    /// Timestamp where the wanted window starts, in `time_base` units: the
    /// file's start time plus the requested start, which is how the ffmpeg
    /// command line counts `-ss`.
    start: i64,
    /// Length of the window, in `time_base` units.
    window: i64,
    /// One frame in `time_base` units, to number frames without timestamps.
    frame_step: i64,
    /// Timestamp for the next frame if it has none.
    next_pts: i64,
    /// For still images: the image and the index of the next repeat.
    still: Option<(Option<frame::Video>, i64)>,
    done: bool,
}

/// Seconds to `time_base` units.
fn to_units(seconds: f64, time_base: Rational) -> i64 {
    (seconds * f64::from(time_base.denominator()) / f64::from(time_base.numerator())).round() as i64
}

impl Source {
    /// Opens a file and its first video stream, like ffmpeg's `-map 0:v:0`.
    pub(super) fn open_file(
        path: &Path,
        decoder: Option<&str>,
        cancel: &CancelToken,
    ) -> Result<Source> {
        cancel.check()?;
        // the interrupt callback below cannot stop an open() or read() that
        // blocks in the OS, as on a FIFO without a writer
        if !std::fs::metadata(path).is_ok_and(|m| m.is_file()) {
            return Err(Error::Probe {
                path: path.to_path_buf(),
                message: "not a regular file; the built-in ffmpeg only reads files".into(),
            });
        }
        // lets ffmpeg give up on reads that take long, like on network drives
        let interrupt = {
            let cancel = cancel.clone();
            move || cancel.is_cancelled()
        };
        let input = format::input_with_interrupt(path, interrupt).map_err(|err| {
            if cancel.is_cancelled() {
                Error::Cancelled
            } else {
                Error::Probe { path: path.to_path_buf(), message: err.to_string() }
            }
        })?;
        let stream = input
            .streams()
            .find(|stream| stream.parameters().medium() == ff::media::Type::Video)
            .ok_or_else(|| Error::NoVideo(path.to_path_buf()))?;
        let (index, time_base) = (stream.index(), stream.time_base());
        let rate = [stream.avg_frame_rate(), stream.rate()]
            .into_iter()
            .find(|r| r.numerator() > 0 && r.denominator() > 0)
            .unwrap_or(Rational::new(25, 1));
        let frame_step = to_units(f64::from(rate.invert()), time_base).max(1);

        let mut context = codec::context::Context::from_parameters(stream.parameters())
            .map_err(libav("reading stream parameters"))?;
        context.set_threading(codec::threading::Config::kind(codec::threading::Type::Frame));
        let decoder = match decoder.and_then(ff::decoder::find_by_name) {
            Some(codec) => context.decoder().open_as(codec).and_then(|d| d.video()),
            None => context.decoder().video(),
        }
        .map_err(libav("opening the decoder"))?;

        // SAFETY: reading a plain field of the open context
        let file_start = unsafe { (*input.as_ptr()).start_time };
        let file_start = if file_start == ff::ffi::AV_NOPTS_VALUE { 0 } else { file_start };
        let start = file_start.rescale(Rational::new(1, ff::ffi::AV_TIME_BASE), time_base);
        Ok(Source {
            decoding: Decoding {
                input,
                stream: index,
                decoder,
                input_done: false,
                cancel: cancel.clone(),
            },
            time_base,
            start,
            window: i64::MAX,
            frame_step,
            next_pts: start,
            still: None,
            done: false,
        })
    }

    /// Opens the planned input, limited to `read` seconds after the start.
    pub(super) fn open(plan: &Plan, read: f64, cancel: &CancelToken) -> Result<Source> {
        Self::open_window(&plan.input, &plan.source, plan.start, read, cancel)
    }

    /// Opens `path`, probed as `probe`, limited to `read` seconds after
    /// `start`.
    pub(super) fn open_window(
        path: &Path,
        probe: &Probe,
        start: f64,
        read: f64,
        cancel: &CancelToken,
    ) -> Result<Source> {
        let mut source = Self::open_file(path, probe.decoder.as_deref(), cancel)?;
        if probe.still_image {
            source.still = Some((None, 0));
            source.window = (read * f64::from(STILL_FPS)).ceil() as i64;
            return Ok(source);
        }
        source.start += to_units(start, source.time_base);
        source.window = to_units(read, source.time_base);
        if start > 0.0 {
            let av_time_base = Rational::new(1, ff::ffi::AV_TIME_BASE);
            let target = source.start.rescale(source.time_base, av_time_base);
            // lands on a keyframe at or before the start; earlier frames are
            // dropped. Like the ffmpeg command line, a failed seek only
            // means decoding from the beginning, as with raw streams.
            let _ = source.decoding.input.seek(target, ..target);
        }
        Ok(source)
    }

    pub(super) fn decoder(&self) -> &ff::decoder::Video {
        &self.decoding.decoder
    }

    pub(super) fn input(&self) -> &format::context::Input {
        &self.decoding.input
    }

    pub(super) fn stream(&self) -> format::stream::Stream<'_> {
        self.decoding.input.stream(self.decoding.stream).expect("opened from this input")
    }

    /// Time base of the frames returned by [`Source::next`].
    pub(super) fn time_base(&self) -> Rational {
        if self.still.is_some() { Rational::new(1, STILL_FPS) } else { self.time_base }
    }

    /// Decodes the next frame of the window into `frame`, with timestamps
    /// starting at 0. Returns false at the end.
    pub(super) fn next(&mut self, frame: &mut frame::Video) -> Result<bool> {
        if self.done {
            return Ok(false);
        }
        if let Some((image, index)) = &mut self.still {
            if image.is_none() {
                let mut decoded = frame::Video::empty();
                if !self.decoding.next(&mut decoded)? {
                    return Ok(false);
                }
                *image = Some(decoded);
            }
            if *index >= self.window {
                self.done = true;
                return Ok(false);
            }
            *frame = image.clone().expect("decoded above");
            frame.set_pts(Some(*index));
            *index += 1;
            return Ok(true);
        }

        while self.decoding.next(frame)? {
            // like the ffmpeg command line, number frames without timestamps
            let pts = frame.timestamp().or(frame.pts()).unwrap_or(self.next_pts);
            self.next_pts = pts.saturating_add(self.frame_step);
            let relative = pts.saturating_sub(self.start);
            if relative < 0 {
                continue;
            }
            if relative >= self.window {
                self.done = true;
                return Ok(false);
            }
            frame.set_pts(Some(relative));
            return Ok(true);
        }
        self.done = true;
        Ok(false)
    }

    /// Arguments of a `buffer` filter that receives `frame`.
    pub(super) fn buffer_args(&self, frame: &frame::Video) -> String {
        let time_base = self.time_base();
        let aspect = frame.aspect_ratio();
        let aspect = if aspect.numerator() > 0 { aspect } else { Rational::new(1, 1) };
        format!(
            "video_size={}x{}:pix_fmt={}:time_base={}/{}:pixel_aspect={}/{}",
            frame.width(),
            frame.height(),
            ff::ffi::AVPixelFormat::from(frame.format()) as i32,
            time_base.numerator(),
            time_base.denominator(),
            aspect.numerator(),
            aspect.denominator(),
        )
    }
}
