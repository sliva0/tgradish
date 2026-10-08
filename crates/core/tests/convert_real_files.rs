//! End-to-end conversions with a real ffmpeg, through every backend: the
//! system ffmpeg and, with the `linked` feature, the built-in one. Skipped
//! when ffmpeg is not on PATH, since it also makes test input; tests using
//! `references/` media skip when it is missing.

use std::path::{Path, PathBuf};
use std::process::Command;

use tgradish_core::backend::Backend;
use tgradish_core::convert::{Request, convert};
use tgradish_core::events::Event;
use tgradish_core::ffmpeg::{self, CancelToken, FfmpegChoice};
use tgradish_core::options::{Fit, Options, Resize, Speed};
use tgradish_core::telegram::{self, Target};
use tgradish_core::{Error, webm};

/// Backends to test, or `None` without a system ffmpeg.
fn backends() -> Option<Vec<Backend>> {
    let Ok(system) = ffmpeg::locate(FfmpegChoice::System, None) else {
        eprintln!("skipped: needs ffmpeg and ffprobe on PATH");
        return None;
    };
    #[allow(unused_mut)]
    let mut backends = vec![Backend::Process(system)];
    #[cfg(feature = "linked")]
    backends.push(Backend::Linked);
    Some(backends)
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

/// Name for assertion messages.
fn name(backend: &Backend) -> &'static str {
    match backend {
        Backend::Process(_) => "process",
        #[allow(unreachable_patterns)]
        _ => "linked",
    }
}

#[test]
fn converts_short_clip_without_spoofing() {
    let (Some(backends), Some(input)) = (backends(), reference("uhh.mp4")) else { return };
    for backend in &backends {
        let dir = tempfile::tempdir().unwrap();
        let request = Request {
            output: Some(dir.path().join("out.webm")),
            options: Options { length: Some(2.0), ..fast(Fit::Bitrate) },
            ..Request::new(input.clone())
        };
        let mut events = Vec::new();
        let outcome =
            convert(backend, &request, &CancelToken::new(), &mut |e| events.push(e)).unwrap();

        let name = name(backend);
        assert!(outcome.issues.is_empty(), "{name}: {:?}", outcome.issues);
        assert!(!outcome.spoofed);
        assert!(outcome.bytes <= telegram::MAX_BYTES);
        assert!(matches!(events.first(), Some(Event::Started { .. })));
        assert!(matches!(events.last(), Some(Event::Finished { .. })));

        let info = webm::inspect_file(&outcome.output).unwrap();
        assert_eq!(info.video_frames, 50, "{name}");
        assert!((info.header_duration.unwrap() - 2.0).abs() < 1e-3, "{name}");
        assert_eq!(info.title, None, "{name}: source metadata must be dropped");
        assert_eq!(info.writing_app.as_deref(), Some(tgradish_core::TOOL_ID), "{name}");
        assert!(info.signature.is_some());
    }
}

#[test]
fn converts_long_clip_with_spoofing() {
    let (Some(backends), Some(input)) = (backends(), reference("pig.mp4")) else { return };
    for backend in &backends {
        let dir = tempfile::tempdir().unwrap();
        let request = Request {
            output: Some(dir.path().join("out.webm")),
            options: Options { title: Some("pig".into()), ..fast(Fit::Bitrate) },
            ..Request::new(input.clone())
        };
        let outcome = convert(backend, &request, &CancelToken::new(), &mut |_| {}).unwrap();

        let name = name(backend);
        assert!(outcome.spoofed);
        assert!(outcome.issues.is_empty(), "{name}: {:?}", outcome.issues);
        let info = webm::inspect_file(&outcome.output).unwrap();
        let video = info.video.unwrap();
        // 258x320 source scaled so the long side is 512
        assert_eq!((video.width, video.height), (412, 512), "{name}");
        assert_eq!(info.video_frames, 316, "{name}");
        assert!(info.header_duration.unwrap() < 1.0);
        assert!(info.content_duration.unwrap() > 12.0);
        assert_eq!(info.title.as_deref(), Some("pig"), "{name}");
        // fitting got close to the limit
        assert!(outcome.bytes as f64 > telegram::MAX_BYTES as f64 * 0.9, "{name}");
    }
}

