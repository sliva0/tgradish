//! Animations assembled from sprite sheets and image sequences.

use std::fmt;
use std::time::Duration;

use crate::frames::{Animation, Cancelled, DecodeOptions, Error, Frame, Limits, Result, decode};

/// Sprite sheet layout: uniform grid cells read left-to-right, top-to-bottom.
/// Construct with [`Sheet::new`] and override fields as needed so future
/// fields retain their defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sheet {
    /// Number of columns across.
    pub columns: u32,
    /// Number of rows down.
    pub rows: u32,
    /// Number of cells containing frames (`None` uses all cells except
    /// trailing fully transparent ones).
    pub frames: Option<u32>,
    /// Duration of each frame (`Duration::ZERO` uses
    /// [`DEFAULT_FRAME_DURATION`](crate::frames::DEFAULT_FRAME_DURATION)).
    pub frame_duration: Duration,
}

impl Sheet {
    /// Creates a `columns × rows` sprite sheet layout that drops trailing
    /// transparent cells and displays each frame for
    /// [`DEFAULT_FRAME_DURATION`](crate::frames::DEFAULT_FRAME_DURATION).
    pub fn new(columns: u32, rows: u32) -> Sheet {
        Sheet { columns, rows, frames: None, frame_duration: Duration::ZERO }
    }
}

/// Decodes a still image from `bytes` and splits it into sprite sheet frames
/// according to `sheet`. Rejects multi-frame animated inputs.
pub fn sprite_sheet(bytes: &[u8], sheet: &Sheet, limits: &Limits) -> Result<Animation> {
    split_sheet(&decode(bytes, &DecodeOptions::default(), limits)?, sheet, limits)
}

/// Splits a decoded single-frame `image` into sprite sheet frames, counting
/// extracted frame memory alongside `image` against `limits`. Multi-frame
/// animations are rejected because the sheet frame would be ambiguous.
pub fn split_sheet(image: &Animation, sheet: &Sheet, limits: &Limits) -> Result<Animation> {
    let [picture] = image.frames() else {
        return Err(Error::SheetAnimated { frames: image.frames().len() });
    };
    let (width, height) = (image.width(), image.height());
    let grid_error =
        || Error::SheetGrid { width, height, columns: sheet.columns, rows: sheet.rows };
    if sheet.columns == 0
        || sheet.rows == 0
        || width % sheet.columns != 0
        || height % sheet.rows != 0
    {
        return Err(grid_error());
    }
    let (cell_width, cell_height) = (width / sheet.columns, height / sheet.rows);
    let cells = sheet.columns * sheet.rows;
    if let Some(frames) = sheet.frames
        && (frames == 0 || frames > cells)
    {
        return Err(Error::SheetFrames { cells, frames });
    }

    let source = &picture.rgba;
    let row_bytes = cell_width as usize * 4;
    let rows = |cell: u32| {
        let left = (cell % sheet.columns * cell_width) as usize;
        let top = (cell / sheet.columns * cell_height) as usize;
        (top..top + cell_height as usize).map(move |y| {
            let start = (y * width as usize + left) * 4;
            &source[start..start + row_bytes]
        })
    };
    let count = match sheet.frames {
        Some(count) => count,
        None => (0..cells)
            .rposition(|cell| {
                rows(cell).any(|row| row.as_chunks::<4>().0.iter().any(|pixel| pixel[3] != 0))
            })
            .map_or(1, |last| last as u32 + 1),
    };
    // the cells are copied out while the sheet is held
    let held = source.len();
    let left = Limits { max_bytes: limits.max_bytes.saturating_sub(held), ..*limits };
    left.check(cell_width, cell_height, count as usize)?;
    let frames = (0..count)
        .map(|cell| {
            let mut rgba = Vec::with_capacity(row_bytes * cell_height as usize);
            rows(cell).for_each(|row| rgba.extend_from_slice(row));
            Frame { rgba, duration: sheet.frame_duration }
        })
        .collect();
    Animation::new(cell_width, cell_height, frames)
}

/// Combines same-sized images into an animation. Still images use
/// `frame_duration` (`Duration::ZERO` uses
/// [`DEFAULT_FRAME_DURATION`](crate::frames::DEFAULT_FRAME_DURATION));
/// animated inputs keep their own frames and durations. Images are decoded as
/// the iterator yields them so callers streaming from disk only hold one
/// encoded file in memory at a time; use [`Sequence`] to push images manually.
pub fn sequence(
    images: impl IntoIterator<Item = impl AsRef<[u8]>>,
    frame_duration: Duration,
    limits: &Limits,
) -> Result<Animation> {
    let mut sequence = Sequence::new(frame_duration, limits);
    for image in images {
        sequence.push(image.as_ref())?;
    }
    sequence.finish()
}

