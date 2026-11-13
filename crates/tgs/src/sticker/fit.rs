//! Fitting a sticker to the limit: reductions chosen by estimates of
//! their size, and those estimates measured against the real encoder.

use tgradish_frames::Animation;

use super::{
    Cancelled, Options, Progress, Rendered, Step, pack, parallel, render, share_of_limits,
};
use crate::Result;
use crate::encode::{Effort, Settings};
use crate::normalise::{self, PixelAnim, Report, normalise};
use crate::reduce::{Kind, Reduction, distortion};

// Fitting works on estimates, since the real encoder and zopfli can take
// minutes: a quick encode at Fast effort packed by `gzip -9`, scaled to
// what the real ones make of it. The scale is ZOPFLI_SHARE until the art
// comes within CALIBRATE_WITHIN times the target, then measured once on
// the art as it is. Each round keeps the reduction that saves the most bytes
// for the least added error, and the last one is weakened while it still
// fits. Should the real result miss all the same, the target drops by
// RETARGET and fitting goes on, while it has reductions left. A result
// over the limit even at ZOPFLI_BEST of its gzip size is packed by gzip.

/// What zopfli packs to, as a share of `gzip -9`, until it is measured on
/// the art; most art packs a little smaller, so estimates lean high.
pub(super) const ZOPFLI_SHARE: f64 = 0.93;
/// A share of `gzip -9` zopfli is taken never to come under: a result over
/// the limit by more is packed quickly rather than at length for nothing.
/// Regular patterns come down to about 0.75, so this is far from the edge;
/// whether a sticker fits is still decided on the file that is packed.
pub(super) const ZOPFLI_BEST: f64 = 0.5;
/// When the real size misses an estimate, the next target is this much
/// lower.
pub(super) const RETARGET: f64 = 0.96;
/// Estimates are measured against the real encoder once fitting comes
/// within this many times the limit, on the art as it is by then: farther
/// over it, fitting shrinks the art a lot anyway, and the real encoder's
/// search of all of it can take minutes.
const CALIBRATE_WITHIN: f64 = 2.0;

/// What a quick encode of an animation comes to before it is scaled to
/// the real encoder: its size packed by gzip, and its share of the
/// server's cost limit or Telegram Desktop's JSON limit, whichever is more.
#[derive(Debug, Clone, Copy)]
pub(super) struct Quick {
    pub(super) packed: usize,
    pub(super) share: f64,
}

impl Quick {
    /// The estimated size of the `.tgs`: the packed size scaled by
    /// `calibration`, or the share of `max_bytes` when that is more, so
    /// reductions that shrink the cost or the JSON count as progress even
    /// before it fits.
    pub(super) fn size(self, calibration: f64, max_bytes: usize) -> usize {
        (self.packed as f64 * calibration).max(self.share * max_bytes as f64) as usize
    }
}

/// Encodes `anim` quickly: `None` when it can't be made at all.
pub(super) fn quick(
    anim: &PixelAnim,
    options: &Options,
    cancelled: Cancelled,
) -> Result<Option<Quick>> {
    let fast = Settings { effort: Effort::Fast, ..Settings::default() };
    match render(anim, &fast, options, cancelled) {
        Ok(rendered) => Ok(Some(Quick {
            packed: rendered.packed.len(),
            share: share_of_limits(&rendered.scene, &rendered.json),
        })),
        Err(crate::Error::Cancelled) => Err(crate::Error::Cancelled),
        Err(_) => Ok(None),
    }
}

/// What scales quick estimates to the real encoder's sizes.
pub(super) struct Calibration {
    pub(super) factor: f64,
    /// What it was measured on, kept for when fitting ends there: the
    /// sticker is then exactly this, with no need to encode and pack it
    /// again.
    pub(super) real: Option<Real>,
}

/// An animation as the real encoder and zopfli make it.
pub(super) struct Real {
    pub(super) anim: PixelAnim,
    pub(super) rendered: Rendered,
    pub(super) tgs: Vec<u8>,
}

/// What the real encoder and zopfli make of `anim` against its `quick`
/// encode: the factor that scales quick sizes to real ones. zopfli packs
/// some art much smaller than `gzip -9`, regular patterns most.
fn calibrate(
    anim: &PixelAnim,
    quick: Quick,
    settings: &Settings,
    options: &Options,
    cancelled: Cancelled,
) -> Result<Calibration> {
    match render(anim, settings, options, cancelled) {
        Ok(rendered) => {
            let tgs = pack(&rendered.json, settings.effort, cancelled)?;
            Ok(Calibration {
                factor: tgs.len() as f64 / quick.packed.max(1) as f64,
                real: Some(Real { anim: anim.clone(), rendered, tgs }),
            })
        }
        Err(crate::Error::Cancelled) => Err(crate::Error::Cancelled),
        Err(_) => Ok(Calibration { factor: ZOPFLI_SHARE, real: None }),
    }
}

