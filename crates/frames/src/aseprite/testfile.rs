//! Aseprite files written byte by byte, for the tests of every part of
//! reading them.

use std::io::Write;

use super::blend::Colour;
use super::{Layers, Sprite};
use crate::{Animation, Limits, Result};

pub(super) const RED: Colour = [255, 0, 0, 255];
pub(super) const GREEN: Colour = [0, 255, 0, 255];
pub(super) const BLUE: Colour = [0, 0, 255, 255];

pub(super) fn chunk(kind: u16, data: &[u8]) -> Vec<u8> {
    let mut out = ((data.len() + 6) as u32).to_le_bytes().to_vec();
    out.extend(kind.to_le_bytes());
    out.extend(data);
    out
}

pub(super) fn string(text: &str) -> Vec<u8> {
    let mut out = (text.len() as u16).to_le_bytes().to_vec();
    out.extend(text.as_bytes());
    out
}

/// A layer chunk: flags (1 visible, 8 background, 64 reference), type
/// (0 image, 1 group, 2 tilemap), level, blend mode, opacity.
pub(super) fn layer(flags: u16, kind: u16, level: u16, blend: u16, opacity: u8) -> Vec<u8> {
    named("layer", flags, kind, level, blend, opacity)
}

pub(super) fn named(
    name: &str,
    flags: u16,
    kind: u16,
    level: u16,
    blend: u16,
    opacity: u8,
) -> Vec<u8> {
    let mut data = Vec::new();
    for word in [flags, kind, level, 0, 0, blend] {
        data.extend(word.to_le_bytes());
    }
    data.extend([opacity, 0, 0, 0]);
    data.extend(string(name));
    if kind == 2 {
        data.extend(7u32.to_le_bytes());
    }
    chunk(0x2004, &data)
}

pub(super) fn cel_header(layer: u16, x: i16, y: i16, opacity: u8, kind: u16) -> Vec<u8> {
    let mut data = layer.to_le_bytes().to_vec();
    data.extend(x.to_le_bytes());
    data.extend(y.to_le_bytes());
    data.push(opacity);
    data.extend(kind.to_le_bytes());
    data.extend([0; 7]);
    data
}

pub(super) fn zlib(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}

/// An image cel; compressed unless `raw`.
pub(super) fn image_cel(
    layer: u16,
    (x, y): (i16, i16),
    (w, h): (u16, u16),
    pixels: &[u8],
    raw: bool,
) -> Vec<u8> {
    let mut data = cel_header(layer, x, y, 255, if raw { 0 } else { 2 });
    data.extend(w.to_le_bytes());
    data.extend(h.to_le_bytes());
    data.extend(if raw { pixels.to_vec() } else { zlib(pixels) });
    chunk(0x2005, &data)
}

pub(super) fn linked_cel(layer: u16, frame: u16) -> Vec<u8> {
    let mut data = cel_header(layer, 0, 0, 255, 1);
    data.extend(frame.to_le_bytes());
    chunk(0x2005, &data)
}

pub(super) fn tags(list: &[(u16, u16, u8, &str)]) -> Vec<u8> {
    let mut data = (list.len() as u16).to_le_bytes().to_vec();
    data.extend([0; 8]);
    for &(from, to, direction, name) in list {
        data.extend(from.to_le_bytes());
        data.extend(to.to_le_bytes());
        data.push(direction);
        data.extend([0; 12]);
        data.extend(string(name));
    }
    chunk(0x2018, &data)
}

pub(super) fn palette(colours: &[Colour]) -> Vec<u8> {
    let mut data = Vec::new();
    for value in [colours.len() as u32, 0, colours.len() as u32 - 1] {
        data.extend(value.to_le_bytes());
    }
    data.extend([0; 8]);
    for colour in colours {
        data.extend([0, 0]);
        data.extend(colour);
    }
    chunk(0x2019, &data)
}

