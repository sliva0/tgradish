//! Aseprite files through `asefile`, which renders frames with every layer
//! blend mode, tilemaps and linked cels. Not supported: per-cel z-index
//! (Aseprite 1.3) and group opacity, which are rare in pixel art.

use std::time::Duration;

use asefile::{AnimationDirection, AsepriteFile};

use crate::{Animation, DEFAULT_FRAME_DURATION, Error, Format, Frame, Result};

/// How a tag plays its frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Forward,
    Reverse,
    /// There and back: `from..=to`, then `to-1..from` without repeating the
    /// ends.
    PingPong,
    /// Back and there: `to..=from`, then `from+1..to`.
    PingPongReverse,
}

/// A named range of frames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    pub from: u32,
    pub to: u32,
    pub direction: Direction,
}

impl Tag {
    /// Frame indices in playing order, for one loop of a looping sticker.
    pub fn frames(&self) -> Vec<u32> {
        let forward = self.from..=self.to;
        // the inner frames of the way back, which a loop doesn't repeat
        let inner = || (self.from + 1..self.to).rev();
        match self.direction {
            Direction::Forward => forward.collect(),
            Direction::Reverse => forward.rev().collect(),
            Direction::PingPong => forward.chain(inner()).collect(),
            Direction::PingPongReverse => {
                forward.clone().rev().chain(inner().rev()).collect::<Vec<_>>()
            }
        }
    }
}

/// A parsed Aseprite file.
pub struct Sprite {
    file: AsepriteFile,
    tags: Vec<Tag>,
}

impl std::fmt::Debug for Sprite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sprite")
            .field("size", &self.file.size())
            .field("frames", &self.file.num_frames())
            .field("tags", &self.tags)
            .finish()
    }
}

fn error(message: impl ToString) -> Error {
    Error::Decode { format: Format::Aseprite, message: message.to_string() }
}

impl Sprite {
    pub fn read(bytes: &[u8]) -> Result<Sprite> {
        // asefile predates the "ping-pong reverse" direction and rejects
        // files that use it, so those tags are read as ping-pong and
        // reversed here.
        let (bytes, reversed) = rewrite_ping_pong_reverse(bytes)?;
        let file = AsepriteFile::read(&bytes[..]).map_err(error)?;
        let tags = (0..file.num_tags())
            .map(|index| {
                let tag = file.tag(index);
                let direction = match tag.animation_direction() {
                    AnimationDirection::Forward => Direction::Forward,
                    AnimationDirection::Reverse => Direction::Reverse,
                    AnimationDirection::PingPong if reversed.contains(&(index as usize)) => {
                        Direction::PingPongReverse
                    }
                    AnimationDirection::PingPong => Direction::PingPong,
                };
                Tag {
                    name: tag.name().to_owned(),
                    from: tag.from_frame(),
                    to: tag.to_frame(),
                    direction,
                }
            })
            .collect::<Vec<_>>();
        if let Some(tag) = tags.iter().find(|tag| tag.from > tag.to || tag.to >= file.num_frames())
        {
            return Err(error(format!("tag {:?} has frames outside the file", tag.name)));
        }
        Ok(Sprite { file, tags })
    }

    pub fn tags(&self) -> &[Tag] {
        &self.tags
    }

    pub fn width(&self) -> u32 {
        self.file.width() as u32
    }

    pub fn height(&self) -> u32 {
        self.file.height() as u32
    }

    /// The frames of `tag`, or all frames. Hidden layers stay hidden.
    pub fn animation(&self, tag: Option<&str>) -> Result<Animation> {
        let order = match tag {
            None => (0..self.file.num_frames()).collect(),
            Some(name) => self
                .tags
                .iter()
                .find(|tag| tag.name == name)
                .ok_or_else(|| Error::NoSuchTag {
                    name: name.to_owned(),
                    available: self.tags.iter().map(|tag| tag.name.clone()).collect(),
                })?
                .frames(),
        };
        let frames = order
            .into_iter()
            .map(|index| {
                let frame = self.file.frame(index);
                let duration = match frame.duration() {
                    0 => DEFAULT_FRAME_DURATION,
                    ms => Duration::from_millis(ms.into()),
                };
                Frame { rgba: frame.image().into_raw(), duration }
            })
            .collect();
        Animation::new(self.width(), self.height(), frames)
    }
}

const TAGS_CHUNK: u16 = 0x2018;
const PING_PONG: u8 = 2;
const PING_PONG_REVERSE: u8 = 3;