/// A reduced animation, its quick encode, its estimated size and its
/// error.
type Candidate = (PixelAnim, Option<Quick>, usize, f64);

/// What fitting aims at and works with.
pub(super) struct Fitting<'a> {
    /// The estimate to get to.
    pub(super) target: usize,
    /// The grid most of the art is on, which snapping to a grid aims for.
    pub(super) likely: Option<u32>,
    /// The real encoder's, for calibrating.
    pub(super) settings: &'a Settings,
    pub(super) options: &'a Options,
    pub(super) cancelled: Cancelled<'a>,
}

/// Reduces `current` until its estimate is at most the target: each round
/// takes the reduction that saves the most bytes for the least change,
/// among those the compromise puts first while any of them help, then the
/// last one is weakened as far as it still fits. Estimates are scaled by
/// `calibration`, which is measured on the art as it is once it comes near
/// the target. `known` is the quick encode of `current`, when the caller
/// has it. Also says whether the estimate got to the target, rather than
/// every reduction being used up first.
pub(super) fn fit(
    original: &PixelAnim,
    mut current: PixelAnim,
    known: Option<Option<Quick>>,
    steps: &mut Vec<Step>,
    calibration: &mut Option<Calibration>,
    fitting: &Fitting,
    progress: &mut dyn FnMut(Progress),
) -> Result<(PixelAnim, bool)> {
    let Fitting { target, likely, settings, options, cancelled } = *fitting;
    let factor = |calibration: &Option<Calibration>| calibration.as_ref().map(|c| c.factor);
    let size_of = |quick: Option<Quick>, factor: Option<f64>| {
        quick.map_or(usize::MAX, |quick| {
            quick.size(factor.unwrap_or(ZOPFLI_SHARE), options.max_bytes)
        })
    };
    // measures the calibration once the art comes near the target
    let near =
        |quick: Option<Quick>| size_of(quick, None) as f64 <= target as f64 * CALIBRATE_WITHIN;
    let measure = |anim: &PixelAnim, quick: Option<Quick>| -> Result<Option<Calibration>> {
        match quick {
            Some(quick) if near(Some(quick)) => {
                calibrate(anim, quick, settings, options, cancelled).map(Some)
            }
            _ => Ok(None),
        }
    };
    let mut quick_now = match known {
        Some(known) => known,
        None => quick(&current, options, cancelled)?,
    };
    // fits even as gzip packs the quick encode: the real encoder and
    // zopfli usually make it smaller still, and the result is checked
    // after fitting all the same
    if quick_now.is_some_and(|quick| quick.size(1.0, options.max_bytes) <= target) {
        return Ok((current, true));
    }
    if calibration.is_none() {
        *calibration = measure(&current, quick_now)?;
    }
    let mut size = size_of(quick_now, factor(calibration));
    if size <= target {
        return Ok((current, true));
    }
    if steps.is_empty() {
        progress(Progress::TooLarge { bytes: size });
    }
    // once snapped, the art is on its grid
    let likely = steps
        .iter()
        .all(|step| step.reduction.kind() != Kind::SnapToGrid)
        .then_some(likely)
        .flatten();
    let ladders: Vec<Vec<Reduction>> =
        options.reductions.iter().map(|kind| kind.ladder(likely)).collect();
    // the next strength of each kind, past what earlier rounds used
    let mut next: Vec<usize> = ladders
        .iter()
        .map(|ladder| {
            ladder
                .iter()
                .position(|reduction| !steps.iter().any(|step| step.reduction == *reduction))
                .unwrap_or(ladder.len())
        })
        .collect();
    let mut distortion_now = steps.last().map_or(0.0, |step| step.distortion);
    // the state before the last step, to weaken it
    let mut before_last: Option<(PixelAnim, usize)> = None;

    while size > target {
        if cancelled() {
            return Err(crate::Error::Cancelled);
        }
        let open: Vec<usize> = (0..ladders.len()).filter(|&k| next[k] < ladders[k].len()).collect();
        if open.is_empty() {
            // every reduction is used up: as small as it gets
            break;
        }
        let first: Vec<usize> = open
            .iter()
            .copied()
            .filter(|&k| options.compromise.first(options.reductions[k]))
            .collect();
        let kinds = if first.is_empty() { open } else { first };
        // each kind's candidate, its estimate and its error, or `None` when
        // it doesn't help at this strength
        let now = factor(calibration);
        let tries = parallel(&kinds, |&k| -> Result<Option<Candidate>> {
            let candidate = ladders[k][next[k]].apply(&current)?;
            let candidate_quick = quick(&candidate, options, cancelled)?;
            let candidate_size = size_of(candidate_quick, now);
            if candidate == current || candidate_size >= size {
                return Ok(None);
            }
            let candidate_distortion = distortion(original, &candidate);
            Ok(Some((candidate, candidate_quick, candidate_size, candidate_distortion)))
        });
        // (value, kind, candidate)
        let mut best: Option<(f64, usize, Candidate)> = None;
        for (&k, tried) in kinds.iter().zip(tries) {
            let Some(candidate) = tried? else {
                // no help at this strength; the next round tries stronger
                next[k] += 1;
                continue;
            };
            let value = (size - candidate.2) as f64 / (candidate.3 - distortion_now).max(1e-9);
            if best.as_ref().is_none_or(|b| value > b.0) {
                best = Some((value, k, candidate));
            }
        }
        let Some((_, k, (candidate, candidate_quick, candidate_size, candidate_distortion))) = best
        else {
            continue;
        };
        before_last = Some((current, k));
        current = candidate;
        quick_now = candidate_quick;
        size = candidate_size;
        distortion_now = candidate_distortion;
        // near the target now: sizes measured against the real encoder
        // decide how much further to go
        if calibration.is_none() {
            *calibration = measure(&current, quick_now)?;
            size = size_of(quick_now, factor(calibration));
        }
        let step = Step { reduction: ladders[k][next[k]], bytes: size, distortion: distortion_now };
        progress(Progress::Reduced { step: step.clone() });
        steps.push(step);
        next[k] += 1;
    }

    // weakening the last step keeps the estimate within the target
    let reached = size <= target;
    // weaken the last step as far as the result still fits, unless that is
    // within a tenth of the target already: weakening takes encodes, and
    // would win little
    if let Some((previous, k)) = before_last
        && size <= target
        && (size as f64) < target as f64 * 0.9
    {
        let last = steps.last().unwrap().reduction;
        let weaker = next[k].checked_sub(2).map(|level| ladders[k][level]);
        let (mut low, mut high) = (0.0, 1.0);
        // five halvings put the strength within a 32nd of the weakest
        // that fits
        for _ in 0..5 {
            let share = (low + high) / 2.0;
            let Some(reduction) = last.between(weaker.as_ref(), share) else { break };
            let candidate = reduction.apply(&previous)?;
            let candidate_size =
                size_of(quick(&candidate, options, cancelled)?, factor(calibration));
            if candidate_size <= target {
                high = share;
                let candidate_distortion = distortion(original, &candidate);
                *steps.last_mut().unwrap() =
                    Step { reduction, bytes: candidate_size, distortion: candidate_distortion };
                current = candidate;
            } else {
                low = share;
            }
        }
    }
    Ok((current, reached))
}

