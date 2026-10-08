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
    pub fps: Option<f64>,
    /// Duration in seconds, missing for still images.
    pub duration: Option<f64>,
    pub alpha: bool,
    pub still_image: bool,
    /// How frames are turned for display. Width and height above already
    /// take it into account.
    pub orientation: Orientation,
    /// Decoder that has to be forced to keep transparency: ffmpeg's native
    /// VP8/VP9 decoders ignore WebM alpha.
    pub decoder: Option<String>,
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

    Ok(Probe {
        width: width.round() as u32,
        height: height.round() as u32,
        fps: parse_ratio(&stream["avg_frame_rate"])
            .or_else(|| parse_ratio(&stream["r_frame_rate"])),
        duration: if still_image {
            None
        } else {
            parse_f64(&stream["duration"]).or_else(|| parse_f64(&json["format"]["duration"]))
        },
        alpha: pix_fmt_has_alpha(pix_fmt) || webm_alpha,
        still_image,
        orientation,
        decoder,
        format: format_name,
        codec,
    })
}
