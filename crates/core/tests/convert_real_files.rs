//! End-to-end conversions with a real ffmpeg. Skipped when ffmpeg is not on
//! PATH; tests using `references/` media skip when it is missing.

use std::path::{Path, PathBuf};
use std::process::Command;

use tgradish_core::convert::{Request, convert};
use tgradish_core::events::Event;
use tgradish_core::ffmpeg::{self, CancelToken, Ffmpeg, FfmpegChoice};
use tgradish_core::options::{Fit, Options, Resize, Speed};
use tgradish_core::telegram::{self, Target};
use tgradish_core::{Error, webm};

fn system_ffmpeg() -> Option<Ffmpeg> {
    let found = ffmpeg::locate(FfmpegChoice::System, None).ok();
    if found.is_none() {
        eprintln!("skipped: needs ffmpeg and ffprobe on PATH");
    }
    found
}

fn reference(name: &str) -> Option<PathBuf> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../references").join(name);
    if !path.exists() {
        eprintln!("skipped: needs references/{name}");
        return None;
    }
    Some(path)
}

fn fast(fit: Fit) -> Options {
    Options { fit: Some(fit), speed: Some(Speed::Fast), ..Default::default() }
}

/// First decoded frame as RGBA, using the libvpx decoder to keep alpha.
fn first_frame_rgba(path: &Path) -> Vec<u8> {
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-c:v", "libvpx-vp9", "-i"])
        .arg(path)
        .args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgba", "-"])
        .output()
        .unwrap();
    assert!(out.status.success());
    out.stdout
}

#[test]
fn converts_short_clip_without_spoofing() {
    let (Some(ffmpeg), Some(input)) = (system_ffmpeg(), reference("uhh.mp4")) else { return };
    let dir = tempfile::tempdir().unwrap();
    let request = Request {
        output: Some(dir.path().join("out.webm")),
        options: Options { length: Some(2.0), ..fast(Fit::Bitrate) },
        ..Request::new(input)
    };
    let mut events = Vec::new();
    let outcome = convert(&ffmpeg, &request, &CancelToken::new(), &mut |e| events.push(e)).unwrap();

    assert!(outcome.issues.is_empty(), "{:?}", outcome.issues);
    assert!(!outcome.spoofed);
    assert!(outcome.bytes <= telegram::MAX_BYTES);
    assert!(matches!(events.first(), Some(Event::Started { .. })));
    assert!(matches!(events.last(), Some(Event::Finished { .. })));

    let info = webm::inspect_file(&outcome.output).unwrap();
    assert!((info.header_duration.unwrap() - 2.0).abs() < 0.05);
    assert_eq!(info.title, None, "source metadata must be dropped");
    assert_eq!(info.writing_app.as_deref(), Some(tgradish_core::TOOL_ID));
    assert!(info.signature.is_some());
}

#[test]
fn converts_long_clip_with_spoofing() {
    let (Some(ffmpeg), Some(input)) = (system_ffmpeg(), reference("pig.mp4")) else { return };
    let dir = tempfile::tempdir().unwrap();
    let request = Request {
        output: Some(dir.path().join("out.webm")),
        options: Options { title: Some("pig".into()), ..fast(Fit::Bitrate) },
        ..Request::new(input)
    };
    let outcome = convert(&ffmpeg, &request, &CancelToken::new(), &mut |_| {}).unwrap();

    assert!(outcome.spoofed);
    assert!(outcome.issues.is_empty(), "{:?}", outcome.issues);
    let info = webm::inspect_file(&outcome.output).unwrap();
    let video = info.video.unwrap();
    // 258x320 source scaled so the long side is 512
    assert_eq!((video.width, video.height), (412, 512));
    assert!(info.header_duration.unwrap() < 1.0);
    assert!(info.content_duration.unwrap() > 12.0);
    assert_eq!(info.title.as_deref(), Some("pig"));
    // fitting got close to the limit
    assert!(outcome.bytes as f64 > telegram::MAX_BYTES as f64 * 0.9);
}

#[test]
fn makes_transparent_emoji_from_image() {
    let Some(ffmpeg) = system_ffmpeg() else { return };
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("wide.png");
    // 300x100, left half transparent
    let status = Command::new("ffmpeg")
        .args(["-v", "error", "-f", "lavfi", "-i", "testsrc2=s=300x100:d=1", "-frames:v", "1"])
        .args(["-vf", "format=rgba,geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':a='if(gt(X,150),255,0)'"])
        .arg(&image)
        .status()
        .unwrap();
    assert!(status.success());

    let request = Request {
        options: Options { target: Some(Target::Emoji), length: Some(1.0), ..fast(Fit::Bitrate) },
        ..Request::new(image.clone())
    };
    let outcome = convert(&ffmpeg, &request, &CancelToken::new(), &mut |_| {}).unwrap();
    assert_eq!(outcome.output, dir.path().join("wide.emoji.webm"));
    assert!(outcome.issues.is_empty(), "{:?}", outcome.issues);

    let info = webm::inspect_file(&outcome.output).unwrap();
    let video = info.video.unwrap();
    assert_eq!((video.width, video.height, video.alpha), (100, 100, true));
    // padded to a square: the top left pixel is in the transparent padding
    let rgba = first_frame_rgba(&outcome.output);
    assert_eq!(rgba.len(), 100 * 100 * 4);
    assert!(rgba[3] < 16, "top left alpha is {}", rgba[3]);
    // the middle right is opaque
    let i = (50 * 100 + 90) * 4;
    assert!(rgba[i + 3] > 240, "middle right alpha is {}", rgba[i + 3]);
}

#[test]
fn rejects_contain_for_emoji() {
    let Some(ffmpeg) = system_ffmpeg() else { return };
    let Some(input) = reference("uhh.mp4") else { return };
    let request = Request {
        options: Options {
            target: Some(Target::Emoji),
            resize: Some(Resize::Contain),
            ..Default::default()
        },
        ..Request::new(input)
    };
    let result = convert(&ffmpeg, &request, &CancelToken::new(), &mut |_| {});
    assert!(matches!(result, Err(Error::InvalidOptions(_))), "{result:?}");
}

#[test]
fn cancels_running_conversion() {
    let (Some(ffmpeg), Some(input)) = (system_ffmpeg(), reference("pig.mp4")) else { return };
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("out.webm");
    let request = Request { output: Some(output.clone()), ..Request::new(input) };
    let cancel = CancelToken::new();

    let start = std::time::Instant::now();
    let result = convert(&ffmpeg, &request, &cancel.clone(), &mut |event| {
        if let Event::Progress { fraction, .. } = event
            && fraction > 0.1
        {
            cancel.cancel();
        }
    });
    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
    assert!(start.elapsed().as_secs() < 10);
    assert!(!output.exists());
}
