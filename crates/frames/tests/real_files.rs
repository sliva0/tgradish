//! Decodes the pixel art corpus in `references/pixelart` (see its README).
//! Skipped when the local `references/` folder is missing.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tgradish_frames::{Animation, DecodeOptions, Error, Sheet, decode, sequence, sprite_sheet};

fn reference(name: &str) -> Option<PathBuf> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../references/pixelart").join(name);
    if !path.exists() {
        eprintln!("skipped: needs references/pixelart/{name}");
        return None;
    }
    Some(path)
}

fn open(path: &Path) -> Animation {
    decode(&std::fs::read(path).unwrap(), &DecodeOptions::default())
        .unwrap_or_else(|err| panic!("{}: {err}", path.display()))
}

#[test]
fn decodes_every_file() {
    let Some(dir) = reference("") else { return };
    let mut count = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if matches!(path.extension().and_then(|e| e.to_str()), Some("gif" | "webp" | "ase")) {
            assert!(open(&path).frames().len() > 1, "{} is animated", path.display());
            count += 1;
        }
    }
    assert!(count >= 60, "{count} files");
}

#[test]
fn reads_sizes_and_timing() {
    // (file, width, height, frames, milliseconds) as in the README
    let expected = [
        ("Kris_battle.gif", 92, 94, 30, 3000),
        ("animation_king_circle.gif", 44, 83, 43, 9850),
        ("animation_hammer.gif", 417, 277, 7, 580),
        // 5 frames of 170 ms, partial alpha
        ("ralsei_fat_blunt.webp", 48, 48, 5, 850),
    ];
    for (name, width, height, frames, ms) in expected {
        let Some(path) = reference(name) else { return };
        let animation = open(&path);
        assert_eq!(
            (animation.width(), animation.height(), animation.frames().len()),
            (width, height, frames),
            "{name}"
        );
        assert_eq!(animation.duration(), Duration::from_millis(ms), "{name}");
    }
}

/// Transparent pixels compare equal whatever colour they carry.
fn visible(animation: &Animation, frame: usize) -> Vec<[u8; 4]> {
    animation.frames()[frame]
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|&pixel| if pixel[3] == 0 { [0; 4] } else { pixel })
        .collect()
}

#[test]
fn aseprite_matches_its_gif() {
    let (Some(ase), Some(gif)) = (reference("Kris_battle.ase"), reference("Kris_battle.gif"))
    else {
        return;
    };
    let gif_bytes = std::fs::read(&gif).unwrap();
    let (ase, gif) = (open(&ase), open(&gif));
    assert_eq!((ase.width(), ase.height()), (gif.width(), gif.height()));
    assert_eq!(ase.frames().len(), gif.frames().len());
    for index in 0..ase.frames().len() {
        assert!(visible(&ase, index) == visible(&gif, index), "frame {index} differs");
        // GIF delays are in centiseconds
        let (a, g) = (ase.frames()[index].duration, gif.frames()[index].duration);
        assert!(a.abs_diff(g) < Duration::from_millis(10), "frame {index}: {a:?} vs {g:?}");
    }
    let tagged = DecodeOptions { tag: Some("idle".into()) };
    assert_eq!(decode(&gif_bytes, &tagged), Err(Error::TagWithoutAseprite));
}

#[test]
fn joins_image_sequences() {
    let Some(dir) = reference("sequences/kris_dance") else { return };
    let files: Vec<Vec<u8>> =
        (1..=4).map(|n| std::fs::read(dir.join(format!("{n}.png"))).unwrap()).collect();
    let animation = sequence(files.iter().map(Vec::as_slice), Duration::from_millis(200)).unwrap();
    assert_eq!((animation.width(), animation.height()), (27, 31));
    assert_eq!(animation.frames().len(), 4);
    assert_eq!(animation.duration(), Duration::from_millis(800));
    // the README says 2.png and 4.png are identical
    assert_eq!(animation.frames()[1], animation.frames()[3]);

    let Some(other) = reference("sequences/ralsei_dance/1.png") else { return };
    let mixed = [files[0].clone(), std::fs::read(other).unwrap()];
    assert!(matches!(
        sequence(mixed.iter().map(Vec::as_slice), Duration::from_millis(200)),
        Err(Error::SequenceSize { index: 1, .. })
    ));
}

fn png(width: u32, height: u32, pixel: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
    let image = image::RgbaImage::from_fn(width, height, |x, y| image::Rgba(pixel(x, y)));
    let mut out = Vec::new();
    image.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
    out
}

#[test]
fn splits_sprite_sheets() {
    // 3x2 cells of 4x3 pixels, cell n has red n and alpha n * 40
    let sheet_png = png(12, 6, |x, y| {
        let cell = (y / 3 * 3 + x / 4) as u8;
        [cell, 0, 0, cell * 40]
    });
    let sheet =
        Sheet { columns: 3, rows: 2, frames: None, frame_duration: Duration::from_millis(50) };
    let animation = sprite_sheet(&sheet_png, &sheet).unwrap();
    assert_eq!((animation.width(), animation.height(), animation.frames().len()), (4, 3, 6));
    assert_eq!(animation.pixel(4, 3, 2), Some([4, 0, 0, 160]));
    assert_eq!(animation.duration(), Duration::from_millis(300));

    // only cell 1 is visible: trailing transparent cells are dropped,
    // leading ones kept
    let sparse = png(12, 6, |x, y| [0, 0, 0, if y / 3 * 3 + x / 4 == 1 { 255 } else { 0 }]);
    assert_eq!(sprite_sheet(&sparse, &sheet).unwrap().frames().len(), 2);
    let three = Sheet { frames: Some(3), ..sheet.clone() };
    assert_eq!(sprite_sheet(&sparse, &three).unwrap().frames().len(), 3);

    let uneven = Sheet { columns: 5, ..sheet.clone() };
    assert!(matches!(sprite_sheet(&sparse, &uneven), Err(Error::SheetGrid { .. })));
    let too_many = Sheet { frames: Some(7), ..sheet };
    assert!(matches!(sprite_sheet(&sparse, &too_many), Err(Error::SheetFrames { .. })));
}
