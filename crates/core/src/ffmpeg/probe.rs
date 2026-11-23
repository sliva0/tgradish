//! Media stream probing ([`Probe`]) and packet timestamp extraction shared by
//! both backends.

use std::path::Path;
use std::process::Command;

use super::orientation::{Orientation, parse_display_matrix};
use super::{Output, run};
use crate::CancelToken;
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::Value;

use super::Ffmpeg;
use crate::error::{Error, Result};

/// Properties of an input video, as reported by ffprobe.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct Probe {
    /// Container format name reported by `ffprobe` (e.g. `matroska,webm`,
    /// `gif`, `png_pipe`).
    pub format: String,
    /// Video codec name reported by `ffprobe` (e.g. `vp9`, `h264`).
    pub codec: String,
    /// Display width in pixels, after rotation and sample aspect ratio.
    pub width: u32,
    /// Display height in pixels, after rotation and sample aspect ratio.
    pub height: u32,
    /// Average frame rate.
    pub fps: Option<f64>,
    /// Stream base frame rate (upper bound for variable-frame-rate video).
    pub base_fps: Option<f64>,
    /// Duration in seconds (`None` for still images).
    pub duration: Option<f64>,
    /// Video stream start offset relative to the file start, in seconds. Cuts
    /// and [`Backend::frame_starts`](crate::backend::Backend::frame_starts)
    /// measure time from here.
    pub start: f64,
    /// True if the pixel format supports transparency (alpha channel, palette,
    /// or WebM `ALPHA_MODE`).
    pub alpha: bool,
    /// True for single still images read by an image demuxer.
    pub still_image: bool,
    /// Display orientation (already reflected in `width` and `height`).
    pub orientation: Orientation,
    /// Forced decoder name when needed to preserve transparency (ffmpeg's
    /// native VP8/VP9 decoders ignore WebM alpha).
    pub decoder: Option<String>,
}

impl Probe {
    /// Returns [`Probe::fps`], defaulting to 25 fps (matching ffmpeg's
    /// default).
    pub fn fps_or_default(&self) -> f64 {
        self.fps.unwrap_or(25.0)
    }

    /// Converts a video-relative `start` time into the file-relative `-ss`
    /// offset for ffmpeg. Every read seeks through this because `-ss` counts
    /// from the file start rather than the video stream start. When
    /// `start <= 0`, seeks 1 ms before `self.start` so the first frame is never
    /// skipped.
    pub fn seek(&self, start: f64) -> f64 {
        if start > 0.0 { start + self.start } else { (self.start - 1e-3).max(0.0) }
    }
}

fn parse_ratio(value: &Value) -> Option<f64> {
    let (num, den) = value.as_str()?.split_once(['/', ':'])?;
    let (num, den): (f64, f64) = (num.parse().ok()?, den.parse().ok()?);
    (num > 0.0 && den > 0.0).then(|| num / den)
}

fn parse_f64(value: &Value) -> Option<f64> {
    value.as_str()?.parse().ok().filter(|v: &f64| v.is_finite() && *v > 0.0)
}

fn pix_fmt_has_alpha(pix_fmt: &str) -> bool {
    ["yuva", "rgba", "bgra", "argb", "abgr", "gbrap", "ya8", "ya16"]
        .iter()
        .any(|prefix| pix_fmt.starts_with(prefix))
        // a palette may hold transparent colours, as GIFs and 8-bit PNGs
        // do, which ffprobe can't say: kept, at the cost of encoding alpha
        // for those that have none
        || pix_fmt == "pal8"
}

/// Raw video stream facts collected by either backend before normalising into
/// a [`Probe`].
pub(crate) struct Facts {
    pub format: String,
    pub codec: String,
    pub pix_fmt: String,
    /// Coded dimensions before sample aspect ratio and orientation.
    pub width: f64,
    pub height: f64,
    pub sample_aspect: Option<f64>,
    pub orientation: Orientation,
    pub avg_fps: Option<f64>,
    pub base_fps: Option<f64>,
    /// `(stream_duration, container_duration)` in seconds.
    pub durations: (Option<f64>, Option<f64>),
    /// See [`Probe::start`].
    pub start: f64,
    /// Matroska `ALPHA_MODE=1` tag (VP8/VP9 alpha requires `libvpx` decoders).
    pub webm_alpha: bool,
    /// Reported stream frame count, if available.
    pub frames: Option<u64>,
    /// Packet-derived timing when container headers omit duration (see
    /// [`Facts::untimed`]).
    pub timing: Option<Timing>,
}

/// Frame count and total span measured directly from video packets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Timing {
    pub frames: usize,
    /// Span from the first frame's start to the last frame's end, in seconds.
    pub length: f64,
}

impl Timing {
    /// Computes [`Timing`] from packet `(pts_seconds, duration_seconds)` pairs.
    pub(crate) fn of(packets: impl IntoIterator<Item = (f64, f64)>) -> Option<Timing> {
        let (mut frames, mut first, mut end) = (0, f64::INFINITY, f64::NEG_INFINITY);
        for (start, length) in packets {
            frames += 1;
            first = first.min(start);
            end = end.max(start + length.max(0.0));
        }
        (frames > 0 && end > first).then_some(Timing { frames, length: end - first })
    }

