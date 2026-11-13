//! The chunks of an Aseprite file: layers, cels, tags, tilesets and
//! palettes.

use super::blend::Colour;
use super::read::{Reader, budget, error, inflate, spend};
use super::{Cel, Content, Direction, Layer, LayerKind, Sprite, Tag, Tileset};
use crate::Result;

impl Sprite {
    pub(super) fn read_cel(&self, chunk: &mut Reader, spent: &mut usize) -> Result<Cel> {
        let layer = chunk.u16()?.into();
        let (x, y) = (chunk.i16()?.into(), chunk.i16()?.into());
        let opacity = chunk.u8()?;
        let kind = chunk.u16()?;
        let z = chunk.i16()?.into();
        // reserved
        chunk.skip(5)?;
        let content = match kind {
            0 | 2 => {
                let (width, height) = (chunk.u16()? as usize, chunk.u16()? as usize);
                let size = budget([width, height, self.depth.bytes()], &self.limits)?;
                let size = spend(spent, size, &self.limits)?;
                let pixels = if kind == 0 {
                    chunk.take(size)?.to_vec()
                } else {
                    inflate(chunk.rest(), size)?
                };
                Content::Image { width, height, pixels }
            }
            1 => Content::Linked(chunk.u16()?.into()),
            3 => {
                let (width, height) = (chunk.u16()? as usize, chunk.u16()? as usize);
                let bits = chunk.u16()?;
                let id_mask = chunk.u32()?;
                let flip_mask = chunk.u32()? | chunk.u32()? | chunk.u32()?;
                chunk.skip(10)?;
                let tile_bytes = match bits {
                    8 | 16 | 32 => usize::from(bits / 8),
                    _ => return Err(error(format!("tiles of {bits} bits"))),
                };
                // kept as 32-bit tiles whatever the file's are, beside
                // the file's own while they are read
                spend(spent, budget([width, height, 4], &self.limits)?, &self.limits)?;
                let size = budget([width, height, tile_bytes], &self.limits)?;
                spend(&mut spent.clone(), size, &self.limits)?;
                let data = inflate(chunk.rest(), size)?;
                let tiles = data
                    .chunks_exact(tile_bytes)
                    .map(|tile| tile.iter().rev().fold(0u32, |id, &byte| id << 8 | u32::from(byte)))
                    .collect();
                Content::Tilemap { width, tiles, id_mask, flip_mask }
            }
            other => return Err(error(format!("unknown cel type {other}"))),
        };
        Ok(Cel { layer, x, y, opacity, z, content })
    }

    pub(super) fn read_tileset(&self, chunk: &mut Reader, spent: &mut usize) -> Result<Tileset> {
        let id = chunk.u32()?;
        let flags = chunk.u32()?;
        let count = chunk.u32()? as usize;
        let (width, height) = (chunk.u16()? as usize, chunk.u16()? as usize);
        // the base index the interface numbers tiles from, and reserved
        chunk.skip(2 + 14)?;
        chunk.string()?;
        // flag 1 links to an external file: its ID and the tileset's ID in
        // it, skipped, since only tiles stored here (flag 2) are read
        if flags & 1 != 0 {
            chunk.skip(8)?;
        }
        let pixels = if flags & 2 != 0 {
            let length = chunk.u32()? as usize;
            let size = budget([width, height, count], &self.limits)?;
            let size = budget([size, self.depth.bytes(), 1], &self.limits)?;
            inflate(chunk.take(length)?, spend(spent, size, &self.limits)?)?
        } else {
            return Err(error("a tileset is stored in another file"));
        };
        Ok(Tileset { id, width, height, count, zero_is_empty: flags & 4 != 0, pixels })
    }
}

pub(super) fn read_layer(chunk: &mut Reader) -> Result<Layer> {
    let flags = chunk.u16()?;
    let kind = chunk.u16()?;
    let level = chunk.u16()?;
    chunk.skip(4)?;
    let blend = chunk.u16()?;
    let opacity = chunk.u8()?;
    chunk.skip(3)?;
    let name = chunk.string()?;
    let kind = match kind {
        0 => LayerKind::Image,
        1 => LayerKind::Group,
        2 => LayerKind::Tilemap { tileset: chunk.u32()? },
        other => return Err(error(format!("unknown layer type {other}"))),
    };
    Ok(Layer {
        name,
        visible: flags & 1 != 0,
        reference: flags & 64 != 0,
        background: flags & 8 != 0,
        kind,
        level,
        blend,
        opacity,
        parent: None,
    })
}

