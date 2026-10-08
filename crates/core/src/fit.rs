//! Searching for encoder settings that land just under the size limit.
//!
//! The search only talks to an [`Encoder`], so it can be tested without
//! ffmpeg.

use std::path::PathBuf;

use crate::convert::{Plan, estimate_bitrate};
use crate::error::{Error, Result};
use crate::events::{Params, Rate};
use crate::options::Fit;

/// Stop searching once a fitting attempt uses this much of the limit.
const GOOD_ENOUGH: f64 = 0.96;
/// Aim a bit under the limit, since sizes are only roughly predictable.
const AIM: f64 = 0.985;
/// Below this many bits per pixel and frame, `fit = auto` tries lower frame
/// rates to give each frame more bits.
const AUTO_FPS_BPP: f64 = 0.12;
/// `fit = auto` does not go below this frame rate.
const AUTO_MIN_FPS: f64 = 8.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Attempt {
    pub number: u32,
    pub params: Params,
    pub bytes: u64,
    pub path: PathBuf,
}

pub trait Encoder {
    fn encode(&mut self, params: Params) -> Result<Attempt>;
    /// Similarity to the source, higher is better.
    fn score(&mut self, attempt: &Attempt) -> Result<f64>;
}

/// Outcome of searching one value.
#[derive(Default)]
struct Search {
    /// Best attempt that fits.
    best: Option<Attempt>,
    /// Size of the smallest attempt, to report when nothing fits.
    smallest: Option<u64>,
    used: u32,
}

impl Search {
    fn record(&mut self, attempt: &Attempt) {
        self.used += 1;
        self.smallest = Some(self.smallest.map_or(attempt.bytes, |s| s.min(attempt.bytes)));
    }
}

/// Finds the largest `x` in `[min, max]` whose encode fits, assuming size
/// grows roughly in proportion to `x` (bitrate, length).
fn maximize(
    encoder: &mut dyn Encoder,
    budget: u32,
    (min, max): (f64, f64),
    initial: f64,
    limit: u64,
    quantize: impl Fn(f64) -> f64,
    params: impl Fn(f64) -> Params,
) -> Result<Search> {
    let limit_f = limit as f64;
    let aim = limit_f * AIM;
    let mut search = Search::default();
    // largest fitting and smallest non-fitting points, as (x, bytes)
    let mut below: Option<(f64, u64)> = None;
    let mut above: Option<(f64, u64)> = None;
    // rounding must not leave the range, use the exact value if it would
    let snap = |x: f64| {
        let x = x.clamp(min, max);
        let rounded = quantize(x);
        if (min..=max).contains(&rounded) { rounded } else { x }
    };
    let mut x = snap(initial);

    while search.used < budget {
        let attempt = encoder.encode(params(x))?;
        search.record(&attempt);
        if attempt.bytes <= limit {
            if below.is_none_or(|(bx, _)| x > bx) {
                below = Some((x, attempt.bytes));
                search.best = Some(attempt.clone());
            }
            if attempt.bytes as f64 >= limit_f * GOOD_ENOUGH {
                break;
            }
        } else if above.is_none_or(|(ax, _)| x < ax) {
            above = Some((x, attempt.bytes));
        }

        let next = match (below, above) {
            (Some((bx, bs)), Some((ax, as_))) => {
                let t = (aim - bs as f64) / (as_ as f64 - bs as f64);
                // stay clear of both ends so every step makes progress
                bx + t.clamp(0.15, 0.85) * (ax - bx)
            }
            (Some((bx, bs)), None) if bx < max => bx * (aim / bs as f64).clamp(1.05, 4.0),
            (None, Some((ax, as_))) if ax > min => ax * (aim / as_ as f64).clamp(0.25, 0.95),
            _ => break,
        };
        let next = snap(next);
        let tried = |p: Option<(f64, u64)>| p.is_some_and(|(px, _)| px == next);
        if next == x || tried(below) || tried(above) {
            break;
        }
        x = next;
    }
    Ok(search)
}

