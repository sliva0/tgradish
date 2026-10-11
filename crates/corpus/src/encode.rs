//! Sprites written in the formats tgradish reads, losslessly: decoding one
//! gives its frames back.

use std::io::Write;

use crate::Sprite;
use crate::canvas::{CLEAR, Canvas, Colour};

/// A GIF, each frame whole and cleared after it shows. GIF keeps
/// centiseconds and on or off transparency, which the sprites keep to.
pub fn gif(sprite: &Sprite) -> Vec<u8> {
    let (width, height) = sprite.size();
    let mut out = Vec::new();
    {
        let mut encoder = gif::Encoder::new(&mut out, width as u16, height as u16, &[]).unwrap();
        encoder.set_repeat(gif::Repeat::Infinite).unwrap();
        for (canvas, &delay) in sprite.frames.iter().zip(&sprite.delays) {
            let mut rgba = canvas.rgba.clone();
            // fewer than 256 colours: an exact palette
            let mut frame = gif::Frame::from_rgba_speed(width as u16, height as u16, &mut rgba, 10);
            frame.delay = (delay / 10) as u16;
            frame.dispose = gif::DisposalMethod::Background;
            encoder.write_frame(&frame).unwrap();
        }
    }
    out
}

/// A still PNG.
pub fn png(canvas: &Canvas) -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, canvas.width, canvas.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().unwrap().write_image_data(&canvas.rgba).unwrap();
    out
}

/// An APNG, each frame replacing the one before.
pub fn apng(sprite: &Sprite) -> Vec<u8> {
    let (width, height) = sprite.size();
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_animated(sprite.frames.len() as u32, 0).unwrap();
    let mut writer = encoder.write_header().unwrap();
    for (canvas, &delay) in sprite.frames.iter().zip(&sprite.delays) {
        writer.set_frame_delay(delay as u16, 1000).unwrap();
        writer.write_image_data(&canvas.rgba).unwrap();
    }
    writer.finish().unwrap();
    out
}

/// A lossless animated WebP: each frame the VP8L image of a still WebP,
/// replacing the one before.
pub fn webp(sprite: &Sprite) -> Vec<u8> {
    let (width, height) = sprite.size();
    let chunk = |name: &[u8], data: &[u8]| {
        let mut out = name.to_vec();
        out.extend((data.len() as u32).to_le_bytes());
        out.extend(data);
        if data.len() % 2 == 1 {
            out.push(0);
        }
        out
    };
    let u24 = |value: u32| value.to_le_bytes()[..3].to_vec();
    // animation and alpha
    let mut header = vec![0x12, 0, 0, 0];
    header.extend(u24(width - 1));
    header.extend(u24(height - 1));
    let mut body = chunk(b"VP8X", &header);
    // a clear background, looping forever
    body.extend(chunk(b"ANIM", &[0, 0, 0, 0, 0, 0]));
    for (canvas, &delay) in sprite.frames.iter().zip(&sprite.delays) {
        let mut still = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(&mut still)
            .encode(&canvas.rgba, width, height, image::ExtendedColorType::Rgba8)
            .unwrap();
        // the still's only chunk, after "RIFF", its size and "WEBP"
        assert_eq!(&still[12..16], b"VP8L");
        let mut frame = [u24(0), u24(0), u24(width - 1), u24(height - 1), u24(delay)].concat();
        // no blending, no disposal
        frame.push(0b10);
        frame.extend(&still[12..]);
        body.extend(chunk(b"ANMF", &frame));
    }
    let mut out = b"RIFF".to_vec();
    out.extend((body.len() as u32 + 4).to_le_bytes());
    out.extend(b"WEBP");
    out.extend(body);
    out
}

