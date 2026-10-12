//! Decodes the synthetic pixel art of `tgradish-corpus` in each format it
//! is written in, and finds the frames and timing it was drawn with.

use std::collections::BTreeSet;
use std::time::Duration;

use tgradish_corpus::{File, Sprite, encode};
use tgradish_frames::{
    Animation, DecodeOptions, Error, Limits, Sheet, decode, sequence, sprite_sheet,
};

fn file(name: &str) -> File {
    tgradish_corpus::files().into_iter().find(|file| file.name == name).unwrap()
}

fn open(file: &File) -> Animation {
    decode(&file.bytes, &DecodeOptions::default())
        .unwrap_or_else(|err| panic!("{}: {err}", file.name))
}

/// Transparent pixels compare equal whatever colour they carry.
fn visible(rgba: &[u8]) -> Vec<[u8; 4]> {
    rgba.as_chunks::<4>()
        .0
        .iter()
        .map(|&pixel| if pixel[3] == 0 { [0; 4] } else { pixel })
        .collect()
}

/// Checks that `animation` has the frames of `sprite` from `first`, and
/// their timing.
fn assert_frames(animation: &Animation, sprite: &Sprite, first: usize, what: &str) {
    let (width, height) = sprite.size();
    assert_eq!((animation.width(), animation.height()), (width, height), "{what}");
    let expected = sprite.frames[first..].iter().zip(&sprite.delays[first..]);
    for (index, (frame, (canvas, &delay))) in animation.frames().iter().zip(expected).enumerate() {
        assert!(visible(&frame.rgba) == visible(&canvas.rgba), "{what}: frame {index} differs");
        assert_eq!(frame.duration, Duration::from_millis(delay.into()), "{what}: frame {index}");
    }
}

#[test]
fn decodes_every_format() {
    let files = tgradish_corpus::files();
    let extensions: BTreeSet<&str> =
        files.iter().filter_map(|file| file.name.rsplit_once('.')).map(|(_, e)| e).collect();
    assert_eq!(extensions, BTreeSet::from(["ase", "gif", "png", "webp"]));
    for file in &files {
        let animation = open(file);
        assert_eq!(animation.frames().len(), file.sprite.frames.len(), "{}", file.name);
        assert_frames(&animation, &file.sprite, 0, &file.name);
    }
}

#[test]
fn plays_aseprite_tags() {
    let ase = file("walker_2x.ase");
    let tagged = DecodeOptions { tag: Some("idle".into()), ..DecodeOptions::default() };
    let idle = decode(&ase.bytes, &tagged).unwrap();
    assert_eq!(idle.frames().len(), 2);
    assert_frames(&idle, &ase.sprite, 0, "idle");
    let missing = DecodeOptions { tag: Some("run".into()), ..DecodeOptions::default() };
    assert!(matches!(decode(&ase.bytes, &missing), Err(Error::NoSuchTag { .. })));
    // only Aseprite files have tags
    assert_eq!(decode(&file("walker_2x.gif").bytes, &tagged), Err(Error::TagWithoutAseprite));
}

#[test]
fn joins_image_sequences() {
    let [(_, steps), (_, wave)] = tgradish_corpus::sequences();
    let files: Vec<Vec<u8>> = steps.iter().map(encode::png).collect();
    let animation =
        sequence(files.iter().map(Vec::as_slice), Duration::from_millis(200), &Limits::default())
            .unwrap();
    assert_eq!((animation.width(), animation.height()), (27, 31));
    assert_eq!(animation.frames().len(), 4);
    assert_eq!(animation.duration(), Duration::from_millis(800));
    // the second and fourth steps are the same
    assert_eq!(animation.frames()[1], animation.frames()[3]);

    let mixed = [files[0].clone(), encode::png(&wave[0])];
    assert!(matches!(
        sequence(mixed.iter().map(Vec::as_slice), Duration::from_millis(200), &Limits::default()),
        Err(Error::SequenceSize { index: 1, .. })
    ));
}

#[test]
fn decodes_each_image_of_a_sequence_in_what_the_others_leave() {
    let [(_, steps), _] = tgradish_corpus::sequences();
    let files: Vec<Vec<u8>> = steps.iter().map(encode::png).collect();
    let joined = |max_bytes| {
        let limits = Limits { max_dimension: 4096, max_bytes };
        sequence(files.iter().map(Vec::as_slice), Duration::from_millis(200), &limits)
    };
    // decoding a PNG takes twice its pixels, beside the three images before
    let frame = 27 * 31 * 4;
    assert!(matches!(joined(5 * frame - 1), Err(Error::TooLarge(_))));
    assert_eq!(joined(5 * frame).unwrap().frames().len(), 4);
}

