//! The icons for menus and the Windows executable, made from the window's
//! own pixel art rather than drawn apart, so they can't drift from it.
//! They are committed, since release packages and the Windows build take
//! them from the repository, and CI checks they still match the window's.

use std::path::Path;

use anyhow::{Context, Result, bail, ensure};

use crate::workspace_root;

/// Sizes of the icons for menus; the Windows executable has those up to 256.
pub const ICON_SIZES: [usize; 10] = [16, 24, 32, 48, 64, 96, 128, 192, 256, 512];

/// The largest side a Windows icon holds: its entries give sides in a
/// byte, with 0 standing for 256.
const ICO_MAX_SIDE: usize = 256;

/// Bytes of a Windows icon's header: reserved, the kind of file and how
/// many images, two bytes each.
const ICO_HEADER: usize = 6;

/// Bytes of each image's entry after the header: its width, height,
/// palette size and a reserved byte, one byte each; planes and bits per
/// pixel, two each; and its length and where it starts, four each.
const ICO_ENTRY: usize = 16;

/// Writes or checks the icons made from the window's: PNGs in
/// `crates/cli/assets/icons`, laid out as in `share/icons`, and
/// `crates/cli/assets/tgradish.ico` for the Windows executable.
pub fn icons(args: &[String]) -> Result<()> {
    let check = match args {
        [] => false,
        [flag] if flag == "--check" => true,
        _ => bail!("usage: cargo xtask icons [--check]"),
    };
    let root = workspace_root();
    let (side, pixels) = read_rgba(&root.join("crates/gui/assets/icon.png"))?;
    let assets = root.join("crates/cli/assets");
    let mut files = Vec::new();
    let mut ico = Vec::new();
    for size in ICON_SIZES {
        let png = rgba_png(size, &scale_pixel_art(&pixels, side, size))?;
        if size <= ICO_MAX_SIDE {
            ico.push((size, png.clone()));
        }
        files.push((assets.join(format!("icons/hicolor/{size}x{size}/apps/tgradish.png")), png));
    }
    files.push((assets.join("tgradish.ico"), ico_file(&ico)));
    for (path, bytes) in files {
        if check {
            ensure!(
                std::fs::read(&path).is_ok_and(|committed| committed == bytes),
                "{} is out of date: run cargo xtask icons and commit it",
                path.display()
            );
        } else {
            std::fs::create_dir_all(path.parent().unwrap())?;
            std::fs::write(&path, bytes)?;
            println!("{}", path.display());
        }
    }
    if check {
        println!("the icons are up to date");
    }
    Ok(())
}

/// The side and straight RGBA pixels of a square 8-bit RGBA PNG.
fn read_rgba(path: &Path) -> Result<(usize, Vec<u8>)> {
    let file = std::io::BufReader::new(std::fs::File::open(path)?);
    let mut reader = png::Decoder::new(file).read_info()?;
    let mut pixels = vec![0; reader.output_buffer_size().context("the PNG is too big")?];
    let info = reader.next_frame(&mut pixels)?;
    ensure!(
        (info.color_type, info.bit_depth) == (png::ColorType::Rgba, png::BitDepth::Eight)
            && info.width == info.height,
        "{} should be a square 8-bit RGBA PNG",
        path.display()
    );
    pixels.truncate(info.buffer_size());
    Ok((info.width as usize, pixels))
}

/// Square pixel art, straight RGBA `from` pixels wide, made `to` pixels
/// wide: each pixel the average of the part of the picture it covers.
/// Whole multiples copy pixels; other sizes blend them only along their
/// edges.
fn scale_pixel_art(pixels: &[u8], from: usize, to: usize) -> Vec<u8> {
    // in 1/(from*to) of the side, the picture's pixel i spans
    // [i*to, (i+1)*to) and the result's pixel j [j*from, (j+1)*from):
    // which of the picture's pixels each of the result's covers, and how
    // much of them
    let spans: Vec<Vec<(usize, u64)>> = (0..to)
        .map(|j| {
            let (start, end) = (j * from, (j + 1) * from);
            (start / to..end.div_ceil(to))
                .map(|i| (i, (end.min((i + 1) * to) - start.max(i * to)) as u64))
                .collect()
        })
        .collect();
    let round = |sum: u64, count: u64| ((2 * sum + count) / (2 * count)) as u8;
    let mut out = Vec::with_capacity(to * to * 4);
    for rows in &spans {
        for columns in &spans {
            // colours weighted by the area they cover and their opacity
            let mut sums = [0u64; 4];
            for &(row, height) in rows {
                for &(column, width) in columns {
                    let pixel = &pixels[(row * from + column) * 4..][..4];
                    let weight = height * width * u64::from(pixel[3]);
                    for (sum, &channel) in sums.iter_mut().zip(&pixel[..3]) {
                        *sum += weight * u64::from(channel);
                    }
                    sums[3] += weight;
                }
            }
            match sums[3] {
                0 => out.extend([0; 4]),
                opacity => {
                    out.extend(sums[..3].iter().map(|&sum| round(sum, opacity)));
                    out.push(round(opacity, (from * from) as u64));
                }
            }
        }
    }
    out
}

/// Straight RGBA pixels, `side` square, as a PNG.
fn rgba_png(side: usize, pixels: &[u8]) -> Result<Vec<u8>> {
    let mut png = Vec::new();
    let mut encoder = png::Encoder::new(&mut png, side as u32, side as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::High);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(pixels)?;
    writer.finish()?;
    Ok(png)
}

/// A Windows icon holding PNGs, each with its side.
fn ico_file(images: &[(usize, Vec<u8>)]) -> Vec<u8> {
    // reserved, an icon rather than a cursor, how many images
    let mut ico: Vec<u8> =
        [0u16, 1, images.len() as u16].iter().flat_map(|n| n.to_le_bytes()).collect();
    let mut offset = ICO_HEADER + ICO_ENTRY * images.len();
    for (side, png) in images {
        // 0 stands for 256
        let side = (side % 256) as u8;
        // no palette, a reserved byte, one plane, 32 bits per pixel
        ico.extend([side, side, 0, 0, 1, 0, 32, 0]);
        ico.extend((png.len() as u32).to_le_bytes());
        ico.extend((offset as u32).to_le_bytes());
        offset += png.len();
    }
    for (_, png) in images {
        ico.extend(png);
    }
    ico
}
