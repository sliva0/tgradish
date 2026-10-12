//! From frames to a finished `.tgs`: normalise, encode, lay out, check,
//! and when it is too large, reduce until it fits.

use serde::{Deserialize, Serialize};
use tgradish_frames::Animation;

use crate::check::{self, Issue, Severity};
use crate::encode::{Effort, EncodeError, Settings, painter};
use crate::layout::{self, lay_out};
use crate::limits::{MAX_RAW_JSON, telegram};
use crate::lottie::Style;
use crate::normalise::{self, PixelAnim, Report, normalise};
use crate::reduce::{Compromise, Kind, Reduction, error};
use crate::scene::Scene;
use crate::{Result, file, mark};

/// What to do when the lossless result is too large.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
    /// Lossless when it fits, otherwise the least visible reductions that
    /// make it fit.
    #[default]
    Auto,
    /// Never reduce; a result that is too large is reported.
    Lossless,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    pub normalise: normalise::Options,
    pub effort: Effort,
    pub fit: Fit,
    /// The reductions fitting may use.
    pub reductions: Vec<Kind>,
    /// Which of them fitting uses first.
    pub compromise: Compromise,
    /// The largest `.tgs` to make.
    pub max_bytes: usize,
    /// Written into the sticker as its name.
    pub name: Option<String>,
    /// Hidden in the order of its rectangles (see [`crate::mark`]).
    pub mark: Option<Vec<u8>>,
    pub style: Style,
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
            style: Style::default(),
        }
    }
}

/// A reduction fitting applied.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Step {
    #[serde(flatten)]
    pub reduction: Reduction,
    /// The estimated size after it, in bytes.
    pub bytes: usize,
    /// How much the animation differs from the lossless one after it (see
    /// [`crate::reduce::error`]).
    pub error: f64,
}

/// Progress, for front-ends to show.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum Progress {
    Normalised {
        report: Box<Report>,
    },
    /// The lossless result, estimated, is too large.
    TooLarge {
        bytes: usize,
    },
    Reduced {
        step: Step,
    },
    Packing,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Sticker {
    #[serde(skip)]
    pub tgs: Vec<u8>,
    pub bytes: usize,
    pub json_bytes: usize,
    pub report: Report,
    /// Reductions applied, in order; empty when lossless.
    pub steps: Vec<Step>,
    pub layers: usize,
    pub groups: usize,
    pub rectangles: usize,
    /// Problems the finished sticker has, from [`check::check`].
    pub issues: Vec<Issue>,
    /// What the sticker shows, cell for cell: the animation after any
    /// reductions.
    #[serde(skip)]
    pub anim: PixelAnim,
}

impl Sticker {
    /// Whether anything visible was given up: fitting reductions, a forced
    /// pixel scale, or the end of a long input.
    pub fn lossy(&self) -> bool {
        !self.steps.is_empty() || self.report.snapped_pixels > 0 || self.report.trimmed
    }

    /// Whether Telegram takes it.
    pub fn fits(&self) -> bool {
        self.issues.iter().all(|issue| issue.severity != Severity::Error)
    }
}

/// zopfli packs about this much smaller than `gzip -9`; estimates lean
/// high.
const ZOPFLI_SHARE: f64 = 0.93;
/// zopfli packs no smaller than this share of `gzip -9`: past it, a
/// sticker can't fit however long zopfli works on it. Regular patterns
/// come down to about 0.75.
const ZOPFLI_BEST: f64 = 0.5;
/// When the real size misses an estimate, the next target is this much
/// lower.
const RETARGET: f64 = 0.96;
/// Estimates are measured against the real encoder once fitting comes
/// within this many times the limit, on the art as it is by then: farther
/// over it, fitting shrinks the art a lot anyway, and the real encoder's
/// search of all of it can take minutes.
const CALIBRATE_WITHIN: f64 = 2.0;

/// Asked between steps: when it says yes, making a sticker stops. It may
/// be asked from several threads at once.
pub type Cancelled<'a> = &'a (dyn Fn() -> bool + Sync);