/// Finds the best integer in `[min, max]` whose encode fits. With
/// `increasing`, size grows with the value and the largest fitting value is
/// best (fps); otherwise the smallest is (CRF).
fn bisect(
    encoder: &mut dyn Encoder,
    budget: u32,
    (min, max): (i64, i64),
    increasing: bool,
    limit: u64,
    params: impl Fn(i64) -> Params,
) -> Result<Search> {
    let mut search = Search::default();
    let (mut lo, mut hi) = (min, max);
    while lo <= hi && search.used < budget {
        let mid = lo + (hi - lo) / 2;
        let attempt = encoder.encode(params(mid))?;
        search.record(&attempt);
        let fits = attempt.bytes <= limit;
        let good_enough = fits && attempt.bytes as f64 >= limit as f64 * GOOD_ENOUGH;
        if fits {
            search.best = Some(attempt);
        }
        match (fits, increasing) {
            (true, true) | (false, false) => lo = mid + 1,
            (true, false) | (false, true) => hi = mid - 1,
        }
        if good_enough {
            break;
        }
    }
    Ok(search)
}

/// Rounds to `decimals` places, dividing last so results print cleanly.
fn round_to(value: f64, decimals: i32) -> f64 {
    let scale = 10f64.powi(decimals);
    (value * scale).round() / scale
}

/// Frame rates `fit = auto` tries, best first.
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

fn fit_bitrate(
    encoder: &mut dyn Encoder,
    plan: &Plan,
    budget: u32,
    fps: f64,
    initial: f64,
    limit: u64,
) -> Result<Search> {
    let range = (plan.fit_range.min, plan.fit_range.max);
    let length = plan.length;
    maximize(
        encoder,
        budget,
        range,
        initial,
        limit,
        |kbps| round_to(kbps, 1),
        |kbps| Params { fps, length, rate: Rate::Bitrate(kbps) },
    )
}

/// Picks the frame rate and bitrate that look best, scoring with SSIM.
fn fit_auto(encoder: &mut dyn Encoder, plan: &Plan, limit: u64) -> Result<Search> {
    let rates = auto_frame_rates(plan, limit);
    let mut total = Search::default();
    let mut best_score = None;
    let mut initial = estimate_bitrate(plan.length);

    for (i, &fps) in rates.iter().enumerate() {
        let remaining = plan.attempts.saturating_sub(total.used);
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
            break;
        }

        let search = fit_bitrate(encoder, plan, budget, fps, initial, limit)?;
        total.used += search.used;
        total.smallest = match (total.smallest, search.smallest) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        let Some(candidate) = search.best else { continue };
        if let Rate::Bitrate(kbps) = candidate.params.rate {
            initial = kbps;
        }
        if rates.len() == 1 {
            total.best = Some(candidate);
            break;
        }

        let score = encoder.score(&candidate)?;
        match best_score {
            // a lower frame rate looking worse means even lower ones will too
            Some(best) if score <= best => break,
            _ => {
                best_score = Some(score);
                total.best = Some(candidate);
            }
        }
    }
    Ok(total)
}

