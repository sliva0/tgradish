//! Progress tracker emitting monotonic [`Progress::Advanced`] updates. Each
//! pipeline phase has a weighted share of the whole; skipped phases complete
//! immediately (for example, when lossless output already fits).
//!
//! Phase weights are medians measured at `Effort::Best` on the corpus and
//! reference art, and with fitting under limits lower than Telegram's; actual
//! timings vary several-fold depending on the artwork.

use std::cell::{Cell, RefCell};

use super::Progress;

/// Share of total work assigned to the initial quick encode (under
/// `Fit::Auto`).
const QUICK: f64 = 0.02;
/// Share of post-quick work spent in fitting (the rest is final encoding and
/// packing). On a [`Progress::Missed`] retry little reduction work remains, so
/// re-encoding and packing dominate.
const FITTING: f64 = 0.75;
const FITTING_AGAIN: f64 = 0.3;
/// Relative weights within fitting: reduction rounds, calibration against the
/// real encoder (which encodes and packs the nearly full-size art), and
/// weakening the final reduction.
const ROUNDS: f64 = 0.12;
const CALIBRATING: f64 = 0.85;
const WEAKENING: f64 = 0.03;
/// Share of final render+pack spent in encoding (rest is zopfli packing;
/// calibration uses the same split). Fitted art has fewer colours and encodes
/// faster, while zopfli packing takes roughly as long.
pub(super) const RENDER: f64 = 0.6;
const RENDER_FITTED: f64 = 0.25;
/// Reserve held back from final render+pack in case fitting misses and must
/// retry (completed immediately once the packed output fits).
const MISSED: f64 = 0.25;

/// Minimum progress delta before emitting another [`Progress::Advanced`] event.
const STEP: f64 = 0.001;

/// A part of making a sticker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Part {
    /// The first quick encode of the art as it is.
    Quick = 0,
    /// Fitting's rounds of reductions.
    Rounds,
    /// Measuring fitting's estimates against the real encoder.
    Calibrating,
    /// Weakening the last reduction.
    Weakening,
    /// Encoding the result with the real encoder.
    Rendering,
    /// Packing it by zopfli.
    Packing,
}

/// The stages front-ends show, each from the progress that starts it:
/// [`Progress::Normalised`], [`Progress::TooLarge`] where fitting starts,
/// and [`Progress::Packing`]; and each try again once the result missed,
/// all of it, from its [`Progress::Missed`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Stage {
    Drawing,
    Fitting,
    Packing,
    Again,
}

/// Follows the parts of making a sticker, saying how far it got.
pub(super) struct Tracker<'a> {
    progress: RefCell<&'a mut dyn FnMut(Progress)>,
    /// The lossless size is said once, by whichever finds it too large
    /// first.
    said_too_large: Cell<bool>,
    /// The first quick encode has its share: fitting may change the art.
    quick: Cell<bool>,
    /// The art is fitted: fitting's parts have their share.
    fitting: Cell<bool>,
    /// The target was lowered after the result missed it.
    aiming_lower: Cell<bool>,
    /// Calibrating has no share, done before the target was lowered.
    calibrated: Cell<bool>,
    /// Where the work left starts: at the start, or where it was when the
    /// target was lowered and fitting went on.
    base: Cell<f64>,
    /// How far each part got, in the order of [`Part`].
    parts: [Cell<f64>; 6],
    /// The most the whole came to, and the most said.
    whole: Cell<f64>,
    said: Cell<f64>,
    /// The stage at work, where it started, how far it got, and the most
    /// said of it.
    stage: Cell<Stage>,
    stage_from: Cell<f64>,
    stage_done: Cell<f64>,
    stage_said: Cell<f64>,
}

