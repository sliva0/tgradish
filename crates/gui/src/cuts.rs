//! Where an input's frames start and which part of it is used, as the
//! timeline shows and edits them and items keep them, written as the cuts
//! converting reads.

use tgradish_core::webm::read_from;

/// How a cut between frames is written, so converting keeps the frames
/// meant, whatever rounding does to the number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cut {
    /// ffmpeg keeps frames from the first that starts at or after a cut,
    /// and counts them over the length: a cut is a hair before the frame
    /// it starts, and the length is the frames'.
    Video,
    /// Normalising keeps every frame a cut overlaps: a cut is a hair
    /// inside the frames it keeps.
    Art,
}

/// Where an input's frames start, then where the last one ends.
#[derive(Debug, Clone, PartialEq)]
pub struct Frames {
    pub times: Vec<f64>,
    pub cut: Cut,
}

impl Frames {
    /// Frames starting at `starts`, in order, over `length`: the first
    /// shows from the beginning, and none starts at the end.
    pub fn starting(starts: &[f64], length: f64, cut: Cut) -> Option<Frames> {
        if starts.is_empty() || !(length > 0.0 && length.is_finite()) {
            return None;
        }
        let mut times = vec![0.0];
        times.extend(
            starts[1..].iter().copied().filter(|&start| start > 1e-6 && start < length - 1e-6),
        );
        times.push(length);
        Some(Frames { times, cut })
    }

    /// Frames of `1 / fps` seconds over `length`, the last one cut short;
    /// a tenth of a frame or less over the last is taken for rounding.
    pub fn even(length: f64, fps: f64, cut: Cut) -> Option<Frames> {
        if !(fps > 0.0 && length > 0.0) {
            return None;
        }
        let count = ((length * fps - 0.1).ceil() as usize).max(1);
        let mut times: Vec<f64> = (0..count).map(|i| i as f64 / fps).collect();
        times.push(length);
        Some(Frames { times, cut })
    }

    pub fn count(&self) -> usize {
        self.times.len() - 1
    }

    /// The frame shown at `time`.
    pub fn at(&self, time: f64) -> usize {
        self.times
            .partition_point(|&start| start <= time + 1e-9)
            .saturating_sub(1)
            .min(self.count() - 1)
    }

    /// The boundary between frames nearest `time`: 0 is before the first,
    /// [`Frames::count`] after the last.
    pub fn nearest(&self, time: f64) -> usize {
        let after = self.times.partition_point(|&boundary| boundary < time);
        match after {
            0 => 0,
            n if n > self.count() => self.count(),
            n if time - self.times[n - 1] <= self.times[n] - time => n - 1,
            n => n,
        }
    }

    /// The used part as boundaries: its first frame and the one after its
    /// last.
    pub fn part(&self, length: f64, start: Option<f64>, used: Option<f64>) -> (usize, usize) {
        let (from, to) = part(length, start, used);
        let first = self.nearest(from).min(self.count() - 1);
        (first, self.nearest(to).max(first + 1))
    }

    /// Uses the frames from `first` up to `after`, writing the cuts into
    /// `start` and `used` as the item stores them.
    pub fn set_part(
        &self,
        (first, after): (usize, usize),
        start: &mut Option<f64>,
        used: &mut Option<f64>,
    ) {
        let times = &self.times;
        let (from, length) = match self.cut {
            Cut::Video if first > 0 => {
                (read_from(times[first], times[first - 1]), times[after] - times[first])
            }
            Cut::Video => (0.0, times[after]),
            Cut::Art if first > 0 => (times[first] + 1e-6, times[after] - times[first] - 2e-6),
            Cut::Art => (0.0, times[after] - 1e-6),
        };
        *start = (first > 0).then_some(from);
        *used = (after < self.count()).then_some(length);
    }
}

/// The used part as `(start, end)`, and as the item stores it.
pub(crate) fn part(length: f64, start: Option<f64>, used: Option<f64>) -> (f64, f64) {
    let from = start.unwrap_or(0.0).clamp(0.0, length);
    (from, used.map_or(length, |used| (from + used).min(length)))
}

pub(crate) fn set_part(
    length: f64,
    (from, to): (f64, f64),
    start: &mut Option<f64>,
    used: &mut Option<f64>,
) {
    *start = (from > 1e-3).then_some(from);
    *used = (to < length - 1e-3).then_some(to - from);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_frames_and_the_boundaries_between_them() {
        // 25 fps, 1 s: frames start every 0.04 s, the last at 0.96
        let frames = Frames::even(1.0, 25.0, Cut::Video).unwrap();
        assert_eq!(frames.count(), 25);
        assert_eq!(
            (frames.at(0.5), frames.at(0.48), frames.at(1.0), frames.at(-1.0)),
            (12, 12, 24, 0)
        );
        assert_eq!((frames.nearest(0.49), frames.nearest(0.51), frames.nearest(2.0)), (12, 13, 25));
        // a short last frame, and one a hair over rounding
        let short = Frames::even(0.97, 25.0, Cut::Video).unwrap();
        assert_eq!((short.count(), short.at(0.965)), (25, 24));
        assert_eq!(Frames::even(1.003, 25.0, Cut::Video).unwrap().count(), 25);
        // frames as long as their packets say, the first from the start
        let uneven = Frames::starting(&[0.02, 0.1, 0.2, 0.7], 0.8, Cut::Video).unwrap();
        assert_eq!(uneven.times, [0.0, 0.1, 0.2, 0.7, 0.8]);
        assert_eq!((uneven.at(0.65), uneven.at(0.7)), (2, 3));
    }

    #[test]
    fn cuts_where_converting_keeps_the_frames_meant() {
        let video = Frames::even(1.0, 25.0, Cut::Video).unwrap();
        // ffmpeg keeps from the first frame at or after the start, and
        // counts frames over the length
        let frames = |part| {
            let (mut start, mut used) = (None, None);
            video.set_part(part, &mut start, &mut used);
            assert_eq!(video.part(1.0, start, used), part);
            let first = start.map_or(0, |start| video.times.partition_point(|&t| t < start));
            (first, tgradish_core::webm::frame_count(used.unwrap(), 25.0) as usize)
        };
        // frames 5 to 9, and 0 to 9
        assert_eq!(frames((5, 10)), (5, 5));
        assert_eq!(frames((0, 10)), (0, 10));
        // the whole of it is no cut at all
        let (mut start, mut used) = (Some(0.3), Some(0.2));
        video.set_part((0, 25), &mut start, &mut used);
        assert_eq!((start, used), (None, None));

        // art keeps every frame a cut overlaps: cuts fall just inside
        let art = Frames { times: vec![0.0, 0.1, 0.25, 0.4], cut: Cut::Art };
        let (mut start, mut used) = (None, None);
        art.set_part((1, 2), &mut start, &mut used);
        let end = start.unwrap() + used.unwrap();
        assert!(start.unwrap() > 0.1 && end < 0.25, "{start:?} {used:?}");
        assert_eq!(art.part(0.4, start, used), (1, 2));

        // frames of different lengths keep their own
        let uneven = Frames::starting(&[0.0, 0.1, 0.2, 0.7], 0.8, Cut::Video).unwrap();
        let (mut start, mut used) = (None, None);
        uneven.set_part((2, 3), &mut start, &mut used);
        let start = start.unwrap();
        let used = used.unwrap();
        assert!(start > 0.1 && start < 0.2 && (used - 0.5).abs() < 1e-9, "{start} {used}");
        assert_eq!(uneven.part(0.8, Some(start), Some(used)), (2, 3));
    }
}
