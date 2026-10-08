use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

use super::args::{input_args, video_filter};
use super::{CancelToken, Ffmpeg, Output, run};
use crate::convert::Plan;
use crate::error::{Error, Result};

/// Compares an encoded attempt with the source at the planned frame rate.
/// Lower frame rates lose points on frames they skip. Returns SSIM from 0
/// to 1, ignoring alpha.
pub(crate) fn ssim(
    ffmpeg: &Ffmpeg,
    plan: &Plan,
    candidate: &Path,
    cancel: &CancelToken,
    on_line: &mut dyn FnMut(String),
) -> Result<f64> {
    // both sides start at 0 with the same timebase, or ssim pairs frames wrongly
    let normalize = "format=yuv420p,settb=AVTB,setpts=PTS-STARTPTS";
    let reference = video_filter(plan, plan.fps, "yuv420p");
    let graph =
        format!("[0:v]{normalize}[a];[1:v]{reference},{normalize}[b];[a][b]ssim=eof_action=endall");

    let mut cmd = Command::new(&ffmpeg.ffmpeg);
    cmd.args(["-hide_banner", "-nostdin", "-nostats", "-loglevel", "info", "-i"])
        .arg(candidate)
        .args(input_args(plan, plan.length))
        .args::<_, OsString>(["-lavfi".into(), graph.into(), "-f".into(), "null".into()])
        .arg("-");
    let tail = run(cmd, "ffmpeg", cancel, &mut |output| {
        if let Output::Line(line) = output {
            on_line(line);
        }
    })?;

    tail.iter().rev().find_map(|line| parse_ssim(line)).ok_or_else(|| Error::Ffmpeg {
        program: "ffmpeg",
        status: "no SSIM in the output".into(),
        stderr: tail.join("\n"),
    })
}

fn parse_ssim(line: &str) -> Option<f64> {
    let rest = line.split_once("SSIM ")?.1;
    let value = rest.split_once("All:")?.1.split_whitespace().next()?;
    value.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ffmpeg_summary() {
        let line = "[Parsed_ssim_7 @ 0x7fde8c012d00] SSIM Y:0.992062 (21.002838) \
                    U:0.993409 (21.810564) V:0.993519 (21.883887) All:0.992529 (21.266426)";
        assert_eq!(parse_ssim(line), Some(0.992529));
        assert_eq!(parse_ssim("frame=  75 fps=0.0"), None);
    }
}