#[test]
fn starts_at_offset() {
    let (Some(backends), Some(input)) = (backends(), reference("pig.mp4")) else { return };
    for backend in &backends {
        let dir = tempfile::tempdir().unwrap();
        let request = Request {
            output: Some(dir.path().join("out.webm")),
            options: Options { start: Some(5.0), length: Some(1.0), ..fast(Fit::Off) },
            ..Request::new(input.clone())
        };
        let outcome = convert(backend, &request, &CancelToken::new(), &mut |_| {}).unwrap();
        let info = webm::inspect_file(&outcome.output).unwrap();
        assert_eq!(info.video_frames, 25, "{}", name(backend));
    }
}

#[test]
fn makes_transparent_emoji_from_image() {
    let Some(backends) = backends() else { return };
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

    for backend in &backends {
        let name = name(backend);
        let output = dir.path().join(format!("{name}.webm"));
        let request = Request {
            output: Some(output.clone()),
            options: Options {
                target: Some(Target::Emoji),
                length: Some(1.0),
                ..fast(Fit::Bitrate)
            },
            ..Request::new(image.clone())
        };
        let outcome = convert(backend, &request, &CancelToken::new(), &mut |_| {}).unwrap();
        assert!(outcome.issues.is_empty(), "{name}: {:?}", outcome.issues);

        let info = webm::inspect_file(&output).unwrap();
        let video = info.video.unwrap();
        assert_eq!((video.width, video.height, video.alpha), (100, 100, true), "{name}");
        assert_eq!(info.video_frames, 25, "{name}: still images repeat at 25 fps");
        // padded to a square: the top left pixel is in the transparent padding
        let rgba = first_frame_rgba(&output);
        assert_eq!(rgba.len(), 100 * 100 * 4);
        assert!(rgba[3] < 16, "{name}: top left alpha is {}", rgba[3]);
        // the middle right is opaque
        let i = (50 * 100 + 90) * 4;
        assert!(rgba[i + 3] > 240, "{name}: middle right alpha is {}", rgba[i + 3]);
    }
}

#[test]
fn keeps_alpha_of_webm_input() {
    let Some(backends) = backends() else { return };
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("alpha.webm");
    let status = Command::new("ffmpeg")
        .args(["-v", "error", "-f", "lavfi", "-i", "testsrc2=s=256x256:d=1"])
        .args(["-vf", "format=rgba,geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':a='if(gt(X,128),255,0)',format=yuva420p"])
        .args(["-c:v", "libvpx-vp9"])
        .arg(&input)
        .status()
        .unwrap();
    assert!(status.success());

    for backend in &backends {
        let name = name(backend);
        let probe = backend.probe(&input, &CancelToken::new()).unwrap();
        assert!(probe.alpha, "{name}");
        assert_eq!(probe.decoder.as_deref(), Some("libvpx-vp9"), "{name}");

        let output = dir.path().join(format!("{name}.webm"));
        let request = Request {
            output: Some(output.clone()),
            options: fast(Fit::Off),
            ..Request::new(input.clone())
        };
        convert(backend, &request, &CancelToken::new(), &mut |_| {}).unwrap();
        let rgba = first_frame_rgba(&output);
        assert!(rgba[3] < 16, "{name}: the left side must stay transparent");
    }
}

#[test]
fn rejects_contain_for_emoji() {
    let (Some(backends), Some(input)) = (backends(), reference("uhh.mp4")) else { return };
    let request = Request {
        options: Options {
            target: Some(Target::Emoji),
            resize: Some(Resize::Contain),
            ..Default::default()
        },
        ..Request::new(input)
    };
    for backend in &backends {
        let result = convert(backend, &request, &CancelToken::new(), &mut |_| {});
        assert!(matches!(result, Err(Error::InvalidOptions(_))), "{result:?}");
    }
}

#[test]
fn cancels_running_conversion() {
    let (Some(backends), Some(input)) = (backends(), reference("pig.mp4")) else { return };
    for backend in &backends {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("out.webm");
        let request = Request { output: Some(output.clone()), ..Request::new(input.clone()) };
        let cancel = CancelToken::new();

        let start = std::time::Instant::now();
        let result = convert(backend, &request, &cancel.clone(), &mut |event| {
            if let Event::Progress { fraction, .. } = event
                && fraction > 0.1
            {
                cancel.cancel();
            }
        });
        assert!(matches!(result, Err(Error::Cancelled)), "{}: {result:?}", name(backend));
        assert!(start.elapsed().as_secs() < 10);
        assert!(!output.exists());
    }
}