/// Encodes and lays out `anim`.
fn render(
    anim: &PixelAnim,
    settings: &Settings,
    options: &Options,
    cancelled: Cancelled,
) -> Result<(Scene, String)> {
    let json = |scene: &Scene| {
        let mut lottie = lay_out(scene, anim, options.name.clone());
        if let Some(mark) = &options.mark {
            mark::embed(&mut lottie, mark);
        }
        lottie.to_json(options.style)
    };
    let score = |scene: &Scene| file::quick_size(json(scene).as_bytes());
    let scene = painter(anim, settings, Some(&score), cancelled).map_err(|err| match err {
        EncodeError::Cancelled => crate::Error::Cancelled,
        err => err.into(),
    })?;
    let json = json(&scene);
    Ok((scene, json))
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

/// What a quick encode of an animation comes to before it is scaled to
/// the real encoder: its size packed by gzip, and its share of the
/// server's cost limit or Telegram Desktop's JSON limit, whichever is more.
#[derive(Debug, Clone, Copy)]
struct Quick {
    packed: usize,
    share: f64,
}

impl Quick {
    /// The estimated size of the `.tgs`: the packed size scaled by
    /// `calibration`, or the share of `max_bytes` when that is more, so
    /// reductions that shrink the cost or the JSON count as progress even
    /// before it fits.
    fn size(self, calibration: f64, max_bytes: usize) -> usize {
        (self.packed as f64 * calibration).max(self.share * max_bytes as f64) as usize
    }
}

/// Encodes `anim` quickly: `None` when it can't be made at all.
fn quick(anim: &PixelAnim, options: &Options, cancelled: Cancelled) -> Result<Option<Quick>> {
    let fast = Settings { effort: Effort::Fast, ..Settings::default() };
    match render(anim, &fast, options, cancelled) {
        Ok((scene, json)) => Ok(Some(Quick {
            packed: file::quick_size(json.as_bytes()),
            share: share_of_limits(&scene, &json),
        })),
        Err(crate::Error::Cancelled) => Err(crate::Error::Cancelled),
        Err(_) => Ok(None),
    }
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
) -> Result<f64> {
    match render(anim, settings, options, cancelled) {
        Ok((_, real)) => {
            Ok(pack(&real, settings.effort, cancelled)?.len() as f64 / quick.packed.max(1) as f64)
        }
        Err(crate::Error::Cancelled) => Err(crate::Error::Cancelled),
        Err(_) => Ok(ZOPFLI_SHARE),
    }
}

/// Makes a sticker of `animation`, reporting `progress`. `cancelled` is
/// asked between steps, and while encoding between colours; when it says
/// yes, this stops with [`Error::Cancelled`](crate::Error::Cancelled).
pub fn make(
    animation: &Animation,
    options: &Options,
    progress: &mut dyn FnMut(Progress),
    cancelled: Cancelled,
) -> Result<Sticker> {
    let check = || if cancelled() { Err(crate::Error::Cancelled) } else { Ok(()) };
    // the lossless size is said once, by whichever finds it too large first
    let mut said_too_large = false;
    let progress = &mut |event: Progress| {
        if matches!(event, Progress::TooLarge { .. }) {
            if said_too_large {
                return;
            }
            said_too_large = true;
        }
        progress(event);
    };
    let (original, report) = normalise(animation, &options.normalise)?;
    progress(Progress::Normalised { report: Box::new(report.clone()) });
    let settings = Settings { effort: options.effort, ..Settings::default() };

    // what scales quick estimates to the real encoder's sizes, measured
    // once fitting comes near the limit
    let mut calibration = None;
    let mut current = original.clone();
    let mut steps: Vec<Step> = Vec::new();
    let mut target = options.max_bytes;
    loop {
        if options.fit == Fit::Auto {
            let fitting = Fitting { target, settings: &settings, options, cancelled };
            current = fit(&original, current, &mut steps, &mut calibration, &fitting, progress)?;
        }
        check()?;
        let (scene, json) = render(&current, &settings, options, cancelled)?;
        let quick_packed = file::pack_quick(json.as_bytes());
        // packing it as small as it goes is worth zopfli's time only when
        // that can fit
        let hopeless = !within_limits(&scene, &json)
            || quick_packed.len() as f64 * ZOPFLI_BEST > options.max_bytes as f64;
        // the estimate was off; aim lower and reduce some more
        let retarget = || -> Result<bool> {
            let again = options.fit == Fit::Auto && target > options.max_bytes / 4;
            Ok(again && quick(&current, options, cancelled)?.is_some())
        };
        if hopeless && retarget()? {
            target = ((target as f64) * RETARGET) as usize;
            continue;
        }
        // too large as it is, kept so or with nothing for fitting to do;
        // short of that, only packing tells
        if hopeless && steps.is_empty() {
            let quick = Quick { packed: quick_packed.len(), share: share_of_limits(&scene, &json) };
            progress(Progress::TooLarge { bytes: quick.size(ZOPFLI_SHARE, options.max_bytes) });
        }
        progress(Progress::Packing);
        let tgs = match hopeless {
            true => quick_packed,
            false => pack(&json, options.effort, cancelled)?,
        };
        check()?;
        let fits = tgs.len() <= options.max_bytes && within_limits(&scene, &json);
        if !fits && retarget()? {
            target = ((target as f64) * RETARGET) as usize;
            continue;
        }
        check()?;
        let (_, issues) =
            check::check(json.as_bytes(), Some(tgs.len())).expect("the writer's JSON parses");
        let groups = scene.layers.iter().map(|layer| layer.groups.len()).sum();
        let rectangles = scene
            .layers
            .iter()
            .flat_map(|layer| &layer.groups)
            .map(|group| group.shapes.len())
            .sum();
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
            anim: current,
        });
    }
}

