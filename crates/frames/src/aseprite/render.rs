//! Drawing frames: layers bottom to top, groups composited apart when the
//! file asks for it, cels by z-index, and tilemaps.

use std::collections::HashMap;

use super::blend::{self, Colour};
use super::read::error;
use super::{Cel, Content, Depth, Layer, LayerKind, Sprite};
use crate::Result;

/// A blend mode: the backdrop, what is drawn over it, and its opacity.
type Mode = fn(Colour, Colour, u8) -> Colour;

/// A rectangle of the canvas, from its left and top edges up to its right
/// and bottom ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Bounds {
    left: usize,
    top: usize,
    right: usize,
    bottom: usize,
}

impl Bounds {
    /// The part of a `width` by `height` rectangle at `x, y` that is on
    /// this one, if any is.
    fn clip(&self, (x, y): (i64, i64), (width, height): (usize, usize)) -> Option<Bounds> {
        let side = |from: usize, to: usize, at: i64, size: usize| {
            let start = at.max(from as i64);
            let end = at.saturating_add(size as i64).min(to as i64);
            (start < end).then_some((start as usize, end as usize))
        };
        let (left, right) = side(self.left, self.right, x, width)?;
        let (top, bottom) = side(self.top, self.bottom, y, height)?;
        Some(Bounds { left, top, right, bottom })
    }

    /// The smallest rectangle holding both.
    fn union(self, other: Bounds) -> Bounds {
        Bounds {
            left: self.left.min(other.left),
            top: self.top.min(other.top),
            right: self.right.max(other.right),
            bottom: self.bottom.max(other.bottom),
        }
    }

    fn width(&self) -> usize {
        self.right - self.left
    }
}

/// A part of the canvas that is drawn on: all of it for a frame, and for a
/// group composited on its own, only what its layers cover.
pub(super) struct Surface {
    bounds: Bounds,
    pixels: Vec<Colour>,
}

impl Surface {
    fn new(bounds: Bounds) -> Surface {
        let pixels = vec![[0; 4]; bounds.width() * (bounds.bottom - bounds.top)];
        Surface { bounds, pixels }
    }

    /// One row of it, from `left` to `right` on the canvas.
    fn row(&mut self, y: usize, left: usize, right: usize) -> &mut [Colour] {
        let start = (y - self.bounds.top) * self.bounds.width();
        &mut self.pixels[start + left - self.bounds.left..start + right - self.bounds.left]
    }

    /// Draws `over`, which lies within this one, onto it.
    fn blend(&mut self, over: &Surface, mode: Mode, opacity: u8) {
        let Bounds { left, top, right, .. } = over.bounds;
        for (row, line) in over.pixels.chunks_exact(over.bounds.width()).enumerate() {
            for (back, &src) in self.row(top + row, left, right).iter_mut().zip(line) {
                *back = mode(*back, src, opacity);
            }
        }
    }
}

/// What a frame draws: for the top level and each group something is
/// drawn in, the layers in it that draw something, and for each such group
/// the part of the canvas its layers cover. Built from the frame's cels, so
/// layers and groups with nothing in the frame cost nothing.
#[derive(Debug, Default)]
pub(super) struct Plan {
    /// By parent, the top level as the number of layers.
    children: HashMap<usize, Vec<usize>>,
    bounds: HashMap<usize, Bounds>,
}

/// How a cel is drawn.
struct Paint<'a> {
    /// Cels on the background layer show the transparent index too.
    background: bool,
    opacity: u8,
    mode: Mode,
    /// The palette of the frame being drawn.
    palette: &'a [Colour],
}

impl Sprite {
    /// The whole canvas.
    pub(super) fn canvas(&self) -> Bounds {
        Bounds { left: 0, top: 0, right: self.width as usize, bottom: self.height as usize }
    }

    // A cel's z-index moves it among the layers as Aseprite's RenderPlan
    // (src/doc/render_plan.cpp) orders them, by where its layer is plus its
    // z-index, the lower z-index first on ties. Where groups are drawn into
    // the layers under them, it orders every layer of the sprite at once,
    // counted bottom first as the file lists them, groups and hidden layers
    // too, so a cel can move into or out of a group; where they are
    // composited on their own, each group's own layers by where they are
    // in it.