/// A file of `width`x`height` with `depth` bits per pixel, header
/// `flags` (1: layer opacity, 2: groups composited) and one list of
/// chunks per frame, each frame 50 ms.
pub(super) fn file(
    width: u16,
    height: u16,
    depth: u16,
    flags: u32,
    frames: &[Vec<Vec<u8>>],
) -> Vec<u8> {
    let mut body = Vec::new();
    for chunks in frames {
        let data: Vec<u8> = chunks.concat();
        body.extend(((data.len() + 16) as u32).to_le_bytes());
        body.extend(0xf1fau16.to_le_bytes());
        body.extend((chunks.len() as u16).to_le_bytes());
        body.extend(50u16.to_le_bytes());
        body.extend([0; 2]);
        body.extend((chunks.len() as u32).to_le_bytes());
        body.extend(data);
    }
    let mut header = vec![0u8; 128];
    header[0..4].copy_from_slice(&((128 + body.len()) as u32).to_le_bytes());
    header[4..6].copy_from_slice(&0xa5e0u16.to_le_bytes());
    header[6..8].copy_from_slice(&(frames.len() as u16).to_le_bytes());
    header[8..10].copy_from_slice(&width.to_le_bytes());
    header[10..12].copy_from_slice(&height.to_le_bytes());
    header[12..14].copy_from_slice(&depth.to_le_bytes());
    header[14..18].copy_from_slice(&flags.to_le_bytes());
    header[18..20].copy_from_slice(&100u16.to_le_bytes());
    header.extend(body);
    header
}

pub(super) fn pixels(animation: &Animation, frame: usize) -> Vec<Colour> {
    (0..animation.height())
        .flat_map(|y| (0..animation.width()).map(move |x| (x, y)))
        .map(|(x, y)| animation.pixel(frame, x, y).unwrap())
        .collect()
}

pub(super) fn decode(bytes: &[u8]) -> Result<Animation> {
    Sprite::read(bytes, &Limits::default())?.animation(None, &Layers::default())
}

/// The tileset layers made by [`layer`] use: two 1x1 tiles (red,
/// green), new format, where tile 0 is empty, so ids 1 and 2.
pub(super) fn tileset() -> Vec<u8> {
    let mut data = 7u32.to_le_bytes().to_vec();
    data.extend((2u32 | 4).to_le_bytes());
    data.extend(3u32.to_le_bytes());
    data.extend([1, 0, 1, 0, 1, 0]);
    data.extend([0; 14]);
    data.extend(string("tiles"));
    let pixels = zlib(&[[0; 4], RED, GREEN].concat());
    data.extend((pixels.len() as u32).to_le_bytes());
    data.extend(pixels);
    chunk(0x2023, &data)
}

/// A tilemap cel on layer 0, of one row of `tiles` kept in `bits` bits
/// each, the top three of 32 for flips.
pub(super) fn tilemap(bits: u16, tiles: &[u32]) -> Vec<u8> {
    let ids = if bits == 32 { 0x1fff_ffff } else { (1u32 << bits) - 1 };
    masked_tilemap(bits, ids, tiles)
}

/// [`tilemap`] with tiles' ids in the bits of `ids`.
pub(super) fn masked_tilemap(bits: u16, ids: u32, tiles: &[u32]) -> Vec<u8> {
    let mut data = cel_header(0, 0, 0, 255, 3);
    data.extend((tiles.len() as u16).to_le_bytes());
    data.extend(1u16.to_le_bytes());
    data.extend(bits.to_le_bytes());
    for mask in [ids, 0x2000_0000, 0x4000_0000, 0x8000_0000] {
        data.extend(mask.to_le_bytes());
    }
    data.extend([0; 10]);
    let bytes = usize::from(bits / 8);
    data.extend(zlib(
        &tiles.iter().flat_map(|t| t.to_le_bytes()[..bytes].to_vec()).collect::<Vec<_>>(),
    ));
    chunk(0x2005, &data)
}
