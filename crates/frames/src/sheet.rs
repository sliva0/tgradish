//! Animations assembled from several pictures: sprite sheets and image
//! sequences.

use std::time::Duration;

use crate::{Animation, DecodeOptions, Error, Frame, Result, decode};

/// How a sprite sheet is laid out: equal cells, frames row by row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sheet {
    pub columns: u32,
    pub rows: u32,
    /// How many cells hold frames. By default all of them, except fully
    /// transparent cells at the end.
    pub frames: Option<u32>,
    pub frame_duration: Duration,
}

/// Splits a still image into the frames of a sprite sheet.
pub fn sprite_sheet(bytes: &[u8], sheet: &Sheet) -> Result<Animation> {
    let image = decode(bytes, &DecodeOptions::default())?;
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

    // Only the first frame of an animated image is used as the sheet.
    let source = &image.frames()[0].rgba;
    let row_bytes = cell_width as usize * 4;
    let mut frames: Vec<Frame> = (0..cells)
        .map(|cell| {
            let left = (cell % sheet.columns * cell_width) as usize;
            let top = (cell / sheet.columns * cell_height) as usize;
            let mut rgba = Vec::with_capacity(row_bytes * cell_height as usize);
            for y in top..top + cell_height as usize {
                let start = (y * width as usize + left) * 4;
                rgba.extend_from_slice(&source[start..start + row_bytes]);
            }
            Frame { rgba, duration: sheet.frame_duration }
        })
        .collect();
    match sheet.frames {
        Some(count) => frames.truncate(count as usize),
        None => {
            let used = frames
                .iter()
                .rposition(|frame| frame.rgba.as_chunks::<4>().0.iter().any(|pixel| pixel[3] != 0))
                .map_or(1, |last| last + 1);
            frames.truncate(used);
        }
    }
    Animation::new(cell_width, cell_height, frames)
}

/// Joins images of one size into an animation. Still images are shown for
/// `frame_duration`; animated ones keep their own frames and timing.
pub fn sequence<'a>(
    images: impl IntoIterator<Item = &'a [u8]>,
    frame_duration: Duration,
) -> Result<Animation> {
    let mut size = None;
    let mut frames = Vec::new();
    for (index, bytes) in images.into_iter().enumerate() {
        let image = decode(bytes, &DecodeOptions::default())?;
        let (width, height) = (image.width(), image.height());
        let (first_width, first_height) = *size.get_or_insert((width, height));
        if (width, height) != (first_width, first_height) {
            return Err(Error::SequenceSize { index, width, height, first_width, first_height });
        }
        let still = image.frames().len() == 1;
        frames.extend(image.into_frames().into_iter().map(|mut frame| {
            if still {
                frame.duration = frame_duration;
            }
            frame
        }));
    }
    let (width, height) = size.ok_or(Error::Empty)?;
    Animation::new(width, height, frames)
}
