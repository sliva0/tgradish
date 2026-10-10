//! Sets of cells as bits.

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Mask {
    width: u32,
    height: u32,
    words: Vec<u64>,
}

impl std::fmt::Debug for Mask {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for y in 0..self.height {
            let row: String =
                (0..self.width).map(|x| if self.get(x, y) { '#' } else { '.' }).collect();
            writeln!(f, "{row}")?;
        }
        Ok(())
    }
}

impl Mask {
    pub fn new(width: u32, height: u32) -> Mask {
        let cells = width as usize * height as usize;
        Mask { width, height, words: vec![0; cells.div_ceil(64)] }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    fn index(&self, x: u32, y: u32) -> usize {
        y as usize * self.width as usize + x as usize
    }

    pub fn get(&self, x: u32, y: u32) -> bool {
        let i = self.index(x, y);
        self.words[i / 64] >> (i % 64) & 1 == 1
    }

    pub fn set(&mut self, x: u32, y: u32) {
        let i = self.index(x, y);
        self.words[i / 64] |= 1 << (i % 64);
    }

    pub fn clear(&mut self, x: u32, y: u32) {
        let i = self.index(x, y);
        self.words[i / 64] &= !(1 << (i % 64));
    }

    pub fn is_empty(&self) -> bool {
        self.words.iter().all(|&word| word == 0)
    }

    pub fn count(&self) -> usize {
        self.words.iter().map(|word| word.count_ones() as usize).sum()
    }

    pub fn union(&mut self, other: &Mask) {
        self.words.iter_mut().zip(&other.words).for_each(|(a, b)| *a |= b);
    }

    pub fn subtract(&mut self, other: &Mask) {
        self.words.iter_mut().zip(&other.words).for_each(|(a, b)| *a &= !b);
    }

    pub fn intersect(&mut self, other: &Mask) {
        self.words.iter_mut().zip(&other.words).for_each(|(a, b)| *a &= b);
    }

    pub fn intersects(&self, other: &Mask) -> bool {
        self.words.iter().zip(&other.words).any(|(a, b)| a & b != 0)
    }

    pub fn is_subset(&self, other: &Mask) -> bool {
        self.words.iter().zip(&other.words).all(|(a, b)| a & !b == 0)
    }

    /// Set cells, row by row.
    pub fn cells(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        self.words.iter().enumerate().flat_map(move |(index, &word)| {
            let mut bits = word;
            std::iter::from_fn(move || {
                if bits == 0 {
                    return None;
                }
                let bit = bits.trailing_zeros() as usize;
                bits &= bits - 1;
                let cell = index * 64 + bit;
                Some(((cell % self.width as usize) as u32, (cell / self.width as usize) as u32))
            })
        })
    }

    /// The cells and their 8 neighbours.
    pub fn grown(&self) -> Mask {
        let mut out = self.clone();
        for (x, y) in self.cells() {
            for ny in y.saturating_sub(1)..=(y + 1).min(self.height - 1) {
                for nx in x.saturating_sub(1)..=(x + 1).min(self.width - 1) {
                    out.set(nx, ny);
                }
            }
        }
        out
    }

    /// The cells in a window of `width` by `height` from `(left, top)`, as
    /// a mask of the window's size.
    pub fn crop(&self, left: u32, top: u32, width: u32, height: u32) -> Mask {
        let mut out = Mask::new(width, height);
        for y in 0..height {
            for x in 0..width {
                if self.get(left + x, top + y) {
                    out.set(x, y);
                }
            }
        }
        out
    }

    /// The set cells in rows `top..bottom`.
    pub fn rows(&self, top: u32, bottom: u32) -> Mask {
        let mut out = Mask::new(self.width, self.height);
        let (start, end) = (self.index(0, top), self.index(0, bottom.min(self.height)));
        for word in start / 64..end.div_ceil(64) {
            let low = (word * 64).max(start) - word * 64;
            let high = ((word + 1) * 64).min(end) - word * 64;
            let keep =
                if high - low == 64 { u64::MAX } else { ((1u64 << (high - low)) - 1) << low };
            out.words[word] = self.words[word] & keep;
        }
        out
    }

    /// The smallest rectangle holding every set cell: `(x, y, width, height)`.
    pub fn bounds(&self) -> Option<(u32, u32, u32, u32)> {
        let (mut left, mut top, mut right, mut bottom) = (u32::MAX, u32::MAX, 0, 0);
        for (x, y) in self.cells() {
            left = left.min(x);
            right = right.max(x + 1);
            top = top.min(y);
            bottom = bottom.max(y + 1);
        }
        (left < right).then(|| (left, top, right - left, bottom - top))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_rows() {
        let mut mask = Mask::new(70, 3);
        for y in 0..3 {
            for x in [0, 63, 64, 69] {
                mask.set(x, y);
            }
        }
        let middle = mask.rows(1, 2);
        assert_eq!(middle.cells().collect::<Vec<_>>(), [(0, 1), (63, 1), (64, 1), (69, 1)]);
        assert_eq!(mask.rows(0, 3), mask);
        assert!(mask.rows(2, 2).is_empty());
    }

    #[test]
    fn sets_and_grows() {
        let mut mask = Mask::new(70, 3);
        mask.set(0, 0);
        mask.set(69, 2);
        assert_eq!(mask.cells().collect::<Vec<_>>(), [(0, 0), (69, 2)]);
        assert_eq!(mask.bounds(), Some((0, 0, 70, 3)));
        let grown = mask.grown();
        assert_eq!(grown.count(), 4 + 4);
        assert!(mask.is_subset(&grown) && !grown.is_subset(&mask));
        let mut both = grown.clone();
        both.intersect(&mask);
        assert_eq!(both, mask);
        assert_eq!(Mask::new(5, 5).bounds(), None);
    }
}
