//! End-to-end `.tgs` sticker pipeline: normalise, encode, lay out, compress,
//! validate, and apply lossy reductions until it fits if needed.

use std::cell::RefCell;
use std::num::NonZeroUsize;

use crate::frames::Animation;
use serde::{Deserialize, Serialize};

use crate::check::{self, Issue, Severity};
use crate::encode::{Effort, Settings, paint};
use crate::layout::{self, lay_out};
use crate::limits::{MAX_RAW_JSON, telegram};
use crate::lottie::Style;
use crate::normalise::{self, PixelAnim, Report, normalise};
use crate::reduce::{Compromise, Kind, Reduction};
use crate::scene::Scene;
use crate::{Result, file, mark};

mod fit;
mod tracker;

use fit::{Fitting, Quick, RETARGET, ZOPFLI_BEST, ZOPFLI_SHARE, fit, likely_scale, quick};
use tracker::{Part, Tracker};

/// How [`make`] handles lossless output that is too large (over `max_bytes` or
/// Telegram's other limits).
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Fit {
    /// Keep lossless when it fits; otherwise apply minimal lossy reductions
    /// until it fits.
    #[default]
    Auto,
    /// Never reduce; return the oversized lossless sticker with its issues.
    Exact,
}

/// Options for [`make`]. Construct with `Options { fit, ..Options::default() }`
/// so future fields retain defaults.
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    /// Normalisation options.
    pub normalise: normalise::Options,
    /// Encoder search effort level.
    pub effort: Effort,
    /// Whether the art may change to fit.
    pub fit: Fit,
    /// Allowed reduction kinds when `fit` is [`Fit::Auto`].
    pub reductions: Vec<Kind>,
    /// Quality dimension sacrificed first when reducing.
    pub compromise: Compromise,
    /// Target compressed `.tgs` size limit in bytes.
    pub max_bytes: usize,
    /// Optional Lottie animation name (`nm`).
    pub name: Option<String>,
    /// Optional payload bytes embedded via rectangle order (or first layer
    /// `ln`); set by `tgradish-core`, `None` disables embedding.
    pub mark: Option<Vec<u8>>,
    /// Maximum worker threads for parallel candidate fitting when the
    /// `threads` feature is enabled (`None` uses
    /// [`std::thread::available_parallelism`], `1` runs on the calling
    /// thread). Output is identical regardless of thread count.
    pub threads: Option<NonZeroUsize>,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            normalise: normalise::Options::default(),
            effort: Effort::default(),
            fit: Fit::default(),
            reductions: Kind::ALL.to_vec(),
            compromise: Compromise::default(),
            max_bytes: telegram::MAX_BYTES,
            name: None,
            mark: None,
            threads: None,
        }
    }
}

/// A lossy reduction step applied during fitting.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct Step {
    /// Reduction and strength applied.
    #[serde(flatten)]
    pub reduction: Reduction,
    /// Estimated `.tgs` size in bytes after this step.
    pub bytes: usize,
    /// Perceptual distortion relative to the lossless animation (see
    /// [`crate::reduce::distortion`]).
    pub distortion: f64,
}

impl Step {
    /// Constructs a [`Step`] (for front-end display or tests).
    pub fn new(reduction: Reduction, bytes: usize, distortion: f64) -> Step {
        Step { reduction, bytes, distortion }
    }
}

/// Progress events emitted by [`make`].
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "stage", rename_all = "kebab-case", rename_all_fields = "kebab-case")]
#[non_exhaustive]
pub enum Progress {
    /// Input animation has been normalised onto its pixel grid.
    Normalised {
        /// Normalisation summary.
        report: Box<Report>,
    },
    /// Estimated lossless output exceeds the size limit.
    TooLarge {
        /// Estimated compressed size in bytes.
        bytes: usize,
    },
    /// Fitting applied a lossy reduction step.
    Reduced {
        /// Applied step and resulting estimate.
        step: Step,
    },
    /// Compressing final Lottie JSON with zopfli (the longest step at
    /// [`Effort::Best`]).
    Packing,
    /// Final output still exceeded the limit because fitting's estimates were
    /// off; fitting retries with a lower target estimate and re-encodes and
    /// packs the result.
    Missed {
        /// Actual compressed size in bytes, or `None` if skipped as too large
        /// or over structural limits.
        bytes: Option<usize>,
        /// New lower byte target for fitting.
        aim: usize,
    },
    /// Monotonic overall and per-stage progress estimate (`0.0..=1.0`),
    /// reaching `1.0` when done. Each phase is weighted by its typical runtime
    /// share and advances by colours drawn, orders tried, reduction rounds, or
    /// elapsed zopfli packing time relative to typical duration.
    Advanced {
        /// Overall progress fraction (`0.0..=1.0`).
        fraction: f64,
        /// Current stage progress fraction (`0.0..=1.0`), resetting on
        /// [`Progress::Normalised`], fitting start (after
        /// [`Progress::TooLarge`], unless `fit` is [`Fit::Exact`]),
        /// [`Progress::Packing`], and each [`Progress::Missed`] retry.
        stage_fraction: f64,
    },
}