/// An Aseprite file in indexed colour, index 0 clear, with one layer and a
/// cel per frame, and `tags` as `(name, first, last)` frames.
pub fn aseprite(sprite: &Sprite, tags: &[(&str, u16, u16)]) -> Vec<u8> {
    let (width, height) = sprite.size();
    let mut palette: Vec<Colour> = vec![CLEAR];
    for colour in sprite.frames.iter().flat_map(Canvas::colours) {
        if !palette.contains(&colour) {
            palette.push(colour);
        }
    }
    assert!(palette.len() <= 256, "{} has too many colours for indexed colour", sprite.name);
    let chunk = |kind: u16, data: &[u8]| {
        let mut out = ((data.len() + 6) as u32).to_le_bytes().to_vec();
        out.extend(kind.to_le_bytes());
        out.extend(data);
        out
    };
    let string = |text: &str| {
        let mut out = (text.len() as u16).to_le_bytes().to_vec();
        out.extend(text.as_bytes());
        out
    };
    let mut frames = Vec::new();
    for (index, (canvas, &delay)) in sprite.frames.iter().zip(&sprite.delays).enumerate() {
        let mut chunks = Vec::new();
        if index == 0 {
            // the palette
            let mut data = Vec::new();
            for value in [palette.len() as u32, 0, palette.len() as u32 - 1] {
                data.extend(value.to_le_bytes());
            }
            data.extend([0; 8]);
            for colour in &palette {
                data.extend([0, 0]);
                data.extend(colour);
            }
            chunks.push(chunk(0x2019, &data));
            // a visible image layer at full opacity
            let mut data = Vec::new();
            for word in [1u16, 0, 0, 0, 0, 0] {
                data.extend(word.to_le_bytes());
            }
            data.extend([255, 0, 0, 0]);
            data.extend(string("art"));
            chunks.push(chunk(0x2004, &data));
            if !tags.is_empty() {
                let mut data = (tags.len() as u16).to_le_bytes().to_vec();
                data.extend([0; 8]);
                for &(name, from, to) in tags {
                    data.extend(from.to_le_bytes());
                    data.extend(to.to_le_bytes());
                    // forward
                    data.push(0);
                    data.extend([0; 12]);
                    data.extend(string(name));
                }
                chunks.push(chunk(0x2018, &data));
            }
        }
        // the frame's pixels as palette indices, compressed
        let indices: Vec<u8> = canvas
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .map(|&pixel| {
                let colour = if pixel[3] == 0 { CLEAR } else { pixel };
                palette.iter().position(|&c| c == colour).unwrap() as u8
            })
            .collect();
        let mut zlib = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        zlib.write_all(&indices).unwrap();
        // layer 0 at 0, 0, opaque, compressed image
        let mut data = vec![0, 0, 0, 0, 0, 0, 255, 2, 0];
        data.extend([0; 7]);
        data.extend((width as u16).to_le_bytes());
        data.extend((height as u16).to_le_bytes());
        data.extend(zlib.finish().unwrap());
        chunks.push(chunk(0x2005, &data));

        let data: Vec<u8> = chunks.concat();
        frames.extend(((data.len() + 16) as u32).to_le_bytes());
        frames.extend(0xf1fau16.to_le_bytes());
        frames.extend((chunks.len() as u16).to_le_bytes());
        frames.extend((delay as u16).to_le_bytes());
        frames.extend([0; 2]);
        frames.extend((chunks.len() as u32).to_le_bytes());
        frames.extend(data);
    }
    let mut header = vec![0u8; 128];
    header[0..4].copy_from_slice(&((128 + frames.len()) as u32).to_le_bytes());
    header[4..6].copy_from_slice(&0xa5e0u16.to_le_bytes());
    header[6..8].copy_from_slice(&(sprite.frames.len() as u16).to_le_bytes());
    header[8..10].copy_from_slice(&(width as u16).to_le_bytes());
    header[10..12].copy_from_slice(&(height as u16).to_le_bytes());
    // 8 bits a pixel: indexed
    header[12..14].copy_from_slice(&8u16.to_le_bytes());
    // layer opacity is valid
    header[14..18].copy_from_slice(&1u32.to_le_bytes());
    header[18..20].copy_from_slice(&100u16.to_le_bytes());
    // index 0 is clear
    header[28] = 0;
    header[32..34].copy_from_slice(&(palette.len() as u16).to_le_bytes());
    header.extend(frames);
    header
}

/// The frames side by side in a grid of `columns`, row by row.
pub fn sheet(sprite: &Sprite, columns: u32) -> Vec<u8> {
    let (width, height) = sprite.size();
    let rows = (sprite.frames.len() as u32).div_ceil(columns);
    let mut sheet = Canvas::new(width * columns, height * rows);
    for (index, canvas) in sprite.frames.iter().enumerate() {
        let (column, row) = (index as u32 % columns, index as u32 / columns);
        sheet.paste(canvas, (column * width) as i32, (row * height) as i32);
    }
    png(&sheet)
}
