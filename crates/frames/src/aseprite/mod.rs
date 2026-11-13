//! Aseprite files (<https://github.com/aseprite/aseprite/blob/main/docs/ase-file-specs.md>),
//! parsed and rendered here. Files can come from anywhere, so every read
//! is checked and malformed files are errors, never panics.
//!
//! Rendering follows Aseprite's: layers bottom to top, hidden and
//! reference layers skipped (hidden ones can be shown, and shown ones
//! hidden, by name), layer and cel opacity, every blend mode,
//! groups composited separately when the file asks for it, linked cels,
//! per-cel z-index, and tilemaps. Flipped tiles are refused rather than
//! drawn wrong.

mod blend;
mod chunks;
mod read;
mod render;
#[cfg(test)]
mod testfile;

use std::time::Duration;

use blend::Colour;
use chunks::{read_layer, read_old_palette, read_palette, read_tags, read_user_colour};
use read::{Reader, budget, error, kept, spend};

use crate::{Animation, DEFAULT_FRAME_DURATION, Error, Frame, Limits, Oversize, Result};

/// Groups nested deeper than this are refused; Aseprite's interface makes
/// nothing close, and rendering recurses once per level.
const MAX_GROUP_DEPTH: usize = 64;

/// How a tag plays its frames.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Direction {
    /// From its first frame to its last.
    #[default]
    Forward,
    /// From its last frame to its first.
    Reverse,
    /// There and back: `from..=to`, then `to-1..from` without repeating the
    /// ends.
    PingPong,
    /// Back and there: `to..=from`, then `from+1..to`.
    PingPongReverse,
}

/// A named range of frames. Only read from files; [`Default`] gives an
/// empty one for tests to fill in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Tag {
    /// What it is called in the file.
    pub name: String,
    /// Its first frame, from 0.
    pub from: u32,
    /// Its last frame, which it includes.
    pub to: u32,
    /// How it plays them.
    pub direction: Direction,
    /// The colour Aseprite shows it in, straight RGBA.
    pub colour: [u8; 4],
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

/// A layer as Aseprite's Layers panel lists it. Reference layers, which
/// exports leave out, aren't listed. Only read from files; [`Default`]
/// gives an empty one for tests to fill in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct LayerInfo {
    /// What it is called in the file.
    pub name: String,
    /// A group, which shows or hides the layers in it.
    pub group: bool,
    /// How deep in groups it is: 0 outside them.
    pub level: u16,
    /// Shown in the file.
    pub visible: bool,
    /// The background layer, which is opaque and lies under the others.
    pub background: bool,
}

/// Layers to show or hide by name, over what the file shows. A name stands
/// for every layer of that name; one in both lists is hidden. Built as
/// `Layers { hide, ..Layers::default() }`, so that what is added later
/// keeps its default.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Layers {
    /// Layers to show, hidden ones too.
    pub show: Vec<String>,
    /// Layers to hide; a hidden group hides the layers in it.
    pub hide: Vec<String>,
}