    /// Snaps `self.length` to `frames / fps` when within 0.05%.
    /// ffmpeg quantises APNG delays to 1/100000 s (e.g. 1/12 s becomes
    /// 0.08333 s), which can fall slightly short and drop the last frame. That
    /// rounding loses at most half a unit per frame (0.03% at 60 fps).
    fn length_at(&self, fps: Option<f64>) -> f64 {
        fps.map(|fps| self.frames as f64 / fps)
            .filter(|even| (even - self.length).abs() <= self.length * 5e-4)
            .unwrap_or(self.length)
    }
}

impl Facts {
    fn still_image(&self) -> bool {
        (self.format.contains("image2") || self.format.ends_with("_pipe"))
            // an image demuxer may read a GIF, which can be animated
            // without its frames being counted
            && self.codec != "gif"
            && self.frames.is_none_or(|frames| frames <= 1)
    }

    /// Whether neither the stream nor the file says how long it plays, as
    /// animated pictures, APNG and WebP, don't: its packets have to.
    pub(crate) fn untimed(&self) -> bool {
        !self.still_image() && self.durations == (None, None)
    }

    pub(crate) fn probe(self, input: &Path) -> Result<Probe> {
        let still_image = self.still_image();
        let Facts { format, codec, pix_fmt, width, height, orientation, start, timing, .. } = self;
        if width == 0.0 || height == 0.0 {
            return Err(Error::Unreadable {
                path: input.to_path_buf(),
                message: "video stream has a zero width or height".into(),
            });
        }
        let (mut width, mut height) = (width * self.sample_aspect.unwrap_or(1.0), height);
        if orientation.swaps_size() {
            std::mem::swap(&mut width, &mut height);
        }
        let decoder = match codec.as_str() {
            "vp9" if self.webm_alpha => Some("libvpx-vp9".to_string()),
            "vp8" if self.webm_alpha => Some("libvpx".to_string()),
            _ => None,
        };
        // the file's, where the stream has none, from where the video
        // starts, or else its packets'
        let (stream_duration, file_duration) = self.durations;
        let duration = match still_image {
            true => None,
            false => stream_duration
                .or_else(|| {
                    let from_start = file_duration.map(|duration| duration - start);
                    from_start.filter(|duration| *duration > 0.0)
                })
                .or(timing.map(|timing| timing.length_at(self.avg_fps.or(self.base_fps)))),
        };
        // where the stream gives no average, its packets', rather than its
        // base rate, which can be its time base
        let counted = timing.map(|timing| timing.frames as f64 / timing.length);
        Ok(Probe {
            width: width.round() as u32,
            height: height.round() as u32,
            fps: self.avg_fps.or(counted).or(self.base_fps),
            base_fps: self.base_fps,
            duration,
            start,
            alpha: pix_fmt_has_alpha(&pix_fmt) || self.webm_alpha,
            still_image,
            orientation,
            decoder,
            format,
            codec,
        })
    }
}

pub(crate) fn probe(ffmpeg: &Ffmpeg, input: &Path, cancel: &CancelToken) -> Result<Probe> {
    let probe_error = |message: String| Error::Unreadable { path: input.to_path_buf(), message };

    let mut cmd = Command::new(&ffmpeg.ffprobe);
    cmd.args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"])
        .args(["-select_streams", "v:0"])
        .arg("-i")
        .arg(input);
    let mut stdout = String::new();
    let result = run(cmd, "ffprobe", cancel, &mut |output| {
        if let Output::Stdout(line) = output {
            stdout.push_str(&line);
            stdout.push('\n');
        }
    });
    match result {
        Err(Error::Ffmpeg { stderr, .. }) => return Err(probe_error(stderr.trim().to_string())),
        result => result?,
    };
    let json: Value = serde_json::from_str(&stdout).map_err(|err| probe_error(err.to_string()))?;

    let stream = json["streams"]
        .as_array()
        .and_then(|streams| streams.first())
        .ok_or_else(|| Error::NoVideo(input.to_path_buf()))?;
    let text = |value: &Value| value.as_str().unwrap_or_default().to_string();
    let time = |value: &Value| value.as_str()?.parse::<f64>().ok().filter(|t| t.is_finite());
    let mut facts = Facts {
        format: text(&json["format"]["format_name"]),
        codec: text(&stream["codec_name"]),
        pix_fmt: text(&stream["pix_fmt"]),
        width: stream["width"].as_u64().unwrap_or(0) as f64,
        height: stream["height"].as_u64().unwrap_or(0) as f64,
        sample_aspect: parse_ratio(&stream["sample_aspect_ratio"]),
        orientation: stream["side_data_list"]
            .as_array()
            .into_iter()
            .flatten()
            .find_map(|data| parse_display_matrix(data["displaymatrix"].as_str()?))
            .map_or(Orientation::Normal, |m| Orientation::from_display_matrix(&m)),
        avg_fps: parse_ratio(&stream["avg_frame_rate"]),
        base_fps: parse_ratio(&stream["r_frame_rate"]),
        durations: (parse_f64(&stream["duration"]), parse_f64(&json["format"]["duration"])),
        start: match (time(&stream["start_time"]), time(&json["format"]["start_time"])) {
            (Some(video), Some(file)) => (video - file).max(0.0),
            _ => 0.0,
        },
        webm_alpha: ["alpha_mode", "ALPHA_MODE"]
            .iter()
            .any(|tag| stream["tags"][tag].as_str() == Some("1")),
        // what isn't a number is many
        frames: stream["nb_frames"].as_str().map(|frames| frames.parse().unwrap_or(u64::MAX)),
        timing: None,
    };
    if facts.untimed() {
        facts.timing = timing(ffmpeg, input, cancel)?;
    }
    facts.probe(input)
}

