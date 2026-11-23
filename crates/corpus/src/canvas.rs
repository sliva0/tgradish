//! Simple RGBA canvas for drawing synthetic pixel art.

use std::collections::BTreeSet;

pub type Colour = [u8; 4];

/// Fully transparent pixel.
pub const CLEAR: Colour = [0; 4];

/// Row-major RGBA pixel buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Canvas {
    pub fn new(width: u32, height: u32) -> Canvas {
        Canvas { width, height, rgba: vec![0; (width * height * 4) as usize] }
    }

    fn index(&self, x: i32, y: i32) -> Option<usize> {
        let inside = x >= 0 && y >= 0 && (x as u32) < self.width && (y as u32) < self.height;
        inside.then(|| ((y as u32 * self.width + x as u32) * 4) as usize)
    }

    /// Returns the pixel at `(x, y)`, or `CLEAR` if out of bounds.
    pub fn get(&self, x: i32, y: i32) -> Colour {
        self.index(x, y).map_or(CLEAR, |i| self.rgba[i..i + 4].try_into().unwrap())
    }

    /// Sets the pixel at `(x, y)`. Out-of-bounds coordinates are ignored.
    pub fn set(&mut self, x: i32, y: i32, colour: Colour) {
        if let Some(i) = self.index(x, y) {
            self.rgba[i..i + 4].copy_from_slice(&colour);
        }
    }

    pub fn rect(&mut self, x: i32, y: i32, width: i32, height: i32, colour: Colour) {
        for y in y..y + height {
            for x in x..x + width {
                self.set(x, y, colour);
            }
        }
    }

    /// Fills all pixels whose centres lie inside the ellipse around `(cx, cy)`.
    pub fn ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, colour: Colour) {
        let (top, bottom) = ((cy - ry).floor() as i32, (cy + ry).ceil() as i32);
        let (left, right) = ((cx - rx).floor() as i32, (cx + rx).ceil() as i32);
        for y in top..=bottom {
            for x in left..=right {
                let (dx, dy) = ((x as f32 + 0.5 - cx) / rx, (y as f32 + 0.5 - cy) / ry);
                if dx * dx + dy * dy <= 1.0 {
                    self.set(x, y, colour);
                }
            }
        }
    }

    /// Draws a line from `from` to `to` with the given stroke `width`.
    pub fn line(&mut self, from: (i32, i32), to: (i32, i32), width: i32, colour: Colour) {
        let (dx, dy) = ((to.0 - from.0).abs(), -(to.1 - from.1).abs());
        let (sx, sy) = ((to.0 - from.0).signum(), (to.1 - from.1).signum());
        let (mut x, mut y, mut error) = (from.0, from.1, dx + dy);
        loop {
            self.rect(x - (width - 1) / 2, y - (width - 1) / 2, width, width, colour);
            if (x, y) == to {
                break;
            }
            let twice = 2 * error;
            if twice >= dy {
                error += dy;
                x += sx;
            }
            if twice <= dx {
                error += dx;
                y += sy;
            }
        }
    }

    /// Adds a 1-pixel outline around non-transparent pixels.
    pub fn outline(&mut self, colour: Colour) {
        let painted = |canvas: &Canvas, x, y| canvas.get(x, y)[3] != 0;
        let mut edge = Vec::new();
        for y in 0..self.height as i32 {
            for x in 0..self.width as i32 {
                let beside = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                    .iter()
                    .any(|&(dx, dy)| painted(self, x + dx, y + dy));
                if !painted(self, x, y) && beside {
                    edge.push((x, y));
                }
            }
        }
        for (x, y) in edge {
            self.set(x, y, colour);
        }
    }

    /// Scales the canvas up by an integer factor using nearest-neighbour
    /// blocks.
    pub fn scaled(&self, scale: u32) -> Canvas {
        let mut out = Canvas::new(self.width * scale, self.height * scale);
        for y in 0..out.height as i32 {
            for x in 0..out.width as i32 {
                out.set(x, y, self.get(x / scale as i32, y / scale as i32));
            }
        }
        out
    }

    /// Returns the `width` by `height` region starting at `(x, y)`.
    pub fn cropped(&self, x: i32, y: i32, width: u32, height: u32) -> Canvas {
        let mut out = Canvas::new(width, height);
        for dy in 0..height as i32 {
            for dx in 0..width as i32 {
                out.set(dx, dy, self.get(x + dx, y + dy));
            }
        }
        out
    }

    /// Copies non-transparent pixels from `other` onto this canvas at `(x, y)`.
    pub fn paste(&mut self, other: &Canvas, x: i32, y: i32) {
        for dy in 0..other.height as i32 {
            for dx in 0..other.width as i32 {
                let colour = other.get(dx, dy);
                if colour[3] != 0 {
                    self.set(x + dx, y + dy, colour);
                }
            }
        }
    }

    /// Returns the distinct colours in the canvas, normalising all transparent
    /// pixels to `CLEAR`.
    pub fn colours(&self) -> BTreeSet<Colour> {
        self.rgba
            .as_chunks::<4>()
            .0
            .iter()
            .map(|&pixel| if pixel[3] == 0 { CLEAR } else { pixel })
            .collect()
    }
}

/// Deterministic xorshift pseudo-random number generator.
pub struct Random(u64);

impl Random {
    pub fn new(seed: u64) -> Random {
        Random(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1)
    }

    pub fn number(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// Returns a number in `0..below`.
    pub fn below(&mut self, below: u32) -> u32 {
        (self.number() % u64::from(below.max(1))) as u32
    }
}