/// `json` packed by zopfli, given up on when `cancelled` says so. zopfli
/// can't be stopped, so with the `threads` feature it packs in a thread of
/// its own, left to finish alone.
fn pack(json: &str, effort: Effort, cancelled: Cancelled) -> Result<Vec<u8>> {
    let iterations = zopfli_iterations(effort);
    #[cfg(feature = "threads")]
    let tgs = {
        use std::sync::mpsc::{RecvTimeoutError, channel};
        let (send, receive) = channel();
        let json = json.as_bytes().to_vec();
        std::thread::spawn(move || send.send(file::pack(&json, iterations)));
        loop {
            if cancelled() {
                return Err(crate::Error::Cancelled);
            }
            match receive.recv_timeout(std::time::Duration::from_millis(50)) {
                Ok(tgs) => break tgs,
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => panic!("zopfli panicked"),
            }
        }
    };
    #[cfg(not(feature = "threads"))]
    let tgs = file::pack(json.as_bytes(), iterations);
    if cancelled() { Err(crate::Error::Cancelled) } else { Ok(tgs) }
}

/// `f` of each item, in threads of their own with the `threads` feature.
fn parallel<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    #[cfg(feature = "threads")]
    {
        std::thread::scope(|scope| {
            let handles: Vec<_> = items.iter().map(|item| scope.spawn(|| f(item))).collect();
            handles.into_iter().map(|handle| handle.join().expect("no panics")).collect()
        })
    }
    #[cfg(not(feature = "threads"))]
    {
        items.iter().map(f).collect()
    }
}

fn zopfli_iterations(effort: Effort) -> u64 {
    match effort {
        Effort::Fast => 5,
        Effort::Balanced => 15,
        Effort::Best => 40,
    }
}

/// A reduced animation, its quick encode, its estimated size and its
/// error.
type Candidate = (PixelAnim, Option<Quick>, usize, f64);

/// What fitting aims at and works with.
struct Fitting<'a> {
    /// The estimate to get to.
    target: usize,
    /// The real encoder's, for calibrating.
    settings: &'a Settings,
    options: &'a Options,
    cancelled: Cancelled<'a>,
}