/// Built `.tgs` sticker and its metadata.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct Sticker {
    /// Gzipped `.tgs` bytes.
    #[serde(skip)]
    pub tgs: Vec<u8>,
    /// Compressed `.tgs` size in bytes.
    pub bytes: usize,
    /// Uncompressed Lottie JSON size in bytes.
    pub json_bytes: usize,
    /// Normalisation report.
    pub report: Report,
    /// Lossy reduction steps applied (empty when lossless).
    pub steps: Vec<Step>,
    /// Number of Lottie layers.
    pub layers: usize,
    /// Total paint groups across all layers.
    pub groups: usize,
    /// Total rectangles across all groups.
    pub rectangles: usize,
    /// Validation issues reported by [`check::check`].
    pub issues: Vec<Issue>,
    /// Validation statistics from [`check::check`].
    #[serde(skip)]
    pub stats: check::Stats,
    /// Final normalised animation after any reductions.
    #[serde(skip)]
    pub anim: PixelAnim,
}

impl Sticker {
    /// Returns true if any visual loss occurred (reductions, forced pixel
    /// snapping, or 3-second trimming).
    pub fn lossy(&self) -> bool {
        !self.steps.is_empty() || self.report.snapped_pixels > 0 || self.report.trimmed
    }

    /// Returns true if the sticker has no validation errors and Telegram will
    /// accept it.
    pub fn fits(&self) -> bool {
        self.issues.iter().all(|issue| issue.severity != Severity::Error)
    }
}

/// Cancellation predicate polled between work steps; may be called from
/// multiple threads simultaneously.
pub type Cancelled<'a> = &'a (dyn Fn() -> bool + Sync);

/// An animation encoded and laid out: its scene, its Lottie JSON, and
/// that packed by `gzip -9`.
struct Rendered {
    scene: Scene,
    json: String,
    packed: Vec<u8>,
}

/// Encodes and lays out `anim`, telling `done` how much of it is done.
fn render(
    anim: &PixelAnim,
    settings: &Settings,
    options: &Options,
    cancelled: Cancelled,
    done: &dyn Fn(f64),
) -> Result<Rendered> {
    let json = |scene: &Scene| {
        let stretch = options.normalise.square == normalise::Square::Stretch;
        let mut lottie = lay_out(scene, anim, options.name.clone(), stretch);
        if let Some(mark) = &options.mark {
            mark::embed(&mut lottie, mark);
        }
        lottie.to_json(Style::default())
    };
    // The smallest scene scored so far, laid out and packed: the encoder
    // keeps the smallest it scores, so this is nearly always the scene it
    // returns, and sometimes one it scores again. Comparing scenes is much
    // quicker than laying one out and packing it.
    let smallest: RefCell<Option<Rendered>> = RefCell::new(None);
    let score = |scene: &Scene| {
        if let Some(known) = &*smallest.borrow()
            && known.scene == *scene
        {
            return known.packed.len();
        }
        let json = json(scene);
        let packed = file::pack_quick(json.as_bytes());
        let size = packed.len();
        let mut smallest = smallest.borrow_mut();
        if smallest.as_ref().is_none_or(|known| size <= known.packed.len()) {
            *smallest = Some(Rendered { scene: scene.clone(), json, packed });
        }
        size
    };
    let scene = paint(anim, settings, Some(&score), cancelled, done)?;
    match smallest.into_inner() {
        Some(known) if known.scene == scene => Ok(known),
        _ => {
            let json = json(&scene);
            let packed = file::pack_quick(json.as_bytes());
            Ok(Rendered { scene, json, packed })
        }
    }
}

/// Whether Telegram's server and every app take a sticker this large,
/// leaving its packed size to the caller.
fn within_limits(scene: &Scene, json: &str) -> bool {
    layout::cost(scene) <= telegram::MAX_COST && json.len() <= MAX_RAW_JSON
}

/// How much of the server's cost limit or Telegram Desktop's JSON limit a
/// sticker takes, whichever is more.
fn share_of_limits(scene: &Scene, json: &str) -> f64 {
    f64::max(
        layout::cost(scene) as f64 / telegram::MAX_COST as f64,
        json.len() as f64 / MAX_RAW_JSON as f64,
    )
}