impl<'a> Tracker<'a> {
    /// Says `progress` as making a sticker goes, with the share of the
    /// first quick encode when `quick`.
    pub(super) fn new(progress: &'a mut dyn FnMut(Progress), quick: bool) -> Tracker<'a> {
        Tracker {
            progress: RefCell::new(progress),
            said_too_large: Cell::new(false),
            quick: Cell::new(quick),
            fitting: Cell::new(false),
            aiming_lower: Cell::new(false),
            calibrated: Cell::new(false),
            base: Cell::new(0.0),
            parts: Default::default(),
            whole: Cell::new(0.0),
            said: Cell::new(0.0),
            stage: Cell::new(Stage::Drawing),
            stage_from: Cell::new(0.0),
            stage_done: Cell::new(0.0),
            stage_said: Cell::new(0.0),
        }
    }

    /// Says `event`, the lossless size only the first time, and starts
    /// the stage it starts.
    pub(super) fn say(&self, event: Progress) {
        match event {
            Progress::TooLarge { .. } if self.said_too_large.replace(true) => return,
            Progress::Normalised { .. } => self.start(Stage::Drawing),
            // a try again is one stage, packing too
            Progress::Packing if self.stage.get() != Stage::Again => self.start(Stage::Packing),
            Progress::Missed { .. } => self.start(Stage::Again),
            _ => {}
        }
        (self.progress.borrow_mut())(event);
    }

    /// Starts `stage`, from where the whole is.
    pub(super) fn start(&self, stage: Stage) {
        self.stage.set(stage);
        self.stage_from.set(self.whole.get());
        self.stage_done.set(0.0);
        self.stage_said.set(0.0);
    }

    /// Says `part` is `share` done; a share less than it said before is
    /// taken as no further. Any of fitting's parts gives fitting its share.
    pub(super) fn at(&self, part: Part, share: f64) {
        if matches!(part, Part::Rounds | Part::Calibrating | Part::Weakening) {
            self.fitting.set(true);
        }
        let cell = &self.parts[part as usize];
        cell.set(cell.get().max(share.clamp(0.0, 1.0)));
        self.tell();
    }

    /// What says `part` is from `from` to `to` of the way done, for the
    /// work in it to tell, as shares of that work.
    pub(super) fn hook(&self, part: Part, from: f64, to: f64) -> impl Fn(f64) + '_ {
        move |share| self.at(part, from + (to - from) * share)
    }

    /// Fitting is over: what of it didn't happen is passed over.
    pub(super) fn fitted(&self) {
        for part in [Part::Quick, Part::Rounds, Part::Calibrating, Part::Weakening] {
            self.parts[part as usize].set(1.0);
        }
        self.tell();
    }

    /// The result came to `bytes`, too large, and fitting goes on aiming
    /// at `aim`, which is said: the work left is spread over the rest,
    /// calibrating again only unless it was done, as a stage of its own.
    pub(super) fn missed(&self, bytes: Option<usize>, aim: usize, calibrated: bool) {
        self.base.set(self.whole.get());
        self.quick.set(false);
        self.fitting.set(true);
        self.aiming_lower.set(true);
        self.calibrated.set(calibrated);
        for part in &self.parts {
            part.set(0.0);
        }
        self.say(Progress::Missed { bytes, aim });
    }

    /// All is done.
    pub(super) fn finish(&self) {
        for part in &self.parts {
            part.set(1.0);
        }
        self.whole.set(1.0);
        self.tell();
    }

    /// How much of the work left `parts` make up, from 0 to 1, as far as
    /// each got in the order of [`Part`].
    fn layout(&self, parts: [f64; 6]) -> f64 {
        let [quick, rounds, calibrating, weakening, rendering, packing] = parts;
        let rest = match self.fitting.get() {
            true => {
                let calibration = if self.calibrated.get() { 0.0 } else { CALIBRATING };
                let fitting = (ROUNDS * rounds + calibration * calibrating + WEAKENING * weakening)
                    / (ROUNDS + calibration + WEAKENING);
                let last = RENDER_FITTED * rendering + (1.0 - RENDER_FITTED) * packing;
                let share = if self.aiming_lower.get() { FITTING_AGAIN } else { FITTING };
                share * fitting + (1.0 - share) * (1.0 - MISSED) * last
            }
            false => RENDER * rendering + (1.0 - RENDER) * packing,
        };
        match self.quick.get() {
            true => QUICK * quick + (1.0 - QUICK) * rest,
            false => rest,
        }
    }

    /// Where the stage at work ends, as the parts are laid out now, of the
    /// work left: drawing where fitting starts or the result is encoded,
    /// fitting where it is encoded, packing at the end.
    fn stage_end(&self) -> f64 {
        let fitting = self.fitting.get();
        match self.stage.get() {
            // fitting may measure its estimates before it starts
            Stage::Drawing if fitting => self.layout([1.0, 0.0, 1.0, 0.0, 0.0, 0.0]),
            Stage::Drawing => self.layout([1.0, 0.0, 0.0, 0.0, 1.0, 0.0]),
            Stage::Fitting => self.layout([1.0, 1.0, 1.0, 1.0, 1.0, 0.0]),
            Stage::Packing | Stage::Again => self.layout([1.0; 6]),
        }
    }

