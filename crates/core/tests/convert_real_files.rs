//! End-to-end conversions with a real ffmpeg, through every backend: the
//! system ffmpeg and, with the `linked` feature, the built-in one. Skipped
//! when ffmpeg is not on PATH, since it also makes test input.

use std::path::{Path, PathBuf};
use std::process::Command;

use tgradish_core::CancelToken;
use tgradish_core::backend::{Backend, FramesRequest};
use tgradish_core::convert::{Request, convert};
use tgradish_core::events::Event;
use tgradish_core::ffmpeg::{self, FfmpegChoice};
use tgradish_core::options::{Crop, Fit, Options, Resize, Scaling, Speed};
use tgradish_core::telegram::{self, Target};
use tgradish_core::{Error, webm};
use tgradish_corpus::Clip;

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

/// A clip like phone footage, made once with ffmpeg and kept for later
/// runs.
fn clip(kind: Clip) -> Option<PathBuf> {
    tgradish_corpus::clip(kind, Path::new(env!("CARGO_TARGET_TMPDIR")))
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
    let (Some(backends), Some(input)) = (backends(), clip(Clip::Square)) else { return };
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
        assert!(outcome.bytes <= telegram::MAX_STICKER_BYTES);
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
fn fits_emoji_into_their_smaller_limit() {
    let (Some(backends), Some(input)) = (backends(), clip(Clip::Square)) else { return };
    for backend in &backends {
        let dir = tempfile::tempdir().unwrap();
        let request = Request {
            output: Some(dir.path().join("out.webm")),
            options: Options {
                target: Some(Target::Emoji),
                length: Some(2.0),
                ..fast(Fit::Bitrate)
            },
            ..Request::new(input.clone())
        };
        let outcome = convert(backend, &request, &CancelToken::new(), &mut |_| {}).unwrap();

        let name = name(backend);
        assert!(outcome.issues.is_empty(), "{name}: {:?}", outcome.issues);
        assert!(outcome.bytes <= telegram::MAX_EMOJI_BYTES, "{name}: {}", outcome.bytes);
        // fitting aimed at the emoji limit, not the sticker one
        assert!(outcome.bytes as f64 > telegram::MAX_EMOJI_BYTES as f64 * 0.8, "{name}");
    }
}

#[test]
fn converts_long_clip_with_spoofing() {
    let (Some(backends), Some(input)) = (backends(), clip(Clip::Tall)) else { return };
    for backend in &backends {
        let dir = tempfile::tempdir().unwrap();
        let request = Request {
            output: Some(dir.path().join("out.webm")),
            options: Options { title: Some("tall".into()), ..fast(Fit::Bitrate) },
            ..Request::new(input.clone())
        };
        let outcome = convert(backend, &request, &CancelToken::new(), &mut |_| {}).unwrap();

        let name = name(backend);
        assert!(outcome.spoofed);
        assert!(outcome.issues.is_empty(), "{name}: {:?}", outcome.issues);
        let info = webm::inspect_file(&outcome.output).unwrap();
        let video = info.video.unwrap();
        // 258x320 scaled so the long side is 512
        assert_eq!((video.width, video.height), (412, 512), "{name}");
        // ffmpeg 6.0's fps filter leaves out the last frame
        assert!((315..=316).contains(&info.video_frames), "{name}: {}", info.video_frames);
        assert!(info.header_duration.unwrap() < 1.0);
        assert!(info.content_duration.unwrap() > 12.0);
        assert_eq!(info.title.as_deref(), Some("tall"), "{name}");
        // fitting got close to the limit
        assert!(outcome.bytes as f64 > telegram::MAX_STICKER_BYTES as f64 * 0.9, "{name}");
    }
}

#[test]
fn starts_at_offset() {
    let (Some(backends), Some(input)) = (backends(), clip(Clip::Tall)) else { return };
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
fn previews_results() {
    let (Some(backends), Some(input)) = (backends(), clip(Clip::Square)) else { return };
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("preview.webm");
    let request = Request {
        output: Some(output.clone()),
        options: Options { length: Some(1.0), ..fast(Fit::Bitrate) },
        ..Request::new(input)
    };
    convert(&backends[0], &request, &CancelToken::new(), &mut |_| {}).unwrap();
    // spoofed, it still plays for a second
    let spoofed = dir.path().join("spoofed.webm");
    let changes = webm::Patch { duration: Some(0.25), ..Default::default() };
    webm::patch_file(&output, &spoofed, &changes, false).unwrap();
    for backend in &backends {
        let name = name(backend);
        for path in [&output, &spoofed] {
            let preview = backend.preview(path, 320, &CancelToken::new()).unwrap();
            // the sticker is 512 px on its longer side; previews are 320
            assert_eq!(preview.width.max(preview.height), 320, "{name}");
            assert!(!preview.frames.is_empty(), "{name}");
            let ticks: u32 = preview.frames.iter().map(|(_, ticks)| ticks).sum();
            assert!((55..=65).contains(&ticks), "{name}: one second is {ticks} ticks");
            for (rgba, _) in &preview.frames {
                assert_eq!(rgba.len(), (preview.width * preview.height * 4) as usize, "{name}");
            }
        }
    }
}

#[test]
fn decodes_frames_of_a_part_of_a_video() {
    let (Some(backends), Some(input)) = (backends(), clip(Clip::Square)) else { return };
    let request =
        FramesRequest { start: 1.0, length: Some(1.0), fps: 10.0, max_side: 200, max_frames: 5 };
    let mut decoded = Vec::new();
    for backend in &backends {
        let name = name(backend);
        let probe = backend.probe(&input, &CancelToken::new()).unwrap();
        let frames = backend.frames(&input, &probe, &request, &CancelToken::new()).unwrap();
        assert_eq!(frames.width.max(frames.height), 200, "{name}");
        // ten frames a second would be too many
        assert_eq!((frames.fps, frames.frames.len()), (5.0, 5), "{name}");
        decoded.push(frames.frames);
    }
    // the backends decode the same frames
    for pair in decoded.windows(2) {
        for (a, b) in pair[0].iter().zip(&pair[1]) {
            let difference: u64 = a.iter().zip(b).map(|(a, b)| u64::from(a.abs_diff(*b))).sum();
            assert!(difference / (a.len() as u64) < 8, "frames differ by {difference}");
        }
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
        // the hidden mark, in the track's UID
        let mark = tgradish_core::mark::read_file(&output).unwrap();
        assert_eq!(mark, tgradish_core::mark::Mark::current(), "{name}");
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
fn crops_odd_sizes_exactly() {
    let Some(backends) = backends() else { return };
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("clip.mp4");
    let status = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=s=200x100:d=0.5",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&input)
        .status()
        .unwrap();
    assert!(status.success());
    for backend in &backends {
        let name = name(backend);
        // odd sizes and places, which chroma subsampling would round
        for (crop, size) in [
            (Crop { x: 1, y: 1, width: 101, height: 51 }, (512, 258)),
            (Crop { x: 3, y: 5, width: 1, height: 1 }, (512, 512)),
        ] {
            let output = dir.path().join(format!("{name}-{}x{}.webm", crop.width, crop.height));
            let request = Request {
                output: Some(output.clone()),
                options: Options { crop: Some(crop), ..fast(Fit::Off) },
                ..Request::new(input.clone())
            };
            convert(backend, &request, &CancelToken::new(), &mut |_| {}).unwrap();
            let video = webm::inspect_file(&output).unwrap().video.unwrap();
            assert_eq!(
                (video.width, video.height),
                (size.0 as u64, size.1 as u64),
                "{name} {crop}"
            );
        }
    }
}

#[test]
fn keeps_pixel_art_sharp() {
    let Some(backends) = backends() else { return };
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("art.png");
    // 8x8 art pixels of red and blue by turns
    let status = Command::new("ffmpeg")
        .args(["-v", "error", "-f", "lavfi", "-i", "color=black:s=8x8:d=1", "-frames:v", "1"])
        .args(["-vf", "format=rgb24,geq=r='255*mod(X+Y,2)':g='0':b='255*(1-mod(X+Y,2))'"])
        .arg(&image)
        .status()
        .unwrap();
    assert!(status.success());

    for backend in &backends {
        let name = name(backend);
        let output = dir.path().join(format!("{name}.webm"));
        let request = Request {
            output: Some(output.clone()),
            options: Options { length: Some(0.2), crf: Some(10), ..fast(Fit::Off) },
            ..Request::new(image.clone())
        };
        let mut scaling = None;
        convert(backend, &request, &CancelToken::new(), &mut |event| {
            if let Event::Started { plan } = event {
                scaling = Some(plan.scaling);
            }
        })
        .unwrap();
        assert_eq!(scaling, Some(Scaling::Sharp), "{name}: few colours, made 64 times larger");
        // 64 output pixels to an art pixel: blocks meet without blending
        let rgba = first_frame_rgba(&output);
        let at = |x: usize, y: usize| &rgba[(y * 512 + x) * 4..][..3];
        let (blue, red) = (at(62, 32), at(65, 32));
        assert!(blue[2] > 200 && blue[0] < 60, "{name}: {blue:?}");
        assert!(red[0] > 200 && red[2] < 60, "{name}: {red:?}");
    }
}

#[test]
fn pads_pictures_of_odd_size_whole() {
    let Some(backends) = backends() else { return };
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("odd.png");
    // 511x255, blue but for a red last column and a green last row; the
    // source makes RGB itself, as ffmpeg 6 rounds odd 4:2:0 sizes down
    let status = Command::new("ffmpeg")
        .args(["-v", "error", "-f", "lavfi", "-i", "color=black:s=511x255:d=1,format=rgb24"])
        .args(["-frames:v", "1"])
        .args([
            "-vf",
            "geq=r='255*eq(X,510)':g='255*eq(Y,254)*lt(X,510)':b='255*lt(X,510)*lt(Y,254)'",
        ])
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
                scaling: Some(Scaling::PixelPerfect),
                length: Some(0.2),
                lossless: Some(true),
                ..fast(Fit::Off)
            },
            ..Request::new(image.clone())
        };
        convert(backend, &request, &CancelToken::new(), &mut |_| {}).unwrap();
        let video = webm::inspect_file(&output).unwrap().video.unwrap();
        assert_eq!((video.width, video.height), (512, 256), "{name}");
        // the last column and row are there, inside the transparent margin;
        // half-resolution colour mixes a line of one pixel with its neighbour
        let rgba = first_frame_rgba(&output);
        let at = |x: usize, y: usize| &rgba[(y * 512 + x) * 4..][..4];
        let [r, _, b, a] = at(510, 100).try_into().unwrap();
        assert!(r > 120 && r > b && a > 200, "{name}: {:?}", at(510, 100));
        let [_, g, b, a] = at(100, 254).try_into().unwrap();
        assert!(g > 120 && g > b && a > 200, "{name}: {:?}", at(100, 254));
        assert!(at(511, 100)[3] < 30, "{name}: {:?}", at(511, 100));
    }
}

#[test]
fn rejects_contain_for_emoji() {
    let (Some(backends), Some(input)) = (backends(), clip(Clip::Square)) else { return };
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
    let (Some(backends), Some(input)) = (backends(), clip(Clip::Tall)) else { return };
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
    let (Some(backends), Some(input)) = (backends(), clip(Clip::Square)) else { return };
    for backend in &backends {
        let dir = tempfile::tempdir().unwrap();
        let request = Request {
            output: Some(dir.path().join("out.webm")),
            options: Options {
                length: Some(3.0),
                fps: Some(23.976),
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
        // 71 frames at 23.976 fps
        assert!((header - 71.0 / 23.976).abs() < 2e-3, "{name}: header duration {header}");
    }
}

#[test]
fn plays_at_a_tempo() {
    let (Some(backends), Some(input)) = (backends(), clip(Clip::Square)) else { return };
    for backend in &backends {
        let dir = tempfile::tempdir().unwrap();
        let request = Request {
            output: Some(dir.path().join("out.webm")),
            options: Options { length: Some(4.0), tempo: Some(2.0), ..fast(Fit::Bitrate) },
            ..Request::new(input.clone())
        };
        let outcome = convert(backend, &request, &CancelToken::new(), &mut |_| {}).unwrap();
        let name = name(backend);
        let info = webm::inspect_file(&outcome.output).unwrap();
        // 4 seconds at 25 fps play in 2, every frame of them
        assert_eq!(info.video_frames, 100, "{name}");
        let header = info.header_duration.unwrap();
        assert!((header - 2.0).abs() < 2e-3, "{name}: header duration {header}");
    }
}

#[test]
fn cancelled_before_start_runs_nothing() {
    let (Some(backends), Some(input)) = (backends(), clip(Clip::Square)) else { return };
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
    let (Some(backends), Some(input)) = (backends(), clip(Clip::Tall)) else { return };
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

/// Runs the system ffmpeg to make test input.
fn make(args: &[&str]) {
    let status = Command::new("ffmpeg").args(["-v", "error", "-y"]).args(args).status().unwrap();
    assert!(status.success(), "ffmpeg {args:?}");
}

/// Mean absolute difference between two RGBA frames.
fn difference(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), b.len());
    a.iter().zip(b).map(|(x, y)| f64::from(x.abs_diff(*y))).sum::<f64>() / a.len() as f64
}

/// Converts with every backend and returns the first frame of each result.
fn first_frames(backends: &[Backend], input: &Path, options: Options) -> Vec<Vec<u8>> {
    let dir = tempfile::tempdir().unwrap();
    backends
        .iter()
        .map(|backend| {
            let output = dir.path().join(format!("{}.webm", name(backend)));
            let request = Request {
                output: Some(output.clone()),
                options: options.clone(),
                ..Request::new(input.to_path_buf())
            };
            let outcome = convert(backend, &request, &CancelToken::new(), &mut |_| {})
                .unwrap_or_else(|err| panic!("{}: {err}", name(backend)));
            assert!(outcome.issues.is_empty(), "{}: {:?}", name(backend), outcome.issues);
            first_frame_rgba(&output)
        })
        .collect()
}

#[test]
fn handles_offset_and_missing_timestamps() {
    let (Some(backends), Some(input)) = (backends(), clip(Clip::Square)) else { return };
    let dir = tempfile::tempdir().unwrap();
    let ts = dir.path().join("clip.ts");
    let raw = dir.path().join("clip.h264");
    let input = input.to_str().unwrap();
    // MPEG-TS starts at a non-zero timestamp, raw H.264 has none
    make(&["-i", input, "-c", "copy", "-f", "mpegts", ts.to_str().unwrap()]);
    make(&["-i", input, "-c", "copy", "-bsf:v", "h264_mp4toannexb", raw.to_str().unwrap()]);

    // seeking into these needs keyframes, which the clip only has at its
    // start; the ffmpeg command line decodes nothing after such a seek too
    for path in [&ts, &raw] {
        for backend in &backends {
            let dir = tempfile::tempdir().unwrap();
            let request = Request {
                output: Some(dir.path().join("out.webm")),
                options: Options { length: Some(1.0), ..fast(Fit::Off) },
                ..Request::new(path.clone())
            };
            let outcome = convert(backend, &request, &CancelToken::new(), &mut |_| {})
                .unwrap_or_else(|err| panic!("{} {}: {err}", name(backend), path.display()));
            let info = webm::inspect_file(&outcome.output).unwrap();
            assert_eq!(info.video_frames, 25, "{} {}", name(backend), path.display());
        }
    }
}

#[test]
fn applies_display_matrix_like_ffmpeg() {
    let (Some(backends), Some(input)) = (backends(), clip(Clip::Square)) else { return };
    let dir = tempfile::tempdir().unwrap();
    let cases: [(&str, &[&str]); 3] = [
        ("hflip", &["-display_hflip"]),
        ("rotate", &["-display_rotation", "90"]),
        ("vflip", &["-display_vflip"]),
    ];
    for (name, display) in cases {
        let turned = dir.path().join(format!("{name}.mp4"));
        let mut args = display.to_vec();
        args.extend(["-t", "1", "-i", input.to_str().unwrap(), "-c", "copy"]);
        args.push(turned.to_str().unwrap());
        make(&args);

        let probes: Vec<_> =
            backends.iter().map(|b| b.probe(&turned, &CancelToken::new()).unwrap()).collect();
        assert_ne!(probes[0].orientation, ffmpeg::Orientation::Normal, "{name}");
        assert!(probes.windows(2).all(|p| p[0] == p[1]), "{name}: {probes:?}");

        // good enough to compare, small enough for a sticker
        let frames = first_frames(&backends, &turned, Options { crf: Some(24), ..fast(Fit::Off) });
        for frame in &frames[1..] {
            let diff = difference(&frames[0], frame);
            assert!(diff < 4.0, "{name}: backends differ by {diff}");
        }
    }
}

#[test]
fn uses_first_video_stream() {
    let Some(backends) = backends() else { return };
    let dir = tempfile::tempdir().unwrap();
    let two = dir.path().join("two.mkv");
    // the second stream is bigger and marked default, the first one wins
    make(&[
        "-f",
        "lavfi",
        "-i",
        "color=red:s=64x64:d=1",
        "-f",
        "lavfi",
        "-i",
        "color=blue:s=128x128:d=1",
        "-map",
        "0",
        "-map",
        "1",
        "-c:v",
        "ffv1",
        "-disposition:v:0",
        "0",
        "-disposition:v:1",
        "default",
        two.to_str().unwrap(),
    ]);
    for backend in &backends {
        let probe = backend.probe(&two, &CancelToken::new()).unwrap();
        assert_eq!((probe.width, probe.height), (64, 64), "{}", name(backend));
    }
    for frame in first_frames(&backends, &two, fast(Fit::Off)) {
        let (red, blue) = (frame[0], frame[2]);
        assert!(red > 200 && blue < 60, "first pixel is {:?}", &frame[..4]);
    }
}

#[test]
fn passes_encoder_options() {
    let (Some(backends), Some(input)) = (backends(), clip(Clip::Square)) else { return };
    let with = |name: &str, value: &str| Options {
        length: Some(0.5),
        encoder_options: Some([(name.to_string(), value.to_string())].into()),
        ..fast(Fit::Off)
    };
    for backend in &backends {
        let dir = tempfile::tempdir().unwrap();
        let request = Request {
            output: Some(dir.path().join("out.webm")),
            options: with("tune-content", "screen"),
            ..Request::new(input.clone())
        };
        convert(backend, &request, &CancelToken::new(), &mut |_| {})
            .unwrap_or_else(|err| panic!("{}: {err}", name(backend)));

        let request = Request {
            output: Some(dir.path().join("bad.webm")),
            options: with("no-such-option", "1"),
            ..Request::new(input.clone())
        };
        let result = convert(backend, &request, &CancelToken::new(), &mut |_| {});
        assert!(result.is_err(), "{}: unknown option was accepted", name(backend));
    }
}

/// A FIFO without a writer would block the built-in ffmpeg in the OS,
/// where cancelling cannot reach.
#[cfg(all(unix, feature = "linked"))]
#[test]
fn builtin_backend_rejects_fifos() {
    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("fifo.mp4");
    assert!(Command::new("mkfifo").arg(&fifo).status().unwrap().success());
    let result = Backend::Linked.probe(&fifo, &CancelToken::new());
    assert!(matches!(result, Err(Error::Probe { .. })), "{result:?}");
}

#[cfg(feature = "linked")]
#[test]
fn builtin_backend_rejects_extra_args() {
    let Some(input) = clip(Clip::Square) else { return };
    let request = Request {
        options: Options {
            extra_args: Some(vec!["-tune-content".into(), "screen".into()]),
            ..Default::default()
        },
        ..Request::new(input)
    };
    let result = convert(&Backend::Linked, &request, &CancelToken::new(), &mut |_| {});
    assert!(matches!(result, Err(Error::InvalidOptions(_))), "{result:?}");
}

/// Red, green, blue and white: the frames of [`uneven_gif`].
const UNEVEN: [[u8; 4]; 4] =
    [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255], [255, 255, 255, 255]];

/// A GIF of [`UNEVEN`] frames that show for `delays` milliseconds.
fn uneven_gif(dir: &Path, delays: [u32; 4]) -> PathBuf {
    let frames = UNEVEN
        .iter()
        .map(|&colour| {
            let mut canvas = tgradish_corpus::Canvas::new(8, 8);
            canvas.rect(0, 0, 8, 8, colour);
            canvas
        })
        .collect();
    let sprite = tgradish_corpus::Sprite {
        name: "uneven",
        about: "frames that show for different times",
        frames,
        delays: delays.to_vec(),
        scale: 1,
        likely_scale: None,
    };
    let path = dir.join(format!("uneven-{}.gif", delays[0]));
    std::fs::write(&path, tgradish_corpus::encode::gif(&sprite)).unwrap();
    path
}

fn near(starts: &[f64], expected: &[f64]) -> bool {
    starts.len() == expected.len()
        && starts.iter().zip(expected).all(|(start, expected)| (start - expected).abs() < 1e-6)
}

#[test]
fn finds_where_frames_start() {
    let Some(backends) = backends() else { return };
    let dir = tempfile::tempdir().unwrap();
    // frames of a tenth and half a second, and of a fiftieth, shorter than
    // a frame at the average rate
    let cases = [
        (
            [100, 100, 500, 100],
            [0.0, 0.1, 0.2, 0.7],
            [(0.0, 0), (0.15, 1), (0.2, 2), (0.65, 2), (0.7, 3)],
        ),
        (
            [20, 20, 500, 20],
            [0.0, 0.02, 0.04, 0.54],
            [(0.0, 0), (0.02, 1), (0.04, 2), (0.3, 2), (0.54, 3)],
        ),
    ];
    for backend in &backends {
        let name = name(backend);
        for (delays, expected, shown) in cases {
            let gif = uneven_gif(dir.path(), delays);
            let probe = backend.probe(&gif, &CancelToken::new()).unwrap();
            let starts = backend.frame_starts(&gif, &probe, &CancelToken::new()).unwrap();
            assert!(near(&starts, &expected), "{name}: {starts:?}");
            // the frame that shows at a time, however long frames show
            let colour_at = |time: f64| {
                let options = Options { start: Some(time), ..Options::default() };
                let output = Some(dir.path().join(format!("{name}-{}-{time}.webp", delays[0])));
                let request = Request { options, output, ..Request::new(gif.clone()) };
                let made = tgradish_core::still::convert(backend, &request, &CancelToken::new());
                let preview = tgradish_core::still::preview(&made.unwrap().output).unwrap();
                <[u8; 4]>::try_from(&preview.frames[0].0[..4]).unwrap()
            };
            for (time, frame) in shown {
                assert_eq!(colour_at(time), UNEVEN[frame], "{name}: {delays:?} at {time} s");
            }
        }
        // a video with frames out of order in the file, at 25 fps
        if let Some(clip) = clip(Clip::Square) {
            let probe = backend.probe(&clip, &CancelToken::new()).unwrap();
            let starts = backend.frame_starts(&clip, &probe, &CancelToken::new()).unwrap();
            let even: Vec<f64> = (0..121).map(|index| f64::from(index) / 25.0).collect();
            assert!(near(&starts, &even), "{name}: {starts:?}");
        }
    }
}

#[test]
fn counts_time_from_where_the_video_starts() {
    let Some(backends) = backends() else { return };
    let dir = tempfile::tempdir().unwrap();
    // sound from the start, and a second of video at 10 fps from half a
    // second in; Matroska says how long the file is, not the video
    for container in ["mp4", "mkv"] {
        let input = dir.path().join(format!("late.{container}"));
        let status = Command::new("ffmpeg")
            .args(["-v", "error", "-f", "lavfi", "-i", "anullsrc=r=8000:cl=mono"])
            .args(["-itsoffset", "0.5", "-f", "lavfi", "-i", "testsrc2=s=64x48:r=10:d=1"])
            .args(["-map", "0:a", "-map", "1:v", "-t", "1.5", "-c:v", "libx264", "-c:a", "aac"])
            // as the frames come: ffmpeg before 7 fills the wait with copies
            // of the first
            .args(["-fps_mode", "passthrough"])
            .arg(&input)
            .status()
            .unwrap();
        assert!(status.success());
        for backend in &backends {
            let name = format!("{} {container}", name(backend));
            let probe = backend.probe(&input, &CancelToken::new()).unwrap();
            let duration = probe.duration.unwrap();
            let starts = backend.frame_starts(&input, &probe, &CancelToken::new()).unwrap();
            // the video starts half a second into the file, or, where ffmpeg
            // before 7 writes the wait into MP4 as an empty edit, its first
            // frame half a second into it
            let late = starts.first().copied().unwrap_or(0.0);
            assert!(late.abs() < 0.01 || (late - 0.5).abs() < 0.01, "{name}: {starts:?}");
            // half a second after the file's start, or more where sound its
            // encoder primes starts before that
            assert!((0.49..0.7).contains(&(probe.start + late)), "{name}: {probe:?}");
            // from where the video starts; where the file says, to where
            // the longest stream, the sound, ends
            assert!((0.95 + late..1.2 + late).contains(&duration), "{name}: {probe:?}");
            let even: Vec<f64> = (0..10).map(|index| late + f64::from(index) / 10.0).collect();
            assert!(near(&starts, &even), "{name}: {starts:?}");
            // all of it, and frames 5 to 7
            let start = tgradish_core::convert::read_from(even[5], even[4]);
            for (part, frames) in [((None, None), 10), ((Some(start), Some(0.3)), 3)] {
                let request = Request {
                    output: Some(dir.path().join(format!("{name}-{frames}.webm"))),
                    options: Options {
                        start: part.0,
                        length: part.1,
                        fps: Some(10.0),
                        ..fast(Fit::Off)
                    },
                    ..Request::new(input.clone())
                };
                let outcome = convert(backend, &request, &CancelToken::new(), &mut |_| {}).unwrap();
                let info = webm::inspect_file(&outcome.output).unwrap();
                assert_eq!(info.video_frames, frames, "{name}: {part:?}");
            }
        }
    }
}

#[test]
fn makes_image_stickers_from_a_frame() {
    let (Some(backends), Some(input)) = (backends(), clip(Clip::Square)) else { return };
    let mut pictures = Vec::new();
    for backend in &backends {
        let dir = tempfile::tempdir().unwrap();
        let name = name(backend);
        // a wide part of the frame at 1 s, as a sticker
        let options = Options {
            start: Some(1.0),
            crop: Some(Crop { x: 0, y: 128, width: 512, height: 256 }),
            ..Options::default()
        };
        let request = Request { options, ..Request::new(input.clone()) };
        let request = Request { output: Some(dir.path().join("out.webp")), ..request };
        let made = tgradish_core::still::convert(backend, &request, &CancelToken::new()).unwrap();
        assert!(made.issues.is_empty(), "{name}: {:?}", made.issues);
        assert_eq!((made.width, made.height), (512, 256), "{name}");
        assert_eq!(made.encoding, tgradish_core::still::Encoding::Lossless, "{name}");
        let preview = tgradish_core::still::preview(&made.output).unwrap();
        assert_eq!((preview.width, preview.height), (512, 256), "{name}");
        pictures.push(preview.frames[0].0.clone());
        // an emoji, padded square
        let emoji = Options { target: Some(Target::Emoji), ..request.options.clone() };
        let request = Request {
            options: emoji,
            output: Some(dir.path().join("emoji.webp")),
            ..request.clone()
        };
        let made = tgradish_core::still::convert(backend, &request, &CancelToken::new()).unwrap();
        assert!(made.issues.is_empty(), "{name}: {:?}", made.issues);
        assert_eq!((made.width, made.height), (100, 100), "{name}");
        let corner = &tgradish_core::still::preview(&made.output).unwrap().frames[0].0[..4];
        assert_eq!(corner[3], 0, "{name}: the padding is transparent");
        // the frame that shows at a time, from where it starts, and the last
        let at = |start| {
            let options = Options { start: Some(start), ..Options::default() };
            let output = Some(dir.path().join(format!("at-{start}.webp")));
            let request = Request { options, output, overwrite: true, ..request.clone() };
            let made = tgradish_core::still::convert(backend, &request, &CancelToken::new());
            tgradish_core::still::preview(&made.unwrap().output).unwrap().frames[0].0.clone()
        };
        assert_eq!(at(0.5), at(0.48), "{name}");
        assert_ne!(at(0.5), at(0.52), "{name}");
        at(4.83);
        // before the start and past the end
        for start in [-1.0, 60.0] {
            let options = Options { start: Some(start), ..Options::default() };
            let request = Request { options, ..request.clone() };
            let made = tgradish_core::still::convert(backend, &request, &CancelToken::new());
            assert!(made.is_err(), "{name}: {start}");
        }
    }
    // the same picture either way
    if let [first, second] = pictures.as_slice() {
        let differ = first.iter().zip(second).filter(|(a, b)| a.abs_diff(**b) > 8).count();
        assert!(differ < first.len() / 100, "{differ} of {} channels differ", first.len());
    }
}

/// The corpus file `corpus`, written to `dir` as `name`.
fn corpus_file(dir: &Path, corpus: &str, name: &str) -> PathBuf {
    let files = tgradish_corpus::files();
    let file = files.iter().find(|file| file.name == corpus).expect("in the corpus");
    let path = dir.join(name);
    std::fs::write(&path, &file.bytes).unwrap();
    path
}

#[test]
fn converts_aseprite_files_through_ffmpeg() {
    let Some(backends) = backends() else { return };
    let dir = tempfile::tempdir().unwrap();
    let ase = corpus_file(dir.path(), "walker_2x.ase", "walker.ase");
    for backend in &backends {
        let name = name(backend);
        // the idle tag: two frames of 100 ms
        let options = Options { tag: Some("idle".into()), ..fast(Fit::Bitrate) };
        let output = Some(dir.path().join(format!("{name}.webm")));
        let request = Request { output, options, ..Request::new(ase.clone()) };
        let mut events = Vec::new();
        let made = convert(backend, &request, &CancelToken::new(), &mut |e| events.push(e));
        let made = made.unwrap();
        assert!(made.issues.is_empty(), "{name}: {:?}", made.issues);
        // told as given, not as drawn for ffmpeg
        let started = events.first();
        assert!(matches!(started, Some(Event::Started { plan }) if plan.input == ase), "{name}");
        let info = webm::inspect_file(&made.output).unwrap();
        assert_eq!(info.video_frames, 2, "{name}");
        // a picture of its first frame
        let output = Some(dir.path().join(format!("{name}.webp")));
        let picture = Request { output, ..request.clone() };
        let made = tgradish_core::still::convert(backend, &picture, &CancelToken::new()).unwrap();
        assert!(made.issues.is_empty(), "{name}: {:?}", made.issues);
    }
}

/// The walker as a lossless video, its 0.8 s walk `loops` times.
fn walking_video(dir: &Path, loops: u32) -> PathBuf {
    let gif = corpus_file(dir, "walker_2x.gif", "walker.gif");
    let video = dir.join(format!("walker-{loops}.mkv"));
    let repeats = (loops - 1).to_string();
    let (gif, out) = (gif.to_str().unwrap(), video.to_str().unwrap());
    make(&["-stream_loop", &repeats, "-i", gif, "-c:v", "ffv1", "-pix_fmt", "bgra", out]);
    video
}

#[test]
fn makes_tgs_stickers_from_videos() {
    use tgradish_core::tgs::{self, TgsOptions, TgsRequest};
    let Some(backends) = backends() else { return };
    let dir = tempfile::tempdir().unwrap();
    let (once, long) = (walking_video(dir.path(), 1), walking_video(dir.path(), 5));
    // raw H.264, where ffmpeg has x264
    let raw = dir.path().join("walker.h264");
    let made = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-i"])
        .arg(dir.path().join("walker.gif"))
        .args(["-c:v", "libx264", "-qp", "0", "-f", "h264"])
        .arg(&raw)
        .status()
        .unwrap();
    let raw = made.success().then_some(raw);
    let request = |input: &Path, output: String, options, backend| TgsRequest {
        inputs: vec![input.to_path_buf()],
        sequence: false,
        output: dir.path().join(output),
        options,
        overwrite: true,
        backend,
    };
    let sticker = |request: TgsRequest| tgs::convert(&request, &CancelToken::new(), &mut |_| {});
    // what the video should come to: the GIF it was made of
    let gif = dir.path().join("walker.gif");
    let direct = sticker(request(&gif, "gif.tgs".into(), TgsOptions::default(), None)).unwrap();
    for backend in &backends {
        let name = name(backend);
        let video = |input: &Path, options| {
            let output = format!("{name}-{}.tgs", input.file_stem().unwrap().display());
            sticker(request(input, output, options, Some(backend.clone())))
        };
        let made = video(&once, TgsOptions::default()).unwrap();
        assert!(made.issues.is_empty() && !made.lossy, "{name}: {:?}", made.issues);
        assert!(made.preview == direct.preview, "{name}: not the same as the GIF");
        // the third walk, read from where it starts
        let third = TgsOptions { start: Some(1.6), length: Some(0.8), ..TgsOptions::default() };
        let made = video(&long, third).unwrap();
        assert!(made.preview == direct.preview, "{name}: not the same as the GIF");
        // at its own pace, what plays past 3 seconds is cut, and said
        let own_pace = TgsOptions { tempo: Some(1.0), ..TgsOptions::default() };
        assert!(video(&long, own_pace).unwrap().lossy, "{name}");
        // all of it, played faster
        let made = video(&long, TgsOptions::default()).unwrap();
        assert!(made.issues.is_empty() && !made.lossy, "{name}: {:?}", made.issues);
        // a cut between frames, as long as asked: 9 sixtieths
        let cut = TgsOptions { length: Some(0.15), ..TgsOptions::default() };
        let made = video(&once, cut).unwrap();
        let ticks: u32 = made.preview.frames.iter().map(|(_, ticks)| ticks).sum();
        assert_eq!(ticks, 9, "{name}");
        let none = TgsOptions { length: Some(0.0), ..TgsOptions::default() };
        assert!(video(&once, none).is_err(), "{name}");
        // half of a frame of a second
        let slow = dir.path().join("slow.mkv");
        let out = slow.to_str().unwrap();
        make(&["-f", "lavfi", "-i", "color=red:s=16x16:r=1:d=2", "-c:v", "ffv1", out]);
        let half = TgsOptions { length: Some(0.5), ..TgsOptions::default() };
        let made = video(&slow, half).unwrap();
        let ticks: u32 = made.preview.frames.iter().map(|(_, ticks)| ticks).sum();
        assert_eq!(ticks, 30, "{name}");
        // a stream that doesn't say how long it is, nor when frames start
        if let Some(raw) = &raw {
            let made = video(raw, TgsOptions::default()).unwrap();
            assert!(made.preview.frames.len() > 1, "{name}: read as a still");
        }
    }
    // without ffmpeg, there is nothing to read a video with
    let none = sticker(request(&once, "none.tgs".into(), TgsOptions::default(), None));
    assert!(matches!(none, Err(Error::FfmpegNotFound(_))));
}

#[test]
fn times_animated_pictures_by_their_frames() {
    let Some(backends) = backends() else { return };
    let dir = tempfile::tempdir().unwrap();
    // APNG files don't say how long they play, nor WebP ones, which ffmpeg
    // reads only from version 8 and so get drawn as APNG
    let apng = corpus_file(dir.path(), "bounce.png", "bounce.png");
    let webp = corpus_file(dir.path(), "glow.webp", "glow.webp");
    let drawn =
        tgradish_core::drawn::readable(&webp, None, &Options::default(), &CancelToken::new());
    let drawn = drawn.unwrap();
    assert!(drawn.is_drawn());
    let webp = drawn.path().to_path_buf();
    // nor how fast, when as short as two frames
    let ase = corpus_file(dir.path(), "walker_2x.ase", "walker.ase");
    let idle = Options { tag: Some("idle".into()), ..Options::default() };
    let two = tgradish_core::drawn::readable(&ase, None, &idle, &CancelToken::new()).unwrap();
    let seconds = |name: &str| f64::from(tgradish_corpus::sprite(name).length()) / 1000.0;
    for backend in &backends {
        let name = name(backend);
        for (path, length) in [(&apng, seconds("bounce")), (&webp, seconds("glow"))] {
            let probe = backend.probe(path, &CancelToken::new()).unwrap();
            let duration = probe.duration.unwrap_or_default();
            assert!((duration - length).abs() < 1e-3, "{name}: {path:?} lasts {duration}");
        }
        let probe = backend.probe(two.path(), &CancelToken::new()).unwrap();
        assert!((probe.duration.unwrap_or_default() - 0.2).abs() < 1e-3, "{name}");
        assert!((probe.fps.unwrap_or_default() - 10.0).abs() < 1e-3, "{name}: {:?}", probe.fps);
    }
}

#[test]
fn makes_sequences_into_videos_and_pictures() {
    let Some(backends) = backends() else { return };
    let dir = tempfile::tempdir().unwrap();
    // a folder, its name dotted, of a red frame and a blue one
    let folder = dir.path().join("walk.v2");
    std::fs::create_dir(&folder).unwrap();
    for (index, colour) in [[255, 0, 0, 255], [0, 0, 255, 255]].into_iter().enumerate() {
        let mut canvas = tgradish_corpus::Canvas::new(8, 8);
        canvas.rect(0, 0, 8, 8, colour);
        let png = tgradish_corpus::encode::png(&canvas);
        std::fs::write(folder.join(format!("{}.png", index + 1)), png).unwrap();
    }
    let sequence = Some(vec![folder.clone()]);
    for backend in &backends {
        let name = name(backend);
        // four frames a second: half a second
        let options = Options { fps: Some(4.0), ..fast(Fit::Bitrate) };
        let request = Request {
            sequence: sequence.clone(),
            options,
            overwrite: true,
            ..Request::new(folder.clone())
        };
        let made = convert(backend, &request, &CancelToken::new(), &mut |_| {}).unwrap();
        assert_eq!(made.output, dir.path().join("walk.v2.sticker.webm"), "{name}");
        let info = webm::inspect_file(&made.output).unwrap();
        assert_eq!(info.video_frames, 2, "{name}");
        assert!((info.header_duration.unwrap() - 0.5).abs() < 1e-3, "{name}");
        // a picture's frames come ten a second, whatever else says
        let picture = |fps| {
            let options = Options { start: Some(0.15), fps, ..Options::default() };
            let request = Request { options, ..request.clone() };
            let made = tgradish_core::still::convert(backend, &request, &CancelToken::new());
            tgradish_core::still::preview(&made.unwrap().output).unwrap().frames[0].0[..4].to_vec()
        };
        assert_eq!(picture(None), [0, 0, 255, 255], "{name}");
        assert_eq!(picture(Some(4.0)), [0, 0, 255, 255], "{name}");
    }
}
