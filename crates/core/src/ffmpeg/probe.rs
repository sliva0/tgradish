//! What ffprobe says about an input, as a [`Probe`] both backends fill
//! the same way, and where its frames start.

use std::path::Path;
use std::process::Command;

use super::orientation::{Orientation, parse_display_matrix};
use super::{CancelToken, Output, run};
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::Value;

use super::Ffmpeg;
use crate::error::{Error, Result};

/// Properties of an input video, as reported by ffprobe.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Probe {
    pub format: String,
    pub codec: String,
    /// Display size, after applying rotation and sample aspect ratio.
    pub width: u32,
    pub height: u32,
    /// The average frame rate.
    pub fps: Option<f64>,
    /// The stream's base frame rate, which a video of variable frame rate
    /// reaches where it changes fastest.
    pub base_fps: Option<f64>,
    /// Duration in seconds, missing for still images.
    pub duration: Option<f64>,
    /// Where the video starts, in seconds after the file's start, which
    /// other streams can start before. Times into the video, as cuts and
    /// [`frame_starts`] give them, count from here.
    pub start: f64,
    pub alpha: bool,
    pub still_image: bool,
    /// How frames are turned for display. Width and height above already
    /// take it into account.
    pub orientation: Orientation,
    /// Decoder that has to be forced to keep transparency: ffmpeg's native
    /// VP8/VP9 decoders ignore WebM alpha.
    pub decoder: Option<String>,
}

impl Probe {
    /// Where reading the file starts for `start` seconds into the video,
    /// at the start a hair before its first frame, so it stays.
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

pub(crate) fn pix_fmt_has_alpha(pix_fmt: &str) -> bool {
    ["yuva", "rgba", "bgra", "argb", "abgr", "gbrap", "ya8", "ya16"]
        .iter()
        .any(|prefix| pix_fmt.starts_with(prefix))
        || pix_fmt == "pal8"
}

pub fn probe(ffmpeg: &Ffmpeg, input: &Path, cancel: &CancelToken) -> Result<Probe> {
    let probe_error = |message: String| Error::Probe { path: input.to_path_buf(), message };

    let mut cmd = Command::new(&ffmpeg.ffprobe);
    cmd.args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"])
        .args(["-select_streams", "v:0"])
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
    let format_name = json["format"]["format_name"].as_str().unwrap_or_default().to_string();
    let codec = stream["codec_name"].as_str().unwrap_or_default().to_string();
    let pix_fmt = stream["pix_fmt"].as_str().unwrap_or_default();

    let (mut width, mut height) = (
        stream["width"].as_u64().unwrap_or(0) as f64,
        stream["height"].as_u64().unwrap_or(0) as f64,
    );
    if width == 0.0 || height == 0.0 {
        return Err(probe_error("video stream has no size".into()));
    }
    if let Some(sar) = parse_ratio(&stream["sample_aspect_ratio"]) {
        width *= sar;
    }
    let orientation = stream["side_data_list"]
        .as_array()
        .into_iter()
        .flatten()
        .find_map(|data| parse_display_matrix(data["displaymatrix"].as_str()?))
        .map_or(Orientation::Normal, |m| Orientation::from_display_matrix(&m));
    if orientation.swaps_size() {
        std::mem::swap(&mut width, &mut height);
    }

    let webm_alpha = stream["tags"]["alpha_mode"].as_str() == Some("1")
        || stream["tags"]["ALPHA_MODE"].as_str() == Some("1");
    let decoder = match codec.as_str() {
        "vp9" if webm_alpha => Some("libvpx-vp9".to_string()),
        "vp8" if webm_alpha => Some("libvpx".to_string()),
        _ => None,
    };
    let still_image = (format_name.contains("image2") || format_name.ends_with("_pipe"))
        && codec != "gif"
        && stream["nb_frames"].as_str().is_none_or(|frames| frames == "1");

    let start = {
        let time = |value: &Value| value.as_str()?.parse::<f64>().ok().filter(|t| t.is_finite());
        match (time(&stream["start_time"]), time(&json["format"]["start_time"])) {
            (Some(video), Some(file)) => (video - file).max(0.0),
            _ => 0.0,
        }
    };
    Ok(Probe {
        width: width.round() as u32,
        height: height.round() as u32,
        fps: parse_ratio(&stream["avg_frame_rate"])
            .or_else(|| parse_ratio(&stream["r_frame_rate"])),
        base_fps: parse_ratio(&stream["r_frame_rate"]),
        // the file's, where the stream has none, from where the video starts
        duration: if still_image {
            None
        } else {
            parse_f64(&stream["duration"]).or_else(|| {
                parse_f64(&json["format"]["duration"])
                    .map(|duration| duration - start)
                    .filter(|duration| *duration > 0.0)
            })
        },
        start,
        alpha: pix_fmt_has_alpha(pix_fmt) || webm_alpha,
        still_image,
        orientation,
        decoder,
        format: format_name,
        codec,
    })
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
pub fn frame_starts(ffmpeg: &Ffmpeg, input: &Path, cancel: &CancelToken) -> Result<Vec<f64>> {
    let mut cmd = Command::new(&ffmpeg.ffprobe);
    cmd.args(["-v", "error", "-select_streams", "v:0", "-of", "compact"])
        .args(["-show_entries", "format=start_time:packet=pts_time"])
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