    /// Works out the whole and the stage at work, and says them when
    /// they went on far enough since they were said.
    fn tell(&self) {
        let base = self.base.get();
        let local = self.layout(self.parts.each_ref().map(Cell::get));
        let whole = self.whole.get().max(base + (1.0 - base) * local);
        self.whole.set(whole);
        let (from, end) = (self.stage_from.get(), base + (1.0 - base) * self.stage_end());
        let stage = match end > from {
            true => ((whole - from) / (end - from)).clamp(0.0, 1.0),
            false => 1.0,
        };
        let stage = self.stage_done.get().max(stage);
        self.stage_done.set(stage);
        let further = whole >= self.said.get() + STEP || stage >= self.stage_said.get() + STEP;
        let end = whole >= 1.0 && self.said.get() < 1.0;
        if further || end {
            self.said.set(whole);
            self.stage_said.set(stage);
            let event = Progress::Advanced { fraction: whole, stage_fraction: stage };
            (self.progress.borrow_mut())(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reduce::Reduction;
    use crate::sticker::Step;

    /// What a tracker said, `fraction` and `stage-fraction`, after `steps`
    /// on one with the first quick encode when `quick`.
    fn said(quick: bool, steps: impl FnOnce(&Tracker)) -> Vec<(f64, f64)> {
        let mut said = Vec::new();
        let mut progress = |event| {
            if let Progress::Advanced { fraction, stage_fraction } = event {
                said.push((fraction, stage_fraction));
            }
        };
        steps(&Tracker::new(&mut progress, quick));
        said
    }

    fn ascending(said: &[(f64, f64)]) -> bool {
        said.windows(2).all(|pair| pair[0].0 <= pair[1].0)
    }

    #[test]
    fn spreads_the_parts_kept_exactly_over_the_whole() {
        // encoded in six-tenths, then packed
        let said = said(false, |tracker| {
            tracker.say(Progress::Packing);
            tracker.at(Part::Rendering, 0.5);
            tracker.at(Part::Rendering, 1.0);
            tracker.at(Part::Packing, 0.5);
            tracker.finish();
        });
        let fractions: Vec<f64> = said.iter().map(|&(fraction, _)| fraction).collect();
        let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
        assert!(near(fractions[0], RENDER / 2.0) && near(fractions[1], RENDER), "{said:?}");
        assert!(near(fractions[2], RENDER + (1.0 - RENDER) / 2.0), "{said:?}");
        assert_eq!(said.last(), Some(&(1.0, 1.0)));
    }

    #[test]
    fn never_goes_back() {
        let said = said(true, |tracker| {
            tracker.at(Part::Quick, 1.0);
            // a part said less than before is no further
            tracker.at(Part::Rendering, 0.6);
            tracker.at(Part::Rendering, 0.2);
            // fitting found after all, its share laid out ahead
            tracker.say(Progress::TooLarge { bytes: 1 });
            tracker.start(Stage::Fitting);
            tracker.at(Part::Rounds, 0.5);
            tracker.at(Part::Calibrating, 0.3);
            tracker.fitted();
            tracker.at(Part::Rendering, 1.0);
            tracker.say(Progress::Packing);
            tracker.at(Part::Packing, 0.9);
            // the result too large all the same: what is left is spread
            // over the rest, a stage of its own
            let before = tracker.whole.get();
            tracker.missed(Some(70_000), 60_000, true);
            assert_eq!((tracker.stage.get(), tracker.stage_done.get()), (Stage::Again, 0.0));
            tracker.at(Part::Rounds, 0.01);
            assert!(tracker.whole.get() >= before);
            let step = Step::new(Reduction::Despeckle, 1, 0.0);
            tracker.say(Progress::Reduced { step });
            assert_eq!(tracker.stage.get(), Stage::Again);
            tracker.at(Part::Rounds, 1.0);
            tracker.say(Progress::Packing);
            assert_eq!(tracker.stage.get(), Stage::Again);
            tracker.at(Part::Packing, 0.5);
            tracker.finish();
        });
        assert!(ascending(&said), "{said:?}");
        assert_eq!(said.last(), Some(&(1.0, 1.0)));
    }

    #[test]
    fn says_how_far_the_stage_at_work_got() {
        let said = said(false, |tracker| {
            tracker.start(Stage::Drawing);
            tracker.at(Part::Rendering, 0.5);
            tracker.at(Part::Rendering, 1.0);
            tracker.say(Progress::Packing);
            tracker.at(Part::Packing, 0.25);
        });
        // drawing half way, then done; packing a quarter of the way
        let stages: Vec<f64> = said.iter().map(|&(_, stage)| stage).collect();
        assert_eq!(stages.len(), 3, "{said:?}");
        assert!((stages[0] - 0.5).abs() < 1e-9 && (stages[1] - 1.0).abs() < 1e-9, "{said:?}");
        assert!((stages[2] - 0.25).abs() < 1e-9, "{said:?}");
    }

    #[test]
    fn says_little_steps_together() {
        let said = said(false, |tracker| {
            for step in 0..=100_000 {
                tracker.at(Part::Rendering, f64::from(step) / 100_000.0);
            }
        });
        assert!(said.len() <= 1001 && said.len() > 500, "{}", said.len());
        assert!(ascending(&said));
    }
}
