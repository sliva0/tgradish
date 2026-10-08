//! Aseprite files (<https://github.com/aseprite/aseprite/blob/main/docs/ase-file-specs.md>),
//! parsed and rendered here. Files can come from anywhere, so every read
//! is checked and malformed files are errors, never panics.
//!
//! Rendering follows Aseprite's: layers bottom to top, hidden and
//! reference layers skipped, layer and cel opacity, every blend mode,
//! groups composited separately when the file asks for it, linked cels,
//! per-cel z-index, and tilemaps. Flipped tiles are refused rather than
//! drawn wrong.

mod blend;

use std::io::Read;
use std::time::Duration;

use blend::Colour;

use crate::{Animation, DEFAULT_FRAME_DURATION, Error, Format, Frame, Limits, Result};

/// Groups nested deeper than this are refused; Aseprite's interface makes
/// nothing close, and rendering recurses once per level.
const MAX_GROUP_DEPTH: usize = 64;

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

fn error(message: impl ToString) -> Error {
    Error::Decode { format: Format::Aseprite, message: message.to_string() }
}

fn truncated() -> Error {
    error("the file ends too early")
}

/// Little-endian reads that fail instead of running past the end.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Reader<'a> {
        Reader { bytes, at: 0 }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        let end = self.at.checked_add(count).filter(|&end| end <= self.bytes.len());
        let end = end.ok_or_else(truncated)?;
        let out = &self.bytes[self.at..end];
        self.at = end;
        Ok(out)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        Ok(self.take(N)?.try_into().unwrap())
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.array::<1>()?[0])
    }

    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    fn i16(&mut self) -> Result<i16> {
        Ok(i16::from_le_bytes(self.array()?))
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    fn skip(&mut self, count: usize) -> Result<()> {
        self.take(count).map(drop)
    }

    fn string(&mut self) -> Result<String> {
        let length = self.u16()?;
        Ok(String::from_utf8_lossy(self.take(length.into())?).into_owned())
    }

    fn rest(&mut self) -> &'a [u8] {
        let out = &self.bytes[self.at..];
        self.at = self.bytes.len();
        out
    }
}

/// Inflates zlib data that must come out exactly `size` bytes long.
fn inflate(data: &[u8], size: usize) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(data)
        .take(size as u64 + 1)
        .read_to_end(&mut out)
        .map_err(|err| error(format!("compressed data is broken: {err}")))?;
    if out.len() != size {
        return Err(error(format!(
            "compressed data has {} bytes, {size} were expected",
            out.len()
        )));
    }
    Ok(out)
}

fn too_large(limits: &Limits) -> Error {
    Error::TooLarge(format!("decoding needs more than {} bytes", limits.max_bytes))
}

/// Bytes for the product of `factors`, within the limits.
fn budget(factors: [usize; 3], limits: &Limits) -> Result<usize> {
    factors
        .iter()
        .try_fold(1usize, |total, &n| total.checked_mul(n))
        .filter(|&bytes| bytes <= limits.max_bytes)
        .ok_or_else(|| too_large(limits))
}

/// Adds `bytes` to what decoding has used so far, within the limits.
fn spend(spent: &mut usize, bytes: usize, limits: &Limits) -> Result<usize> {
    *spent = spent
        .checked_add(bytes)
        .filter(|&total| total <= limits.max_bytes)
        .ok_or_else(|| too_large(limits))?;
    Ok(bytes)
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
    shown: bool,
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

/// A parsed Aseprite file.
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
    /// Deepest nesting of groups.
    depth_of_groups: usize,
    /// Each frame's duration and cels, sorted by layer.
    frames: Vec<(Duration, Vec<Cel>)>,
    tags: Vec<Tag>,
    tilesets: Vec<Tileset>,
    limits: Limits,
}

impl Sprite {
    pub fn read(bytes: &[u8]) -> Result<Sprite> {
        Sprite::read_with(bytes, &Limits::default())
    }