#[test]
fn stays_under_three_seconds_without_spoofing() {
    let (Some(backends), Some(input)) = (backends(), reference("uhh.mp4")) else { return };
    for backend in &backends {
        let dir = tempfile::tempdir().unwrap();
        let request = Request {
            output: Some(dir.path().join("out.webm")),
            options: Options {
                length: Some(3.0),
                fps: Some(29.97),
                spoof: Some(tgradish_core::options::Spoof::Never),
                ..fast(Fit::Bitrate)
            },
            ..Request::new(input.clone())
        };
        let outcome = convert(backend, &request, &CancelToken::new(), &mut |_| {}).unwrap();
        let name = name(backend);
        assert!(outcome.issues.is_empty(), "{name}: {:?}", outcome.issues);
        let header = webm::inspect_file(&outcome.output).unwrap().header_duration.unwrap();
        assert!(header <= telegram::MAX_SECONDS, "{name}: header duration {header}");
        // 89 frames at 29.97 fps
        assert!((header - 89.0 / 29.97).abs() < 2e-3, "{name}: header duration {header}");
    }
}

#[test]
fn cancelled_before_start_runs_nothing() {
    let (Some(backends), Some(input)) = (backends(), reference("uhh.mp4")) else { return };
    for backend in &backends {
        let cancel = CancelToken::new();
        cancel.cancel();
        let mut events = 0;
        let result = convert(backend, &Request::new(input.clone()), &cancel, &mut |_| events += 1);
        assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
        assert_eq!(events, 0);
    }
}

#[test]
fn ssim_penalizes_dropped_frames() {
    use tgradish_core::convert::plan;

    let Some(backends) = backends() else { return };
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("flash.mkv");
    // dark frames with a bright flash on every third one
    let status = Command::new("ffmpeg")
        .args(["-v", "error", "-f", "lavfi", "-i"])
        .arg("color=black:s=512x512:r=30:d=2,geq=lum='if(eq(mod(N\\,3)\\,2)\\,235\\,16)':cb=128:cr=128")
        .args(["-c:v", "ffv1"])
        .arg(&source)
        .status()
        .unwrap();
    assert!(status.success());
    let candidate = |fps: &str| {
        let path = dir.path().join(format!("{fps}.mkv"));
        let status = Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(&source)
            .args(["-vf", &format!("fps={fps}"), "-c:v", "ffv1"])
            .arg(&path)
            .status()
            .unwrap();
        assert!(status.success());
        path
    };
    let (full_rate, third_rate) = (candidate("30"), candidate("10"));

    for backend in &backends {
        let cancel = CancelToken::new();
        let probe = backend.probe(&source, &cancel).unwrap();
        let (plan, _) = plan(&Request::new(source.clone()), probe).unwrap();
        let full = backend.ssim(&plan, &full_rate, 30.0, &cancel, &mut |_| {}).unwrap();
        let dropped = backend.ssim(&plan, &third_rate, 10.0, &cancel, &mut |_| {}).unwrap();
        let name = name(backend);
        assert!(full > 0.99, "{name}: full frame rate scored {full}");
        assert!(dropped < 0.9, "{name}: dropping every flash scored {dropped}");
    }
}

/// Both backends should produce the same probe results and similar files.
#[test]
fn backends_agree() {
    let (Some(backends), Some(input)) = (backends(), reference("pig.mp4")) else { return };
    if backends.len() < 2 {
        return;
    }
    let probes: Vec<_> =
        backends.iter().map(|b| b.probe(&input, &CancelToken::new()).unwrap()).collect();
    assert_eq!(probes[0], probes[1]);

    let sizes: Vec<_> = backends
        .iter()
        .map(|backend| {
            let dir = tempfile::tempdir().unwrap();
            let request = Request {
                output: Some(dir.path().join("out.webm")),
                options: Options { bitrate: Some(150.0), length: Some(4.0), ..fast(Fit::Off) },
                ..Request::new(input.clone())
            };
            convert(backend, &request, &CancelToken::new(), &mut |_| {}).unwrap().bytes as f64
        })
        .collect();
    let difference = (sizes[0] - sizes[1]).abs() / sizes[0];
    assert!(difference < 0.05, "sizes differ by {:.1}%: {sizes:?}", difference * 100.0);
}