pub(super) fn read_tags(chunk: &mut Reader) -> Result<Vec<Tag>> {
    let count = chunk.u16()?;
    chunk.skip(8)?;
    (0..count)
        .map(|_| {
            let (from, to) = (chunk.u16()?.into(), chunk.u16()?.into());
            let direction = match chunk.u8()? {
                0 => Direction::Forward,
                1 => Direction::Reverse,
                2 => Direction::PingPong,
                3 => Direction::PingPongReverse,
                other => return Err(error(format!("unknown tag direction {other}"))),
            };
            // repeats, and reserved
            chunk.skip(2 + 6)?;
            // the colour of files before Aseprite 1.3, which later ones
            // keep in user data
            let [red, green, blue] = chunk.array()?;
            chunk.skip(1)?;
            let colour = [red, green, blue, 255];
            Ok(Tag { name: chunk.string()?, from, to, direction, colour })
        })
        .collect()
}

/// The colour a user data chunk gives what it belongs to, if it gives one.
pub(super) fn read_user_colour(chunk: &mut Reader) -> Result<Option<[u8; 4]>> {
    let flags = chunk.u32()?;
    if flags & 1 != 0 {
        chunk.string()?;
    }
    Ok(if flags & 2 != 0 { Some(chunk.array()?) } else { None })
}

/// The palette chunk changes entries `from..=to` and sets the size.
pub(super) fn read_palette(chunk: &mut Reader, palette: &mut Vec<Colour>) -> Result<()> {
    let size = chunk.u32()? as usize;
    let (from, to) = (chunk.u32()? as usize, chunk.u32()? as usize);
    chunk.skip(8)?;
    if size > 65536 || from > to || to >= size {
        return Err(error("a palette is broken"));
    }
    palette.resize(size, [0; 4]);
    for entry in &mut palette[from..=to] {
        let flags = chunk.u16()?;
        *entry = chunk.array()?;
        if flags & 1 != 0 {
            chunk.string()?;
        }
    }
    Ok(())
}