/// Builds a `.tgs` [`Sticker`] from `animation` (consuming it once normalised)
/// while reporting [`Progress`] events. Polls `cancelled` between steps and
/// colours, returning [`crate::Error::Cancelled`] if triggered.
///
/// With the `threads` feature enabled (no-op on wasm), candidate fitting runs
/// across up to [`Options::threads`] worker threads and zopfli runs on a
/// background thread so cancellation returns immediately:
///
/// - zopfli itself cannot be interrupted, so a cancelled pack thread continues
///   until finished in the background;
/// - at most two zopfli threads run process-wide (including abandoned ones) so
///   repeated cancellations cannot accumulate threads; additional calls wait
///   for a slot while still polling `cancelled`.
pub fn make(
    animation: Animation,
    options: &Options,
    progress: &mut dyn FnMut(Progress),
    cancelled: Cancelled,
) -> Result<Sticker> {
    let check = || if cancelled() { Err(crate::Error::Cancelled) } else { Ok(()) };
    let tracker = Tracker::new(progress, options.fit == Fit::Auto);
    let (original, report) = normalise(animation, &options.normalise)?;
    tracker.say(Progress::Normalised { report: Box::new(report.clone()) });
    let settings = Settings { effort: options.effort, ..Settings::default() };

    // what scales quick estimates to the real encoder's sizes, measured
    // once fitting comes near the limit
    let mut calibration = None;
    // the grid snapping aims for, worked out once: normalising again is
    // slow, and reductions don't change it
    let likely = match options.fit {
        Fit::Auto => likely_scale(&original, &report, &options.normalise),
        Fit::Exact => None,
    };
    let mut current = original.clone();
    // the quick encode of `current`, when making the sticker has one for
    // fitting to start from
    let mut current_quick = None;
    let mut steps: Vec<Step> = Vec::new();
    let mut target = options.max_bytes;
    // whether fitting got its estimate to the target, rather than running
    // out of reductions
    let mut reached = true;
    loop {
        if options.fit == Fit::Auto {
            let fitting = Fitting { target, likely, settings: &settings, options, cancelled };
            let known = current_quick.take();
            (current, reached) =
                fit(&original, current, known, &mut steps, &mut calibration, &fitting, &tracker)?;
        }
        tracker.fitted();
        check()?;
        // fitting may end on the art it calibrated on, already made by the
        // real encoder and zopfli
        let real = calibration
            .as_ref()
            .and_then(|calibration| calibration.real.as_ref())
            .filter(|real| real.anim == current);
        let rendered;
        let Rendered { scene, json, packed: quick_packed } = match real {
            Some(real) => &real.rendered,
            None => {
                let done = tracker.hook(Part::Rendering, 0.0, 1.0);
                rendered = render(&current, &settings, options, cancelled, &done)?;
                &rendered
            }
        };
        tracker.at(Part::Rendering, 1.0);
        // packing it as small as it goes is worth zopfli's time only when
        // that can fit
        let hopeless = !within_limits(scene, json)
            || quick_packed.len() as f64 * ZOPFLI_BEST > options.max_bytes as f64;
        // the estimate was off; aim lower and reduce some more, which only
        // helps while fitting has reductions left to make
        let mut retarget = || -> Result<bool> {
            // aiming at a quarter of the limit, it is far off the mark
            // whatever the estimates say, and stops there
            let again = options.fit == Fit::Auto && reached && target > options.max_bytes / 4;
            if !again {
                return Ok(false);
            }
            // fitting starts from this, so it is kept for it
            let known = match current_quick {
                Some(known) => known,
                None => *current_quick.insert(quick(&current, options, cancelled, &|_| {})?),
            };
            Ok(known.is_some())
        };
        if hopeless && retarget()? {
            target = ((target as f64) * RETARGET) as usize;
            tracker.missed(None, target, calibration.is_some());
            continue;
        }
        // too large as it is, kept so or with nothing for fitting to do;
        // short of that, only packing tells
        if hopeless && steps.is_empty() {
            let quick = Quick { packed: quick_packed.len(), share: share_of_limits(scene, json) };
            let bytes = quick.size(ZOPFLI_SHARE, options.max_bytes);
            tracker.say(Progress::TooLarge { bytes });
        }
        tracker.say(Progress::Packing);
        let tgs = match (hopeless, real) {
            (true, _) => quick_packed.clone(),
            (false, Some(real)) => real.tgs.clone(),
            (false, None) => {
                pack(json, options.effort, cancelled, &tracker.hook(Part::Packing, 0.0, 1.0))?
            }
        };
        check()?;
        let fits = tgs.len() <= options.max_bytes && within_limits(scene, json);
        if !fits && retarget()? {
            target = ((target as f64) * RETARGET) as usize;
            // within Telegram's other limits, as it wasn't hopeless: too
            // large by its size
            tracker.missed(Some(tgs.len()), target, calibration.is_some());
            continue;
        }
        check()?;
        let (stats, issues) =
            check::check(json.as_bytes(), Some(tgs.len())).expect("the writer's JSON parses");
        let groups = scene.layers.iter().map(|layer| layer.groups.len()).sum();
        let rectangles = scene
            .layers
            .iter()
            .flat_map(|layer| &layer.groups)
            .map(|group| group.shapes.len())
            .sum();
        tracker.finish();
        return Ok(Sticker {
            bytes: tgs.len(),
            json_bytes: json.len(),
            tgs,
            report,
            steps,
            layers: scene.layers.len(),
            groups,
            rectangles,
            issues,
            stats,
            anim: current,
        });
    }
}

