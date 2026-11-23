//! ffmpeg command-line builders and filter chains shared by both the process
//! and built-in backends so both produce identical output.
//!
//! Filter order: tempo (`setpts`), frame rate and exact frame count (`fps`,
//! `trim`), crop in display pixels, scaling (in linear light when blending),
//! centring/padding, and output pixel format.

use std::ffi::OsString;
use std::path::Path;

use crate::config::Threads;
use crate::options::Scaling;
use crate::picture::pad_offset;
use crate::presets::Format;
use crate::webm::{Params, Rate};
use crate::webm::{Plan, frame_count};

/// Formats a number for ffmpeg without float noise like `0.30000000000000004`.
fn num(value: f64) -> String {
    let text = format!("{value:.6}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Builds `[option, N]` when a thread limit is configured, or empty for `Auto`
/// (leaving the choice to ffmpeg). Used for `-threads` (decoder before `-i`,
/// encoder after `-c:v`) and global `-filter_threads` /
/// `-filter_complex_threads` (which govern `-vf` and `-lavfi`). A conversion's
/// commands set all of them.
pub(crate) fn threads_args(threads: Threads, option: &str) -> Vec<OsString> {
    match threads.limit() {
        Some(threads) => vec![option.into(), threads.to_string().into()],
        None => Vec::new(),
    }
}

/// Duration in seconds to read for `count` frames at `fps` (includes one extra
/// frame; see [`input_args`]).
pub(crate) fn read_seconds(count: u64, fps: f64) -> f64 {
    (count + 1) as f64 / fps
}

/// Filter chain for preview frames (`width`x`height` at `fps`). Skips `fps=`
/// when `once` is true so resampling cannot drop a short first frame.
pub(crate) fn frames_filter(once: bool, fps: f64, (width, height): (u32, u32)) -> String {
    let resample = if once { String::new() } else { format!("fps={fps},") };
    format!("{resample}scale={width}:{height}:flags=bilinear,setsar=1")
}

/// Filter graph comparing an encoded attempt (at `fps`) against the planned
/// source using SSIM (for `fit = auto`). The source pad `source` runs through
/// `before` and the plan's filters, the attempt pad is `attempt`, and scores
/// leave through `out` when named. Rebuilds timestamps from frame indices
/// because WebM's millisecond rounding can misalign frames. The source (first
/// input to `ssim`) drives the timeline; the attempt's last frame is repeated
/// if it ends early.
pub(crate) fn ssim_graph(
    plan: &Plan,
    fps: f64,
    (source, attempt): (&str, &str),
    before: &str,
    out: &str,
) -> String {
    let retime = |fps: f64| format!("format=yuv420p,settb=AVTB,setpts=N/({fps}*TB)");
    let reference = video_filter(plan, plan.fps, plan.length, "yuv420p");
    format!(
        "[{source}]{before}{reference},{}[s];[{attempt}]{}[a];[s][a]ssim=eof_action=repeat{out}",
        retime(plan.fps),
        retime(fps),
    )
}

/// Input flags and `-i <path>` for `length` seconds at `fps`. Reads one extra
/// frame so `-t` (in input time) never truncates the final frame before the
/// filter chain's `trim` cuts to the exact frame count.
pub(crate) fn input_args(plan: &Plan, length: f64, fps: f64) -> Vec<OsString> {
    let mut args: Vec<OsString> = Vec::new();
    if plan.source.still_image {
        args.extend(["-loop".into(), "1".into()]);
    } else if plan.source.seek(plan.start) > 0.0 {
        args.extend(["-ss".into(), num(plan.source.seek(plan.start)).into()]);
    }
    // Convert output read window back into input time using tempo.
    let read = read_seconds(frame_count(length, fps), fps) * plan.tempo;
    args.extend(["-t".into(), num(read).into()]);
    if let Some(decoder) = &plan.source.decoder {
        args.extend(["-c:v".into(), decoder.into()]);
    }
    args.extend(threads_args(plan.threads, "-threads"));
    args.extend(["-i".into(), plan.input.clone().into()]);
    args
}

/// Filter chain producing `length` seconds of frames at `fps` in `pix_fmt`.
///
/// Trims via `trim=end_frame` rather than `-frames:v` because ffmpeg < 7.0
/// stops the encoder at `-frames:v` without flushing it, causing libvpx
/// first-pass stats to miss EOF and the second pass to fail.
pub(crate) fn video_filter(plan: &Plan, fps: f64, length: f64, pix_fmt: &str) -> String {
    let mut filters = Vec::new();
    if plan.tempo != 1.0 {
        // Use a fine time base so sped-up frames do not collide on coarse
        // time bases (e.g. 1/25).
        filters.extend(["settb=AVTB".into(), format!("setpts=(PTS-STARTPTS)/{}", num(plan.tempo))]);
    }
    filters.extend([
        format!("fps={}", num(fps)),
        format!("trim=end_frame={}", frame_count(length, fps)),
        picture_filter(plan, Format::Webm, pix_fmt),
    ]);
    filters.join(",")
}

/// Spatial filter chain (crop, scale, pad, SAR normalisation, pixel format).
pub(crate) fn picture_filter(plan: &Plan, format: Format, pix_fmt: &str) -> String {
    let (w, h) = (plan.width, plan.height);
    let (sw, sh) = (plan.scaled_width, plan.scaled_height);
    let mut filters = Vec::new();
    if let Some(crop) = plan.crop {
        // Scale crop coordinates from display pixels to stored pixels (which
        // differ by the sample aspect ratio); `exact=1` prevents rounding odd
        // sizes and positions to chroma subsampling multiples (which would turn
        // a 1x1 crop into nothing).
        let (width, height) = (plan.source.width, plan.source.height);
        filters.push(format!(
            "crop=w=iw*{}/{width}:h=ih*{}/{height}:x=iw*{}/{width}:y=ih*{}/{height}:exact=1",
            crop.width, crop.height, crop.x, crop.y
        ));
    }
    filters.extend(scale_filters(plan));
    // Centre-crop any overflow.
    if sw > w || sh > h {
        filters.push(format!("crop={}:{}", w.min(sw), h.min(sh)));
    }
    if sw < w || sh < h {
        filters.extend([
            // Convert to RGBA before padding so transparent margins work and
            // odd sizes are not clipped by 4:2:0 subsampling.
            "format=rgba".into(),
            format!(
                "pad={w}:{h}:{}:{}:color=black@0",
                pad_offset(format, w, sw.min(w)),
                pad_offset(format, h, sh.min(h))
            ),
        ]);
    }
    filters.extend(["setsar=1".into(), format!("format={pix_fmt}")]);
    filters.join(",")
}

/// sRGB <-> linear transfer expressions for `lutrgb` (commas escaped for
/// ffmpeg).
const TO_LINEAR: &str =
    "if(lte(val/maxval\\,0.04045)\\,val/12.92\\,maxval*pow((val/maxval+0.055)/1.055\\,2.4))";
const TO_SRGB: &str =
    "if(lte(val/maxval\\,0.0031308)\\,val*12.92\\,maxval*(1.055*pow(val/maxval\\,1/2.4)-0.055))";

/// Wraps `scale` in linear-light conversion, premultiplying alpha when present
/// so transparent pixels do not bleed colour into edges.
fn in_linear_light(plan: &Plan, scale: String) -> Vec<String> {
    let alpha = plan.source.alpha;
    let lut = |expr: &str| format!("lutrgb=r='{expr}':g='{expr}':b='{expr}'");
    let mut filters =
        vec![format!("format={}", if alpha { "gbrap16le" } else { "gbrp16le" }), lut(TO_LINEAR)];
    if alpha {
        filters.push("premultiply=inplace=1".into());
    }
    filters.push(scale);
    if alpha {
        filters.push("unpremultiply=inplace=1".into());
    }
    filters.push(lut(TO_SRGB));
    filters
}

/// Builds the scaling filter steps for `plan`.
fn scale_filters(plan: &Plan) -> Vec<String> {
    let (sw, sh) = (plan.scaled_width, plan.scaled_height);
    let (used_w, used_h) = plan.used();
    match plan.scaling {
        // Already at target size: skip linear-light round-trip to avoid noise.
        Scaling::Smooth | Scaling::Auto if (sw, sh) == (used_w, used_h) => {
            vec![format!("scale={sw}:{sh}:flags=neighbor")]
        }
        Scaling::Smooth | Scaling::Auto => {
            in_linear_light(plan, format!("scale={sw}:{sh}:flags=lanczos"))
        }
        // Upscale by nearest-neighbour to the next integer multiple, then
        // area-downscale so only fractional pixel boundaries blend.
        Scaling::Sharp => {
            let blocks = (
                used_w * sw.div_ceil(used_w.max(1)).max(1),
                used_h * sh.div_ceil(used_h.max(1)).max(1),
            );
            let mut filters = Vec::new();
            if blocks != (used_w, used_h) {
                filters.push(format!("scale={}:{}:flags=neighbor", blocks.0, blocks.1));
            }
            if blocks != (sw, sh) {
                filters.extend(in_linear_light(plan, format!("scale={sw}:{sh}:flags=area")));
            }
            // Already the displayed size, but stored pixels may not be square:
            // scale to it so setsar=1 makes them square.
            if filters.is_empty() {
                filters.push(format!("scale={sw}:{sh}:flags=neighbor"));
            }
            filters
        }
        Scaling::Integer => vec![format!("scale={sw}:{sh}:flags=neighbor")],
    }
}

/// Returns the equivalent `ffmpeg` command lines for one encode with `params`
/// (two passes, or one for lossless). Pass log and output paths are
/// placeholders; actual runs write into a temporary directory first.
pub fn commands(plan: &Plan, params: &Params) -> Vec<Vec<OsString>> {
    let log = Path::new("tgradish-pass");
    let output = plan.output.as_path();
    let with_name = |args: Vec<OsString>| std::iter::once("ffmpeg".into()).chain(args).collect();
    if params.rate == Rate::Lossless {
        return vec![with_name(encode_args(plan, params, None, Some(output)))];
    }
    vec![
        with_name(encode_args(plan, params, Some((1, log)), None)),
        with_name(encode_args(plan, params, Some((2, log)), Some(output))),
    ]
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
    args.extend(threads_args(plan.threads, "-filter_threads"));
    args.extend(input_args(plan, params.length, params.fps));

    let pix_fmt = if plan.alpha { "yuva420p" } else { "yuv420p" };
    let mut push = |items: &[&str]| args.extend(items.iter().map(OsString::from));
    push(&["-map", "0:v:0", "-map_metadata", "-1", "-map_chapters", "-1", "-an", "-sn", "-dn"]);
    push(&["-vf", &video_filter(plan, params.fps, params.length, pix_fmt)]);
    push(&["-c:v", "libvpx-vp9"]);
    // before what the user gives, which goes over it
    if let Some(threads) = plan.threads.limit() {
        push(&["-threads", &threads.to_string()]);
    }
    match params.rate {
        Rate::Bitrate(kbps) => push(&["-b:v", &format!("{}", (kbps * 1000.0).round() as u64)]),
        Rate::Crf(crf) => push(&["-crf", &crf.to_string(), "-b:v", "0"]),
        Rate::Lossless => push(&["-lossless", "1"]),
    }
    push(&["-deadline", "good", "-cpu-used", &plan.effort.cpu_used().to_string(), "-row-mt", "1"]);
    if plan.watermark {
        // stored as WritingApp by ffmpeg's Matroska muxer
        push(&["-metadata", &format!("encoding_tool={}", crate::TOOL_ID)]);
    }
    if let Some(title) = &plan.title {
        push(&["-metadata", &format!("title={title}")]);
    }
    // `:v` limits them to the video encoder, so they can't change anything
    // else; names are checked while planning
    for (name, value) in &plan.encoder_options {
        args.extend([format!("-{name}:v").into(), value.into()]);
    }
    args.extend(plan.ffmpeg_args.iter().map(OsString::from));
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