    /// A frame, with the layers `shown` says, which are hidden with their
    /// groups.
    pub(super) fn render(&self, frame: usize, shown: &[bool]) -> Result<Vec<Colour>> {
        let mut canvas = Surface::new(self.canvas());
        if self.composite_groups {
            let plan = self.plan(frame, shown);
            self.draw_children(self.layers.len(), frame, &plan, &mut canvas)?;
        } else {
            let mut order: Vec<(i64, i64, usize)> = self.frames[frame]
                .1
                .iter()
                .filter(|cel| shown[cel.layer] && self.layers[cel.layer].kind != LayerKind::Group)
                .filter(|cel| self.cel_bounds(frame, cel.layer).is_some())
                .map(|cel| (cel.layer as i64 + cel.z, cel.z, cel.layer))
                .collect();
            order.sort_unstable();
            for (_, _, index) in order {
                self.draw_layer(index, frame, &mut canvas)?;
            }
        }
        Ok(canvas.pixels)
    }

    /// The part of the canvas a layer's cel in a frame covers, if it has
    /// one and any of it is on the canvas.
    fn cel_bounds(&self, frame: usize, index: usize) -> Option<Bounds> {
        let (cel, content) = self.cel(frame, index)?;
        let size = match content {
            Content::Image { width, height, .. } => (*width, *height),
            Content::Tilemap { width, tiles, .. } => {
                let LayerKind::Tilemap { tileset } = self.layers[index].kind else { return None };
                // checked when reading: the tileset exists
                let set = self.tilesets.iter().find(|t| t.id == tileset).unwrap();
                let rows = tiles.len().checked_div(*width).unwrap_or(0);
                (width.saturating_mul(set.width), rows.saturating_mul(set.height))
            }
            Content::Linked(_) => return None,
        };
        self.canvas().clip((cel.x, cel.y), size)
    }

    /// What a frame draws of the layers `shown` says, groups composited
    /// on their own.
    fn plan(&self, frame: usize, shown: &[bool]) -> Plan {
        let mut plan = Plan::default();
        for stored in &self.frames[frame].1 {
            let index = stored.layer;
            if !shown[index] || self.layers[index].kind == LayerKind::Group {
                continue;
            }
            let Some(bounds) = self.cel_bounds(frame, index) else { continue };
            // the layer, then each group it is in, joins its parent's list
            // the first time it is met
            let (mut child, mut new) = (index, true);
            loop {
                let parent = self.layers[child].parent;
                if new {
                    plan.children
                        .entry(parent.unwrap_or(self.layers.len()))
                        .or_default()
                        .push(child);
                }
                let Some(group) = parent else { break };
                new = !plan.bounds.contains_key(&group);
                plan.bounds.entry(group).and_modify(|b| *b = b.union(bounds)).or_insert(bounds);
                child = group;
            }
        }
        plan
    }

    /// Draws the layers of a group (or of the top level) that `plan` says
    /// draw something, in order, each composited on its own.
    fn draw_children(
        &self,
        parent: usize,
        frame: usize,
        plan: &Plan,
        canvas: &mut Surface,
    ) -> Result<()> {
        let Some(children) = plan.children.get(&parent) else { return Ok(()) };
        let mut order: Vec<(i64, i64, usize)> = children
            .iter()
            .map(|&index| {
                // groups have no cels of their own
                let z = match self.layers[index].kind {
                    LayerKind::Group => 0,
                    _ => self.stored_cel(frame, index).map_or(0, |cel| cel.z),
                };
                (self.positions[index] as i64 + z, z, index)
            })
            .collect();
        order.sort_unstable();
        for (_, _, index) in order {
            let layer = &self.layers[index];
            if layer.kind != LayerKind::Group {
                self.draw_layer(index, frame, canvas)?;
                continue;
            }
            let (mode, opacity) = self.paint_of(layer)?;
            // only over what its layers cover: a canvas of the frame's size
            // for every group let a small file of many groups on a large
            // canvas take hours
            let mut group = Surface::new(plan.bounds[&index]);
            self.draw_children(index, frame, plan, &mut group)?;
            canvas.blend(&group, mode, opacity);
        }
        Ok(())
    }

