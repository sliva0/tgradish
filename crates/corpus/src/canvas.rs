//! A canvas to draw pixel art on.

use std::collections::BTreeSet;

pub type Colour = [u8; 4];

/// No paint.
pub const CLEAR: Colour = [0; 4];

/// RGBA pixels, row by row.
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

    /// The pixel at `x`, `y`, clear outside the canvas.
    pub fn get(&self, x: i32, y: i32) -> Colour {
        self.index(x, y).map_or(CLEAR, |i| self.rgba[i..i + 4].try_into().unwrap())
    }

    /// Paints a pixel; outside the canvas, nothing.
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

    /// The pixels whose middles are inside the ellipse around `cx`, `cy`.
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

    /// A line from one pixel to another, `width` pixels thick.
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

    /// Outlines what is painted: clear pixels beside painted ones become
    /// `colour`.
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

    /// Each pixel as a block of `scale` by `scale`.
    pub fn scaled(&self, scale: u32) -> Canvas {
        let mut out = Canvas::new(self.width * scale, self.height * scale);
        for y in 0..out.height as i32 {
            for x in 0..out.width as i32 {
                out.set(x, y, self.get(x / scale as i32, y / scale as i32));
            }
        }
        out
    }

    /// The part at `x`, `y` that is `width` by `height`.
    pub fn cropped(&self, x: i32, y: i32, width: u32, height: u32) -> Canvas {
        let mut out = Canvas::new(width, height);
        for dy in 0..height as i32 {
            for dx in 0..width as i32 {
                out.set(dx, dy, self.get(x + dx, y + dy));
            }
        }
        out
    }

    /// `other` drawn over this one at `x`, `y`; its clear pixels leave this
    /// one's.
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

    /// The colours it has, clear ones as one.
    pub fn colours(&self) -> BTreeSet<Colour> {
        self.rgba
            .as_chunks::<4>()
            .0
            .iter()
            .map(|&pixel| if pixel[3] == 0 { CLEAR } else { pixel })
            .collect()
    }
}

/// A fixed sequence of pseudo-random numbers.
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

    /// A number from 0 to `below`, without it.
    pub fn below(&mut self, below: u32) -> u32 {
        (self.number() % u64::from(below.max(1))) as u32
    }
}