/// How the frames of `input`'s first video stream play, from its packets:
/// `None` when some carry no time.
fn timing(ffmpeg: &Ffmpeg, input: &Path, cancel: &CancelToken) -> Result<Option<Timing>> {
    let mut cmd = Command::new(&ffmpeg.ffprobe);
    cmd.args(["-v", "error", "-select_streams", "v:0", "-of", "compact"])
        .args(["-show_entries", "packet=pts_time,duration_time"])
        .arg("-i")
        .arg(input);
    let (mut packets, mut untimed) = (Vec::new(), false);
    run(cmd, "ffprobe", cancel, &mut |output| {
        let Output::Stdout(line) = output else { return };
        let field = |name| compact_field(&line, "packet", name)?.parse::<f64>().ok();
        match (field("pts_time"), field("duration_time")) {
            (Some(start), Some(length)) => packets.push((start, length)),
            _ if line.trim().starts_with("packet") => untimed = true,
            _ => {}
        }
    })?;
    Ok(if untimed { None } else { Timing::of(packets) })
}

/// The field `name` of a line of ffprobe's compact output about `section`:
/// the section's name, then its fields, each `name=value`, between bars.
fn compact_field<'a>(line: &'a str, section: &str, name: &str) -> Option<&'a str> {
    let mut fields = line.trim().split('|');
    if fields.next()? != section {
        return None;
    }
    fields.find_map(|field| field.strip_prefix(name)?.strip_prefix('='))
}

/// Where the frames of `input`'s first video stream start, in seconds
/// after the file's start, as its packets come: empty when some carry no
/// time.
pub(crate) fn frame_starts(
    ffmpeg: &Ffmpeg,
    input: &Path,
    cancel: &CancelToken,
) -> Result<Vec<f64>> {
    let mut cmd = Command::new(&ffmpeg.ffprobe);
    cmd.args(["-v", "error", "-select_streams", "v:0", "-of", "compact"])
        .args(["-show_entries", "format=start_time:packet=pts_time"])
        .arg("-i")
        .arg(input);
    let (mut file_start, mut starts, mut untimed) = (0.0, Vec::new(), false);
    run(cmd, "ffprobe", cancel, &mut |output| {
        let Output::Stdout(line) = output else { return };
        if let Some(time) = compact_field(&line, "packet", "pts_time") {
            match time.parse::<f64>() {
                Ok(time) => starts.push(time),
                Err(_) => untimed = true,
            }
        } else if let Some(start) = compact_field(&line, "format", "start_time") {
            file_start = start.parse().unwrap_or(0.0);
        }
    })?;
    if untimed {
        return Ok(Vec::new());
    }
    Ok(starts.into_iter().map(|start| start - file_start).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_frames_by_their_rate_past_rounding() {
        let packets = |ticks: f64| (0..4).map(move |frame| (f64::from(frame) * ticks, ticks));
        let timing = Timing::of(packets(0.08333)).unwrap();
        // short of 4 frames at 12 fps, as ffmpeg reads them from an APNG
        assert!(timing.length * 12.0 < 4.0 - 1e-6);
        assert_eq!(timing.length_at(Some(12.0)), 4.0 / 12.0);
        // long, where the unit rounds up
        let timing = Timing::of(packets(0.06667)).unwrap();
        assert_eq!(timing.length_at(Some(15.0)), 4.0 / 15.0);
        // a rate that isn't how the frames play changes nothing, nor none
        let timing = Timing::of(packets(0.1)).unwrap();
        assert_eq!(timing.length_at(Some(25.0)), timing.length);
        assert_eq!(timing.length_at(None), timing.length);
    }

    #[test]
    fn reads_fields_of_compact_output() {
        // as MPEG-TS packets come, with a bar after the last field
        assert_eq!(
            compact_field("packet|pts_time=1.440000|", "packet", "pts_time"),
            Some("1.440000")
        );
        assert_eq!(compact_field("packet|pts_time=N/A", "packet", "pts_time"), Some("N/A"));
        assert_eq!(compact_field("format|start_time=1.4\n", "format", "start_time"), Some("1.4"));
        assert_eq!(compact_field("side_data|pts_time=1", "packet", "pts_time"), None);
        assert_eq!(compact_field("packet|dts_time=1", "packet", "pts_time"), None);
    }
}