    /// A layer's blend mode, and its opacity where the file says to use it.
    fn paint_of(&self, layer: &Layer) -> Result<(Mode, u8)> {
        let opacity = if self.layer_opacity { layer.opacity } else { 255 };
        let mode = blend::by_number(layer.blend)
            .ok_or_else(|| error(format!("unknown blend mode {}", layer.blend)))?;
        Ok((mode, opacity))
    }

    /// Draws the cel of a layer that isn't a group, which it has in `frame`.
    fn draw_layer(&self, index: usize, frame: usize, canvas: &mut Surface) -> Result<()> {
        let layer = &self.layers[index];
        let (mode, opacity) = self.paint_of(layer)?;
        let (cel, content) = self.cel(frame, index).expect("only layers with cels are drawn");
        let paint = Paint {
            background: layer.background,
            opacity: blend::mul_un8(cel.opacity.into(), opacity.into()) as u8,
            mode,
            palette: &self.palettes[self.frame_palettes[frame]],
        };
        self.draw_cel(cel, content, layer, &paint, canvas)
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
        canvas: &mut Surface,
    ) {
        let Some(Bounds { left, top, right, bottom }) = canvas.bounds.clip((x, y), (width, height))
        else {
            return;
        };
        let bytes = self.depth.bytes();
        for cy in top..bottom {
            // where the row starts in the image, and its first pixel drawn
            let row = (cy as i64 - y) as usize * width;
            let column = (left as i64 - x) as usize;
            let line = &pixels[(row + column) * bytes..(row + column + right - left) * bytes];
            for (back, pixel) in
                canvas.row(cy, left, right).iter_mut().zip(line.chunks_exact(bytes))
            {
                *back = (paint.mode)(*back, self.colour(pixel, paint), paint.opacity);
            }
        }
    }

