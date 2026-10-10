use std::ffi::OsString;
use std::path::Path;

use crate::convert::{Plan, frame_count};
use crate::events::{Params, Rate};
use crate::options::Scaling;

/// Formats a number for ffmpeg without float noise like `0.30000000000000004`.
fn num(value: f64) -> String {
    let text = format!("{value:.6}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Input options and the input itself, for a clip of `length` seconds at
/// `fps`. Reads one frame more than needed; the filter chain cuts it to an
/// exact frame count.
pub(crate) fn input_args(plan: &Plan, length: f64, fps: f64) -> Vec<OsString> {
    let mut args: Vec<OsString> = Vec::new();
    if plan.source.still_image {
        args.extend(["-loop".into(), "1".into()]);
    } else if plan.start > 0.0 {
        args.extend(["-ss".into(), num(plan.start).into()]);
    }
    let read = (frame_count(length, fps) + 1) as f64 / fps;
    args.extend(["-t".into(), num(read).into()]);
    if let Some(decoder) = &plan.source.decoder {
        args.extend(["-c:v".into(), decoder.into()]);
    }
    args.extend(["-i".into(), plan.input.clone().into()]);
    args
}

/// Filter chain that turns the source into `length` seconds of frames of
/// the planned size.
///
/// The frame count is cut here rather than with `-frames:v`: ffmpeg before
/// 7.0 stops the encoder at that limit without flushing it, so libvpx's
/// first pass misses its end-of-stream statistics and the second pass
/// fails.
pub(crate) fn video_filter(plan: &Plan, fps: f64, length: f64, pix_fmt: &str) -> String {
    let (w, h) = (plan.width, plan.height);
    let (sw, sh) = (plan.scaled_width, plan.scaled_height);
    let mut filters =
        vec![format!("fps={}", num(fps)), format!("trim=end_frame={}", frame_count(length, fps))];
    if let Some(crop) = plan.crop {
        // in display pixels, which differ from stored ones by the sample
        // aspect ratio; exact, or odd sizes and places are rounded to the
        // chroma planes' (a 1x1 crop to nothing)
        let (width, height) = (plan.source.width, plan.source.height);
        filters.push(format!(
            "crop=w=iw*{}/{width}:h=ih*{}/{height}:x=iw*{}/{width}:y=ih*{}/{height}:exact=1",
            crop.width, crop.height, crop.x, crop.y
        ));
    }
    filters.extend(scale_filters(plan));
    // around the middle; pad keeps chroma whole, so pixel blocks stay
    // apart from their neighbours' colour
    if sw > w || sh > h {
        filters.push(format!("crop={}:{}", w.min(sw), h.min(sh)));
    }
    if sw < w || sh < h {
        filters.extend([
            // padding has to be transparent, so alpha must exist first
            "format=yuva420p".into(),
            format!("pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:color=black@0"),
        ]);
    }
    filters.extend(["setsar=1".into(), format!("format={pix_fmt}")]);
    filters.join(",")
}

/// sRGB to linear light and back, for `lutrgb`, which escapes the commas.
const TO_LINEAR: &str =
    "if(lte(val/maxval\\,0.04045)\\,val/12.92\\,maxval*pow((val/maxval+0.055)/1.055\\,2.4))";
const TO_SRGB: &str =
    "if(lte(val/maxval\\,0.0031308)\\,val*12.92\\,maxval*(1.055*pow(val/maxval\\,1/2.4)-0.055))";

/// `scale`, blending in linear light, with alpha premultiplied so
/// transparent pixels lend no colour to the edges.
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

/// Scales the part used to the planned size.
fn scale_filters(plan: &Plan) -> Vec<String> {
    let (sw, sh) = (plan.scaled_width, plan.scaled_height);
    let (used_w, used_h) = plan.used();
    match plan.scaling {
        Scaling::Smooth | Scaling::Auto => {
            in_linear_light(plan, format!("scale={sw}:{sh}:flags=lanczos"))
        }
        // blocks of whole pixels at least as large as asked, then down to
        // the size: only the edges that fall between pixels blend
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
            filters
        }
        Scaling::PixelPerfect => vec![format!("scale={sw}:{sh}:flags=neighbor")],
    }
}

/// The ffmpeg commands of one encode with `params`, as tgradish runs them
/// with ffmpeg as a separate program: both passes, or one when lossless.
/// The built-in ffmpeg does the same. The pass log and the output are
/// placeholders: tgradish encodes into a folder of its own, then writes
/// the result.
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
    args.extend(input_args(plan, params.length, params.fps));

    let pix_fmt = if plan.alpha { "yuva420p" } else { "yuv420p" };
    let mut push = |items: &[&str]| args.extend(items.iter().map(OsString::from));
    push(&["-map", "0:v:0", "-map_metadata", "-1", "-map_chapters", "-1", "-an", "-sn", "-dn"]);
    push(&["-vf", &video_filter(plan, params.fps, params.length, pix_fmt)]);
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
    // `:v` limits them to the video encoder, so they can't change anything
    // else; names are checked while planning
    for (name, value) in &plan.encoder_options {
        args.extend([format!("-{name}:v").into(), value.into()]);
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