/// Incremental builder for [`sequence`], allowing each input buffer to be
/// dropped right after decoding:
///
/// ```
/// use std::time::Duration;
/// use tgradish_tgs::frames::{Limits, Sequence};
///
/// // A 1x1 black pixel GIF.
/// let gif = b"GIF89a\x01\0\x01\0\x80\0\0\0\0\0\xff\xff\xff,\0\0\0\0\x01\0\x01\0\0\x02\x02D\x01\0;";
/// let mut sequence = Sequence::new(Duration::from_millis(50), &Limits::default());
/// for _ in 0..3 {
///     sequence.push(gif)?;
/// }
/// assert_eq!(sequence.finish()?.duration(), Duration::from_millis(150));
/// # Ok::<(), tgradish_tgs::frames::Error>(())
/// ```
pub struct Sequence<'a> {
    frame_duration: Duration,
    limits: Limits,
    cancelled: Option<Cancelled<'a>>,
    /// Dimensions of the first image, once pushed.
    size: Option<(u32, u32)>,
    frames: Vec<Frame>,
    /// Total RGBA bytes of frames accumulated so far.
    held: usize,
    /// Number of images pushed so far.
    images: usize,
}

impl fmt::Debug for Sequence<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Sequence")
            .field("frame_duration", &self.frame_duration)
            .field("limits", &self.limits)
            .field("cancelled", &self.cancelled.is_some())
            .field("size", &self.size)
            .field("frames", &self.frames.len())
            .finish()
    }
}

impl<'a> Sequence<'a> {
    /// Creates an empty sequence builder with the given still-image
    /// `frame_duration` and memory `limits`.
    pub fn new(frame_duration: Duration, limits: &Limits) -> Sequence<'a> {
        Sequence {
            frame_duration,
            limits: *limits,
            cancelled: None,
            size: None,
            frames: Vec::new(),
            held: 0,
            images: 0,
        }
    }

    /// Sets a cancellation callback polled before each image and frame (like
    /// [`DecodeOptions::cancelled`]).
    pub fn cancelled_by(self, cancelled: Cancelled<'a>) -> Sequence<'a> {
        Sequence { cancelled: Some(cancelled), ..self }
    }

    /// Decodes `bytes` within the remaining memory budget and appends its
    /// frames. If decoding fails or the dimensions differ from the first
    /// image, returns an error (with 0-based index in [`Error::SequenceSize`])
    /// and leaves `self` unchanged.
    pub fn push(&mut self, bytes: &[u8]) -> Result<()> {
        let index = self.images;
        let limits = &self.limits;
        let left = Limits { max_bytes: limits.max_bytes.saturating_sub(self.held), ..*limits };
        let options = DecodeOptions { cancelled: self.cancelled, ..DecodeOptions::default() };
        let image = decode(bytes, &options, &left)?;
        let (width, height) = (image.width(), image.height());
        let (first_width, first_height) = self.size.unwrap_or((width, height));
        if (width, height) != (first_width, first_height) {
            return Err(Error::SequenceSize { index, width, height, first_width, first_height });
        }
        self.size = Some((width, height));
        self.images += 1;
        self.held += image.frames().iter().map(|frame| frame.rgba.len()).sum::<usize>();
        let still = image.frames().len() == 1;
        let frame_duration = self.frame_duration;
        self.frames.extend(image.into_frames().into_iter().map(|mut frame| {
            if still {
                frame.duration = frame_duration;
            }
            frame
        }));
        Ok(())
    }

    /// Finishes and returns the combined [`Animation`], or [`Error::Empty`] if
    /// no images were pushed.
    pub fn finish(self) -> Result<Animation> {
        let (width, height) = self.size.ok_or(Error::Empty)?;
        Animation::new(width, height, self.frames)
    }
}

#[cfg(test)]
mod tests {
    use tgradish_corpus::{Canvas, encode};

    use super::*;
    use crate::frames::DEFAULT_FRAME_DURATION;

    #[test]
    fn shows_frames_of_no_time_for_the_default() {
        let mut canvas = Canvas::new(2, 1);
        canvas.rect(0, 0, 2, 1, [200, 40, 40, 255]);
        let png = encode::png(&canvas);
        let limits = Limits::default();
        let durations = |animation: Animation| -> Vec<Duration> {
            animation.frames().iter().map(|frame| frame.duration).collect()
        };

        let sheet = |frame_duration| Sheet { columns: 2, rows: 1, frames: None, frame_duration };
        let split = sprite_sheet(&png, &sheet(Duration::ZERO), &limits).unwrap();
        assert_eq!(durations(split), [DEFAULT_FRAME_DURATION; 2]);
        let split = sprite_sheet(&png, &sheet(Duration::from_millis(1)), &limits).unwrap();
        assert_eq!(durations(split), [Duration::from_millis(1); 2]);

        let joined = sequence([&png[..], &png[..]], Duration::ZERO, &limits).unwrap();
        assert_eq!(durations(joined.clone()), [DEFAULT_FRAME_DURATION; 2]);

        // an animation isn't a sheet
        let animated = split_sheet(&joined, &sheet(Duration::ZERO), &limits);
        assert_eq!(animated, Err(Error::SheetAnimated { frames: 2 }));
    }
}