/// Whether work is given to threads: with the `threads` feature, but on
/// wasm, where starting one panics. Features are the same for every target
/// of a build, so one that also builds for the web can have it on there.
const THREADS: bool = cfg!(all(feature = "threads", not(target_family = "wasm")));

/// `json` packed by zopfli, given up on when `cancelled` says so. zopfli
/// can't be stopped, so with [`THREADS`] it packs in a thread of its own,
/// left to finish alone; at most [`PACKING`] of them run at once in the
/// whole process, so stopping and starting again doesn't pile them up.
///
/// zopfli can't say how far it got either: with [`THREADS`], `done` is
/// told the time it has taken against about how long it takes, by
/// [`ZOPFLI_PACE`] and [`ZOPFLI_ITERATION`], creeping on once that is
/// past.
fn pack(json: &str, effort: Effort, cancelled: Cancelled, done: &dyn Fn(f64)) -> Result<Vec<u8>> {
    let check = || if cancelled() { Err(crate::Error::Cancelled) } else { Ok(()) };
    check()?;
    let iterations = zopfli_iterations(effort);
    let tgs = if THREADS {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::mpsc::{RecvTimeoutError, channel};
        static RUNNING: AtomicUsize = AtomicUsize::new(0);
        /// Counts a thread as running until it ends, however it ends.
        struct Running;
        impl Drop for Running {
            fn drop(&mut self) {
                RUNNING.fetch_sub(1, Ordering::SeqCst);
            }
        }
        while RUNNING
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |running| {
                (running < PACKING).then_some(running + 1)
            })
            .is_err()
        {
            check()?;
            std::thread::sleep(POLL);
        }
        let running = Running;
        // stopped while waiting: the slot is given back
        check()?;
        let (send, receive) = channel();
        let expected = iterations as f64 * (ZOPFLI_ITERATION + json.len() as f64 * ZOPFLI_PACE);
        let json = json.as_bytes().to_vec();
        let started = std::time::Instant::now();
        std::thread::spawn(move || {
            let _running = running;
            send.send(file::pack(&json, iterations))
        });
        loop {
            check()?;
            match receive.recv_timeout(POLL) {
                Ok(tgs) => break tgs,
                Err(RecvTimeoutError::Timeout) => {
                    done(packed_by(started.elapsed().as_secs_f64() / expected));
                }
                Err(RecvTimeoutError::Disconnected) => panic!("zopfli panicked"),
            }
        }
    } else {
        file::pack(json.as_bytes(), iterations)
    };
    check()?;
    Ok(tgs)
}

/// How many threads zopfli packs in at once, at most, in the whole process:
/// one at work, and one left to finish after a stop.
const PACKING: usize = 2;

/// About how long zopfli takes over a byte of JSON in one iteration, and
/// over the rest of an iteration, in seconds: on the reference art on a
/// desktop of 2025 it took from two thirds to twice this, more over
/// smaller art.
const ZOPFLI_PACE: f64 = 0.2e-6;
const ZOPFLI_ITERATION: f64 = 4e-3;

/// How far packing is taken to have got, from 0 to 1, by `time` it has
/// taken as a share of how long it is expected to: as much until four
/// fifths, then creeping on ever more slowly, never all the way, as zopfli
/// takes several times longer over some art.
fn packed_by(time: f64) -> f64 {
    match time {
        time if time <= 0.8 => time.max(0.0),
        time => (1.0 - 0.2 * (-(time - 0.8) / 0.6).exp()).min(0.999),
    }
}

/// How many threads [`parallel`] works in with `threads` asked for: as
/// many, or as many as the machine runs at once, which a round's few
/// candidates seldom reach; one without [`THREADS`].
fn workers(threads: Option<NonZeroUsize>) -> usize {
    if !THREADS {
        return 1;
    }
    threads.or_else(|| std::thread::available_parallelism().ok()).map_or(1, NonZeroUsize::get)
}

/// `f` of each item, in order, in at most `workers` threads at once, each
/// taking the next item left; in the calling thread with one. `f` tells
/// the hook it is given how much of its item is done, and `done` is told,
/// in the calling thread, how much of all of them is: as `f` tells it with
/// one thread, and every [`POLL`] while threads work.
fn parallel<T: Sync, R: Send>(
    items: &[T],
    workers: usize,
    done: &dyn Fn(f64),
    f: impl Fn(&T, &dyn Fn(f64)) -> R + Sync,
) -> Vec<R> {
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
    // how much of each item is done, the bits of an f64
    let shares: Vec<AtomicU64> = items.iter().map(|_| AtomicU64::new(0)).collect();
    let all = || {
        let sum: f64 =
            shares.iter().map(|share| f64::from_bits(share.load(Ordering::Relaxed))).sum();
        sum / items.len().max(1) as f64
    };
    let set = |index: usize, share: f64| shares[index].store(share.to_bits(), Ordering::Relaxed);
    if workers <= 1 || items.len() <= 1 {
        let each = items.iter().enumerate().map(|(index, item)| {
            let result = f(item, &|share| {
                set(index, share);
                done(all());
            });
            set(index, 1.0);
            done(all());
            result
        });
        return each.collect();
    }
    let next = AtomicUsize::new(0);
    // each thread wakes the calling one as it ends, which otherwise wakes
    // to say how far they got; one that panics is found ended all the same
    let ended = AtomicUsize::new(0);
    let caller = std::thread::current();
    let mut results: Vec<(usize, R)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..workers.min(items.len()))
            .map(|_| {
                scope.spawn(|| {
                    let mut finished = Vec::new();
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(item) = items.get(index) else { break };
                        finished.push((index, f(item, &|share| set(index, share))));
                        set(index, 1.0);
                    }
                    ended.fetch_add(1, Ordering::SeqCst);
                    caller.unpark();
                    finished
                })
            })
            .collect();
        while ended.load(Ordering::SeqCst) < workers.len()
            && !workers.iter().all(|worker| worker.is_finished())
        {
            std::thread::park_timeout(POLL);
            done(all());
        }
        workers.into_iter().flat_map(|worker| worker.join().expect("no panics")).collect()
    });
    done(1.0);
    results.sort_by_key(|(index, _)| *index);
    results.into_iter().map(|(_, result)| result).collect()
}