impl Layers {
    /// Whether it leaves the file's choice as it is.
    pub fn is_empty(&self) -> bool {
        self.show.is_empty() && self.hide.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Depth {
    Rgba,
    Grayscale,
    Indexed,
}

impl Depth {
    fn bytes(self) -> usize {
        match self {
            Depth::Rgba => 4,
            Depth::Grayscale => 2,
            Depth::Indexed => 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LayerKind {
    Image,
    Group,
    Tilemap { tileset: u32 },
}

#[derive(Debug, Clone)]
struct Layer {
    name: String,
    /// Shown in the file.
    visible: bool,
    /// For drawing over, never exported.
    reference: bool,
    background: bool,
    kind: LayerKind,
    level: u16,
    blend: u16,
    opacity: u8,
    parent: Option<usize>,
}

#[derive(Debug, Clone)]
enum Content {
    Image { width: usize, height: usize, pixels: Vec<u8> },
    Linked(usize),
    Tilemap { width: usize, tiles: Vec<u32>, id_mask: u32, flip_mask: u32 },
}

#[derive(Debug, Clone)]
struct Cel {
    layer: usize,
    x: i64,
    y: i64,
    opacity: u8,
    z: i64,
    content: Content,
}

#[derive(Debug, Clone)]
struct Tileset {
    id: u32,
    width: usize,
    height: usize,
    count: usize,
    /// Tile 0 is the empty tile; in older files it is `0xffffffff`.
    zero_is_empty: bool,
    pixels: Vec<u8>,
}

/// A parsed Aseprite file, from which animations of its tags and layers
/// are drawn. [`decode`](crate::decode) reads one and draws it at once;
/// reading it here first lists its tags and layers to choose from.
#[derive(Debug, Clone)]
pub struct Sprite {
    width: u32,
    height: u32,
    depth: Depth,
    transparent: u8,
    layer_opacity: bool,
    composite_groups: bool,
    /// Every version of the palette, and which one each frame uses: a
    /// palette chunk changes it from its frame on.
    palettes: Vec<Vec<Colour>>,
    frame_palettes: Vec<usize>,
    layers: Vec<Layer>,
    /// Children of each layer, and of the top level last.
    children: Vec<Vec<usize>>,
    /// Where each layer is among its parent's children, from the bottom.
    positions: Vec<usize>,
    /// Deepest nesting of groups.
    depth_of_groups: usize,
    /// Each frame's duration and cels, sorted by layer.
    frames: Vec<(Duration, Vec<Cel>)>,
    tags: Vec<Tag>,
    tilesets: Vec<Tileset>,
    limits: Limits,
    /// Bytes it holds: pixels, tiles and palettes, and what it keeps of
    /// its layers, cels, tags and frames. The frames drawn from it are held
    /// beside them, so the limit is for both together.
    spent: usize,
}

impl Sprite {
    /// Parses an Aseprite file, holding what it stores within `limits`;
    /// [`Sprite::animation`] draws frames within them too.
    pub fn read(bytes: &[u8], limits: &Limits) -> Result<Sprite> {
        let mut file = Reader::new(bytes);
        let mut header = Reader::new(file.take(128)?);
        header.skip(4)?;
        if header.u16()? != 0xa5e0 {
            return Err(error("not an Aseprite file"));
        }
        let frame_count = header.u16()?;
        let (width, height) = (u32::from(header.u16()?), u32::from(header.u16()?));
        let depth = match header.u16()? {
            32 => Depth::Rgba,
            16 => Depth::Grayscale,
            8 => Depth::Indexed,
            other => return Err(error(format!("unknown colour depth {other}"))),
        };
        let flags = header.u32()?;
        // deprecated by each frame's own duration, but still the one to go
        // by where a frame says 0
        let speed = header.u16()?;
        // two DWORDs that must be 0
        header.skip(8)?;
        let transparent = header.u8()?;
        header.skip(3)?;
        // the palette before any palette chunk changes it, in opaque black,
        // as Aseprite starts it; 0 in old files means 256
        let colours = match header.u16()? {
            0 => 256,
            n => usize::from(n),
        };
        if width == 0 || height == 0 {
            return Err(error(format!("the canvas is {width}x{height}")));
        }
        if width.max(height) > limits.max_dimension {
            return Err(limits.dimension(Oversize::Canvas { width, height }));
        }

        // what the sprite keeps of each frame, before room is made for them
        let mut spent = 0;
        let frame_cost = kept::<(Duration, Vec<Cel>)>(1) + kept::<usize>(1);
        spend(&mut spent, frame_cost.saturating_mul(frame_count.into()), limits)?;
        let mut sprite = Sprite {
            width,
            height,
            depth,
            transparent,
            layer_opacity: flags & 1 != 0,
            composite_groups: flags & 2 != 0,
            palettes: Vec::new(),
            frame_palettes: Vec::new(),
            layers: Vec::new(),
            children: Vec::new(),
            positions: Vec::new(),
            depth_of_groups: 0,
            frames: Vec::with_capacity(frame_count.into()),
            tags: Vec::new(),
            tilesets: Vec::new(),
            limits: *limits,
            spent: 0,
        };
        // Aseprite 1.1 saved old palette chunks beside new ones, which are
        // to be read instead: like Aseprite, old ones count only until a
        // new one has been read, anywhere in the file
        let mut new_palette = false;
        // only indexed colour has a use for one
        let colours = if depth == Depth::Indexed { colours } else { 0 };
        let mut palette: Vec<Colour> = vec![[0, 0, 0, 255]; colours];
        for _ in 0..frame_count {
            let length = file.u32()? as usize;
            if file.u16()? != 0xf1fa {
                return Err(error("a frame header is broken"));
            }
            let old_count = file.u16()?;
            let duration = file.u16()?;
            // reserved
            file.skip(2)?;
            let new_count = file.u32()?;
            let body_length =
                length.checked_sub(16).ok_or_else(|| error("a frame is too short"))?;
            let mut body = Reader::new(file.take(body_length)?);
            // the newer count, when set, overrides the old WORD one, which
            // says 0xFFFF when there are more chunks than it holds; older
            // files leave the newer one 0
            let chunks = if new_count != 0 { new_count } else { old_count.into() };
            // user data chunks right after the tags are theirs, in order
            let mut tag_data: Option<usize> = None;
            let mut cels = Vec::new();
            let mut changed = sprite.palettes.is_empty();
            for _ in 0..chunks {
                let size = body.u32()? as usize;
                let kind = body.u16()?;
                let size = size.checked_sub(6).ok_or_else(|| error("a chunk is too short"))?;
                let mut chunk = Reader::new(body.take(size)?);
                let next_tag = tag_data.take();
                match kind {
                    // 4 and 11, written as 0x0011 in the specification but 11
                    // in Aseprite's own source, each changing the palette
                    // from its frame on, as new ones do
                    0x0004 | 0x000b if !new_palette => {
                        read_old_palette(&mut chunk, kind == 0x000b, &mut palette)?;
                        changed = true;
                    }
                    0x2004 => {
                        // the layer, its name, which the chunk holds, and
                        // where it is among its group's
                        let linking = kept::<Vec<usize>>(1) + kept::<usize>(2);
                        let cost = kept::<Layer>(1) + chunk.left() + linking;
                        spend(&mut spent, cost, limits)?;
                        sprite.layers.push(read_layer(&mut chunk)?);
                    }
                    0x2005 => {
                        spend(&mut spent, kept::<Cel>(1), limits)?;
                        cels.push(sprite.read_cel(&mut chunk, &mut spent)?);
                    }
                    0x2018 => {
                        sprite.tags = read_tags(&mut chunk, &mut spent, limits)?;
                        tag_data = Some(0);
                    }
                    0x2020 if let Some(index) = next_tag => {
                        if let (Some(tag), Some(colour)) =
                            (sprite.tags.get_mut(index), read_user_colour(&mut chunk)?)
                        {
                            tag.colour = colour;
                        }
                        tag_data = Some(index + 1);
                    }
                    0x2019 => {
                        new_palette = true;
                        read_palette(&mut chunk, &mut palette)?;
                        changed = true;
                    }
                    0x2023 => {
                        spend(&mut spent, kept::<Tileset>(1), limits)?;
                        let tileset = sprite.read_tileset(&mut chunk, &mut spent)?;
                        sprite.tilesets.push(tileset);
                    }
                    _ => {}
                }
            }
            // a frame of 0 ms takes the header's deprecated speed, and the
            // default when that is 0 too
            let duration = match (duration, speed) {
                (0, 0) => DEFAULT_FRAME_DURATION,
                (0, speed) => Duration::from_millis(speed.into()),
                (ms, _) => Duration::from_millis(ms.into()),
            };
            // the first cel of each layer counts, like in Aseprite
            cels.sort_by_key(|cel: &Cel| cel.layer);
            cels.dedup_by_key(|cel| cel.layer);
            sprite.frames.push((duration, cels));
            if changed {
                spend(&mut spent, kept::<Vec<Colour>>(1) + kept::<Colour>(palette.len()), limits)?;
                sprite.palettes.push(palette.clone());
            }
            sprite.frame_palettes.push(sprite.palettes.len() - 1);
        }
        if sprite.frames.is_empty() {
            return Err(Error::Empty);
        }
        sprite.spent = spent;
        sprite.link_layers()?;
        sprite.check_cels()?;
        let frames = sprite.frames.len() as u32;
        if let Some(tag) = sprite.tags.iter().find(|tag| tag.from > tag.to || tag.to >= frames) {
            return Err(error(format!("tag {:?} has frames outside the file", tag.name)));
        }
        Ok(sprite)
    }

    /// Finds each layer's parent group from the nesting levels.
    fn link_layers(&mut self) -> Result<()> {
        // the last layer seen at each level
        let mut open: Vec<usize> = Vec::new();
        for index in 0..self.layers.len() {
            let level = usize::from(self.layers[index].level);
            if level > MAX_GROUP_DEPTH {
                return Err(error(format!("groups are nested more than {MAX_GROUP_DEPTH} deep")));
            }
            if level > open.len() {
                return Err(error("a layer is nested deeper than its group"));
            }
            open.truncate(level);
            let parent = open.last().copied();
            if let Some(parent) = parent
                && self.layers[parent].kind != LayerKind::Group
            {
                return Err(error("a layer is inside a layer that isn't a group"));
            }
            self.layers[index].parent = parent;
            open.push(index);
            self.depth_of_groups = self.depth_of_groups.max(level);
        }
        self.children = vec![Vec::new(); self.layers.len() + 1];
        self.positions = Vec::with_capacity(self.layers.len());
        for (index, layer) in self.layers.iter().enumerate() {
            let siblings = &mut self.children[layer.parent.unwrap_or(self.layers.len())];
            self.positions.push(siblings.len());
            siblings.push(index);
        }
        Ok(())
    }

    /// Cels must name existing layers, links existing image cels.
    fn check_cels(&self) -> Result<()> {
        for (_, cels) in &self.frames {
            for cel in cels {
                let layer =
                    self.layers.get(cel.layer).ok_or_else(|| error("a cel's layer is missing"))?;
                match (&cel.content, layer.kind) {
                    (Content::Linked(frame), _) => {
                        let target = self.stored_cel(*frame, cel.layer);
                        if !matches!(
                            target,
                            Some(Cel {
                                content: Content::Image { .. } | Content::Tilemap { .. },
                                ..
                            })
                        ) {
                            return Err(error("a linked cel points at nothing"));
                        }
                    }
                    (Content::Tilemap { .. }, LayerKind::Tilemap { tileset }) => {
                        if !self.tilesets.iter().any(|t| t.id == tileset) {
                            return Err(error("a tilemap's tileset is missing"));
                        }
                    }
                    (Content::Tilemap { .. }, _) => {
                        return Err(error("a tilemap cel is on another kind of layer"));
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    /// The tags, in the file's order.
    pub fn tags(&self) -> &[Tag] {
        &self.tags
    }

    /// How long each frame of the file shows, which tags count from 0.
    pub fn durations(&self) -> Vec<Duration> {
        self.frames.iter().map(|(duration, _)| *duration).collect()
    }

    /// The canvas's width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// The canvas's height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The layers, as Aseprite's Layers panel lists them: the top one first,
    /// each group before the layers in it.
    pub fn layers(&self) -> Vec<LayerInfo> {
        let mut listed = Vec::new();
        self.list_layers(self.layers.len(), &mut listed);
        listed
    }

    fn list_layers(&self, parent: usize, listed: &mut Vec<LayerInfo>) {
        // children are stored bottom first
        for &index in self.children[parent].iter().rev() {
            let layer = &self.layers[index];
            if layer.reference {
                continue;
            }
            let group = layer.kind == LayerKind::Group;
            let (name, level, visible) = (layer.name.clone(), layer.level, layer.visible);
            let background = layer.background;
            listed.push(LayerInfo { name, group, level, visible, background });
            if group {
                self.list_layers(index, listed);
            }
        }
    }

    /// Which layers are drawn: those the file shows, with `layers` over
    /// them, and never reference layers.
    fn shown(&self, layers: &Layers) -> Result<Vec<bool>> {
        let named = |name: &String| self.layers.iter().any(|l| !l.reference && l.name == *name);
        if let Some(name) = layers.show.iter().chain(&layers.hide).find(|name| !named(name)) {
            let mut available: Vec<String> = Vec::new();
            for layer in self.layers() {
                if !available.contains(&layer.name) {
                    available.push(layer.name);
                }
            }
            return Err(Error::NoSuchLayer { name: name.clone(), available });
        }
        Ok(self
            .layers
            .iter()
            .map(|layer| {
                !layer.reference
                    && !layers.hide.contains(&layer.name)
                    && (layer.visible || layers.show.contains(&layer.name))
            })
            .collect())
    }

    /// The frames of `tag`, or all frames, with the layers the file shows
    /// and `layers` changes.
    pub fn animation(&self, tag: Option<&str>, layers: &Layers) -> Result<Animation> {
        // a hidden group hides what is in it; groups come before their layers
        let mut shown = self.shown(layers)?;
        for index in 0..shown.len() {
            if let Some(parent) = self.layers[index].parent {
                shown[index] &= shown[parent];
            }
        }
        let order: Vec<u32> = match tag {
            None => (0..self.frames.len() as u32).collect(),
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
        // the frames, and while drawing one, a canvas for each group level,
        // all beside what the sprite holds
        let frame_size = budget([self.width as usize, self.height as usize, 4], &self.limits)?;
        let canvases = order.len().saturating_add(self.depth_of_groups + 1);
        let drawn = budget([frame_size, canvases, 1], &self.limits)?;
        spend(&mut self.spent.clone(), drawn, &self.limits)?;
        let frames = order
            .into_iter()
            .map(|index| {
                let (duration, _) = self.frames[index as usize];
                let canvas = self.render(index as usize, &shown)?;
                Ok(Frame { rgba: canvas.into_flattened(), duration })
            })
            .collect::<Result<_>>()?;
        Animation::new(self.width, self.height, frames)
    }

    /// The cel of a layer in a frame, as stored (links unresolved).
    fn stored_cel(&self, frame: usize, layer: usize) -> Option<&Cel> {
        let cels = &self.frames.get(frame)?.1;
        cels.binary_search_by_key(&layer, |cel| cel.layer).ok().map(|index| &cels[index])
    }

    /// The cel of a layer in a frame, and its pixels: a linked cel keeps
    /// its own position, opacity and z-index, and shows the pixels of the
    /// cel it links to.
    fn cel(&self, frame: usize, layer: usize) -> Option<(&Cel, &Content)> {
        let cel = self.stored_cel(frame, layer)?;
        match cel.content {
            // checked when reading: links point at image or tilemap cels
            Content::Linked(target) => Some((cel, &self.stored_cel(target, layer)?.content)),
            ref content => Some((cel, content)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aseprite::testfile::*;

    #[test]
    fn plays_tags_in_order() {
        let tag = |from, to, direction| {
            Tag { name: String::new(), from, to, direction, colour: [0; 4] }.frames()
        };
        assert_eq!(tag(2, 5, Direction::Forward), [2, 3, 4, 5]);
        assert_eq!(tag(2, 5, Direction::Reverse), [5, 4, 3, 2]);
        assert_eq!(tag(2, 5, Direction::PingPong), [2, 3, 4, 5, 4, 3]);
        assert_eq!(tag(2, 5, Direction::PingPongReverse), [5, 4, 3, 2, 3, 4]);
        assert_eq!(tag(3, 3, Direction::PingPong), [3]);
        assert_eq!(tag(3, 4, Direction::PingPongReverse), [4, 3]);
    }

    #[test]
    fn reads_tags_and_links() {
        let bytes = file(
            1,
            1,
            32,
            1,
            &[
                vec![
                    layer(1, 0, 0, 0, 255),
                    tags(&[(0, 2, 3, "back"), (1, 2, 0, "on")]),
                    image_cel(0, (0, 0), (1, 1), &RED, false),
                ],
                vec![image_cel(0, (0, 0), (1, 1), &GREEN, true)],
                vec![linked_cel(0, 0)],
            ],
        );
        let sprite = Sprite::read(&bytes, &Limits::default()).unwrap();
        let first = |tag| -> Vec<Colour> {
            let animation = sprite.animation(tag, &Layers::default()).unwrap();
            (0..animation.frames().len()).map(|i| animation.pixel(i, 0, 0).unwrap()).collect()
        };
        assert_eq!(first(None), [RED, GREEN, RED]);
        // ping-pong reverse over 0..=2: 2, 1, 0, 1
        assert_eq!(first(Some("back")), [RED, GREEN, RED, GREEN]);
        assert_eq!(first(Some("on")), [GREEN, RED]);
        assert_eq!(
            sprite.animation(None, &Layers::default()).unwrap().duration(),
            Duration::from_millis(150)
        );
        assert!(matches!(
            sprite.animation(Some("nope"), &Layers::default()),
            Err(Error::NoSuchTag { available, .. }) if available == ["back", "on"]
        ));
    }

    #[test]
    fn shows_and_hides_layers_by_name() {
        // bottom first: a red layer, a group of a green layer and a hidden
        // blue one, a reference layer and a hidden blue layer
        let bytes = file(
            4,
            1,
            32,
            1,
            &[vec![
                named("Back", 1, 0, 0, 0, 255),
                named("Arms", 1, 1, 0, 0, 255),
                named("Left", 1, 0, 1, 0, 255),
                named("Right", 0, 0, 1, 0, 255),
                named("Guide", 1 | 64, 0, 0, 0, 255),
                named("Shadow", 0, 0, 0, 0, 255),
                image_cel(0, (0, 0), (1, 1), &RED, false),
                image_cel(2, (1, 0), (1, 1), &GREEN, false),
                image_cel(3, (2, 0), (1, 1), &BLUE, false),
                image_cel(4, (3, 0), (1, 1), &GREEN, false),
                image_cel(5, (3, 0), (1, 1), &BLUE, false),
            ]],
        );
        let sprite = Sprite::read(&bytes, &Limits::default()).unwrap();
        let listed: Vec<_> = sprite
            .layers()
            .into_iter()
            .map(|layer| (layer.name, layer.group, layer.level, layer.visible))
            .collect();
        let name = String::from;
        assert_eq!(
            listed,
            [
                (name("Shadow"), false, 0, false),
                (name("Arms"), true, 0, true),
                (name("Right"), false, 1, false),
                (name("Left"), false, 1, true),
                (name("Back"), false, 0, true),
            ]
        );
        let drawn = |show: &[&str], hide: &[&str]| {
            let names = |list: &[&str]| list.iter().map(|&n| n.to_owned()).collect();
            let layers = Layers { show: names(show), hide: names(hide) };
            sprite.animation(None, &layers).map(|animation| pixels(&animation, 0))
        };
        const CLEAR: Colour = [0; 4];
        assert_eq!(drawn(&[], &[]), Ok(vec![RED, GREEN, CLEAR, CLEAR]));
        assert_eq!(drawn(&["Right", "Shadow"], &[]), Ok(vec![RED, GREEN, BLUE, BLUE]));
        // a hidden group hides what is in it, shown or not
        assert_eq!(drawn(&["Right"], &["Arms"]), Ok(vec![RED, CLEAR, CLEAR, CLEAR]));
        // named in both, hidden
        assert_eq!(drawn(&["Back"], &["Back"]), Ok(vec![CLEAR, GREEN, CLEAR, CLEAR]));
        // reference layers are never drawn, so can't be named
        assert!(matches!(
            drawn(&["Guide"], &[]),
            Err(Error::NoSuchLayer { available, .. })
                if available == ["Shadow", "Arms", "Right", "Left", "Back"]
        ));
    }

    #[test]
    fn refuses_malformed_files() {
        let valid = file(
            2,
            2,
            32,
            1,
            &[vec![
                layer(1, 0, 0, 0, 255),
                tags(&[(0, 0, 0, "t")]),
                image_cel(0, (0, 0), (2, 2), &[RED; 4].concat(), false),
            ]],
        );
        decode(&valid).unwrap();
        // cut anywhere
        for length in 0..valid.len() {
            assert!(decode(&valid[..length]).is_err(), "cut at {length}");
        }
        let broken = [
            // a link to a frame that doesn't exist
            file(1, 1, 32, 1, &[vec![layer(1, 0, 0, 0, 255), linked_cel(0, 1)]]),
            // a first layer inside a group that doesn't exist
            file(1, 1, 32, 1, &[vec![layer(1, 0, 1, 0, 255)]]),
            // a layer inside an image layer
            file(1, 1, 32, 1, &[vec![layer(1, 0, 0, 0, 255), layer(1, 0, 1, 0, 255)]]),
            // a cel on a missing layer
            file(1, 1, 32, 1, &[vec![image_cel(3, (0, 0), (1, 1), &RED, false)]]),
            // pixel data of the wrong size
            file(
                1,
                1,
                32,
                1,
                &[vec![layer(1, 0, 0, 0, 255), image_cel(0, (0, 0), (2, 2), &RED, false)]],
            ),
            // a tag past the last frame
            file(1, 1, 32, 1, &[vec![layer(1, 0, 0, 0, 255), tags(&[(0, 4, 0, "t")])]]),
            // a tilemap whose tileset is missing
            file(1, 1, 32, 1, &[vec![layer(1, 2, 0, 0, 255)]]),
        ];
        for (index, bytes) in broken.iter().enumerate() {
            let result = decode(bytes);
            // the tileset is only needed once a cel uses it
            if index == 6 {
                assert!(result.is_ok());
            } else {
                assert!(matches!(result, Err(Error::Decode { .. })), "case {index}: {result:?}");
            }
        }
        // groups nested past the limit
        let mut deep = vec![layer(1, 1, 0, 0, 255)];
        for level in 1..=MAX_GROUP_DEPTH as u16 + 1 {
            deep.push(layer(1, 1, level, 0, 255));
        }
        let refused = decode(&file(1, 1, 32, 1, &[deep]));
        assert!(
            matches!(&refused, Err(Error::Decode { message, .. }) if message.contains("64 deep")),
            "{refused:?}"
        );
    }

    #[test]
    fn shows_frames_of_0_ms_at_the_headers_speed() {
        let bytes = |frame_ms: [u16; 2], speed: u16| {
            let frame = |colour: Colour| vec![image_cel(0, (0, 0), (1, 1), &colour, true)];
            let mut bytes = file(
                1,
                1,
                32,
                1,
                &[[vec![layer(1, 0, 0, 0, 255)], frame(RED)].concat(), frame(BLUE)],
            );
            bytes[18..20].copy_from_slice(&speed.to_le_bytes());
            // each frame's duration is 8 bytes into its header
            let second = 128 + u32::from_le_bytes(bytes[128..132].try_into().unwrap()) as usize;
            for (at, ms) in [128, second].into_iter().zip(frame_ms) {
                bytes[at + 8..at + 10].copy_from_slice(&ms.to_le_bytes());
            }
            bytes
        };
        let durations = |bytes: Vec<u8>| -> Vec<u128> {
            let animation = decode(&bytes).unwrap();
            animation.frames().iter().map(|frame| frame.duration.as_millis()).collect()
        };
        assert_eq!(durations(bytes([30, 0], 70)), [30, 70]);
        assert_eq!(durations(bytes([0, 0], 70)), [70, 70]);
        // and the default where the header says 0 too
        assert_eq!(durations(bytes([0, 40], 0)), [100, 40]);
        assert_eq!(
            Sprite::read(&bytes([0, 40], 0), &Limits::default()).unwrap().durations()[0],
            DEFAULT_FRAME_DURATION
        );
    }

    #[test]
    fn links_keep_their_own_place() {
        // the second frame links to the first frame's red pixel, but one
        // pixel to the right and at half opacity
        let mut moved = cel_header(0, 1, 0, 128, 1);
        moved.extend(0u16.to_le_bytes());
        let bytes = file(
            2,
            1,
            32,
            1,
            &[
                vec![layer(1, 0, 0, 0, 255), image_cel(0, (0, 0), (1, 1), &RED, false)],
                vec![chunk(0x2005, &moved)],
            ],
        );
        let animation = decode(&bytes).unwrap();
        assert_eq!(pixels(&animation, 0), [RED, [0; 4]]);
        assert_eq!(pixels(&animation, 1), [[0; 4], [255, 0, 0, 128]]);
    }
}