#[test]
fn splits_sprite_sheets_back_into_frames() {
    let walker = tgradish_corpus::sprite("walker");
    let sheet =
        Sheet { columns: 4, rows: 2, frames: None, frame_duration: Duration::from_millis(100) };
    let animation = sprite_sheet(&encode::sheet(&walker, 4), &sheet, &Limits::default()).unwrap();
    assert_eq!(animation.frames().len(), 8);
    assert_frames(&animation, &walker, 0, "sheet");
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
    let animation = sprite_sheet(&sheet_png, &sheet, &Limits::default()).unwrap();
    assert_eq!((animation.width(), animation.height(), animation.frames().len()), (4, 3, 6));
    assert_eq!(animation.pixel(4, 3, 2), Some([4, 0, 0, 160]));
    assert_eq!(animation.duration(), Duration::from_millis(300));

    // only cell 1 is visible: trailing transparent cells are dropped,
    // leading ones kept
    let sparse = png(12, 6, |x, y| [0, 0, 0, if y / 3 * 3 + x / 4 == 1 { 255 } else { 0 }]);
    assert_eq!(sprite_sheet(&sparse, &sheet, &Limits::default()).unwrap().frames().len(), 2);
    let three = Sheet { frames: Some(3), ..sheet.clone() };
    assert_eq!(sprite_sheet(&sparse, &three, &Limits::default()).unwrap().frames().len(), 3);

    let uneven = Sheet { columns: 5, ..sheet.clone() };
    assert!(matches!(
        sprite_sheet(&sparse, &uneven, &Limits::default()),
        Err(Error::SheetGrid { .. })
    ));
    let too_many = Sheet { frames: Some(7), ..sheet };
    assert!(matches!(
        sprite_sheet(&sparse, &too_many, &Limits::default()),
        Err(Error::SheetFrames { .. })
    ));
}

#[test]
fn survives_corrupted_files() {
    let original = file("walker_2x.ase").bytes;
    // a fixed pseudo-random sequence
    let mut state = 0x9e37_79b9_7f4a_7c15u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let limits = Limits { max_dimension: 4096, max_bytes: 64 << 20 };
    let options = DecodeOptions { tag: None, limits };
    for _ in 0..3000 {
        let mut bytes = original.clone();
        for _ in 0..1 + next() % 8 {
            let at = (next() % bytes.len() as u64) as usize;
            bytes[at] = next() as u8;
        }
        if next() % 4 == 0 {
            bytes.truncate((next() % bytes.len() as u64) as usize);
        }
        // errors are fine, panics and running out of memory aren't
        let _ = decode(&bytes, &options);
    }
}

#[test]
fn refuses_huge_gifs() {
    // a 65535x65535 logical screen with one 1x1 frame
    let mut gif = b"GIF89a".to_vec();
    gif.extend([0xff, 0xff, 0xff, 0xff, 0x80, 0, 0]);
    gif.extend([0, 0, 0, 255, 255, 255]);
    gif.extend([0x2c, 0, 0, 0, 0, 1, 0, 1, 0, 0]);
    gif.extend([2, 2, 0x44, 0x01, 0, 0x3b]);
    assert!(matches!(decode(&gif, &DecodeOptions::default()), Err(Error::TooLarge(_))));
}

#[test]
fn counts_the_rgba_copy_of_still_images() {
    // a 256x256 RGB PNG is 196608 bytes decoded, 262144 as RGBA
    let image = image::RgbImage::from_pixel(256, 256, image::Rgb([1, 2, 3]));
    let mut bytes = Vec::new();
    image.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
    let tight =
        DecodeOptions { tag: None, limits: Limits { max_dimension: 4096, max_bytes: 300_000 } };
    assert!(matches!(decode(&bytes, &tight), Err(Error::TooLarge(_))));
    let roomy =
        DecodeOptions { tag: None, limits: Limits { max_dimension: 4096, max_bytes: 500_000 } };
    assert_eq!(decode(&bytes, &roomy).unwrap().frames().len(), 1);
}

#[test]
fn reads_jpeg_and_bmp_stills() {
    // 16x8: red on the left, blue on the right; JPEG keeps them roughly
    let pixel = |x: u32, _| if x < 8 { [255, 0, 0, 255] } else { [0, 0, 255, 255] };
    let image = image::RgbImage::from_fn(16, 8, |x, y| {
        let [r, g, b, _] = pixel(x, y);
        image::Rgb([r, g, b])
    });
    for format in [image::ImageFormat::Jpeg, image::ImageFormat::Bmp] {
        let mut bytes = Vec::new();
        image.write_to(&mut std::io::Cursor::new(&mut bytes), format).unwrap();
        let animation = decode(&bytes, &DecodeOptions::default()).unwrap();
        assert_eq!((animation.width(), animation.height()), (16, 8), "{format:?}");
        assert_eq!(animation.frames().len(), 1);
        let [r, _, b, a] = animation.pixel(0, 2, 4).unwrap();
        assert!(r > 200 && b < 60 && a == 255, "{format:?}: {r} {b} {a}");
        let [r, _, b, _] = animation.pixel(0, 13, 4).unwrap();
        assert!(b > 200 && r < 60, "{format:?}: {r} {b}");
    }
}