/// Changes "ping-pong reverse" tags to "ping-pong" and returns their
/// indices. Walks frames and chunks the way the file format describes; a
/// file this can't walk is left as is for asefile to report.
fn rewrite_ping_pong_reverse(bytes: &[u8]) -> Result<(std::borrow::Cow<'_, [u8]>, Vec<usize>)> {
    let u16_at = |at: usize| bytes.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
    let u32_at = |at: usize| {
        bytes.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
    };
    let mut patches = Vec::new();
    let mut reversed = Vec::new();
    let frame_count = u16_at(6).unwrap_or(0);
    let mut frame_at = 128;
    'frames: for _ in 0..frame_count {
        let Some(frame_len) = u32_at(frame_at).filter(|&len| len >= 16) else { break };
        let frame_end = frame_at.saturating_add(frame_len).min(bytes.len());
        let mut chunk_at = frame_at + 16;
        while chunk_at + 6 <= frame_end {
            let (Some(chunk_len), Some(kind)) = (u32_at(chunk_at), u16_at(chunk_at + 4)) else {
                break 'frames;
            };
            if chunk_len < 6 {
                break 'frames;
            }
            if kind == TAGS_CHUNK {
                // count, 8 reserved bytes, then the tags
                let mut tag_at = chunk_at + 6 + 2 + 8;
                for index in 0..u16_at(chunk_at + 6).unwrap_or(0) as usize {
                    // from, to, then the direction byte
                    let direction_at = tag_at + 4;
                    if bytes.get(direction_at) == Some(&PING_PONG_REVERSE) {
                        patches.push(direction_at);
                        reversed.push(index);
                    }
                    // direction, repeat, 6 + 3 + 1 bytes, then the name
                    let name_at = direction_at + 1 + 2 + 10;
                    let Some(name_len) = u16_at(name_at) else { break 'frames };
                    tag_at = name_at + 2 + name_len as usize;
                }
            }
            chunk_at = chunk_at.saturating_add(chunk_len);
        }
        frame_at = frame_at.saturating_add(frame_len);
    }
    if patches.is_empty() {
        return Ok((bytes.into(), reversed));
    }
    let mut owned = bytes.to_vec();
    for at in patches {
        owned[at] = PING_PONG;
    }
    Ok((owned.into(), reversed))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(from: u32, to: u32, direction: Direction) -> Vec<u32> {
        Tag { name: String::new(), from, to, direction }.frames()
    }

    #[test]
    fn plays_tags_in_order() {
        assert_eq!(tag(2, 5, Direction::Forward), [2, 3, 4, 5]);
        assert_eq!(tag(2, 5, Direction::Reverse), [5, 4, 3, 2]);
        assert_eq!(tag(2, 5, Direction::PingPong), [2, 3, 4, 5, 4, 3]);
        assert_eq!(tag(2, 5, Direction::PingPongReverse), [5, 4, 3, 2, 3, 4]);
        assert_eq!(tag(3, 3, Direction::PingPong), [3]);
        assert_eq!(tag(3, 4, Direction::PingPongReverse), [4, 3]);
    }
}

#[cfg(test)]
mod file_tests {
    use super::*;

    /// A 1x1 RGBA sprite with a red and a green frame and two tags over
    /// both: "back" (ping-pong reverse) and "on" (forward).
    fn sprite() -> Vec<u8> {
        fn chunk(kind: u16, data: &[u8]) -> Vec<u8> {
            let mut out = ((data.len() + 6) as u32).to_le_bytes().to_vec();
            out.extend(kind.to_le_bytes());
            out.extend(data);
            out
        }
        fn frame(chunks: &[Vec<u8>]) -> Vec<u8> {
            let body: Vec<u8> = chunks.concat();
            let mut out = ((body.len() + 16) as u32).to_le_bytes().to_vec();
            out.extend(0xf1fau16.to_le_bytes());
            out.extend((chunks.len() as u16).to_le_bytes());
            out.extend(50u16.to_le_bytes()); // duration
            out.extend([0; 2]);
            out.extend((chunks.len() as u32).to_le_bytes());
            out.extend(body);
            out
        }
        // raw cel at 0,0 on layer 0
        let cel = |rgba: [u8; 4]| {
            let mut data = vec![0, 0, 0, 0, 0, 0, 255, 0, 0, 0, 0];
            data.extend([0; 5]);
            data.extend([1, 0, 1, 0]);
            data.extend(rgba);
            chunk(0x2005, &data)
        };
        // visible normal layer "L", opacity 255
        let layer = chunk(0x2004, &[1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 0, 0, 0, 1, 0, b'L']);
        let mut tags = vec![2, 0];
        tags.extend([0; 8]);
        for (direction, name) in [(PING_PONG_REVERSE, b"back".as_slice()), (0, b"on")] {
            tags.extend([0, 0, 1, 0, direction, 0, 0]);
            tags.extend([0; 10]);
            tags.extend((name.len() as u16).to_le_bytes());
            tags.extend(name);
        }
        let frames = [
            frame(&[layer, chunk(TAGS_CHUNK, &tags), cel([255, 0, 0, 255])]),
            frame(&[cel([0, 255, 0, 255])]),
        ]
        .concat();

        let mut header = vec![0u8; 128];
        header[0..4].copy_from_slice(&((128 + frames.len()) as u32).to_le_bytes());
        header[4..6].copy_from_slice(&0xa5e0u16.to_le_bytes());
        header[6..8].copy_from_slice(&2u16.to_le_bytes()); // frames
        header[8..10].copy_from_slice(&1u16.to_le_bytes()); // width
        header[10..12].copy_from_slice(&1u16.to_le_bytes()); // height
        header[12..14].copy_from_slice(&32u16.to_le_bytes()); // RGBA
        header[14..18].copy_from_slice(&1u32.to_le_bytes()); // layer opacity valid
        header[18..20].copy_from_slice(&100u16.to_le_bytes()); // old speed
        header.extend(frames);
        header
    }

    #[test]
    fn reads_ping_pong_reverse_tags() {
        let sprite = Sprite::read(&sprite()).unwrap();
        let directions: Vec<_> = sprite.tags().iter().map(|tag| tag.direction).collect();
        assert_eq!(directions, [Direction::PingPongReverse, Direction::Forward]);

        let colours = |animation: Animation| -> Vec<[u8; 4]> {
            (0..animation.frames().len()).map(|i| animation.pixel(i, 0, 0).unwrap()).collect()
        };
        let red = [255, 0, 0, 255];
        let green = [0, 255, 0, 255];
        assert_eq!(colours(sprite.animation(Some("back")).unwrap()), [green, red]);
        assert_eq!(colours(sprite.animation(Some("on")).unwrap()), [red, green]);
        let all = sprite.animation(None).unwrap();
        assert_eq!(all.duration(), Duration::from_millis(100));
        assert!(matches!(
            sprite.animation(Some("nope")),
            Err(Error::NoSuchTag { available, .. }) if available == ["back", "on"]
        ));
    }
}
