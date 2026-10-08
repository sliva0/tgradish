//! Normalises the pixel art corpus in `references/pixelart` (see its
//! README). Skipped when the local `references/` folder is missing.

use std::path::{Path, PathBuf};

use tgradish_tgs::frames::{Animation, DecodeOptions, decode};
use tgradish_tgs::normalise::{Options, Report, normalise};

fn corpus() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../references/pixelart");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        eprintln!("skipped: needs references/pixelart");
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            matches!(path.extension().and_then(|e| e.to_str()), Some("gif" | "webp" | "ase"))
        })
        .collect();
    files.sort();
    files
}

fn name(path: &Path) -> &str {
    path.file_name().unwrap().to_str().unwrap()
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
    for path in corpus() {
        let input = decode(&std::fs::read(&path).unwrap(), &DecodeOptions::default()).unwrap();
        let (anim, report) = normalise(&input, &Options::default()).unwrap();
        assert!(report.ticks <= 180 && report.ticks > 0, "{}", name(&path));
        if report.speed == 1.0 && report.dropped_frames == 0 {
            let actual: Vec<Vec<u8>> = (0..anim.frames().len()).map(|i| anim.rgba(i)).collect();
            assert!(actual == expected_frames(&input, &report), "{} isn't lossless", name(&path));
        }
    }
}

#[test]
fn detects_pixel_scales() {
    let files = corpus();
    if files.is_empty() {
        return;
    }
    // exact upscales; the README's run-length GCD also counted the canvas
    // edges and found 1x for some of them
    let exact = [
        ("Kris_battle.gif", 2),
        ("Kris_battle.ase", 2),
        ("Noelle_battle_enter.gif", 2),
        ("Queen_battle.gif", 2),
        ("Ralsei_battle_item.gif", 2),
        ("Spamton_battle_head_enlarge.gif", 2),
        ("Susie_battle_act.gif", 2),
        ("Susie_overworld_laugh.gif", 2),
        // cut through an art pixel on the right
        ("animation_hammer.gif", 4),
    ];
    // mostly on a grid, with some pixels off it
    let likely = [
        ("Noelle_battle_act.gif", 2),
        ("Spamton_overworld_glitched_laugh.gif", 2),
        ("Spamton_trembling.gif", 2),
    ];
    for path in files {
        let input = decode(&std::fs::read(&path).unwrap(), &DecodeOptions::default()).unwrap();
        let (_, report) = normalise(&input, &Options::default()).unwrap();
        let name = name(&path);
        if let Some(&(_, scale)) = exact.iter().find(|(file, _)| *file == name) {
            assert_eq!((report.scale, report.likely_scale), (scale, None), "{name}");
        } else if let Some(&(_, scale)) = likely.iter().find(|(file, _)| *file == name) {
            assert_eq!(report.scale, 1, "{name}");
            assert_eq!(report.likely_scale.map(|likely| likely.scale), Some(scale), "{name}");
        }
    }
}

#[test]
fn encodes_valid_lottie() {
    use tgradish_tgs::check::{Severity, check};
    use tgradish_tgs::encode;
    use tgradish_tgs::layout::lay_out;
    use tgradish_tgs::lottie::Style;

    for path in corpus() {
        let input = decode(&std::fs::read(&path).unwrap(), &DecodeOptions::default()).unwrap();
        let (anim, _) = normalise(&input, &Options::default()).unwrap();
        let scene = encode::runs(&anim);
        assert_eq!(scene.compare(&anim), None, "{}", name(&path));
        let json = lay_out(&scene, &anim, Some("tgradish".into())).to_json(Style::default());
        let (stats, issues) = check(json.as_bytes(), None).unwrap();
        let errors: Vec<_> = issues.iter().filter(|i| i.severity == Severity::Error).collect();
        assert!(errors.is_empty(), "{}: {errors:?}", name(&path));
        assert_eq!(stats.frames, f64::from(anim.ticks()));
    }
}

#[test]
fn checks_stickers_from_1x() {
    use tgradish_tgs::check::{Severity, check};
    use tgradish_tgs::file::unpack;

    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../references/pixelart/1x-uploaded");
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for path in entries.map(|entry| entry.unwrap().path()) {
        let tgs = std::fs::read(&path).unwrap();
        let (stats, issues) = check(&unpack(&tgs).unwrap(), Some(tgs.len())).unwrap();
        // Telegram accepted these, with merge paths and strokes in every group
        assert!(
            issues.iter().all(|issue| issue.severity == Severity::Warning),
            "{}: {issues:?}",
            name(&path)
        );
        assert!(stats.features.contains("merge paths") && stats.features.contains("strokes"));
    }
}
