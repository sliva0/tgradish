//! Decoding the input the way the ffmpeg command line does: seek to the
//! start, drop frames before it, stop after the read window, repeat still
//! images.

use std::path::Path;

use ff::{Packet, Rational, codec, format, frame};
use ffmpeg_next as ff;

use super::libav;
use crate::convert::Plan;
use crate::error::{Error, Result};

/// Frame rate still images are repeated at, like ffmpeg's image demuxer.
const STILL_FPS: i32 = 25;

pub(super) struct Source {
    input: format::context::Input,
    stream: usize,
    decoder: ff::decoder::Video,
    time_base: Rational,
    /// Start of the wanted window, in `time_base` units.
    start: i64,
    /// Length of the window, in `time_base` units.
    window: i64,
    /// For still images: the image and the index of the next repeat.
    still: Option<(Option<frame::Video>, i64)>,
    input_done: bool,
    done: bool,
}

/// Seconds to `time_base` units.
fn to_units(seconds: f64, time_base: Rational) -> i64 {
    (seconds * f64::from(time_base.denominator()) / f64::from(time_base.numerator())).round() as i64
}

impl Source {
    /// Opens a file and its best video stream with the default decoder.
    pub(super) fn open_file(path: &Path, decoder: Option<&str>) -> Result<Source> {
        let input = format::input(path)
            .map_err(|err| Error::Probe { path: path.to_path_buf(), message: err.to_string() })?;
        let stream = input
            .streams()
            .best(ff::media::Type::Video)
            .ok_or_else(|| Error::NoVideo(path.to_path_buf()))?;
        let (index, time_base) = (stream.index(), stream.time_base());
        let mut context = codec::context::Context::from_parameters(stream.parameters())
            .map_err(libav("reading stream parameters"))?;
        context.set_threading(codec::threading::Config::kind(codec::threading::Type::Frame));
        let decoder = match decoder.and_then(ff::decoder::find_by_name) {
            Some(codec) => context.decoder().open_as(codec).and_then(|d| d.video()),
            None => context.decoder().video(),
        }
        .map_err(libav("opening the decoder"))?;
        Ok(Source {
            input,
            stream: index,
            decoder,
            time_base,
            start: 0,
            window: i64::MAX,
            still: None,
            input_done: false,
            done: false,
        })
    }

    /// Opens the planned input, limited to `read` seconds after the start.
    pub(super) fn open(plan: &Plan, read: f64) -> Result<Source> {
        let mut source = Self::open_file(&plan.input, plan.source.decoder.as_deref())?;
        if plan.source.still_image {
            source.still = Some((None, 0));
            source.window = (read * f64::from(STILL_FPS)).ceil() as i64;
            return Ok(source);
        }
        source.start = to_units(plan.start, source.time_base);
        source.window = to_units(read, source.time_base);
        if plan.start > 0.0 {
            let target = (plan.start * f64::from(ff::ffi::AV_TIME_BASE)) as i64;
            // lands on a keyframe at or before the start; earlier frames are dropped
            source.input.seek(target, ..target).map_err(libav("seeking"))?;
        }
        Ok(source)
    }

    pub(super) fn decoder(&self) -> &ff::decoder::Video {
        &self.decoder
    }

    pub(super) fn input(&self) -> &format::context::Input {
        &self.input
    }

    pub(super) fn stream(&self) -> format::stream::Stream<'_> {
        self.input.stream(self.stream).expect("opened from this input")
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
                if !Self::decode(
                    &mut self.input,
                    self.stream,
                    &mut self.decoder,
                    &mut self.input_done,
                    &mut decoded,
                )? {
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

        while Self::decode(
            &mut self.input,
            self.stream,
            &mut self.decoder,
            &mut self.input_done,
            frame,
        )? {
            let Some(pts) = frame.timestamp().or(frame.pts()) else { continue };
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

    /// Decodes the next frame of the stream, reading packets as needed.
    fn decode(
        input: &mut format::context::Input,
        stream: usize,
        decoder: &mut ff::decoder::Video,
        input_done: &mut bool,
        frame: &mut frame::Video,
    ) -> Result<bool> {
        loop {
            match decoder.receive_frame(frame) {
                Ok(()) => return Ok(true),
                Err(ff::Error::Eof) => return Ok(false),
                Err(ff::Error::Other { errno }) if errno == ff::util::error::EAGAIN => {}
                Err(err) => return Err(libav("decoding")(err)),
            }
            if *input_done {
                return Ok(false);
            }
            let mut packet = Packet::empty();
            match packet.read(input) {
                Ok(()) if packet.stream() == stream => {
                    decoder.send_packet(&packet).map_err(libav("decoding"))?;
                }
                Ok(()) | Err(ff::Error::InvalidData) => {}
                Err(ff::Error::Eof) => {
                    *input_done = true;
                    decoder.send_eof().map_err(libav("decoding"))?;
                }
                Err(err) => return Err(libav("reading the input")(err)),
            }
        }
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

/// Filters that apply the display matrix, as ffmpeg's autorotation does.
pub(super) fn rotation_filters(rotation: u16) -> &'static str {
    match rotation {
        90 => "transpose=clock,",
        180 => "hflip,vflip,",
        270 => "transpose=cclock,",
        _ => "",
    }
}
