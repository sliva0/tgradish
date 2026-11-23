//! Where an input's frames start and which part of it is used, as the timeline
//! shows them, items keep them and conversion reads them.

use tgradish_core::webm::{edge_leeway, frame_edge, read_from};

/// How frame-accurate cuts are encoded into `start` and `length` options so
/// conversion selects the intended frames despite float rounding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cut {
    /// Video inputs: ffmpeg includes frames starting at or after `start` and
    /// counts frames over `length`, so `start` is placed slightly before the
    /// first frame ([`read_from`]) and `length` equals the selected frames'
    /// span.
    Video,
    /// Pixel-art inputs: `.tgs` normalisation and drawn WebM conversion snap
    /// cuts within [`edge_leeway`] of a frame boundary, so `start` and
    /// `length` are formatted with the fewest decimal places that stay within
    /// a tenth of that leeway.
    Art,
}

/// Frame start timestamps followed by the end timestamp of the final frame.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Frames {
    pub(crate) times: Vec<f64>,
    pub(crate) cut: Cut,
}

impl Frames {
    /// Frames starting at `starts`, in order, over `length`: the first
    /// shows from the beginning, and none starts at the end.
    pub(crate) fn starting(starts: &[f64], length: f64, cut: Cut) -> Option<Frames> {
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
    pub(crate) fn even(length: f64, fps: f64, cut: Cut) -> Option<Frames> {
        if !(fps > 0.0 && length > 0.0) {
            return None;
        }
        let count = ((length * fps - 0.1).ceil() as usize).max(1);
        let mut times: Vec<f64> = (0..count).map(|i| i as f64 / fps).collect();
        times.push(length);
        Some(Frames { times, cut })
    }

    pub(crate) fn count(&self) -> usize {
        self.times.len() - 1
    }

    /// The frame shown at `time`, or that starts a hair after it, as
    /// converting takes a time written with few digits.
    pub(crate) fn at(&self, time: f64) -> usize {
        let shown = self.times.partition_point(|&start| start <= time).saturating_sub(1);
        frame_edge(time, &self.times).unwrap_or(shown).min(self.count() - 1)
    }

    /// Where the frame `index` starts, as a picture made of it is cut:
    /// none for the first.
    pub(crate) fn start_of(&self, index: usize) -> Option<f64> {
        (index > 0).then(|| self.written(self.times[index], index))
    }

    /// `seconds`, a time at the boundary `index` or a length to it, in as
    /// few digits as converting still takes for it: pixel art's within a
    /// tenth of how near it takes them, a video's as they are.
    fn written(&self, seconds: f64, index: usize) -> f64 {
        let within = match self.cut {
            Cut::Video => 1e-9,
            Cut::Art => edge_leeway(&self.times, index) / 10.0,
        };
        shortest(seconds, within)
    }

    /// The boundary between frames nearest `time`: 0 is before the first,
    /// [`Frames::count`] after the last.
    pub(crate) fn nearest(&self, time: f64) -> usize {
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
    pub(crate) fn part(
        &self,
        length: f64,
        start: Option<f64>,
        used: Option<f64>,
    ) -> (usize, usize) {
        let (from, to) = part(length, start, used);
        let first = self.nearest(from).min(self.count() - 1);
        (first, self.nearest(to).max(first + 1))
    }

    /// Uses the frames from `first` up to `after`, writing the cuts into
    /// `start` and `used` as the item stores them.
    pub(crate) fn set_part(
        &self,
        (first, after): (usize, usize),
        start: &mut Option<f64>,
        used: &mut Option<f64>,
    ) {
        let times = &self.times;
        let (from, frames) = match self.cut {
            _ if first == 0 => (0.0, times[after]),
            Cut::Video => (
                shortest(read_from(times[first], times[first - 1]), 1e-9),
                times[after] - times[first],
            ),
            // to where the frames end, from where the cut is written
            Cut::Art => {
                let from = self.written(times[first], first);
                (from, times[after] - from)
            }
        };
        *start = (first > 0).then_some(from);
        *used = (after < self.count()).then(|| self.written(frames, after));
    }
}

/// `seconds` in the fewest decimals within `within` of it, as the number
/// is written: 0.16 rather than 0.16000000000000003, or for a third of a
/// second, 0.33333 where that is near enough.
fn shortest(seconds: f64, within: f64) -> f64 {
    (0..=12)
        .map(|digits| {
            let scale = 10f64.powi(digits);
            (seconds * scale).round() / scale
        })
        .find(|written| (written - seconds).abs() <= within)
        .unwrap_or(seconds)
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
    use std::time::Duration;
    use tgradish_core::tgradish_tgs::frames::{Animation, Frame};
    use tgradish_core::tgradish_tgs::normalise::PixelFrame;

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

        // art is cut where its frames start and end, as converting takes
        // a cut near them, written as short as it is
        let art = Frames { times: vec![0.0, 0.1, 0.1 + 0.06, 0.4], cut: Cut::Art };
        let (mut start, mut used) = (None, None);
        art.set_part((1, 2), &mut start, &mut used);
        assert_eq!((start, used), (Some(0.1), Some(0.06)));
        assert_eq!(art.part(0.4, start, used), (1, 2));
        // as it was once written, a hair inside the frames, it is the same
        assert_eq!(art.part(0.4, Some(0.100001), Some(0.059998)), (1, 2));
        art.set_part((0, 2), &mut start, &mut used);
        assert_eq!((start, used), (None, Some(0.16)));
        assert_eq!((art.start_of(0), art.start_of(2)), (None, Some(0.16)));

        // frames of different lengths keep their own
        let uneven = Frames::starting(&[0.0, 0.1, 0.2, 0.7], 0.8, Cut::Video).unwrap();
        let (mut start, mut used) = (None, None);
        uneven.set_part((2, 3), &mut start, &mut used);
        let start = start.unwrap();
        let used = used.unwrap();
        assert!(start > 0.1 && start < 0.2 && (used - 0.5).abs() < 1e-9, "{start} {used}");
        assert_eq!(uneven.part(0.8, Some(start), Some(used)), (2, 3));
    }

    /// Frames of one colour each, red from 10 up, lasting `seconds`, and
    /// where they start as the GUI reads them, then where the last ends.
    fn art(seconds: &[f64]) -> (Animation, Frames) {
        let frames: Vec<Frame> = (0..seconds.len())
            .map(|index| Frame {
                rgba: vec![10 + index as u8, 0, 0, 255],
                duration: Duration::from_secs_f64(seconds[index]),
            })
            .collect();
        let mut times = vec![0.0];
        for frame in &frames {
            times.push(times.last().unwrap() + frame.duration.as_secs_f64());
        }
        (Animation::new(1, 1, frames).unwrap(), Frames { times, cut: Cut::Art })
    }

    /// Which frames of `animation` normalising for a `.tgs` keeps, by
    /// their colour, and how many 60 fps frames each shows for, then how
    /// many it drops as too short for one and joins: from
    /// `start` for `length` seconds, as the command line reads them, or
    /// else from the frame `exactly.0` up to the one before `exactly.1`.
    fn kept(
        animation: &Animation,
        (start, length): (Option<f64>, Option<f64>),
        exactly: Option<(usize, usize)>,
    ) -> (Vec<(u8, u32)>, usize, usize) {
        let part = tgradish_core::options::Selection { start, length, ..Default::default() };
        let options = tgradish_core::tgs::Options::default();
        let mut normalise = options.sticker_options(&part).unwrap().normalise;
        if let Some((first, after)) = exactly {
            let time = |index: usize| -> Duration {
                animation.frames()[..index].iter().map(|frame| frame.duration).sum()
            };
            normalise.start = time(first);
            normalise.length = Some(time(after) - time(first));
        }
        let (kept, report) =
            tgradish_core::tgradish_tgs::normalise(animation.clone(), &normalise).unwrap();
        let colour = |frame: &PixelFrame| kept.palette()[usize::from(frame.pixels[0])][0];
        let frames = kept.frames().iter().map(|frame| (colour(frame), frame.ticks)).collect();
        (frames, report.dropped_frames, report.merged_frames)
    }

    #[test]
    fn art_cut_as_its_flags_say_keeps_the_frames_meant() {
        // a third of a second, 33 ms and 16.67 ms frames among others, as
        // frame folders and Aseprite files time them
        let fixtures: [&[f64]; 4] = [
            &[1.0 / 3.0; 5],
            &[0.033, 0.017, 0.033, 0.1, 0.007, 0.033, 0.25],
            &[1.0 / 60.0, 1.0 / 60.0, 1.0 / 12.0, 1.0 / 60.0, 1.0 / 12.0, 1.0 / 7.0],
            &[0.08; 9],
        ];
        for seconds in fixtures {
            let (animation, frames) = art(seconds);
            let count = frames.count();
            let total = frames.times[count];
            for first in 0..count {
                for after in first + 1..=count {
                    let (mut start, mut length) = (None, None);
                    frames.set_part((first, after), &mut start, &mut length);
                    assert_eq!(frames.part(total, start, length), (first, after));
                    // as the command line shows them and reads them back
                    let shown = tgradish_core::options::flags(&tgradish_core::options::Selection {
                        start,
                        length,
                        ..Default::default()
                    });
                    let read = |name: &str| {
                        let text = shown.iter().find_map(|flag| flag.strip_prefix(name))?;
                        // written short: no float noise, at most 5 decimals
                        let decimals =
                            text.split_once('.').map_or(0, |(_, decimals)| decimals.len());
                        assert!(decimals <= 5, "{shown:?}");
                        let value: f64 = text.parse().unwrap();
                        let key = &name[2..name.len() - 1];
                        assert_eq!(tgradish_core::options::wrong_value(key, value), None);
                        Some(value)
                    };
                    let (start, length) = (read("--start="), read("--length="));
                    let what = format!("{seconds:?} {first}..{after}: {shown:?}");
                    // normalised for a .tgs, the frames meant, each whole
                    let whole = kept(&animation, (None, None), Some((first, after)));
                    let colours: Vec<u8> = whole.0.iter().map(|&(colour, _)| colour).collect();
                    let meant: Vec<u8> = (first..after).map(|index| 10 + index as u8).collect();
                    // but those too short for a 60 fps frame
                    assert!(colours.iter().all(|colour| meant.contains(colour)), "{what}");
                    assert_eq!(kept(&animation, (start, length), None), whole, "{what}");
                    // drawn for ffmpeg, read from a hair before the first,
                    // for as long as the frames
                    let options =
                        tgradish_core::webm::Options { start, length, ..Default::default() };
                    let cut = tgradish_core::webm::drawn_cut(&options, &frames.times);
                    let times = &frames.times;
                    let from = (first > 0).then(|| read_from(times[first], times[first - 1]));
                    assert!(
                        cut.start.zip(from).is_none_or(|(a, b)| (a - b).abs() < 1e-12),
                        "{what}"
                    );
                    assert_eq!(cut.start.is_some(), first > 0, "{what}");
                    let frames_length = times[after] - times[first];
                    assert!(
                        cut.length.is_none_or(|cut| (cut - frames_length).abs() < 1e-12),
                        "{what}"
                    );
                    assert_eq!(cut.length.is_some(), after < count, "{what}");
                }
            }
        }
        // frames of whole milliseconds are cut at them
        let (_, frames) = art(&[0.08; 9]);
        let (mut start, mut length) = (None, None);
        frames.set_part((2, 8), &mut start, &mut length);
        assert_eq!((start, length), (Some(0.16), Some(0.48)));
    }
}