/// How often what waits on other threads says how far they got, and
/// packing asks whether to stop: the GUI can't tell a stop noticed within
/// this apart from one at once.
const POLL: std::time::Duration = std::time::Duration::from_millis(50);

internal! {
/// Returns the zopfli iteration count used for `effort`.
pub fn zopfli_iterations(effort: Effort) -> u64 {
    match effort {
        Effort::Fast => 5,
        Effort::Balanced => 15,
        Effort::Best => 40,
    }
}
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::frames::Frame;

    use super::fit::{Calibration, normalise_likely_scale};
    use super::*;
    use crate::encode::EncodeError;

    /// A quick estimate scaled by `calibration`.
    fn estimate(anim: &PixelAnim, options: &Options, calibration: f64) -> Option<usize> {
        let quick = quick(anim, options, &|| false, &|_| {}).unwrap();
        quick.map(|quick| quick.size(calibration, options.max_bytes))
    }

    /// Noise in many colours, 64 by 64 in 4 frames: far over the limit
    /// as it is.
    fn noise() -> Animation {
        let mut seed = 7u32;
        let frames = (0..4)
            .map(|_| Frame {
                rgba: (0..64 * 64)
                    .flat_map(|_| {
                        seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                        let colour = (seed >> 16) % 200;
                        [colour as u8, (colour * 7) as u8, (colour * 13) as u8, 255]
                    })
                    .collect(),
                duration: Duration::from_millis(100),
            })
            .collect();
        Animation::new(64, 64, frames).unwrap()
    }

    #[test]
    fn gives_up_with_nothing_left_to_reduce() {
        // fitting with no reductions to make comes to keeping it as it is,
        // rather than aiming lower and encoding again some 34 times
        let input = noise();
        let lossless = Options { fit: Fit::Exact, effort: Effort::Fast, ..Options::default() };
        let started = std::time::Instant::now();
        let kept = make(input.clone(), &lossless, &mut |_| {}, &|| false).unwrap();
        let once = started.elapsed();
        let options = Options { fit: Fit::Auto, reductions: Vec::new(), ..lossless };
        let started = std::time::Instant::now();
        let fitted = make(input.clone(), &options, &mut |_| {}, &|| false).unwrap();
        let took = started.elapsed();
        assert!(fitted.tgs == kept.tgs && fitted.steps.is_empty());
        assert!(took < once * 4 + Duration::from_secs(1), "{took:?} against {once:?}");
    }

    #[test]
    fn fits_alike_in_any_number_of_threads() {
        // limits that leave fitting several rounds of candidates to try,
        // more than two of them at a time
        let files = tgradish_corpus::files();
        for (name, max_bytes) in
            [("walker.gif", 600), ("sunrise.gif", 1000), ("waterfall.gif", 2300)]
        {
            let file = files.iter().find(|file| file.name == name).unwrap();
            let input =
                crate::frames::decode(&file.bytes, &Default::default(), &Default::default())
                    .unwrap();
            let made = |threads: Option<usize>| {
                let threads = threads.and_then(NonZeroUsize::new);
                let options =
                    Options { max_bytes, threads, effort: Effort::Fast, ..Options::default() };
                make(input.clone(), &options, &mut |_| {}, &|| false).unwrap()
            };
            let alone = made(Some(1));
            assert!(alone.steps.len() > 1, "{name}: {:?}", alone.steps);
            for threads in [Some(2), None] {
                let again = made(threads);
                assert!(again.tgs == alone.tgs, "{name} in {threads:?} threads");
                assert_eq!(again.steps, alone.steps, "{name} in {threads:?} threads");
            }
        }
        // in order, however many at once
        let items: Vec<usize> = (0..9).collect();
        for workers in [1, 2, 4, 16] {
            assert_eq!(
                parallel(&items, workers, &|_| {}, |&item, _| item * 2),
                [0, 2, 4, 6, 8, 10, 12, 14, 16]
            );
        }
    }

    #[test]
    fn says_how_far_it_got_only_ever_further() {
        let walker = tgradish_corpus::files().into_iter().find(|f| f.name == "walker.gif").unwrap();
        let input =
            crate::frames::decode(&walker.bytes, &Default::default(), &Default::default()).unwrap();
        // kept as it is, and fitted under a limit that takes several
        // rounds, in one thread and in several
        let exact = Options { fit: Fit::Exact, effort: Effort::Best, ..Options::default() };
        let fitted = |threads| Options {
            fit: Fit::Auto,
            max_bytes: 600,
            threads: NonZeroUsize::new(threads),
            ..exact.clone()
        };
        for options in [exact.clone(), fitted(1), fitted(4)] {
            // (fraction, stage fraction, stages started before it)
            let mut said: Vec<(f64, f64, usize)> = Vec::new();
            let (mut stages, mut missed) = (0, 0);
            let mut progress = |event| match event {
                Progress::Advanced { fraction, stage_fraction } => {
                    said.push((fraction, stage_fraction, stages))
                }
                Progress::Normalised { .. } | Progress::TooLarge { .. } => stages += 1,
                // a try again is a stage of its own, compressing too
                Progress::Packing if missed == 0 => stages += 1,
                Progress::Missed { .. } => (stages, missed) = (stages + 1, missed + 1),
                Progress::Packing | Progress::Reduced { .. } => {}
            };
            let made = make(input.clone(), &options, &mut progress, &|| false).unwrap();
            let fit = options.fit;
            assert_eq!(made.steps.is_empty(), fit == Fit::Exact, "{fit:?}");
            // the walker under so low a limit misses it once, and tries again
            assert_eq!(missed, usize::from(fit == Fit::Auto), "{fit:?}");
            assert!(said.len() > 20, "{fit:?}: {said:?}");
            assert!(said.windows(2).all(|pair| pair[0].0 <= pair[1].0), "{fit:?}: {said:?}");
            assert!(said.iter().all(|&(_, stage, _)| (0.0..=1.0).contains(&stage)), "{said:?}");
            // each stage only ever further too
            for pair in said.windows(2).filter(|pair| pair[0].2 == pair[1].2) {
                assert!(pair[0].1 <= pair[1].1, "{fit:?}: {said:?}");
            }
            assert_eq!(said.last().map(|&(fraction, ..)| fraction), Some(1.0), "{fit:?}");
            // and no more than about once in a thousandth
            assert!(said.len() < 2500, "{fit:?}: {}", said.len());
        }
    }

    #[test]
    fn tells_how_far_packing_is_by_the_time_it_takes() {
        assert_eq!(packed_by(0.0), 0.0);
        assert!((packed_by(0.5) - 0.5).abs() < 1e-9);
        // past four fifths ever more slowly, never all the way
        let later: Vec<f64> = [0.8, 1.0, 2.0, 3.0].into_iter().map(packed_by).collect();
        assert!(later.windows(2).all(|pair| pair[0] < pair[1]), "{later:?}");
        assert!(later[1] - later[0] < 0.2 && packed_by(1000.0) < 1.0, "{later:?}");
    }

    #[test]
    fn packs_art_that_cant_fit_quickly() {
        // noise in many colours, kept as it is: far over the limit
        let input = noise();
        let options = Options { fit: Fit::Exact, effort: Effort::Fast, ..Options::default() };
        let mut too_large = None;
        let made = make(
            input.clone(),
            &options,
            &mut |event| {
                if let Progress::TooLarge { bytes } = event {
                    too_large = Some(bytes);
                }
            },
            &|| false,
        )
        .unwrap();
        // said so before packing, and packed with gzip rather than zopfli
        assert!(too_large.is_some_and(|bytes| bytes > options.max_bytes), "{too_large:?}");
        assert!(!made.fits() && made.steps.is_empty());
        let mut json = Vec::new();
        std::io::Read::read_to_end(&mut flate2::read::GzDecoder::new(&made.tgs[..]), &mut json)
            .unwrap();
        assert_eq!(made.tgs, file::pack_quick(&json));
    }

    #[test]
    fn fits_what_only_zopfli_packs_small_enough() {
        // a checkerboard, which zopfli packs a quarter smaller than gzip
        let frames = (0..2)
            .map(|frame| Frame {
                rgba: (0..48 * 48)
                    .flat_map(|i| match (i % 48 + i / 48 + frame) % 2 {
                        0 => [255, 0, 0, 255],
                        _ => [0, 0, 255, 255],
                    })
                    .collect(),
                duration: Duration::from_millis(100),
            })
            .collect();
        let input = Animation::new(48, 48, frames).unwrap();
        let lossless = Options { fit: Fit::Exact, effort: Effort::Fast, ..Options::default() };
        let made = make(input.clone(), &lossless, &mut |_| {}, &|| false).unwrap();
        let json = file::unpack(&made.tgs).unwrap();
        let gzip = file::pack_quick(&json).len();
        assert!((made.bytes as f64) < gzip as f64 * 0.85, "{} of {gzip}", made.bytes);
        // at a limit only zopfli's size fits, it fits as it is either way
        for fit in [Fit::Exact, Fit::Auto] {
            let options = Options { fit, max_bytes: made.bytes, ..lossless.clone() };
            let mut too_large = false;
            let mut progress = |event| too_large |= matches!(event, Progress::TooLarge { .. });
            let again = make(input.clone(), &options, &mut progress, &|| false).unwrap();
            assert_eq!((again.bytes, again.steps.len(), too_large), (made.bytes, 0, false));
        }
    }

    #[test]
    fn stops_inside_an_encode() {
        let walker = tgradish_corpus::files().into_iter().find(|f| f.name == "walker.gif").unwrap();
        let input =
            crate::frames::decode(&walker.bytes, &Default::default(), &Default::default()).unwrap();
        let (anim, _) = normalise(input.clone(), &normalise::Options::default()).unwrap();
        // between colours, whatever the effort
        let best = Settings { effort: Effort::Best, ..Settings::default() };
        let stopped = crate::encode::painter(&anim, &best, None, &|| true);
        assert!(matches!(stopped, Err(EncodeError::Cancelled)), "{stopped:?}");
        // and making a sticker ends there too, after a few questions
        let asked = std::sync::atomic::AtomicUsize::new(0);
        let cancelled = || asked.fetch_add(1, std::sync::atomic::Ordering::Relaxed) >= 3;
        let made = make(input.clone(), &Options::default(), &mut |_| {}, &cancelled);
        assert!(matches!(made, Err(crate::Error::Cancelled)), "{:?}", made.map(|s| s.bytes));
        // stopped while packing, with nothing left to encode: no sticker
        let packing = std::sync::atomic::AtomicBool::new(false);
        let stopped_packing = || packing.load(std::sync::atomic::Ordering::Relaxed);
        let mut progress = |event: Progress| {
            if matches!(event, Progress::Packing) {
                packing.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        };
        let made = make(input.clone(), &Options::default(), &mut progress, &stopped_packing);
        assert!(matches!(made, Err(crate::Error::Cancelled)), "{:?}", made.map(|s| s.bytes));
    }

    #[test]
    fn takes_the_likely_scale_from_normalising_when_it_is_the_same() {
        let crop = normalise::Options { square: normalise::Square::Crop, ..Default::default() };
        let canvas = normalise::Options { keep_canvas: true, ..Default::default() };
        let forced = normalise::Options { pixel_scale: Some(2), ..Default::default() };
        for file in tgradish_corpus::files() {
            let input =
                crate::frames::decode(&file.bytes, &Default::default(), &Default::default())
                    .unwrap();
            for options in [&normalise::Options::default(), &crop, &canvas, &forced] {
                let (anim, report) = normalise(input.clone(), options).unwrap();
                let again = normalise_likely_scale(&anim);
                assert_eq!(likely_scale(&anim, &report, options), again, "{}", file.name);
            }
        }
    }

    #[test]
    fn estimates_json_over_desktops_limit_as_too_large() {
        let input = Animation::new(
            2,
            2,
            vec![Frame { rgba: vec![255; 16], duration: Duration::from_millis(100) }],
        )
        .unwrap();
        let (anim, _) = normalise(input.clone(), &normalise::Options::default()).unwrap();
        let named = |bytes: usize| Options { name: Some("x".repeat(bytes)), ..Options::default() };
        let small = estimate(&anim, &named(0), 1.0).unwrap();
        assert!(small < 1000, "{small}");
        // the name packs to almost nothing, but the JSON is too large
        let over = estimate(&anim, &named(MAX_RAW_JSON + 100_000), 1.0).unwrap();
        let further = estimate(&anim, &named(MAX_RAW_JSON + 200_000), 1.0).unwrap();
        assert!(telegram::MAX_BYTES < over && over < further, "{over} {further}");
    }

    #[test]
    fn gives_up_what_the_compromise_puts_first() {
        // speckled art of six colours sliding right a pixel a frame: both
        // fewer frames and a coarser picture make it smaller
        const SIDE: u32 = 32;
        let colours = [[230, 40, 40], [40, 40, 230], [40, 200, 60], [240, 220, 40], [20, 20, 20]];
        let frames = (0..16)
            .map(|frame| {
                let rgba = (0..SIDE * SIDE)
                    .flat_map(|index| {
                        let (x, y) = ((index % SIDE + SIDE - frame) % SIDE, index / SIDE);
                        let hash = (x * 7 + y * 13 + x * y * 3) % 11;
                        match colours.get(hash as usize) {
                            Some(&[r, g, b]) => [r, g, b, 255],
                            None => [255; 4],
                        }
                    })
                    .collect();
                Frame { rgba, duration: Duration::from_millis(100) }
            })
            .collect();
        let input = Animation::new(SIDE, SIDE, frames).unwrap();
        let normalise_options = normalise::Options { keep_canvas: true, ..Default::default() };
        let (anim, _) = normalise(input.clone(), &normalise_options).unwrap();
        let lossless = estimate(&anim, &Options::default(), 1.0).unwrap();
        let fitted = |compromise: Compromise| {
            let options = Options { compromise, ..Options::default() };
            let mut steps = Vec::new();
            let settings = Settings::default();
            let fitting = Fitting {
                target: lossless / 20,
                likely: normalise_likely_scale(&anim),
                settings: &settings,
                options: &options,
                cancelled: &|| false,
            };
            let mut calibration = Some(Calibration { factor: 1.0, real: None });
            let (current, mut progress) = (anim.clone(), |_| {});
            let tracker = Tracker::new(&mut progress, true);
            fit(&anim, current, None, &mut steps, &mut calibration, &fitting, &tracker).unwrap();
            assert!(!steps.is_empty());
            steps.iter().map(|step| step.reduction.kind().motion()).collect::<Vec<bool>>()
        };
        // left to itself, fitting coarsens this picture; asked to, it gives
        // up frames, and detail only once fewer frames stop helping
        assert!(!fitted(Compromise::Auto)[0]);
        let motion = fitted(Compromise::Motion);
        assert!(motion[0] && motion.is_sorted_by_key(|&motion| !motion), "{motion:?}");
        assert!(motion.contains(&false), "{motion:?}");
        let detail = fitted(Compromise::Detail);
        assert!(!detail[0] && detail.is_sorted(), "{detail:?}");
    }

    #[test]
    fn never_fits_art_by_blanking_it() {
        // lone pixels, one in each 4x4 block, moving a pixel a frame:
        // snapping or downscaling them to blocks leaves nothing, the
        // smallest sticker of all
        const SIDE: u32 = 32;
        let frames = (0..4)
            .map(|frame| {
                let rgba = (0..SIDE * SIDE)
                    .flat_map(|index| {
                        let (x, y) = (index % SIDE, index / SIDE);
                        match x % 4 == frame && y % 4 == frame {
                            true => [230, 40, 40, 255],
                            false => [0; 4],
                        }
                    })
                    .collect();
                Frame { rgba, duration: Duration::from_millis(100) }
            })
            .collect();
        let input = Animation::new(SIDE, SIDE, frames).unwrap();
        let normalise_options = normalise::Options { keep_canvas: true, ..Default::default() };
        let (anim, _) = normalise(input.clone(), &normalise_options).unwrap();
        let options = Options { effort: Effort::Fast, ..Options::default() };
        let settings = Settings { effort: Effort::Fast, ..Settings::default() };
        let fitting = Fitting {
            target: 1,
            likely: Some(4),
            settings: &settings,
            options: &options,
            cancelled: &|| false,
        };
        let (mut steps, mut calibration) =
            (Vec::new(), Some(Calibration { factor: 1.0, real: None }));
        let (current, mut progress) = (anim.clone(), |_| {});
        let tracker = Tracker::new(&mut progress, true);
        let (fitted, reached) =
            fit(&anim, current, None, &mut steps, &mut calibration, &fitting, &tracker).unwrap();
        // every reduction used up, as far as it gets without blanking it
        assert!(!reached && !steps.is_empty());
        assert!(fitted.shows(), "{steps:?}");
    }

    #[test]
    fn estimates_shapes_over_the_servers_limit_as_too_large() {
        // isolated pixels on one of the four cells of every 2x2 block, a
        // different one in each frame, red and blue by turns: 8100
        // rectangles a frame that nothing can merge or reuse
        const SIDE: u32 = 180;
        let frames = (0..4)
            .map(|frame| {
                let rgba = (0..SIDE * SIDE)
                    .flat_map(|index| {
                        let (x, y) = (index % SIDE, index / SIDE);
                        let on = x % 2 == frame % 2 && y % 2 == frame / 2;
                        match (on, (x / 2 + y / 2) % 2) {
                            (false, _) => [0; 4],
                            (true, 0) => [230, 40, 40, 255],
                            (true, _) => [40, 40, 230, 255],
                        }
                    })
                    .collect();
                Frame { rgba, duration: Duration::from_millis(100) }
            })
            .collect();
        let input = Animation::new(SIDE, SIDE, frames).unwrap();
        let normalise_options = normalise::Options { keep_canvas: true, ..Default::default() };
        let (anim, _) = normalise(input.clone(), &normalise_options).unwrap();
        let options = Options::default();
        let quick = Settings { effort: Effort::Fast, ..Settings::default() };
        let Rendered { scene, json, .. } =
            render(&anim, &quick, &options, &|| false, &|_| {}).unwrap();
        assert!(layout::cost(&scene) > telegram::MAX_COST, "{}", layout::cost(&scene));
        assert!(!within_limits(&scene, &json));
        // with no weight on the packed size, what is left is the cost
        assert!(estimate(&anim, &options, 0.0).unwrap() > telegram::MAX_BYTES);
    }
}