/// Reduces `current` until its estimate is at most the target: each round
/// takes the reduction that saves the most bytes for the least change,
/// among those the compromise puts first while any of them help, then the
/// last one is weakened as far as it still fits. Estimates are scaled by
/// `calibration`, which is measured on the art as it is once it comes near
/// the target.
fn fit(
    original: &PixelAnim,
    mut current: PixelAnim,
    steps: &mut Vec<Step>,
    calibration: &mut Option<f64>,
    fitting: &Fitting,
    progress: &mut dyn FnMut(Progress),
) -> Result<PixelAnim> {
    let Fitting { target, settings, options, cancelled } = *fitting;
    let size_of = |quick: Option<Quick>, calibration: Option<f64>| {
        quick.map_or(usize::MAX, |quick| {
            quick.size(calibration.unwrap_or(ZOPFLI_SHARE), options.max_bytes)
        })
    };
    // measures the calibration once the art comes near the target
    let near =
        |quick: Option<Quick>| size_of(quick, None) as f64 <= target as f64 * CALIBRATE_WITHIN;
    let measure = |anim: &PixelAnim, quick: Option<Quick>| -> Result<Option<f64>> {
        match quick {
            Some(quick) if near(Some(quick)) => {
                calibrate(anim, quick, settings, options, cancelled).map(Some)
            }
            _ => Ok(None),
        }
    };
    let mut quick_now = quick(&current, options, cancelled)?;
    // fits even as gzip packs the quick encode: the real encoder and
    // zopfli only make it smaller
    if quick_now.is_some_and(|quick| quick.size(1.0, options.max_bytes) <= target) {
        return Ok(current);
    }
    if calibration.is_none() {
        *calibration = measure(&current, quick_now)?;
    }
    let mut size = size_of(quick_now, *calibration);
    if size <= target {
        return Ok(current);
    }
    if steps.is_empty() {
        progress(Progress::TooLarge { bytes: size });
    }
    let likely = steps
        .iter()
        .all(|step| step.reduction.kind() != Kind::SnapToGrid)
        .then(|| normalise_likely_scale(original))
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
    let mut error_now = steps.last().map_or(0.0, |step| step.error);
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
        let now = *calibration;
        let tries = parallel(&kinds, |&k| -> Result<Option<Candidate>> {
            let candidate = ladders[k][next[k]].apply(&current)?;
            let candidate_quick = quick(&candidate, options, cancelled)?;
            let candidate_size = size_of(candidate_quick, now);
            if candidate == current || candidate_size >= size {
                return Ok(None);
            }
            let candidate_error = error(original, &candidate);
            Ok(Some((candidate, candidate_quick, candidate_size, candidate_error)))
        });
        // (value, kind, candidate)
        let mut best: Option<(f64, usize, Candidate)> = None;
        for (&k, tried) in kinds.iter().zip(tries) {
            let Some(candidate) = tried? else {
                // no help at this strength; the next round tries stronger
                next[k] += 1;
                continue;
            };
            let value = (size - candidate.2) as f64 / (candidate.3 - error_now).max(1e-9);
            if best.as_ref().is_none_or(|b| value > b.0) {
                best = Some((value, k, candidate));
            }
        }
        let Some((_, k, (candidate, candidate_quick, candidate_size, candidate_error))) = best
        else {
            continue;
        };
        before_last = Some((current, k));
        current = candidate;
        quick_now = candidate_quick;
        size = candidate_size;
        error_now = candidate_error;
        // near the target now: sizes measured against the real encoder
        // decide how much further to go
        if calibration.is_none() {
            *calibration = measure(&current, quick_now)?;
            size = size_of(quick_now, *calibration);
        }
        let step = Step { reduction: ladders[k][next[k]], bytes: size, error: error_now };
        progress(Progress::Reduced { step: step.clone() });
        steps.push(step);
        next[k] += 1;
    }

    // weaken the last step as far as the result still fits
    if let Some((previous, k)) = before_last
        && size <= target
        && (size as f64) < target as f64 * 0.9
    {
        let last = steps.last().unwrap().reduction;
        let weaker = next[k].checked_sub(2).map(|level| ladders[k][level]);
        let (mut low, mut high) = (0.0, 1.0);
        for _ in 0..5 {
            let share = (low + high) / 2.0;
            let Some(reduction) = last.between(weaker.as_ref(), share) else { break };
            let candidate = reduction.apply(&previous)?;
            let candidate_size = size_of(quick(&candidate, options, cancelled)?, *calibration);
            if candidate_size <= target {
                high = share;
                let candidate_error = error(original, &candidate);
                *steps.last_mut().unwrap() =
                    Step { reduction, bytes: candidate_size, error: candidate_error };
                current = candidate;
            } else {
                low = share;
            }
        }
    }
    Ok(current)
}