    pub fn read_with(bytes: &[u8], limits: &Limits) -> Result<Sprite> {
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
        let speed = header.u16()?;
        header.skip(8)?;
        let transparent = header.u8()?;
        if width == 0 || height == 0 {
            return Err(error(format!("the canvas is {width}x{height}")));
        }
        if width.max(height) > limits.max_dimension {
            return Err(Error::TooLarge(format!("the canvas is {width}x{height}")));
        }

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
            depth_of_groups: 0,
            frames: Vec::with_capacity(frame_count.into()),
            tags: Vec::new(),
            tilesets: Vec::new(),
            limits: *limits,
        };
        let mut old_palette: Option<Vec<Colour>> = None;
        let mut new_palette = false;
        let mut palette: Vec<Colour> = Vec::new();
        // bytes of pixels decoded so far
        let mut spent = 0;
        for _ in 0..frame_count {
            let length = file.u32()? as usize;
            if file.u16()? != 0xf1fa {
                return Err(error("a frame header is broken"));
            }
            let old_count = file.u16()?;
            let duration = file.u16()?;
            file.skip(2)?;
            let new_count = file.u32()?;
            let body_length =
                length.checked_sub(16).ok_or_else(|| error("a frame is too short"))?;
            let mut body = Reader::new(file.take(body_length)?);
            let chunks = if new_count != 0 { new_count } else { old_count.into() };
            let mut cels = Vec::new();
            let mut changed = sprite.palettes.is_empty();
            for _ in 0..chunks {
                let size = body.u32()? as usize;
                let kind = body.u16()?;
                let size = size.checked_sub(6).ok_or_else(|| error("a chunk is too short"))?;
                let mut chunk = Reader::new(body.take(size)?);
                match kind {
                    0x0004 | 0x0011 if !new_palette => {
                        old_palette = Some(read_old_palette(&mut chunk, kind == 0x0011)?);
                    }
                    0x2004 => sprite.layers.push(read_layer(&mut chunk)?),
                    0x2005 => cels.push(sprite.read_cel(&mut chunk, &mut spent)?),
                    0x2018 => sprite.tags = read_tags(&mut chunk)?,
                    0x2019 => {
                        new_palette = true;
                        read_palette(&mut chunk, &mut palette)?;
                        changed = true;
                    }
                    0x2023 => {
                        let tileset = sprite.read_tileset(&mut chunk, &mut spent)?;
                        sprite.tilesets.push(tileset);
                    }
                    _ => {}
                }
            }
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
                spend(&mut spent, palette.len() * 4, limits)?;
                sprite.palettes.push(palette.clone());
            }
            sprite.frame_palettes.push(sprite.palettes.len() - 1);
        }
        // old files have only old palette chunks, which apply throughout
        if !new_palette && let Some(old) = old_palette {
            sprite.palettes = vec![old];
            sprite.frame_palettes.fill(0);
        }
        if sprite.frames.is_empty() {
            return Err(Error::Empty);
        }
        sprite.link_layers()?;
        sprite.check_cels()?;
        let frames = sprite.frames.len() as u32;
        if let Some(tag) = sprite.tags.iter().find(|tag| tag.from > tag.to || tag.to >= frames) {
            return Err(error(format!("tag {:?} has frames outside the file", tag.name)));
        }
        Ok(sprite)
    }

    fn read_cel(&self, chunk: &mut Reader, spent: &mut usize) -> Result<Cel> {
        let layer = chunk.u16()?.into();
        let (x, y) = (chunk.i16()?.into(), chunk.i16()?.into());
        let opacity = chunk.u8()?;
        let kind = chunk.u16()?;
        let z = chunk.i16()?.into();
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
                let size = budget([width, height, tile_bytes], &self.limits)?;
                let data = inflate(chunk.rest(), spend(spent, size, &self.limits)?)?;
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

    fn read_tileset(&self, chunk: &mut Reader, spent: &mut usize) -> Result<Tileset> {
        let id = chunk.u32()?;
        let flags = chunk.u32()?;
        let count = chunk.u32()? as usize;
        let (width, height) = (chunk.u16()? as usize, chunk.u16()? as usize);
        chunk.skip(2 + 14)?;
        chunk.string()?;
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

    /// Finds each layer's parent group from the nesting levels.
    fn link_layers(&mut self) -> Result<()> {
        // the last layer seen at each level
        let mut open: Vec<usize> = Vec::new();
        for index in 0..self.layers.len() {
            let level = usize::from(self.layers[index].level);
            if level > open.len() || level > MAX_GROUP_DEPTH {
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
        for (index, layer) in self.layers.iter().enumerate() {
            self.children[layer.parent.unwrap_or(self.layers.len())].push(index);
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

    pub fn tags(&self) -> &[Tag] {
        &self.tags
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// The frames of `tag`, or all frames. Hidden layers stay hidden.
    pub fn animation(&self, tag: Option<&str>) -> Result<Animation> {
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
        // the frames, and while drawing one, a canvas for each group level
        let frame_size = budget([self.width as usize, self.height as usize, 4], &self.limits)?;
        let canvases = order.len().saturating_add(self.depth_of_groups + 1);
        budget([frame_size, canvases, 1], &self.limits)?;
        let frames = order
            .into_iter()
            .map(|index| {
                let (duration, _) = self.frames[index as usize];
                let canvas = self.render(index as usize)?;
                Ok(Frame { rgba: canvas.into_iter().flatten().collect(), duration })
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

    fn render(&self, frame: usize) -> Result<Vec<Colour>> {
        let mut canvas = vec![[0; 4]; self.width as usize * self.height as usize];
        self.draw_children(self.layers.len(), frame, &mut canvas)?;
        Ok(canvas)
    }

    /// Draws the children of a group (or of the top level) in order.
    fn draw_children(&self, parent: usize, frame: usize, canvas: &mut [Colour]) -> Result<()> {
        // a cel's z-index moves it among the layers, as Aseprite orders them
        let mut order: Vec<(i64, i64, usize)> = self.children[parent]
            .iter()
            .map(|&index| {
                let z = self.stored_cel(frame, index).map_or(0, |cel| cel.z);
                (index as i64 + z, z, index)
            })
            .collect();
        order.sort_unstable();
        for (_, _, index) in order {
            let layer = &self.layers[index];
            if !layer.shown {
                continue;
            }
            let opacity = if self.layer_opacity { layer.opacity } else { 255 };
            let mode = blend::by_number(layer.blend)
                .ok_or_else(|| error(format!("unknown blend mode {}", layer.blend)))?;
            match layer.kind {
                LayerKind::Group if self.composite_groups => {
                    let mut group = vec![[0; 4]; canvas.len()];
                    self.draw_children(index, frame, &mut group)?;
                    for (back, src) in canvas.iter_mut().zip(group) {
                        *back = mode(*back, src, opacity);
                    }
                }
                LayerKind::Group => self.draw_children(index, frame, canvas)?,
                LayerKind::Image | LayerKind::Tilemap { .. } => {
                    if let Some((cel, content)) = self.cel(frame, index) {
                        let paint = Paint {
                            background: layer.background,
                            opacity: blend::mul_un8(cel.opacity.into(), opacity.into()) as u8,
                            mode,
                            palette: &self.palettes[self.frame_palettes[frame]],
                        };
                        self.draw_cel(cel, content, layer, &paint, canvas)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn colour(&self, pixel: &[u8], paint: &Paint) -> Colour {
        match self.depth {
            Depth::Rgba => [pixel[0], pixel[1], pixel[2], pixel[3]],
            Depth::Grayscale => [pixel[0], pixel[0], pixel[0], pixel[1]],
            Depth::Indexed if !paint.background && pixel[0] == self.transparent => [0; 4],
            Depth::Indexed => paint.palette.get(usize::from(pixel[0])).copied().unwrap_or([0; 4]),
        }
    }

    /// Draws an image with its top left at `x, y`, clipped to the canvas.
    fn draw_image(
        &self,
        pixels: &[u8],
        (width, height): (usize, usize),
        (x, y): (i64, i64),
        paint: &Paint,
        canvas: &mut [Colour],
    ) {
        let bytes = self.depth.bytes();
        let (canvas_width, canvas_height) = (i64::from(self.width), i64::from(self.height));
        for row in 0..height {
            let cy = y + row as i64;
            if !(0..canvas_height).contains(&cy) {
                continue;
            }
            for column in 0..width {
                let cx = x + column as i64;
                if !(0..canvas_width).contains(&cx) {
                    continue;
                }
                let start = (row * width + column) * bytes;
                let src = self.colour(&pixels[start..start + bytes], paint);
                let back = &mut canvas[(cy * canvas_width + cx) as usize];
                *back = (paint.mode)(*back, src, paint.opacity);
            }
        }
    }

    fn draw_cel(
        &self,
        cel: &Cel,
        content: &Content,
        layer: &Layer,
        paint: &Paint,
        canvas: &mut [Colour],
    ) -> Result<()> {
        match content {
            Content::Image { width, height, pixels } => {
                self.draw_image(pixels, (*width, *height), (cel.x, cel.y), paint, canvas);
            }
            Content::Tilemap { width, tiles, id_mask, flip_mask } => {
                let LayerKind::Tilemap { tileset } = layer.kind else { return Ok(()) };
                // checked when reading: the tileset exists
                let set = self.tilesets.iter().find(|t| t.id == tileset).unwrap();
                let tile_bytes = set.width * set.height * self.depth.bytes();
                for (index, &tile) in tiles.iter().enumerate() {
                    if tile & flip_mask != 0 {
                        return Err(error("flipped tiles aren't supported"));
                    }
                    let id = tile & id_mask;
                    let empty = if set.zero_is_empty { id == 0 } else { id == *id_mask };
                    if empty {
                        continue;
                    }
                    let id = id as usize;
                    if id >= set.count {
                        return Err(error(format!("tile {id} isn't in its tileset")));
                    }
                    let (column, row) = ((index % width) as i64, (index / width) as i64);
                    let at = (cel.x + column * set.width as i64, cel.y + row * set.height as i64);
                    let pixels = &set.pixels[id * tile_bytes..(id + 1) * tile_bytes];
                    self.draw_image(pixels, (set.width, set.height), at, paint, canvas);
                }
            }
            Content::Linked(_) => {}
        }
        Ok(())
    }
}

/// How a cel is drawn.
struct Paint<'a> {
    /// Cels on the background layer show the transparent index too.
    background: bool,
    opacity: u8,
    mode: fn(Colour, Colour, u8) -> Colour,
    /// The palette of the frame being drawn.
    palette: &'a [Colour],
}

fn read_layer(chunk: &mut Reader) -> Result<Layer> {
    let flags = chunk.u16()?;
    let kind = chunk.u16()?;
    let level = chunk.u16()?;
    chunk.skip(4)?;
    let blend = chunk.u16()?;
    let opacity = chunk.u8()?;
    chunk.skip(3)?;
    chunk.string()?;
    let kind = match kind {
        0 => LayerKind::Image,
        1 => LayerKind::Group,
        2 => LayerKind::Tilemap { tileset: chunk.u32()? },
        other => return Err(error(format!("unknown layer type {other}"))),
    };
    Ok(Layer {
        // visible, and not a reference layer, which exports leave out
        shown: flags & 1 != 0 && flags & 64 == 0,
        background: flags & 8 != 0,
        kind,
        level,
        blend,
        opacity,
        parent: None,
    })
}

fn read_tags(chunk: &mut Reader) -> Result<Vec<Tag>> {
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
            chunk.skip(2 + 6 + 3 + 1)?;
            Ok(Tag { name: chunk.string()?, from, to, direction })
        })
        .collect()
}

/// The palette chunk changes entries `from..=to` and sets the size.
fn read_palette(chunk: &mut Reader, palette: &mut Vec<Colour>) -> Result<()> {
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

/// The palettes of old files: packets of opaque colours, with 6-bit
/// channels in chunk 0x0011.
fn read_old_palette(chunk: &mut Reader, six_bits: bool) -> Result<Vec<Colour>> {
    let mut palette = vec![[0, 0, 0, 255]; 256];
    let mut index = 0usize;
    for _ in 0..chunk.u16()? {
        index += usize::from(chunk.u8()?);
        let count = match chunk.u8()? {
            0 => 256,
            n => usize::from(n),
        };
        for _ in 0..count {
            let [r, g, b] = chunk.array::<3>()?;
            let scale = |c: u8| if six_bits { (u16::from(c.min(63)) * 255 / 63) as u8 } else { c };
            if let Some(entry) = palette.get_mut(index) {
                *entry = [scale(r), scale(g), scale(b), 255];
            }
            index += 1;
        }
    }
    Ok(palette)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn chunk(kind: u16, data: &[u8]) -> Vec<u8> {
        let mut out = ((data.len() + 6) as u32).to_le_bytes().to_vec();
        out.extend(kind.to_le_bytes());
        out.extend(data);
        out
    }

    fn string(text: &str) -> Vec<u8> {
        let mut out = (text.len() as u16).to_le_bytes().to_vec();
        out.extend(text.as_bytes());
        out
    }

    /// A layer chunk: flags (1 visible, 8 background, 64 reference), type
    /// (0 image, 1 group, 2 tilemap), level, blend mode, opacity.
    fn layer(flags: u16, kind: u16, level: u16, blend: u16, opacity: u8) -> Vec<u8> {
        let mut data = Vec::new();
        for word in [flags, kind, level, 0, 0, blend] {
            data.extend(word.to_le_bytes());
        }
        data.extend([opacity, 0, 0, 0]);
        data.extend(string("layer"));
        if kind == 2 {
            data.extend(7u32.to_le_bytes());
        }
        chunk(0x2004, &data)
    }

    fn cel_header(layer: u16, x: i16, y: i16, opacity: u8, kind: u16) -> Vec<u8> {
        let mut data = layer.to_le_bytes().to_vec();
        data.extend(x.to_le_bytes());
        data.extend(y.to_le_bytes());
        data.push(opacity);
        data.extend(kind.to_le_bytes());
        data.extend([0; 7]);
        data
    }

    fn zlib(bytes: &[u8]) -> Vec<u8> {
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }

    /// An image cel; compressed unless `raw`.
    fn image_cel(
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

    fn linked_cel(layer: u16, frame: u16) -> Vec<u8> {
        let mut data = cel_header(layer, 0, 0, 255, 1);
        data.extend(frame.to_le_bytes());
        chunk(0x2005, &data)
    }

    fn tags(list: &[(u16, u16, u8, &str)]) -> Vec<u8> {
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

    fn palette(colours: &[Colour]) -> Vec<u8> {
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
    fn file(width: u16, height: u16, depth: u16, flags: u32, frames: &[Vec<Vec<u8>>]) -> Vec<u8> {
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

    const RED: Colour = [255, 0, 0, 255];
    const GREEN: Colour = [0, 255, 0, 255];
    const BLUE: Colour = [0, 0, 255, 255];

    fn pixels(animation: &Animation, frame: usize) -> Vec<Colour> {
        (0..animation.height())
            .flat_map(|y| (0..animation.width()).map(move |x| (x, y)))
            .map(|(x, y)| animation.pixel(frame, x, y).unwrap())
            .collect()
    }

    fn decode(bytes: &[u8]) -> Result<Animation> {
        Sprite::read(bytes)?.animation(None)
    }

    #[test]
    fn plays_tags_in_order() {
        let tag = |from, to, direction| Tag { name: String::new(), from, to, direction }.frames();
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
        let sprite = Sprite::read(&bytes).unwrap();
        let first = |tag| -> Vec<Colour> {
            let animation = sprite.animation(tag).unwrap();
            (0..animation.frames().len()).map(|i| animation.pixel(i, 0, 0).unwrap()).collect()
        };
        assert_eq!(first(None), [RED, GREEN, RED]);
        // ping-pong reverse over 0..=2: 2, 1, 0, 1
        assert_eq!(first(Some("back")), [RED, GREEN, RED, GREEN]);
        assert_eq!(first(Some("on")), [GREEN, RED]);
        assert_eq!(sprite.animation(None).unwrap().duration(), Duration::from_millis(150));
        assert!(matches!(
            sprite.animation(Some("nope")),
            Err(Error::NoSuchTag { available, .. }) if available == ["back", "on"]
        ));
    }

    #[test]
    fn renders_layers() {
        // 3x1: a red background, a half-opaque blue layer over its middle,
        // a hidden green layer and a green reference layer
        let bytes = file(
            3,
            1,
            32,
            1,
            &[vec![
                layer(1 | 8, 0, 0, 0, 255),
                layer(1, 0, 0, 0, 128),
                layer(0, 0, 0, 0, 255),
                layer(1 | 64, 0, 0, 0, 255),
                image_cel(0, (0, 0), (3, 1), &[RED, RED, RED].concat(), false),
                image_cel(1, (1, 0), (1, 1), &BLUE, false),
                image_cel(2, (0, 0), (3, 1), &[GREEN; 3].concat(), false),
                image_cel(3, (0, 0), (3, 1), &[GREEN; 3].concat(), false),
            ]],
        );
        let animation = decode(&bytes).unwrap();
        assert_eq!(pixels(&animation, 0), [RED, [127, 0, 128, 255], RED]);

        // a group of two layers at half opacity, composited on its own
        // when the header says so
        let group = |flags| {
            file(
                1,
                1,
                32,
                flags,
                &[vec![
                    layer(1, 0, 0, 0, 255),
                    layer(1, 1, 0, 0, 128),
                    layer(1, 0, 1, 0, 255),
                    image_cel(0, (0, 0), (1, 1), &RED, false),
                    image_cel(2, (0, 0), (1, 1), &BLUE, false),
                ]],
            )
        };
        assert_eq!(pixels(&decode(&group(1 | 2)).unwrap(), 0), [[127, 0, 128, 255]]);
        // otherwise the group's opacity is ignored
        assert_eq!(pixels(&decode(&group(1)).unwrap(), 0), [BLUE]);
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
        assert!(decode(&file(1, 1, 32, 1, &[deep])).is_err());
    }

    #[test]
    fn draws_tiles_and_refuses_flips() {
        // a tileset of two 1x1 tiles (red, green), new format: tile 0 is
        // empty, so ids 1 and 2
        let tileset = {
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
        };
        let tilemap = |tiles: &[u32]| {
            let mut data = cel_header(0, 0, 0, 255, 3);
            data.extend((tiles.len() as u16).to_le_bytes());
            data.extend(1u16.to_le_bytes());
            data.extend(32u16.to_le_bytes());
            for mask in [0x1fff_ffffu32, 0x2000_0000, 0x4000_0000, 0x8000_0000] {
                data.extend(mask.to_le_bytes());
            }
            data.extend([0; 10]);
            data.extend(zlib(&tiles.iter().flat_map(|t| t.to_le_bytes()).collect::<Vec<_>>()));
            chunk(0x2005, &data)
        };
        let sprite = |tiles: &[u32]| {
            file(3, 1, 32, 1, &[vec![layer(1, 2, 0, 0, 255), tileset.clone(), tilemap(tiles)]])
        };
        assert_eq!(pixels(&decode(&sprite(&[2, 0, 1])).unwrap(), 0), [GREEN, [0; 4], RED]);
        assert!(matches!(decode(&sprite(&[2 | 0x2000_0000, 0, 1])), Err(Error::Decode { .. })));
        assert!(matches!(decode(&sprite(&[9, 0, 1])), Err(Error::Decode { .. })));
    }

    #[test]
    fn keeps_within_limits() {
        let big = file(5000, 5000, 32, 1, &[vec![layer(1, 0, 0, 0, 255)]]);
        let small = Limits { max_dimension: 4096, max_bytes: 1 << 20 };
        assert!(matches!(Sprite::read_with(&big, &small), Err(Error::TooLarge(_))));
        let roomy = Limits { max_dimension: 8192, max_bytes: 1 << 20 };
        // the canvas is fine, 100 MB of frames aren't
        let sprite = Sprite::read_with(&big, &roomy).unwrap();
        assert!(matches!(sprite.animation(None), Err(Error::TooLarge(_))));
        // nor is a cel that claims to be huge
        let cel = file(
            1,
            1,
            32,
            1,
            &[vec![layer(1, 0, 0, 0, 255), image_cel(0, (0, 0), (60000, 60000), &RED, false)]],
        );
        assert!(matches!(Sprite::read_with(&cel, &roomy), Err(Error::TooLarge(_))));
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
