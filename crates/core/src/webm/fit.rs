//! Searching for encoder settings that land just under the size limit.
//!
//! The search only talks to an [`Encoder`], so it can be tested without
//! ffmpeg. It spends an encode only where the encodes so far say it can
//! do better, and says why it stops, or tries a change or not, as a
//! [`Decision`]:
//!
//! - an encode that fits and uses at least [`IN_BAND`] of the limit, as a
//!   share shown to a tenth of a percent, is the answer;
//! - the next value is predicted from the encodes so far, size taken to
//!   grow along a line with the value: between the largest that fits and
//!   the smallest that doesn't, the line through them (regula falsi);
//!   beyond them, the line through the two nearest far enough apart to
//!   tell ([`SPREAD`]), or in proportion from one. An encode predicted to
//!   use under [`MIN_GAIN`] more of the limit than the best that fits, or
//!   to look no more like the input by more than [`ALIKE_NOISE`], isn't
//!   made. Where nothing fits, the least of the range is encoded before
//!   the search gives up, and at once where it is predicted not to fit:
//!   a prediction never shows that nothing fits;
//! - `fit = auto` lowers the frame rate only where nothing fits at it, or
//!   the best that fits looks poor (under [`ALIKE_GOOD`]) and the encodes
//!   don't show that looks hardly depend on bits, as fewer frames only
//!   give each more of them.

use std::path::PathBuf;

use super::{Decision, Params, Plan, Rate, estimate_bitrate};
use crate::error::{Error, Result};
use crate::options::Fit;

/// An encode that fits and uses at least this share of the limit, as
/// shown to a tenth of a percent, ends the search.
const IN_BAND: f64 = 0.96;
/// Aim a bit under the limit, since sizes are only roughly predictable.
const AIM: f64 = 0.985;
/// Another encode is made only where it is predicted to use at least
/// this much more of the limit than the best that fits.
const MIN_GAIN: f64 = 0.01;
/// Below this many bits per pixel and frame, `fit = auto` may try lower
/// frame rates to give each frame more bits.
const AUTO_FPS_BPP: f64 = 0.12;
/// `fit = auto` does not go below this frame rate.
const AUTO_MIN_FPS: f64 = 8.0;
/// Bitrate searches stop once an attempt looks this much like the input
/// (SSIM): more bits can't make it look better, only larger, as with
/// simple pictures, which come out well under the limit at any bitrate.
const ALIKE_ENOUGH: f64 = 0.995;
/// `fit = auto` keeps the frame rate where the best that fits at it looks
/// at least this like the input: fewer frames could only make up for
/// what is hardly to be seen.
const ALIKE_GOOD: f64 = 0.95;
/// SSIM differences under this are noise: no one sees them, and encodes
/// of nearly the same settings differ by about as much.
const ALIKE_NOISE: f64 = 0.002;
/// Two encodes tell how size or looks change with a value only where it
/// differs between them by at least this factor: closer, the encoder's
/// own noise would swamp what they tell.
const SPREAD: f64 = 1.05;
/// The least share a value changes by in one step, so each step moves.
const LEAST_STEP: f64 = 0.01;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Attempt {
    pub(crate) number: u32,
    pub(crate) params: Params,
    pub(crate) bytes: u64,
    pub(crate) path: PathBuf,
}

pub(crate) trait Encoder {
    fn encode(&mut self, params: Params) -> Result<Attempt>;
    /// Similarity to the source, higher is better.
    fn score(&mut self, attempt: &Attempt) -> Result<f64>;
    /// Hears why fitting stopped, or tried a change.
    fn decided(&mut self, _decision: Decision) {}
    /// Whether fitting was asked to settle on the best that fits so far,
    /// see [`CancelToken::settle`](crate::CancelToken::settle): asked only
    /// once an encode fits.
    fn settling(&self) -> bool {
        false
    }
}

/// What fitting keeps.
#[derive(Debug)]
pub(crate) struct Fitted {
    pub(crate) best: Attempt,
    /// Asked to, it settled on the best that fit so far.
    pub(crate) settled: bool,
}

/// An encode as the search weighs it: the value tried, its size, which
/// it was, and how like the input it looks, where scored.
#[derive(Debug, Clone, Copy)]
struct Point {
    x: f64,
    bytes: u64,
    number: u32,
    alike: Option<f64>,
}

/// Size as a line of the value: `bytes` at `x`, rising by `slope` a unit.
#[derive(Debug, Clone, Copy)]
struct Line {
    x: f64,
    bytes: f64,
    slope: f64,
}

impl Line {
    fn through(a: Point, b: Point) -> Line {
        let slope = (b.bytes as f64 - a.bytes as f64) / (b.x - a.x);
        Line { x: b.x, bytes: b.bytes as f64, slope }
    }

    /// Size in proportion to the value, as two-pass bitrates aim for.
    fn proportional(p: Point) -> Line {
        Line { x: p.x, bytes: p.bytes as f64, slope: p.bytes as f64 / p.x }
    }

    fn at(self, x: f64) -> f64 {
        self.bytes + self.slope * (x - self.x)
    }

    /// The value where it comes to `bytes`, where it rises at all.
    fn solve(self, bytes: f64) -> Option<f64> {
        (self.slope > 0.0).then(|| self.x + (bytes - self.bytes) / self.slope)
    }
}

/// The encoder as the searches share it: counting encodes against the
/// most allowed, passing on what they decide, and stopping them once
/// asked to settle.
struct Fitter<'a> {
    encoder: &'a mut dyn Encoder,
    limit: u64,
    used: u32,
    attempts: u32,
    /// Asked to settle: no more encodes are made, and each search ends
    /// with the best it has.
    settled: bool,
}

impl<'a> Fitter<'a> {
    fn new(encoder: &'a mut dyn Encoder, limit: u64, attempts: u32) -> Self {
        Fitter { encoder, limit, used: 0, attempts, settled: false }
    }

    /// Whether to settle now, as asked at any time since.
    fn settles(&mut self) -> bool {
        self.settled |= self.encoder.settling();
        self.settled
    }