/// The scale snapping would use: the likely scale normalising found.
fn normalise_likely_scale(anim: &PixelAnim) -> Option<u32> {
    // a fresh look at the cells as they are now
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tgradish_frames::Frame;

    use super::*;

    /// A quick estimate scaled by `calibration`.
    fn estimate(anim: &PixelAnim, options: &Options, calibration: f64) -> Option<usize> {
        let quick = quick(anim, options, &|| false).unwrap();
        quick.map(|quick| quick.size(calibration, options.max_bytes))
    }

    #[test]
    fn packs_art_that_cant_fit_quickly() {
        // noise in many colours, kept as it is: far over the limit
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
        let input = Animation::new(64, 64, frames).unwrap();
        let options = Options { fit: Fit::Lossless, effort: Effort::Fast, ..Options::default() };
        let mut too_large = None;
        let made = make(
            &input,
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
        let lossless = Options { fit: Fit::Lossless, effort: Effort::Fast, ..Options::default() };
        let made = make(&input, &lossless, &mut |_| {}, &|| false).unwrap();
        let json = file::unpack(&made.tgs).unwrap();
        let gzip = file::pack_quick(&json).len();
        assert!((made.bytes as f64) < gzip as f64 * 0.85, "{} of {gzip}", made.bytes);
        // at a limit only zopfli's size fits, it fits as it is either way
        for fit in [Fit::Lossless, Fit::Auto] {
            let options = Options { fit, max_bytes: made.bytes, ..lossless.clone() };
            let mut too_large = false;
            let mut progress = |event| too_large |= matches!(event, Progress::TooLarge { .. });
            let again = make(&input, &options, &mut progress, &|| false).unwrap();
            assert_eq!((again.bytes, again.steps.len(), too_large), (made.bytes, 0, false));
        }
    }

    #[test]
    fn stops_inside_an_encode() {
        let walker = tgradish_corpus::files().into_iter().find(|f| f.name == "walker.gif").unwrap();
        let input = tgradish_frames::decode(&walker.bytes, &Default::default()).unwrap();
        let (anim, _) = normalise(&input, &normalise::Options::default()).unwrap();
        // between colours, whatever the effort
        let best = Settings { effort: Effort::Best, ..Settings::default() };
        let stopped = painter(&anim, &best, None, &|| true);
        assert!(matches!(stopped, Err(EncodeError::Cancelled)), "{stopped:?}");
        // and making a sticker ends there too, after a few questions
        let asked = std::sync::atomic::AtomicUsize::new(0);
        let cancelled = || asked.fetch_add(1, std::sync::atomic::Ordering::Relaxed) >= 3;
        let made = make(&input, &Options::default(), &mut |_| {}, &cancelled);
        assert!(matches!(made, Err(crate::Error::Cancelled)), "{:?}", made.map(|s| s.bytes));
        // stopped while packing, with nothing left to encode: no sticker
        let packing = std::sync::atomic::AtomicBool::new(false);
        let stopped_packing = || packing.load(std::sync::atomic::Ordering::Relaxed);
        let mut progress = |event: Progress| {
            if matches!(event, Progress::Packing) {
                packing.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        };
        let made = make(&input, &Options::default(), &mut progress, &stopped_packing);
        assert!(matches!(made, Err(crate::Error::Cancelled)), "{:?}", made.map(|s| s.bytes));
    }

    #[test]
    fn estimates_json_over_desktops_limit_as_too_large() {
        let input = Animation::new(
            2,
            2,
            vec![Frame { rgba: vec![255; 16], duration: Duration::from_millis(100) }],
        )
        .unwrap();
        let (anim, _) = normalise(&input, &normalise::Options::default()).unwrap();
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
        let (anim, _) = normalise(&input, &normalise_options).unwrap();
        let lossless = estimate(&anim, &Options::default(), 1.0).unwrap();
        let fitted = |compromise: Compromise| {
            let options = Options { compromise, ..Options::default() };
            let mut steps = Vec::new();
            let settings = Settings::default();
            let fitting = Fitting {
                target: lossless / 20,
                settings: &settings,
                options: &options,
                cancelled: &|| false,
            };
            let mut calibration = Some(1.0);
            fit(&anim, anim.clone(), &mut steps, &mut calibration, &fitting, &mut |_| {}).unwrap();
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
        let (anim, _) = normalise(&input, &normalise_options).unwrap();
        let options = Options::default();
        let quick = Settings { effort: Effort::Fast, ..Settings::default() };
        let (scene, json) = render(&anim, &quick, &options, &|| false).unwrap();
        assert!(layout::cost(&scene) > telegram::MAX_COST, "{}", layout::cost(&scene));
        assert!(!within_limits(&scene, &json));
        // with no weight on the packed size, what is left is the cost
        assert!(estimate(&anim, &options, 0.0).unwrap() > telegram::MAX_BYTES);
    }
}
