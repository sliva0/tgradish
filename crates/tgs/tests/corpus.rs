//! Normalises and encodes the synthetic pixel art of `tgradish-corpus`,
//! which stands for the cases real art has.

use tgradish_corpus::File;
use tgradish_tgs::frames::{Animation, DecodeOptions, decode};
use tgradish_tgs::normalise::{Options, Report, normalise};

fn corpus() -> Vec<File> {
    tgradish_corpus::files()
}

fn input(file: &File) -> Animation {
    decode(&file.bytes, &DecodeOptions::default())
        .unwrap_or_else(|err| panic!("{}: {err}", file.name))
}

/// The input's frames, cropped like the report says, transparent pixels as
/// zeros, consecutive copies joined.
fn expected_frames(input: &Animation, report: &Report) -> Vec<Vec<u8>> {
    let mut frames: Vec<Vec<u8>> = Vec::new();
    for index in 0..input.frames().len() {
        let crop = report.crop;
        let mut frame = Vec::new();
        for y in crop.y..crop.y + crop.height {
            for x in crop.x..crop.x + crop.width {
                let pixel = input.pixel(index, x, y).unwrap();
                frame.extend(if pixel[3] == 0 { [0; 4] } else { pixel });
            }
        }
        if frames.last() != Some(&frame) {
            frames.push(frame);
        }
    }
    frames
}

#[test]
fn normalises_losslessly() {
    for file in corpus() {
        let input = input(&file);
        let (anim, report) = normalise(&input, &Options::default()).unwrap();
        assert!(report.ticks <= 180 && report.ticks > 0, "{}", file.name);
        if report.tempo == 1.0 && report.dropped_frames == 0 {
            let actual: Vec<Vec<u8>> = (0..anim.frames().len()).map(|i| anim.rgba(i)).collect();
            assert!(actual == expected_frames(&input, &report), "{} isn't lossless", file.name);
        }
    }
}

#[test]
fn detects_pixel_scales() {
    // exact upscales, one cut through an art pixel, and art mostly on a
    // grid with some pixels off it
    for file in corpus() {
        let (_, report) = normalise(&input(&file), &Options::default()).unwrap();
        let likely = report.likely_scale.map(|likely| likely.scale);
        let sprite = &file.sprite;
        assert_eq!((report.scale, likely), (sprite.scale, sprite.likely_scale), "{}", file.name);
    }
}

#[test]
fn encodes_valid_lottie() {
    use tgradish_tgs::check::{Severity, check};
    use tgradish_tgs::encode;
    use tgradish_tgs::layout::lay_out;
    use tgradish_tgs::lottie::Style;

    for file in corpus() {
        let (anim, _) = normalise(&input(&file), &Options::default()).unwrap();
        let fast = encode::Settings { effort: encode::Effort::Fast, ..Default::default() };
        let scene = encode::painter(&anim, &fast, None, &|| false).unwrap();
        assert_eq!(scene.compare(&anim), None, "{}", file.name);
        let json = lay_out(&scene, &anim, Some("tgradish".into()), false).to_json(Style::default());
        let (stats, issues) = check(json.as_bytes(), None).unwrap();
        let errors: Vec<_> = issues.iter().filter(|i| i.severity == Severity::Error).collect();
        assert!(errors.is_empty(), "{}: {errors:?}", file.name);
        assert_eq!(stats.frames, f64::from(anim.ticks()));
    }
}

#[test]
fn checks_stickers_like_1x_made() {
    use tgradish_tgs::check::{Severity, check};

    // as pixelart2tgs 1.x wrote them, and Telegram accepted them in 2022:
    // merge paths and a stroke in every group, no "tgs" key, a fractional
    // `op`, and paths with empty tangents
    let path = |points: &[[u32; 2]]| {
        let empty = vec!["[]"; points.len()].join(",");
        let points: Vec<String> = points.iter().map(|[x, y]| format!("[{x},{y}]")).collect();
        format!(
            r#"{{"ty":"sh","ks":{{"k":{{"i":[{empty}],"o":[{empty}],"v":[{}]}}}}}}"#,
            points.join(",")
        )
    };
    let colour = r#"{"k":[0.97,0.68,0.03]}"#;
    let group = format!(
        r#"{{"ty":"gr","it":[{},{},{{"ty":"mm","mm":1}},{{"ty":"st","c":{colour},"w":{{"k":0.033}}}},{{"ty":"fl","c":{colour}}},{{"ty":"tr"}}]}}"#,
        path(&[[0, 0], [4, 0], [4, 4], [0, 4], [0, 0]]),
        path(&[[6, 1], [8, 1], [8, 3], [6, 3], [6, 1]])
    );
    let json = format!(
        r#"{{"v":"5.7.2","fr":60,"ip":0,"op":39.6,"w":512,"h":512,"layers":[{{"ty":4,"ks":{{"p":{{"k":[30,0]}},"s":{{"k":[3000,3000]}}}},"shapes":[{group}],"ip":0,"op":39.6}}]}}"#
    );
    let (stats, issues) = check(json.as_bytes(), None).unwrap();
    // now Telegram refuses their empty tangents, and only those
    let errors: Vec<_> = issues.iter().filter(|i| i.severity == Severity::Error).collect();
    assert!(errors.len() == 1 && errors[0].message.contains("empty tangents"), "{issues:?}");
    assert!(stats.features.contains("merge paths") && stats.features.contains("strokes"));
}