    /// Encodes, unless asked to settle; `None` once it is. The encode at
    /// work when it is asked is given up, as its ffmpeg is stopped: what
    /// went wrong then is of no account.
    fn encode(&mut self, params: Params) -> Result<Option<Attempt>> {
        if self.settles() {
            return Ok(None);
        }
        self.used += 1;
        match self.encoder.encode(params) {
            Ok(attempt) => Ok(Some(attempt)),
            Err(_) if self.settles() => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Scores, unless asked to settle; `None` once it is.
    fn score(&mut self, attempt: &Attempt) -> Result<Option<f64>> {
        if self.settles() {
            return Ok(None);
        }
        match self.encoder.score(attempt) {
            Ok(ssim) => Ok(Some(ssim)),
            Err(_) if self.settles() => Ok(None),
            Err(err) => Err(err),
        }
    }

    fn decide(&mut self, decision: Decision) {
        self.encoder.decided(decision);
    }

    fn left(&self) -> u32 {
        self.attempts.saturating_sub(self.used)
    }

    fn share(&self, bytes: u64) -> f64 {
        bytes as f64 / self.limit as f64
    }

    /// Fits, and uses at least [`IN_BAND`] of the limit as a share shown
    /// to a tenth of a percent: what shows as 96.0% counts.
    fn in_band(&self, bytes: u64) -> bool {
        bytes <= self.limit && (self.share(bytes) * 1000.0).round() >= IN_BAND * 1000.0
    }

    /// Says a search stopped for want of encodes, where none are left
    /// at all rather than only its share.
    fn out_of_attempts(&mut self) {
        if self.left() == 0 {
            self.decide(Decision::OutOfAttempts { attempts: self.attempts });
        }
    }
}

/// Outcome of searching one value.
#[derive(Default)]
struct Search {
    /// Best attempt that fits.
    best: Option<Attempt>,
    /// Every encode, in the order made.
    points: Vec<Point>,
}

impl Search {
    fn best_point(&self) -> Option<Point> {
        let number = self.best.as_ref()?.number;
        self.points.iter().find(|point| point.number == number).copied()
    }

    /// Size of the smallest attempt, to report when nothing fits.
    fn smallest(&self) -> Option<u64> {
        self.points.iter().map(|point| point.bytes).min()
    }

    /// Notes how like the input encode `number` looks.
    fn scored(&mut self, number: u32, alike: f64) {
        if let Some(point) = self.points.iter_mut().find(|point| point.number == number) {
            point.alike = Some(alike);
        }
    }

    /// How SSIM grows with the log of the value, by the scored encodes
    /// that fit furthest apart, with the one with less and the one with
    /// more, where they are far enough apart to tell.
    fn alike_slope(&self, limit: u64) -> Option<(f64, Point, Point)> {
        let scored = || self.points.iter().filter(|p| p.bytes <= limit && p.alike.is_some());
        let low = *scored().min_by(|a, b| a.x.total_cmp(&b.x))?;
        let high = *scored().max_by(|a, b| a.x.total_cmp(&b.x))?;
        if high.x < low.x * SPREAD {
            return None;
        }
        let gained = high.alike? - low.alike?;
        Some((gained / (high.x / low.x).ln(), low, high))
    }
}

/// What the encodes so far say to do next.
enum Next {
    /// Try this value, whose size is predicted by the line.
    Try(f64, Line),
    Stop(Decision),
}

/// The next value to try in `[min, max]`, as the encodes so far predict
/// it, or why there is none.
fn next_value(points: &[Point], limit: u64, (min, max): (f64, f64)) -> Next {
    let aim = limit as f64 * AIM;
    let fits = |point: &&Point| point.bytes <= limit;
    let by_x = |a: &&Point, b: &&Point| a.x.total_cmp(&b.x);
    // the largest that fits, and the smallest past it that doesn't: an
    // encoder whose sizes don't always grow may leave smaller ones that
    // don't fit, which say nothing of what is left to try
    let below = points.iter().filter(fits).max_by(by_x).copied();
    let above = points
        .iter()
        .filter(|point| point.bytes > limit && below.is_none_or(|below| point.x > below.x))
        .min_by(by_x)
        .copied();
    match (below, above) {
        (Some(below), Some(above)) => {
            let line = Line::through(below, above);
            let t = (aim - below.bytes as f64) / (above.bytes as f64 - below.bytes as f64);
            // stay clear of both ends so every step makes progress
            Next::Try(below.x + t.clamp(0.15, 0.85) * (above.x - below.x), line)
        }
        (Some(below), None) => {
            if below.x >= max {
                return Next::Stop(Decision::NothingLeft);
            }
            let other = points
                .iter()
                .filter(fits)
                .filter(|point| point.x * SPREAD <= below.x)
                .max_by(by_x)
                .copied();
            let line = other.map_or(Line::proportional(below), |other| Line::through(other, below));
            // at most 4 times, so one wild size can't throw the search off
            let x = line.solve(aim).unwrap_or(f64::INFINITY);
            Next::Try(x.clamp(below.x * (1.0 + LEAST_STEP), below.x * 4.0).min(max), line)
        }
        (None, Some(above)) => {
            if above.x <= min {
                let share = above.bytes as f64 / limit as f64;
                return Next::Stop(Decision::TooLargeAtLeast { attempt: above.number, share });
            }
            let other = points
                .iter()
                .filter(|point| point.x >= above.x * SPREAD)
                .min_by(by_x)
                .copied()
                .map(|other| Line::through(above, other))
                .filter(|line| line.slope > 0.0);
            // even the least predicted not to fit: encoding it is what
            // shows whether anything does, as sizes may flatten out where
            // a line through two larger ones can't tell
            if let Some(line) = other
                && line.at(min) > limit as f64
            {
                return Next::Try(min, line);
            }
            let line = other.unwrap_or(Line::proportional(above));
            let x = line.solve(aim).unwrap_or(0.0);
            Next::Try(x.clamp(above.x * 0.25, above.x * (1.0 - LEAST_STEP)).max(min), line)
        }
        (None, None) => Next::Stop(Decision::NothingLeft),
    }
}

/// Why an encode of `next`, predicted by `line`, isn't worth making, if
/// it isn't: it would use too little more of the limit than the best that
/// fits, or, by the scored encodes, look no more like the input.
fn not_worth_it(f: &Fitter, search: &Search, next: f64, line: Line) -> Option<Decision> {
    let best = search.best_point()?;
    let predicted = line.at(next);
    let (share, predicted) = (f.share(best.bytes), predicted / f.limit as f64);
    if predicted <= 1.0 && predicted - share < MIN_GAIN {
        return Some(Decision::LittleToGain { attempt: best.number, share, predicted });
    }
    let (slope, low, high) = search.alike_slope(f.limit)?;
    let (gained, ssim) = (high.alike? - low.alike?, high.alike?);
    let more = slope.max(0.0) * (next / best.x).ln();
    (high.number == best.number && gained < ALIKE_NOISE && more < ALIKE_NOISE)
        .then_some(Decision::AlikeUnchanged { attempt: high.number, than: low.number, ssim })
}

/// Finds the largest `x` in `[min, max]` whose encode fits, as size grows
/// with `x` (bitrate, length), in `budget` encodes. With `scoring`, an
/// attempt that fits under the band is scored, and one that looks
/// [`ALIKE_ENOUGH`] like the input ends the search.
#[allow(clippy::too_many_arguments)]
fn maximise(
    f: &mut Fitter,
    budget: u32,
    (min, max): (f64, f64),
    initial: f64,
    scoring: bool,
    quantise: impl Fn(f64) -> f64,
    params: impl Fn(f64) -> Params,
) -> Result<Search> {
    let mut search = Search::default();
    // rounding must not leave the range, use the exact value if it would
    let snap = |x: f64| {
        let x = x.clamp(min, max);
        let rounded = quantise(x);
        if (min..=max).contains(&rounded) { rounded } else { x }
    };
    let mut x = snap(initial);
    let mut used = 0;
    loop {
        if used >= budget {
            f.out_of_attempts();
            break;
        }
        let Some(attempt) = f.encode(params(x))? else { break };
        used += 1;
        let fits = attempt.bytes <= f.limit;
        let mut point = Point { x, bytes: attempt.bytes, number: attempt.number, alike: None };
        if fits && search.best_point().is_none_or(|best| x > best.x) {
            search.best = Some(attempt.clone());
        }
        let stop = if f.in_band(attempt.bytes) {
            let share = f.share(attempt.bytes);
            Some(Decision::InBand { attempt: attempt.number, share, least: IN_BAND })
        } else if fits && scoring {
            // more of the value would only be tried for a larger file
            let Some(ssim) = f.score(&attempt)? else {
                search.points.push(point);
                break;
            };
            point.alike = Some(ssim);
            (ssim >= ALIKE_ENOUGH)
                .then_some(Decision::AlikeEnough { attempt: attempt.number, ssim })
        } else {
            None
        };
        search.points.push(point);
        if let Some(decision) = stop {
            f.decide(decision);
            break;
        }

        let tried = |x: f64| search.points.iter().any(|point| point.x == x);
        let next = match next_value(&search.points, f.limit, (min, max)) {
            Next::Try(next, _) if tried(snap(next)) => Err(Decision::NothingLeft),
            Next::Try(next, line) => match not_worth_it(f, &search, snap(next), line) {
                Some(decision) => Err(decision),
                None => Ok(snap(next)),
            },
            Next::Stop(decision) => Err(decision),
        };
        x = match next {
            Ok(next) => next,
            // predictions order encodes and skip them, but only an encode
            // shows that nothing fits: the least of the range, before
            // giving up on the value
            Err(_) if search.best.is_none() && !tried(snap(min)) => snap(min),
            Err(decision) => {
                f.decide(decision);
                break;
            }
        };
    }
    Ok(search)
}

/// Finds the best integer in `[min, max]` whose encode fits. With
/// `increasing`, size grows with the value and the largest fitting value is
/// best (fps); otherwise the smallest is (CRF).
fn bisect(
    f: &mut Fitter,
    budget: u32,
    (min, max): (i64, i64),
    increasing: bool,
    params: impl Fn(i64) -> Params,
) -> Result<Search> {
    let mut search = Search::default();
    let (mut lo, mut hi) = (min, max);
    let mut used = 0;
    loop {
        if lo > hi {
            f.decide(Decision::NothingLeft);
            break;
        }
        if used >= budget {
            f.out_of_attempts();
            break;
        }
        let mid = lo + (hi - lo) / 2;
        let Some(attempt) = f.encode(params(mid))? else { break };
        used += 1;
        let fits = attempt.bytes <= f.limit;
        let point =
            Point { x: mid as f64, bytes: attempt.bytes, number: attempt.number, alike: None };
        search.points.push(point);
        let in_band = f.in_band(attempt.bytes);
        let share = f.share(attempt.bytes);
        let number = attempt.number;
        if fits {
            search.best = Some(attempt);
        }
        if in_band {
            f.decide(Decision::InBand { attempt: number, share, least: IN_BAND });
            break;
        }
        match (fits, increasing) {
            (true, true) | (false, false) => lo = mid + 1,
            (true, false) | (false, true) => hi = mid - 1,
        }
    }
    Ok(search)
}

/// Rounds to `decimals` places, dividing last so results print cleanly.
fn round_to(value: f64, decimals: i32) -> f64 {
    let scale = 10f64.powi(decimals);
    (value * scale).round() / scale
}

/// Frame rates `fit = auto` may try, best first.
fn auto_frame_rates(plan: &Plan, limit: u64) -> Vec<f64> {
    let full = plan.fps;
    let pixels = f64::from(plan.width) * f64::from(plan.height);
    let bits_per_pixel = limit as f64 * 8.0 / (pixels * full * plan.length);
    if !plan.auto_fps || bits_per_pixel >= AUTO_FPS_BPP {
        return vec![full];
    }
    let mut rates = vec![full];
    for rate in [full * 2.0 / 3.0, full / 2.0].map(|r| round_to(r, 2)) {
        if rate >= AUTO_MIN_FPS && rates.iter().all(|&r| (r - rate).abs() > 0.5) {
            rates.push(rate);
        }
    }
    rates
}

fn fit_bitrate(f: &mut Fitter, plan: &Plan, budget: u32, fps: f64, initial: f64) -> Result<Search> {
    let range = (plan.fit_range.min, plan.fit_range.max);
    let length = plan.length;
    maximise(
        f,
        budget,
        range,
        initial,
        true,
        |kbps| round_to(kbps, 1),
        |kbps| Params { fps, length, rate: Rate::Bitrate(kbps) },
    )
}

/// Fits the bitrate at the planned frame rate, and at lower ones only
/// where nothing fits at it or the best that does looks poor in a way
/// more bits a frame could mend; keeps what looks most like the input,
/// scoring with SSIM.
fn fit_auto(f: &mut Fitter, plan: &Plan) -> Result<Search> {
    let rates = auto_frame_rates(plan, f.limit);
    let mut total = Search::default();
    // the best so far, how like the input it looks, and at what rate
    let mut kept: Option<(Attempt, f64, f64)> = None;
    let mut initial = estimate_bitrate(plan.length, f.limit);

    for (i, &fps) in rates.iter().enumerate() {
        let remaining = f.left();
        let rates_left = (rates.len() - i) as u32;
        // the first rate also finds the bitrate the others start from, so it
        // gets everything except 2 attempts for each other rate
        let budget = if i == 0 {
            remaining.saturating_sub(2 * (rates_left - 1)).max(2)
        } else {
            (remaining / rates_left).max(2)
        }
        .min(remaining);
        if budget == 0 {
            f.out_of_attempts();
            break;
        }

        let mut search = fit_bitrate(f, plan, budget, fps, initial)?;
        total.points.extend(search.points.iter().copied());
        // settled: the best kept so far, or else this rate's, which only
        // a lower rate that looks better would have taken the place of
        if f.settled {
            if kept.is_none() {
                total.best = search.best;
            }
            break;
        }
        let lower = rates.get(i + 1).copied();
        let Some(candidate) = search.best.clone() else {
            // fewer frames make a smaller file at the same bitrate
            match lower {
                Some(lower) if kept.is_none() && f.left() > 0 => {
                    f.decide(Decision::LowersFrameRateToFit { from: fps, fps: lower });
                    continue;
                }
                _ => break,
            }
        };
        if let Rate::Bitrate(kbps) = candidate.params.rate {
            initial = kbps;
        }
        if rates.len() == 1 {
            total.best = Some(candidate);
            break;
        }

        let scored = match search.best_point().and_then(|best| best.alike) {
            Some(ssim) => Some(ssim),
            None => f.score(&candidate)?,
        };
        let Some(ssim) = scored else {
            if kept.is_none() {
                total.best = Some(candidate);
            }
            break;
        };
        search.scored(candidate.number, ssim);
        // a lower frame rate looking worse means even lower ones will too;
        // one that looks better by no more than noise isn't worth its
        // jerkier motion either
        if let Some((best, kept_ssim, kept_fps)) = &kept
            && ssim < *kept_ssim + ALIKE_NOISE
        {
            f.decide(Decision::LowerLooksNoBetter {
                fps,
                ssim,
                kept: best.number,
                kept_fps: *kept_fps,
                kept_ssim: *kept_ssim,
            });
            break;
        }
        let number = candidate.number;
        kept = Some((candidate, ssim, fps));
        let Some(lower) = lower else { break };
        // fewer frames give each more bits, which only helps where bits
        // made it look more like the input, and is only worth jerkier
        // motion where it looks poor
        let more_bits = search.alike_slope(f.limit).map(|(slope, ..)| slope.max(0.0));
        if more_bits.is_some_and(|slope| slope * (fps / lower).ln() < ALIKE_NOISE)
            || ssim >= ALIKE_GOOD
        {
            break;
        }
        if f.left() == 0 {
            f.out_of_attempts();
            break;
        }
        f.decide(Decision::LowersFrameRateForLooks {
            from: fps,
            fps: lower,
            attempt: number,
            ssim,
        });
    }
    if let Some((best, ..)) = kept {
        total.best = Some(best);
    }
    Ok(total)
}

/// What fitting encodes first: what it then changes depends on the sizes
/// that come out. Found by running the search with an encoder that stops
/// at its first attempt, so it always agrees with it.
pub fn first_params(plan: &Plan) -> Params {
    /// Notes the first attempt asked for, and stops the search there.
    struct First(Option<Params>);
    impl Encoder for First {
        fn encode(&mut self, params: Params) -> Result<Attempt> {
            self.0.get_or_insert(params);
            Err(Error::Cancelled)
        }
        fn score(&mut self, _: &Attempt) -> Result<f64> {
            Err(Error::Cancelled)
        }
    }
    let mut first = First(None);
    let _ = run(plan, &mut first);
    first.0.expect("every search encodes at least once")
}

/// Runs the search configured in `plan` and returns the attempt to keep:
/// the best under its target's size limit, or the best so far where it
/// was asked to settle. With `fit = off` that is the single attempt, even
/// if it is too big.
pub(crate) fn run(plan: &Plan, encoder: &mut dyn Encoder) -> Result<Fitted> {
    let limit = plan.target.max_bytes();
    let (fps, length, budget) = (plan.fps, plan.length, plan.attempts);
    let quality = if plan.lossless { Rate::Lossless } else { Rate::Crf(plan.crf) };
    let range = plan.fit_range;

    if plan.fit == Fit::Off {
        let rate = match () {
            _ if plan.lossless => Rate::Lossless,
            _ if plan.constant_quality => Rate::Crf(plan.crf),
            _ => Rate::Bitrate(plan.bitrate),
        };
        let best = encoder.encode(Params { fps, length, rate })?;
        return Ok(Fitted { best, settled: false });
    }
    let mut f = Fitter::new(encoder, limit, budget);
    let search = match plan.fit {
        Fit::Off => unreachable!("encoded above"),
        Fit::Auto => fit_auto(&mut f, plan)?,
        Fit::Bitrate => fit_bitrate(&mut f, plan, budget, fps, plan.bitrate)?,
        Fit::Crf => bisect(
            &mut f,
            budget,
            (range.min.ceil() as i64, range.max.floor() as i64),
            false,
            |crf| Params { fps, length, rate: Rate::Crf(crf as u8) },
        )?,
        Fit::Fps => bisect(
            &mut f,
            budget,
            (range.min.ceil() as i64, range.max.floor() as i64),
            true,
            |fps| Params { fps: fps as f64, length, rate: quality },
        )?,
        Fit::Length => maximise(
            &mut f,
            budget,
            (range.min, range.max),
            range.max,
            false,
            |secs| round_to(secs, 2),
            |secs| Params { fps, length: secs, rate: quality },
        )?,
    };

    let smallest = search.smallest().unwrap_or(0);
    let best = search.best.ok_or(Error::NothingFits { smallest, limit })?;
    if f.settled {
        f.decide(Decision::Settled { attempt: best.number });
    }
    Ok(Fitted { best, settled: f.settled })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffmpeg::Probe;
    use crate::options::Range;
    use crate::telegram::MAX_STICKER_BYTES as MAX_BYTES;
    use crate::webm::Options;
    use crate::webm::{Request, plan};

    /// Pretends to encode: bitrate encodes overshoot their target by 3%,
    /// constant quality encodes shrink 9% per CRF step. SSIM rewards bits per
    /// frame and penalises dropped frames.
    struct FakeEncoder {
        calls: Vec<Params>,
        motion: f64,
        /// A simple picture: no larger than this at any bitrate, and as
        /// like the input as this.
        simple: Option<(f64, f64)>,
        scored: u32,
        decisions: Vec<Decision>,
    }

    impl FakeEncoder {
        fn new(motion: f64) -> Self {
            Self { calls: Vec::new(), motion, simple: None, scored: 0, decisions: Vec::new() }
        }

        fn simple(bytes: f64, alike: f64) -> Self {
            Self { simple: Some((bytes, alike)), ..Self::new(0.0) }
        }
    }

    impl Encoder for FakeEncoder {
        fn encode(&mut self, params: Params) -> Result<Attempt> {
            self.calls.push(params);
            let bytes = match params.rate {
                Rate::Bitrate(kbps) => 2000.0 + kbps * 1000.0 / 8.0 * params.length * 1.03,
                Rate::Crf(crf) => {
                    3_000_000.0 * 0.91f64.powi(crf.into()) * params.fps / 30.0 * params.length
                }
                Rate::Lossless => 50_000.0 * params.fps * params.length,
            };
            let bytes = self.simple.map_or(bytes, |(most, _)| bytes.min(most));
            Ok(Attempt {
                number: self.calls.len() as u32,
                params,
                bytes: bytes as u64,
                path: PathBuf::new(),
            })
        }

        fn score(&mut self, attempt: &Attempt) -> Result<f64> {
            self.scored += 1;
            if let Some((_, alike)) = self.simple {
                return Ok(alike);
            }
            let Rate::Bitrate(kbps) = attempt.params.rate else { unreachable!() };
            let bits_per_frame = kbps / attempt.params.fps;
            Ok(1.0 - 1.0 / bits_per_frame - self.motion * (1.0 - attempt.params.fps / 30.0))
        }

        fn decided(&mut self, decision: Decision) {
            self.decisions.push(decision);
        }
    }

    fn test_plan(options: Options, duration: f64) -> Plan {
        let source = Probe {
            format: "mov,mp4".into(),
            codec: "h264".into(),
            width: 1280,
            height: 720,
            fps: Some(30.0),
            base_fps: Some(30.0),
            duration: Some(duration),
            start: 0.0,
            alpha: false,
            still_image: false,
            orientation: Default::default(),
            decoder: None,
        };
        let request =
            Request { options, ..Request::new(crate::input::Input::File("in.mp4".into())) };
        plan(&request, source).unwrap().0
    }

    fn options(fit: Fit) -> Options {
        Options { fit: Some(fit), ..Default::default() }
    }

    fn assert_good_fit(attempt: &Attempt) {
        let used = attempt.bytes as f64 / MAX_BYTES as f64;
        assert!((IN_BAND - 0.0005..=1.0).contains(&used), "uses {used:.3} of the limit");
    }

    #[test]
    fn says_what_it_encodes_first() {
        // whatever the search, what it says first is what it encodes first
        for fit in [Fit::Off, Fit::Auto, Fit::Bitrate, Fit::Crf, Fit::Fps, Fit::Length] {
            let plan = test_plan(Options { fit: Some(fit), ..Options::default() }, 4.84);
            let mut encoder = FakeEncoder::new(0.0);
            let _ = run(&plan, &mut encoder);
            assert_eq!(encoder.calls.first(), Some(&first_params(&plan)), "{fit:?}");
        }
    }

    #[test]
    fn bitrate_fit_converges_quickly() {
        for duration in [1.0, 3.0, 12.6, 60.0] {
            let plan = test_plan(options(Fit::Bitrate), duration);
            let mut encoder = FakeEncoder::new(0.0);
            let best = run(&plan, &mut encoder).unwrap().best;
            assert_good_fit(&best);
            assert!(encoder.calls.len() <= 3, "{duration} s took {:?}", encoder.calls);
        }
    }

    #[test]
    fn bitrate_fit_recovers_from_bad_start() {
        for bitrate in [1.0, 20_000.0] {
            let plan = test_plan(Options { bitrate: Some(bitrate), ..options(Fit::Bitrate) }, 3.0);
            let mut encoder = FakeEncoder::new(0.0);
            assert_good_fit(&run(&plan, &mut encoder).unwrap().best);
        }
    }

    #[test]
    fn reports_when_nothing_fits() {
        let fit_range = Some(Range { min: 2000.0, max: 4000.0 });
        let plan = test_plan(Options { fit_range, ..options(Fit::Bitrate) }, 3.0);
        let result = run(&plan, &mut FakeEncoder::new(0.0));
        assert!(matches!(result, Err(Error::NothingFits { .. })));
    }

    #[test]
    fn crf_fit_finds_smallest_fitting_value() {
        let plan = test_plan(options(Fit::Crf), 3.0);
        let mut encoder = FakeEncoder::new(0.0);
        let best = run(&plan, &mut encoder).unwrap().best;
        let Rate::Crf(crf) = best.params.rate else { panic!() };
        // one step better quality must not fit
        let better = encoder.encode(Params { rate: Rate::Crf(crf - 1), ..best.params }).unwrap();
        assert!(better.bytes > MAX_BYTES);
        assert!(encoder.calls.len() <= 8);
    }

    #[test]
    fn fps_and_length_fits_stay_in_range() {
        let plan = test_plan(Options { crf: Some(30), ..options(Fit::Fps) }, 3.0);
        let best = run(&plan, &mut FakeEncoder::new(0.0)).unwrap().best;
        assert!((1.0..=30.0).contains(&best.params.fps) && best.bytes <= MAX_BYTES);

        let plan = test_plan(Options { crf: Some(30), ..options(Fit::Length) }, 10.0);
        let best = run(&plan, &mut FakeEncoder::new(0.0)).unwrap().best;
        assert!(best.params.length < 10.0);
        assert_good_fit(&best);
    }

    #[test]
    fn auto_lowers_fps_for_long_static_videos() {
        let plan = test_plan(options(Fit::Auto), 30.0);
        assert_eq!(auto_frame_rates(&plan, MAX_BYTES), [30.0, 20.0, 15.0]);
        let mut encoder = FakeEncoder::new(0.0);
        let best = run(&plan, &mut encoder).unwrap().best;
        assert!(best.params.fps < 30.0);
        assert!(encoder.calls.len() as u32 <= plan.attempts);
        assert_good_fit(&best);
    }

    #[test]
    fn auto_keeps_fps_for_moving_videos() {
        let plan = test_plan(options(Fit::Auto), 30.0);
        let mut encoder = FakeEncoder::new(1.0);
        let best = run(&plan, &mut encoder).unwrap().best;
        assert_eq!(best.params.fps, 30.0);
        // stopped after the second frame rate looked worse
        let rates: Vec<_> = encoder.calls.iter().map(|p| p.fps).collect();
        assert!(!rates.contains(&15.0), "{rates:?}");
    }

    #[test]
    fn stops_once_it_looks_like_the_input() {
        // a simple picture, a fifth of the limit at any bitrate: raising
        // the bitrate or lowering the frame rate only wastes encodes
        for fit in [Fit::Auto, Fit::Bitrate] {
            let plan = test_plan(options(fit), 30.0);
            let mut encoder = FakeEncoder::simple(MAX_BYTES as f64 / 5.0, 0.999);
            let best = run(&plan, &mut encoder).unwrap().best;
            assert_eq!(encoder.calls.len(), 1, "{fit:?}: {:?}", encoder.calls);
            // scored once, not again to compare frame rates
            assert_eq!((encoder.scored, best.number), (1, 1), "{fit:?}");
        }
        // not alike enough: the search goes on, until the size shows it
        // takes no more of the bitrate
        let plan = test_plan(options(Fit::Bitrate), 3.0);
        let mut encoder = FakeEncoder::simple(MAX_BYTES as f64 / 5.0, 0.98);
        run(&plan, &mut encoder).unwrap();
        assert_eq!(encoder.calls.len(), 2);
    }

    #[test]
    fn auto_skips_fps_search_when_bits_are_plenty() {
        let plan = test_plan(options(Fit::Auto), 1.0);
        assert_eq!(auto_frame_rates(&plan, MAX_BYTES), [30.0]);
        let plan = test_plan(Options { fps: Some(24.0), ..options(Fit::Auto) }, 30.0);
        assert_eq!(auto_frame_rates(&plan, MAX_BYTES), [24.0]);
    }

    #[test]
    fn counts_a_share_shown_as_in_the_band_as_in_it() {
        let mut encoder = FakeEncoder::new(0.0);
        let f = Fitter::new(&mut encoder, 100_000, 8);
        // 95.96% shows as 96.0%
        assert!(f.in_band(95_960) && f.in_band(99_000) && f.in_band(100_000));
        assert!(!f.in_band(95_940) && !f.in_band(100_001));
    }

    #[test]
    fn rounding_stays_in_range() {
        let exact = Options {
            length: Some(2.996),
            crf: Some(50),
            fit_range: Some(Range { min: 2.996, max: 2.996 }),
            ..options(Fit::Length)
        };
        let plan = test_plan(exact, 10.0);
        let best = run(&plan, &mut FakeEncoder::new(0.0)).unwrap().best;
        assert_eq!(best.params.length, 2.996);

        let fit_range = Some(Range { min: 1.0, max: 1.04 });
        let plan = test_plan(Options { fit_range, ..options(Fit::Bitrate) }, 3.0);
        let mut encoder = FakeEncoder::new(0.0);
        run(&plan, &mut encoder).unwrap();
        for params in encoder.calls {
            let Rate::Bitrate(kbps) = params.rate else { panic!() };
            assert!((1.0..=1.04).contains(&kbps), "{kbps}");
        }
    }

    #[test]
    fn off_encodes_once() {
        let plan = test_plan(Options { crf: Some(10), ..options(Fit::Off) }, 3.0);
        let mut encoder = FakeEncoder::new(0.0);
        let best = run(&plan, &mut encoder).unwrap().best;
        assert_eq!(encoder.calls.len(), 1);
        assert_eq!(best.params.rate, Rate::Crf(10));
        assert!(best.bytes > MAX_BYTES);
    }

    /// An encoder given by functions of the bitrate and frame rate: the
    /// share of the limit an encode takes, and how like the input it looks.
    struct Scenario {
        calls: Vec<Params>,
        scored: u32,
        decisions: Vec<Decision>,
        share: Box<dyn Fn(f64, f64) -> f64>,
        alike: Box<dyn Fn(f64, f64) -> f64>,
        /// The conversion's token, which it says fits to as conversions do.
        token: crate::CancelToken,
        /// Asked to settle after the encode of this number, or while it
        /// encodes it, and whether that took.
        settle: Option<(u32, Settle)>,
        settled: Option<bool>,
    }

    #[derive(Clone, Copy, PartialEq)]
    enum Settle {
        After,
        During,
    }

    impl Scenario {
        fn new(
            share: impl Fn(f64, f64) -> f64 + 'static,
            alike: impl Fn(f64, f64) -> f64 + 'static,
        ) -> Self {
            let token = crate::CancelToken::new();
            token.begin_fitting();
            Self {
                calls: Vec::new(),
                scored: 0,
                decisions: Vec::new(),
                share: Box::new(share),
                alike: Box::new(alike),
                token,
                settle: None,
                settled: None,
            }
        }

        fn settling(self, number: u32, when: Settle) -> Self {
            Self { settle: Some((number, when)), ..self }
        }

        /// What it encoded, as frame rate @ bitrate.
        fn calls(&self) -> Vec<String> {
            let call = |params: &Params| match params.rate {
                Rate::Bitrate(kbps) => format!("{}@{kbps}", params.fps),
                rate => format!("{}@{rate:?}", params.fps),
            };
            self.calls.iter().map(call).collect()
        }

        fn decided(&self, wanted: impl Fn(&Decision) -> bool) -> bool {
            self.decisions.iter().any(wanted)
        }
    }

    impl Encoder for Scenario {
        fn encode(&mut self, params: Params) -> Result<Attempt> {
            self.calls.push(params);
            let number = self.calls.len() as u32;
            // asked while it encodes: ffmpeg is stopped, and fails
            if self.settle == Some((number, Settle::During)) {
                self.settled = Some(self.token.settle());
                return Err(Error::Cancelled);
            }
            let Rate::Bitrate(kbps) = params.rate else { unreachable!() };
            let bytes = ((self.share)(kbps, params.fps) * MAX_BYTES as f64) as u64;
            if bytes <= MAX_BYTES {
                self.token.allow_settling();
            }
            if self.settle == Some((number, Settle::After)) {
                self.settled = Some(self.token.settle());
            }
            Ok(Attempt { number, params, bytes, path: PathBuf::new() })
        }

        fn score(&mut self, attempt: &Attempt) -> Result<f64> {
            self.scored += 1;
            let Rate::Bitrate(kbps) = attempt.params.rate else { unreachable!() };
            Ok((self.alike)(kbps, attempt.params.fps))
        }

        fn decided(&mut self, decision: Decision) {
            self.decisions.push(decision);
        }

        fn settling(&self) -> bool {
            self.token.is_settling()
        }
    }

    /// Up to a share of noise either way, the same for the same bitrate.
    fn noise(kbps: f64) -> f64 {
        let mut x = kbps.to_bits().wrapping_mul(0x9E37_79B9_7F4A_7C15);
        x ^= x >> 29;
        (x % 2001) as f64 / 1000.0 - 1.0
    }

    /// The bitrate fitting starts from for `length` seconds.
    fn start(length: f64) -> f64 {
        round_to(estimate_bitrate(length, MAX_BYTES), 1)
    }

    /// Runs `fit = auto` on a clip `length` seconds long, 30 fps, through
    /// `encoder`.
    fn auto(length: f64, encoder: &mut Scenario) -> Attempt {
        let plan = test_plan(options(Fit::Auto), length);
        let best = run(&plan, encoder).unwrap().best;
        assert!(encoder.calls.len() as u32 <= plan.attempts, "{:?}", encoder.calls());
        best
    }

    #[test]
    fn keeps_the_owners_second_attempt_without_a_third() {
        // as reported: 416×512, 25.17 s; 78 kbit/s came to 95.7% and 81 to
        // 99.0% of the limit, 20 fps at 81 to 98.7%, and all looked 96.7%
        // like the input, where the search used to try 20 fps all the same
        let length = 25.17;
        let from = start(length);
        let mut encoder = Scenario::new(
            move |kbps, fps| 0.957 + (kbps - from) * 0.00846 - if fps < 30.0 { 0.003 } else { 0.0 },
            |_, _| 0.967,
        );
        let best = auto(length, &mut encoder);
        assert_eq!(encoder.calls.len(), 2, "{:?}", encoder.calls());
        assert_eq!(best.number, 2);
        assert_good_fit(&best);
        assert!(encoder.calls.iter().all(|params| params.fps == 30.0));
        // the frame rate kept as planned goes unsaid
        assert!(
            matches!(encoder.decisions[..], [Decision::InBand { attempt: 2, .. }]),
            "{:?}",
            encoder.decisions
        );
    }

    #[test]
    fn stops_at_a_first_attempt_in_the_band() {
        let length = 25.17;
        let from = start(length);
        let mut encoder = Scenario::new(move |kbps, _| 0.97 * kbps / from, |_, _| 0.97);
        let best = auto(length, &mut encoder);
        assert_eq!((encoder.calls.len(), best.number), (1, 1), "{:?}", encoder.calls());
        // scored once, to know whether fewer frames could look better
        assert_eq!(encoder.scored, 1);
    }

    #[test]
    fn keeps_the_frame_rate_where_looks_did_not_depend_on_bits() {
        // looks poor, but no better for more bits: fewer frames, each
        // with more, wouldn't look better either
        let length = 25.17;
        let from = start(length);
        let mut encoder = Scenario::new(move |kbps, _| 0.8 * kbps / from, |_, _| 0.9);
        let best = auto(length, &mut encoder);
        assert_eq!(encoder.calls.len(), 2, "{:?}", encoder.calls());
        assert_good_fit(&best);
        assert!(encoder.calls.iter().all(|params| params.fps == 30.0));
        assert!(
            matches!(encoder.decisions[..], [Decision::InBand { attempt: 2, .. }]),
            "{:?}",
            encoder.decisions
        );
    }

    #[test]
    fn lowers_the_frame_rate_where_nothing_fits_at_the_lowest_bitrate() {
        // each frame costs a share of the limit whatever the bitrate, so
        // that 30 of them a second never fit
        let length = 30.0;
        let from = start(length);
        let mut encoder = Scenario::new(
            move |kbps, fps| 1.05 * fps / 30.0 + 0.3 * kbps / from,
            |kbps, fps| 1.0 - 1.0 / (kbps / fps * 10.0),
        );
        let best = auto(length, &mut encoder);
        assert!(best.params.fps < 30.0, "{:?}", encoder.calls());
        assert!(encoder.decided(|d| matches!(d, Decision::LowersFrameRateToFit { .. })));
        assert!(encoder.decided(|d| matches!(d, Decision::TooLargeAtLeast { .. })));
    }

    #[test]
    fn lowers_the_frame_rate_where_it_looks_poor_and_better_lower() {
        let length = 30.0;
        let from = start(length);
        let mut encoder =
            Scenario::new(move |kbps, _| 0.958 * kbps / from, |kbps, fps| 1.0 - 1.0 / (kbps / fps));
        let best = auto(length, &mut encoder);
        assert!(best.params.fps < 30.0, "{:?}", encoder.calls());
        assert_good_fit(&best);
        assert!(encoder.decided(|d| matches!(d, Decision::LowersFrameRateForLooks { .. })));
    }

    #[test]
    fn copes_with_sizes_that_dont_always_grow() {
        let length = 25.17;
        let from = start(length);
        let mut encoder = Scenario::new(
            move |kbps, _| 0.9 * kbps / from * (1.0 + 0.06 * (kbps / 3.0).sin()),
            |_, _| 0.97,
        );
        let best = auto(length, &mut encoder);
        assert!(best.bytes <= MAX_BYTES && encoder.calls.len() <= 3, "{:?}", encoder.calls());
    }

    #[test]
    fn copes_with_noisy_sizes() {
        let length = 25.17;
        let from = start(length);
        let mut encoder = Scenario::new(
            move |kbps, _| 0.9 * kbps / from * (1.0 + 0.03 * noise(kbps)),
            |_, _| 0.97,
        );
        let best = auto(length, &mut encoder);
        assert_good_fit(&best);
        assert!(encoder.calls.len() <= 3, "{:?}", encoder.calls());
    }

    #[test]
    fn says_why_it_stops() {
        // every search ends with a reason, the last thing it decides
        for fit in [Fit::Auto, Fit::Bitrate, Fit::Crf, Fit::Fps, Fit::Length] {
            for (length, attempts) in [(3.0, 8), (30.0, 8), (30.0, 1)] {
                let options = Options { crf: Some(30), attempts: Some(attempts), ..options(fit) };
                let plan = test_plan(options, length);
                let mut encoder = FakeEncoder::new(0.0);
                let _ = run(&plan, &mut encoder);
                let last = encoder.decisions.last();
                let says = |d: &&Decision| !matches!(d, Decision::LowersFrameRateForLooks { .. });
                assert!(last.filter(says).is_some(), "{fit:?} {length}: {:?}", encoder.decisions);
            }
        }
    }

    /// Runs `fit = bitrate` from `bitrate` in the range `from`..=`to`
    /// kbit/s, on a clip 3 seconds long, through `encoder`.
    fn bitrate(bitrate: f64, (from, to): (f64, f64), encoder: &mut Scenario) -> Result<Fitted> {
        let fit_range = Some(Range { min: from, max: to });
        let options = Options { bitrate: Some(bitrate), fit_range, ..options(Fit::Bitrate) };
        run(&test_plan(options, 3.0), encoder)
    }

    #[test]
    fn encodes_the_least_before_saying_nothing_fits() {
        // sizes that flatten out at high bitrates, but fall away at low
        // ones: the line through the first two says even 1 kbit/s comes
        // to 110%, where it comes to 83%
        let share = |kbps: f64, _| 0.8 + 0.3 * (1.0 - (-kbps / 10.0).exp());
        let mut encoder = Scenario::new(share, |_, _| 0.9);
        let fitted = bitrate(100.0, (1.0, 1000.0), &mut encoder).unwrap();
        let calls = encoder.calls();
        assert_eq!(calls[..3], ["30@100", "30@89.5", "30@1"], "{calls:?}");
        // something fits, at the least if nothing else
        assert!(fitted.best.bytes <= MAX_BYTES, "{calls:?}");
        assert!(!encoder.decided(|d| matches!(d, Decision::TooLargeAtLeast { .. })));
    }

    #[test]
    fn encodes_the_least_where_rounding_leaves_nothing_else() {
        // 2 kbit/s just over the limit: the next, a little under it,
        // rounds to 2 again, which says nothing of 1 kbit/s, which fits
        let mut encoder = Scenario::new(|kbps, _| 0.9 + 0.0505 * kbps, |_, _| 0.9);
        let fitted = bitrate(2.0, (1.0, 1000.0), &mut encoder).unwrap();
        let calls = encoder.calls();
        assert_eq!(calls[..2], ["30@2", "30@1"], "{calls:?}");
        assert_good_fit(&fitted.best);
    }

    #[test]
    fn says_nothing_fits_once_the_least_is_encoded() {
        // never under 105%: the least is encoded at once, as predicted
        // not to fit, and shows it
        let mut encoder = Scenario::new(|kbps, _| 1.05 + kbps / 1000.0, |_, _| 0.9);
        let fitted = bitrate(100.0, (1.0, 1000.0), &mut encoder);
        assert!(matches!(fitted, Err(Error::NothingFits { .. })), "{fitted:?}");
        let calls = encoder.calls();
        assert_eq!(calls.last().map(String::as_str), Some("30@1"), "{calls:?}");
        let least = calls.len() as u32;
        let said = |d: &Decision| matches!(d, Decision::TooLargeAtLeast { attempt, .. } if *attempt == least);
        assert!(encoder.decided(said), "{:?}", encoder.decisions);
    }

    /// Fits with 80% of the limit at the first bitrate, and looks the same
    /// at any, settling at `number` as `when` says.
    fn settling(number: u32, when: Settle) -> (Fitted, Scenario) {
        let length = 25.17;
        let from = start(length);
        let encoder = Scenario::new(move |kbps, _| 0.8 * kbps / from, |_, _| 0.9);
        let mut encoder = encoder.settling(number, when);
        let plan = test_plan(options(Fit::Auto), length);
        (run(&plan, &mut encoder).unwrap(), encoder)
    }

    #[test]
    fn settles_on_the_first_attempt_once_it_fits() {
        let (fitted, encoder) = settling(1, Settle::After);
        assert_eq!(encoder.settled, Some(true));
        assert!(fitted.settled);
        // no more encodes, nor scoring the one kept
        assert_eq!((fitted.best.number, encoder.calls.len(), encoder.scored), (1, 1, 0));
        assert_eq!(encoder.decisions.last(), Some(&Decision::Settled { attempt: 1 }));
    }

    #[test]
    fn settles_on_the_best_before_the_attempt_it_gives_up() {
        // asked while the second encodes, which fails as ffmpeg is stopped
        let (fitted, encoder) = settling(2, Settle::During);
        assert_eq!(encoder.settled, Some(true));
        assert!(fitted.settled);
        assert_eq!((fitted.best.number, encoder.calls.len()), (1, 2));
        assert_eq!(encoder.decisions.last(), Some(&Decision::Settled { attempt: 1 }));
    }

    #[test]
    fn cant_settle_before_anything_fits() {
        // nothing fits at 30 fps: asked to settle then, it says it can't,
        // and fitting goes on as if never asked
        let length = 30.0;
        let from = start(length);
        let share = move |kbps, fps| 1.05 * fps / 30.0 + 0.3 * kbps / from;
        let alike = |kbps: f64, fps: f64| 1.0 - 1.0 / (kbps / fps * 10.0);
        let plan = test_plan(options(Fit::Auto), length);
        let mut asked = Scenario::new(share, alike).settling(1, Settle::After);
        let fitted = run(&plan, &mut asked).unwrap();
        assert_eq!(asked.settled, Some(false));
        let mut never = Scenario::new(share, alike);
        let unasked = run(&plan, &mut never).unwrap();
        assert!(!fitted.settled && fitted.best == unasked.best);
        assert_eq!(asked.calls, never.calls);
        assert!(!asked.decided(|d| matches!(d, Decision::Settled { .. })));
    }

    #[test]
    fn settles_on_the_rate_that_looked_best_so_far() {
        // 20 fps fits in the band, but isn't yet shown to look better
        // than 30 fps did: 30 fps is kept
        let length = 30.0;
        let from = start(length);
        let encoder =
            Scenario::new(move |kbps, _| 0.958 * kbps / from, |kbps, fps| 1.0 - 1.0 / (kbps / fps));
        let mut encoder = encoder.settling(3, Settle::After);
        let plan = test_plan(options(Fit::Auto), length);
        let fitted = run(&plan, &mut encoder).unwrap();
        assert!(fitted.settled, "{:?}", encoder.calls());
        assert_eq!((fitted.best.number, fitted.best.params.fps), (2, 30.0));
        assert_eq!(encoder.calls.len(), 3);
    }
}
