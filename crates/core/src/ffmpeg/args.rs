use std::ffi::OsString;
use std::path::Path;

use crate::convert::Plan;
use crate::events::{Params, Rate};
use crate::options::Resize;

/// Formats a number for ffmpeg without float noise like `0.30000000000000004`.
fn num(value: f64) -> String {
    let text = format!("{value:.6}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Input options and the input itself, for a clip of `length` seconds.
pub(crate) fn input_args(plan: &Plan, length: f64) -> Vec<OsString> {
    let mut args: Vec<OsString> = Vec::new();
    if plan.source.still_image {
        args.extend(["-loop".into(), "1".into()]);
    } else if plan.start > 0.0 {
        args.extend(["-ss".into(), num(plan.start).into()]);
    }
    args.extend(["-t".into(), num(length).into()]);
    if let Some(decoder) = &plan.source.decoder {
        args.extend(["-c:v".into(), decoder.into()]);
    }
    args.extend(["-i".into(), plan.input.clone().into()]);
    args
}

/// Filter chain that turns the source into frames of the planned size.
pub(crate) fn video_filter(plan: &Plan, fps: f64, pix_fmt: &str) -> String {
    let (w, h) = (plan.width, plan.height);
    let (sw, sh) = (plan.scaled_width, plan.scaled_height);
    let scale = format!("scale={sw}:{sh}:flags=lanczos");
    let mut filters = vec![format!("fps={}", num(fps))];
    match plan.resize {
        Resize::Contain | Resize::Stretch => filters.push(scale),
        Resize::Crop => filters.extend([scale, format!("crop={w}:{h}")]),
        Resize::Pad => filters.extend([
            scale,
            // padding has to be transparent, so alpha must exist first
            "format=yuva420p".into(),
            format!("pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:color=black@0"),
        ]),
    }
    filters.extend(["setsar=1".into(), format!("format={pix_fmt}")]);
    filters.join(",")
}

/// Arguments for one libvpx-vp9 encode. `pass` is the pass number and log
/// file prefix for two-pass encoding. Without `output` the result is
/// discarded, as needed for the first pass.
pub(crate) fn encode_args(
    plan: &Plan,
    params: &Params,
    pass: Option<(u8, &Path)>,
    output: Option<&Path>,
) -> Vec<OsString> {
    let mut args: Vec<OsString> =
        ["-hide_banner", "-nostdin", "-nostats", "-y", "-loglevel", "warning"]
            .map(OsString::from)
            .into();
    args.extend(input_args(plan, params.length));

    let pix_fmt = if plan.alpha { "yuva420p" } else { "yuv420p" };
    let mut push = |items: &[&str]| args.extend(items.iter().map(OsString::from));
    push(&["-map", "0:v:0", "-map_metadata", "-1", "-map_chapters", "-1", "-an", "-sn", "-dn"]);
    push(&["-vf", &video_filter(plan, params.fps, pix_fmt)]);
    push(&["-c:v", "libvpx-vp9"]);
    match params.rate {
        Rate::Bitrate(kbps) => push(&["-b:v", &format!("{}", (kbps * 1000.0).round() as u64)]),
        Rate::Crf(crf) => push(&["-crf", &crf.to_string(), "-b:v", "0"]),
        Rate::Lossless => push(&["-lossless", "1"]),
    }
    push(&["-deadline", "good", "-cpu-used", &plan.speed.cpu_used().to_string(), "-row-mt", "1"]);
    if plan.watermark {
        // stored as WritingApp by ffmpeg's Matroska muxer
        push(&["-metadata", &format!("encoding_tool={}", crate::TOOL_ID)]);
    }
    if let Some(title) = &plan.title {
        push(&["-metadata", &format!("title={title}")]);
    }
    args.extend(plan.extra_args.iter().map(OsString::from));
    if let Some((pass, log)) = pass {
        args.extend(["-pass".into(), pass.to_string().into(), "-passlogfile".into(), log.into()]);
    }
    args.extend(["-progress", "pipe:1"].map(OsString::from));
    match output {
        Some(path) => args.extend(["-f".into(), "webm".into(), path.into()]),
        None => args.extend(["-f", "null", "-"].map(OsString::from)),
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_numbers_without_noise() {
        assert_eq!(num(25.0), "25");
        assert_eq!(num(0.1 + 0.2), "0.3");
        assert_eq!(num(50.0 / 3.0), "16.666667");
    }
}