#[test]
fn painter_is_exact_without_seams() {
    use tgradish_tgs::encode::{Settings, painter};

    for file in corpus() {
        let (anim, _) = normalise(&input(&file), &Options::default()).unwrap();
        for lifetimes in [true, false] {
            let settings = Settings { lifetimes, ..Settings::default() };
            let scene = painter(&anim, &settings, None, &|| false).unwrap();
            assert_eq!(scene.compare(&anim), None, "{} ({lifetimes})", file.name);
            let seams = scene.seams(anim.palette());
            assert!(
                seams.is_empty(),
                "{} ({lifetimes}): {:?}",
                file.name,
                &seams[..seams.len().min(5)]
            );
        }
    }
}

/// Copies of an animation side by side, `across` by `down`.
fn tiled(input: &Animation, across: u32, down: u32) -> Animation {
    let (width, height) = (input.width(), input.height());
    let frames = input
        .frames()
        .iter()
        .map(|frame| {
            let mut rgba = Vec::new();
            for y in 0..height * down {
                for _ in 0..across {
                    let start = ((y % height) * width * 4) as usize;
                    rgba.extend_from_slice(&frame.rgba[start..start + width as usize * 4]);
                }
            }
            tgradish_tgs::frames::Frame { rgba, duration: frame.duration }
        })
        .collect();
    Animation::new(width * across, height * down, frames).unwrap()
}

#[test]
fn fits_large_animations() {
    use tgradish_tgs::sticker::{Fit, Options as StickerOptions, make};

    // much of it changes every frame
    let file = corpus().into_iter().find(|file| file.sprite.name == "dance").unwrap();
    let input = input(&file);
    let small = make(&input, &StickerOptions::default(), &mut |_| {}, &|| false).unwrap();
    assert!(small.fits() && small.steps.is_empty(), "{:?}", small.issues);

    let big = tiled(&input, 2, 2);
    let lossless = StickerOptions { fit: Fit::Lossless, ..StickerOptions::default() };
    let too_large = make(&big, &lossless, &mut |_| {}, &|| false).unwrap();
    assert!(!too_large.fits() && too_large.steps.is_empty());
    assert!(too_large.bytes > 65536, "{}", too_large.bytes);

    let started = std::time::Instant::now();
    let fitted = make(&big, &StickerOptions::default(), &mut |_| {}, &|| false).unwrap();
    eprintln!(
        "{} bytes in {:?}, from {} lossless: {:?}",
        fitted.bytes,
        started.elapsed(),
        too_large.bytes,
        fitted.steps
    );
    assert!(fitted.fits(), "{:?}", fitted.issues);
    assert!(fitted.bytes <= 65536 && fitted.lossy());
}

#[test]
fn fits_close_to_the_limit() {
    use tgradish_tgs::sticker::{Options as StickerOptions, make};

    // a limit just past where estimates are first measured against the
    // real encoder: fitting measures them as it nears it, and stops close
    let file = corpus().into_iter().find(|file| file.sprite.name == "dance").unwrap();
    for max_bytes in [14_500, 9_000] {
        let options = StickerOptions { max_bytes, ..StickerOptions::default() };
        let made = make(&input(&file), &options, &mut |_| {}, &|| false).unwrap();
        eprintln!("{max_bytes}: {} bytes, {:?}", made.bytes, made.steps);
        assert!(made.bytes <= max_bytes, "{} bytes", made.bytes);
        assert!(made.bytes as f64 > max_bytes as f64 * 0.9, "{} bytes", made.bytes);
    }
}