/// The scale snapping would use for `anim`, which normalising with
/// `options` made along with `report`: see [`normalise_likely_scale`].
/// Normalising found it already, unless a forced scale left it out, or
/// cutting the art square made neighbouring frames equal, which
/// normalising again would join, counting colour edges differently.
pub(super) fn likely_scale(
    anim: &PixelAnim,
    report: &Report,
    options: &normalise::Options,
) -> Option<u32> {
    let frames = anim.frames();
    let found = options.pixel_scale.is_none()
        && frames.windows(2).all(|pair| pair[0].pixels != pair[1].pixels);
    match found {
        true => report.likely_scale.map(|likely| likely.scale),
        false => normalise_likely_scale(anim),
    }
}

/// The scale snapping would use: the likely scale of `anim`'s cells, as
/// normalising finds it.
pub(super) fn normalise_likely_scale(anim: &PixelAnim) -> Option<u32> {
    let rgba: Vec<_> = (0..anim.frames().len())
        .map(|frame| tgradish_frames::Frame {
            rgba: anim.rgba(frame),
            duration: std::time::Duration::from_millis(100),
        })
        .collect();
    let width = *anim.grid().columns.last()?;
    let height = *anim.grid().rows.last()?;
    let animation = Animation::new(width, height, rgba).ok()?;
    let options = normalise::Options { keep_canvas: true, ..normalise::Options::default() };
    normalise(&animation, &options).ok()?.1.likely_scale.map(|likely| likely.scale)
}