/// Runs the search configured in `plan` and returns the attempt to keep.
/// With `fit = off` that is the single attempt, even if it is too big.
pub fn run(plan: &Plan, encoder: &mut dyn Encoder, limit: u64) -> Result<Attempt> {
    let (fps, length, budget) = (plan.fps, plan.length, plan.attempts);
    let quality = if plan.lossless { Rate::Lossless } else { Rate::Crf(plan.crf) };
    let range = plan.fit_range;

    let search = match plan.fit {
        Fit::Off => {
            let rate = match () {
                _ if plan.lossless => Rate::Lossless,
                _ if plan.constant_quality => Rate::Crf(plan.crf),
                _ => Rate::Bitrate(plan.bitrate),
            };
            return encoder.encode(Params { fps, length, rate });
        }
        Fit::Auto => fit_auto(encoder, plan, limit)?,
        Fit::Bitrate => fit_bitrate(encoder, plan, budget, fps, plan.bitrate, limit)?,
        Fit::Crf => bisect(
            encoder,
            budget,
            (range.min.ceil() as i64, range.max.floor() as i64),
            false,
            limit,
            |crf| Params { fps, length, rate: Rate::Crf(crf as u8) },
        )?,
        Fit::Fps => bisect(
            encoder,
            budget,
            (range.min.ceil() as i64, range.max.floor() as i64),
            true,
            limit,
            |fps| Params { fps: fps as f64, length, rate: quality },
        )?,
        Fit::Length => maximize(
            encoder,
            budget,
            (range.min, range.max),
            range.max,
            limit,
            |secs| round_to(secs, 2),
            |secs| Params { fps, length: secs, rate: quality },
        )?,
    };

    search.best.ok_or(Error::NothingFits { smallest: search.smallest.unwrap_or(0), limit })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert::{Request, plan};
    use crate::ffmpeg::Probe;
    use crate::options::{Options, Range};
    use crate::telegram::MAX_BYTES;

    /// Pretends to encode: bitrate encodes overshoot their target by 3%,
    /// constant quality encodes shrink 9% per CRF step. SSIM rewards bits per
    /// frame and penalizes dropped frames.
    struct FakeEncoder {
        calls: Vec<Params>,
        motion: f64,
    }

    impl FakeEncoder {
        fn new(motion: f64) -> Self {
            Self { calls: Vec::new(), motion }
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
            Ok(Attempt {
                number: self.calls.len() as u32,
                params,
                bytes: bytes as u64,
                path: PathBuf::new(),
            })
        }

        fn score(&mut self, attempt: &Attempt) -> Result<f64> {
            let Rate::Bitrate(kbps) = attempt.params.rate else { unreachable!() };
            let bits_per_frame = kbps / attempt.params.fps;
            Ok(1.0 - 1.0 / bits_per_frame - self.motion * (1.0 - attempt.params.fps / 30.0))
        }
    }

    fn test_plan(options: Options, duration: f64) -> Plan {
        let source = Probe {
            format: "mov,mp4".into(),
            codec: "h264".into(),
            width: 1280,
            height: 720,
            fps: Some(30.0),
            duration: Some(duration),
            alpha: false,
            still_image: false,
            orientation: Default::default(),
            decoder: None,
        };
        let request = Request { options, ..Request::new("in.mp4".into()) };
        plan(&request, source).unwrap().0
    }

    fn options(fit: Fit) -> Options {
        Options { fit: Some(fit), ..Default::default() }
    }

    fn assert_good_fit(attempt: &Attempt) {
        let used = attempt.bytes as f64 / MAX_BYTES as f64;
        assert!((GOOD_ENOUGH..=1.0).contains(&used), "uses {used:.3} of the limit");
    }

    #[test]
    fn bitrate_fit_converges_quickly() {
        for duration in [1.0, 3.0, 12.6, 60.0] {
            let plan = test_plan(options(Fit::Bitrate), duration);
            let mut encoder = FakeEncoder::new(0.0);
            let best = run(&plan, &mut encoder, MAX_BYTES).unwrap();
            assert_good_fit(&best);
            assert!(encoder.calls.len() <= 3, "{duration} s took {:?}", encoder.calls);
        }
    }

    #[test]
    fn bitrate_fit_recovers_from_bad_start() {
        for bitrate in [1.0, 20_000.0] {
            let plan = test_plan(Options { bitrate: Some(bitrate), ..options(Fit::Bitrate) }, 3.0);
            let mut encoder = FakeEncoder::new(0.0);
            assert_good_fit(&run(&plan, &mut encoder, MAX_BYTES).unwrap());
        }
    }

    #[test]
    fn reports_when_nothing_fits() {
        let fit_range = Some(Range { min: 2000.0, max: 4000.0 });
        let plan = test_plan(Options { fit_range, ..options(Fit::Bitrate) }, 3.0);
        let result = run(&plan, &mut FakeEncoder::new(0.0), MAX_BYTES);
        assert!(matches!(result, Err(Error::NothingFits { .. })));
    }

    #[test]
    fn crf_fit_finds_smallest_fitting_value() {
        let plan = test_plan(options(Fit::Crf), 3.0);
        let mut encoder = FakeEncoder::new(0.0);
        let best = run(&plan, &mut encoder, MAX_BYTES).unwrap();
        let Rate::Crf(crf) = best.params.rate else { panic!() };
        // one step better quality must not fit
        let better = encoder.encode(Params { rate: Rate::Crf(crf - 1), ..best.params }).unwrap();
        assert!(better.bytes > MAX_BYTES);
        assert!(encoder.calls.len() <= 8);
    }

    #[test]
    fn fps_and_length_fits_stay_in_range() {
        let plan = test_plan(Options { crf: Some(30), ..options(Fit::Fps) }, 3.0);
        let best = run(&plan, &mut FakeEncoder::new(0.0), MAX_BYTES).unwrap();
        assert!((1.0..=30.0).contains(&best.params.fps) && best.bytes <= MAX_BYTES);

        let plan = test_plan(Options { crf: Some(30), ..options(Fit::Length) }, 10.0);
        let best = run(&plan, &mut FakeEncoder::new(0.0), MAX_BYTES).unwrap();
        assert!(best.params.length < 10.0);
        assert_good_fit(&best);
    }

    #[test]
    fn auto_lowers_fps_for_long_static_videos() {
        let plan = test_plan(options(Fit::Auto), 30.0);
        assert_eq!(auto_frame_rates(&plan, MAX_BYTES), [30.0, 20.0, 15.0]);
        let mut encoder = FakeEncoder::new(0.0);
        let best = run(&plan, &mut encoder, MAX_BYTES).unwrap();
        assert!(best.params.fps < 30.0);
        assert!(encoder.calls.len() as u32 <= plan.attempts);
        assert_good_fit(&best);
    }

    #[test]
    fn auto_keeps_fps_for_moving_videos() {
        let plan = test_plan(options(Fit::Auto), 30.0);
        let mut encoder = FakeEncoder::new(1.0);
        let best = run(&plan, &mut encoder, MAX_BYTES).unwrap();
        assert_eq!(best.params.fps, 30.0);
        // stopped after the second frame rate looked worse
        let rates: Vec<_> = encoder.calls.iter().map(|p| p.fps).collect();
        assert!(!rates.contains(&15.0), "{rates:?}");
    }

    #[test]
    fn auto_skips_fps_search_when_bits_are_plenty() {
        let plan = test_plan(options(Fit::Auto), 1.0);
        assert_eq!(auto_frame_rates(&plan, MAX_BYTES), [30.0]);
        let plan = test_plan(Options { fps: Some(24.0), ..options(Fit::Auto) }, 30.0);
        assert_eq!(auto_frame_rates(&plan, MAX_BYTES), [24.0]);
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
        let best = run(&plan, &mut FakeEncoder::new(0.0), MAX_BYTES).unwrap();
        assert_eq!(best.params.length, 2.996);

        let fit_range = Some(Range { min: 1.0, max: 1.04 });
        let plan = test_plan(Options { fit_range, ..options(Fit::Bitrate) }, 3.0);
        let mut encoder = FakeEncoder::new(0.0);
        run(&plan, &mut encoder, MAX_BYTES).unwrap();
        for params in encoder.calls {
            let Rate::Bitrate(kbps) = params.rate else { panic!() };
            assert!((1.0..=1.04).contains(&kbps), "{kbps}");
        }
    }

    #[test]
    fn off_encodes_once() {
        let plan = test_plan(Options { crf: Some(10), ..options(Fit::Off) }, 3.0);
        let mut encoder = FakeEncoder::new(0.0);
        let best = run(&plan, &mut encoder, MAX_BYTES).unwrap();
        assert_eq!(encoder.calls.len(), 1);
        assert_eq!(best.params.rate, Rate::Crf(10));
        assert!(best.bytes > MAX_BYTES);
    }
}