    fn draw_cel(
        &self,
        cel: &Cel,
        content: &Content,
        layer: &Layer,
        paint: &Paint,
        canvas: &mut Surface,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aseprite::testfile::*;
    use std::time::Duration;

    use crate::Error;

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
    fn composites_groups_over_what_they_cover() {
        // a red background; over it a group at half opacity of a green
        // pixel and a group, inside it, of a blue pair at its right
        let bytes = |flags| {
            file(
                4,
                2,
                32,
                flags,
                &[vec![
                    layer(1 | 8, 0, 0, 0, 255),
                    layer(1, 1, 0, 0, 128),
                    layer(1, 0, 1, 0, 255),
                    layer(1, 1, 1, 0, 255),
                    layer(1, 0, 2, 0, 255),
                    // an empty group, and one with only hidden layers
                    layer(1, 1, 0, 0, 255),
                    layer(1, 1, 0, 0, 255),
                    layer(0, 0, 1, 0, 255),
                    image_cel(0, (0, 0), (4, 2), &[RED; 8].concat(), false),
                    image_cel(2, (0, 0), (1, 1), &GREEN, false),
                    image_cel(4, (2, 1), (2, 1), &[BLUE; 2].concat(), false),
                    image_cel(7, (0, 0), (4, 2), &[GREEN; 8].concat(), false),
                ]],
            )
        };
        let half_green = [127, 128, 0, 255];
        let half_blue = [127, 0, 128, 255];
        assert_eq!(
            pixels(&decode(&bytes(1 | 2)).unwrap(), 0),
            [half_green, RED, RED, RED, RED, RED, half_blue, half_blue]
        );
        // not composited, the group's opacity is ignored
        assert_eq!(
            pixels(&decode(&bytes(1)).unwrap(), 0),
            [GREEN, RED, RED, RED, RED, RED, BLUE, BLUE]
        );
    }

    #[test]
    fn draws_many_groups_on_a_large_canvas_quickly() {
        // 30 000 groups on a 1024x1024 canvas, most empty and some of a
        // pixel each: a canvas each would be 120 billion blends a frame
        let mut chunks = vec![layer(1, 0, 0, 0, 255)];
        let mut cels = Vec::new();
        for group in 0..30_000u16 {
            chunks.push(layer(1, 1, 0, 0, 200));
            if group % 100 == 0 {
                chunks.push(layer(1, 0, 1, 0, 255));
                let at = (group / 100) as i16;
                cels.push(image_cel(chunks.len() as u16 - 1, (at, at), (1, 1), &BLUE, true));
            }
        }
        chunks.push(image_cel(0, (0, 0), (1, 1), &RED, true));
        chunks.extend(cels);
        let bytes = file(1024, 1024, 32, 1 | 2, &[chunks]);
        let started = std::time::Instant::now();
        let animation = decode(&bytes).unwrap();
        assert!(started.elapsed() < Duration::from_secs(5), "{:?}", started.elapsed());
        // the first group's blue at 200 over red
        assert_eq!(animation.pixel(0, 0, 0), Some([55, 0, 200, 255]));
        assert_eq!(animation.pixel(0, 299, 299), Some([0, 0, 255, 200]));
        assert_eq!(animation.pixel(0, 300, 300), Some([0; 4]));
    }

    /// A 1x1 image cel of `colour` with a z-index.
    fn z_cel(layer: u16, z: i16, colour: Colour) -> Vec<u8> {
        let mut data = cel_header(layer, 0, 0, 255, 0);
        data[9..11].copy_from_slice(&z.to_le_bytes());
        data.extend(1u16.to_le_bytes());
        data.extend(1u16.to_le_bytes());
        data.extend(colour);
        chunk(0x2005, &data)
    }

    #[test]
    fn orders_cels_by_z_index_as_aseprite_does() {
        let top = |flags, chunks: Vec<Vec<u8>>| {
            let animation = decode(&file(1, 1, 32, flags, &[chunks])).unwrap();
            animation.pixel(0, 0, 0).unwrap()
        };
        // bottom first: red, a group of green, and blue, where green's cel
        // moves 2 up from 2, to 4, over blue at 3
        let into = |flags| {
            top(
                flags,
                vec![
                    layer(1, 0, 0, 0, 255),
                    layer(1, 1, 0, 0, 255),
                    layer(1, 0, 1, 0, 255),
                    layer(1, 0, 0, 0, 255),
                    z_cel(0, 0, RED),
                    z_cel(2, 2, GREEN),
                    z_cel(3, 0, BLUE),
                ],
            )
        };
        // which takes it out of its group, unless groups are composited on
        // their own, where it moves among the group's layers alone
        assert_eq!(into(1), GREEN);
        assert_eq!(into(1 | 2), BLUE);

        // red, a group of green under a hidden layer, then blue moved 1
        // down: composited on their own, it goes from 2 to 1, the group's
        // place, and under it, being the lower z-index; otherwise from 4 to
        // 3, the hidden layer's, and over green at 2
        let out_of = |flags| {
            top(
                flags,
                vec![
                    layer(1, 0, 0, 0, 255),
                    layer(1, 1, 0, 0, 255),
                    layer(1, 0, 1, 0, 255),
                    layer(0, 0, 1, 0, 255),
                    layer(1, 0, 0, 0, 255),
                    z_cel(0, 0, RED),
                    z_cel(2, 0, GREEN),
                    z_cel(3, 0, RED),
                    z_cel(4, -1, BLUE),
                ],
            )
        };
        assert_eq!(out_of(1 | 2), GREEN);
        assert_eq!(out_of(1), BLUE);

        // on a tie the lower z-index goes under, whichever layer is lower:
        // red at 0 moved up to 1 and green at 1 make 1 each, red on top
        let tie = |z| {
            top(
                1,
                vec![
                    layer(1, 0, 0, 0, 255),
                    layer(1, 0, 0, 0, 255),
                    z_cel(0, z, RED),
                    z_cel(1, 0, GREEN),
                ],
            )
        };
        assert_eq!((tie(0), tie(1), tie(-5)), (GREEN, RED, GREEN));
    }

    #[test]
    fn draws_tiles_and_refuses_flips() {
        let sprite = |tiles: &[u32]| {
            file(3, 1, 32, 1, &[vec![layer(1, 2, 0, 0, 255), tileset(), tilemap(32, tiles)]])
        };
        assert_eq!(pixels(&decode(&sprite(&[2, 0, 1])).unwrap(), 0), [GREEN, [0; 4], RED]);
        assert!(matches!(decode(&sprite(&[2 | 0x2000_0000, 0, 1])), Err(Error::Decode { .. })));
        assert!(matches!(decode(&sprite(&[9, 0, 1])), Err(Error::Decode { .. })));
    }
}