/// An old palette chunk, which changes entries of `palette` in packets of
/// opaque colours, with 6-bit channels in chunk 11. As Aseprite reads them
/// (`readColorChunk` in src/dio/aseprite_decoder.cpp): each packet starts
/// as many entries after the one before started as it says, and entries
/// past the palette's size are left out.
pub(super) fn read_old_palette(
    chunk: &mut Reader,
    six_bits: bool,
    palette: &mut [Colour],
) -> Result<()> {
    // Aseprite's `scale_6bits_to_8bits`
    let scale = |c: u8| if six_bits { c.min(63) << 2 | c.min(63) >> 4 } else { c };
    let mut start = 0usize;
    for _ in 0..chunk.u16()? {
        start += usize::from(chunk.u8()?);
        let count = match chunk.u8()? {
            0 => 256,
            n => usize::from(n),
        };
        for index in start..start + count {
            let [r, g, b] = chunk.array::<3>()?;
            if let Some(entry) = palette.get_mut(index) {
                *entry = [scale(r), scale(g), scale(b), 255];
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aseprite::Sprite;
    use crate::aseprite::build::*;
    use crate::{Error, Limits};

    #[test]
    fn reads_tag_colours() {
        // the colour in the tag, as older files keep it, and in user data
        // after the tags, as newer ones do, which wins
        let tag = |name: &str, rgb: [u8; 3]| {
            let mut data = vec![0, 0, 0, 0, 0];
            data.extend([0; 8]);
            data.extend(rgb);
            data.push(0);
            data.extend(string(name));
            data
        };
        let mut tags = 2u16.to_le_bytes().to_vec();
        tags.extend([0; 8]);
        tags.extend(tag("old", [1, 2, 3]));
        tags.extend(tag("new", [4, 5, 6]));
        let user_data = |colour: Option<[u8; 4]>| {
            let mut data = u32::from(colour.is_some() as u8 * 2).to_le_bytes().to_vec();
            data.extend(colour.unwrap_or_default());
            chunk(0x2020, &data)
        };
        let bytes = file(
            1,
            1,
            32,
            1,
            &[vec![
                layer(1, 0, 0, 0, 255),
                chunk(0x2018, &tags),
                user_data(None),
                user_data(Some([7, 8, 9, 200])),
                image_cel(0, (0, 0), (1, 1), &RED, false),
            ]],
        );
        let sprite = Sprite::read(&bytes, &Limits::default()).unwrap();
        let colours: Vec<_> = sprite.tags().iter().map(|tag| tag.colour).collect();
        assert_eq!(colours, [[1, 2, 3, 255], [7, 8, 9, 200]]);
    }

    #[test]
    fn reads_indexed_and_grayscale() {
        // index 0 is transparent except on the background layer
        let indexed = |background: bool| {
            file(
                2,
                1,
                8,
                1,
                &[vec![
                    palette(&[GREEN, RED]),
                    layer(if background { 1 | 8 } else { 1 }, 0, 0, 0, 255),
                    image_cel(0, (0, 0), (2, 1), &[0, 1], false),
                ]],
            )
        };
        assert_eq!(pixels(&decode(&indexed(false)).unwrap(), 0), [[0; 4], RED]);
        assert_eq!(pixels(&decode(&indexed(true)).unwrap(), 0), [GREEN, RED]);
        let gray = file(
            1,
            1,
            16,
            1,
            &[vec![layer(1, 0, 0, 0, 255), image_cel(0, (0, 0), (1, 1), &[90, 200], false)]],
        );
        assert_eq!(pixels(&decode(&gray).unwrap(), 0), [[90, 90, 90, 200]]);
    }

    #[test]
    fn reads_tiles_of_8_and_16_bits() {
        for bits in [8, 16, 32] {
            let sprite = |tiles: &[u32]| {
                file(3, 1, 32, 1, &[vec![layer(1, 2, 0, 0, 255), tileset(), tilemap(bits, tiles)]])
            };
            let drawn = decode(&sprite(&[2, 0, 1])).unwrap();
            assert_eq!(pixels(&drawn, 0), [GREEN, [0; 4], RED], "{bits} bits");
            // a tile the set lacks, of an id that still fits a byte
            let lacking = decode(&sprite(&[2, 0, 200]));
            assert!(matches!(lacking, Err(Error::Decode { .. })), "{bits} bits");
        }
        let odd = file(3, 1, 32, 1, &[vec![layer(1, 2, 0, 0, 255), tileset(), tilemap(24, &[1])]]);
        assert!(
            matches!(decode(&odd), Err(Error::Decode { message, .. }) if message.contains("24"))
        );
    }

    #[test]
    fn changes_palettes_between_frames() {
        // index 1 is red in the first frame and green from the second on
        let bytes = file(
            1,
            1,
            8,
            1,
            &[
                vec![
                    palette(&[[0; 4], RED]),
                    layer(1, 0, 0, 0, 255),
                    image_cel(0, (0, 0), (1, 1), &[1], false),
                ],
                vec![palette(&[[0; 4], GREEN]), image_cel(0, (0, 0), (1, 1), &[1], false)],
                vec![image_cel(0, (0, 0), (1, 1), &[1], false)],
            ],
        );
        let animation = decode(&bytes).unwrap();
        let colours: Vec<Colour> = (0..3).map(|f| animation.pixel(f, 0, 0).unwrap()).collect();
        assert_eq!(colours, [RED, GREEN, GREEN]);
    }

    /// An old palette chunk of `packets`, each of entries to skip from the
    /// last packet's start and its colours.
    fn old_palette(kind: u16, packets: &[(u8, &[[u8; 3]])]) -> Vec<u8> {
        let mut data = (packets.len() as u16).to_le_bytes().to_vec();
        for &(skip, colours) in packets {
            data.extend([skip, colours.len() as u8]);
            data.extend(colours.concat());
        }
        chunk(kind, &data)
    }

    #[test]
    fn reads_old_palette_chunks() {
        // indices 0 to 6 on a background layer, which shows index 0 too
        let indexed = |frames: Vec<Vec<Vec<u8>>>| {
            let mut frames = frames;
            for (index, chunks) in frames.iter_mut().enumerate() {
                if index == 0 {
                    chunks.push(layer(1 | 8, 0, 0, 0, 255));
                }
                chunks.push(image_cel(0, (0, 0), (7, 1), &[0, 1, 2, 3, 4, 5, 6], false));
            }
            let animation = decode(&file(7, 1, 8, 1, &frames)).unwrap();
            (0..animation.frames().len()).map(|frame| pixels(&animation, frame)).collect::<Vec<_>>()
        };
        const BLACK: Colour = [0, 0, 0, 255];
        // chunk 4, of 8 bits: the second packet starts 5 after the first
        // started, and what no packet sets stays opaque black
        let eight = old_palette(4, &[(0, &[[255, 0, 0], [0, 255, 0]]), (5, &[[0, 0, 255]])]);
        assert_eq!(indexed(vec![vec![eight]]), [[RED, GREEN, BLACK, BLACK, BLACK, BLUE, BLACK]]);
        // chunk 11, of 6 bits, as Aseprite scales them
        let six = old_palette(11, &[(1, &[[63, 32, 1]])]);
        let scaled = [255, 130, 4, 255];
        assert_eq!(indexed(vec![vec![six.clone()]])[0][..3], [BLACK, scaled, BLACK]);
        // each from its frame on, over the palette before it
        let red = old_palette(4, &[(2, &[[255, 0, 0]])]);
        let frames = indexed(vec![vec![six.clone()], vec![red.clone()], vec![]]);
        let changed: Vec<_> = frames.iter().map(|frame| (frame[1], frame[2])).collect();
        assert_eq!(changed, [(scaled, BLACK), (scaled, RED), (scaled, RED)]);
        // but not once a new palette chunk has been read: Aseprite 1.1 wrote
        // both, and the new one is the one to read
        let new = palette(&[GREEN, BLUE]);
        let frames = indexed(vec![vec![six.clone(), new], vec![red]]);
        assert_eq!(frames[1][..3], [GREEN, BLUE, [0; 4]]);
    }
}
