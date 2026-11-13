//! From frames to a finished `.tgs`: normalise, encode, lay out, check,
//! and when it is too large, reduce until it fits.

use std::cell::RefCell;

use serde::{Deserialize, Serialize};
use tgradish_frames::Animation;

use crate::check::{self, Issue, Severity};
use crate::encode::{Effort, Settings, painter};
use crate::layout::{self, lay_out};
use crate::limits::{MAX_RAW_JSON, telegram};
use crate::lottie::Style;
use crate::normalise::{self, PixelAnim, Report, normalise};
use crate::reduce::{Compromise, Kind, Reduction};
use crate::scene::Scene;
use crate::{Result, file, mark};

mod fit;

use fit::{Fitting, Quick, RETARGET, ZOPFLI_BEST, ZOPFLI_SHARE, fit, likely_scale, quick};

/// What to do when the lossless result is too large.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Fit {
    /// Lossless when it fits, otherwise the least visible reductions that
    /// make it fit.
    #[default]
    Auto,
    /// Never reduce, keeping the art exactly as it is; a result that is
    /// too large is reported.
    Exact,
}

/// How [`make`] makes a sticker. Built as `Options { fit,
/// ..Options::default() }`, so that what is added later keeps its default.
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    /// How the animation is read as pixel art.
    pub normalise: normalise::Options,
    /// How hard the encoder looks for a small sticker.
    pub effort: Effort,
    /// Whether the art may change to fit.
    pub fit: Fit,
    /// The reductions fitting may use.
    pub reductions: Vec<Kind>,
    /// Which of them fitting uses first.
    pub compromise: Compromise,
    /// The largest `.tgs` to make.
    pub max_bytes: usize,
    /// Written into the sticker as its name.
    pub name: Option<String>,
    /// For tgradish-core's own use: `None` elsewhere.
    pub mark: Option<Vec<u8>>,
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
        }
    }
}

/// A reduction fitting applied.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct Step {
    /// The reduction, at the strength applied.
    #[serde(flatten)]
    pub reduction: Reduction,
    /// The estimated size after it, in bytes.
    pub bytes: usize,
    /// How much the animation differs from the lossless one after it (see
    /// [`crate::reduce::distortion`]).
    pub distortion: f64,
}

impl Step {
    /// A step of `reduction`, which fitting estimated to come to `bytes`
    /// and `distortion`: for front-ends to show one made up, as in tests.
    pub fn new(reduction: Reduction, bytes: usize, distortion: f64) -> Step {
        Step { reduction, bytes, distortion }
    }
}

/// Progress, for front-ends to show.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "stage", rename_all = "kebab-case", rename_all_fields = "kebab-case")]
#[non_exhaustive]
pub enum Progress {
    /// The animation is read as pixel art.
    Normalised {
        /// What normalising found and did.
        report: Box<Report>,
    },
    /// The lossless result, estimated, is too large.
    TooLarge {
        /// The estimate, in bytes.
        bytes: usize,
    },
    /// Fitting applied a reduction.
    Reduced {
        /// Which, and what it came to.
        step: Step,
    },
    /// The result is being packed, the longest step at best effort.
    Packing,
}

/// A finished sticker, and what went into it.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct Sticker {
    /// The `.tgs` file's bytes.
    #[serde(skip)]
    pub tgs: Vec<u8>,
    /// How many there are.
    pub bytes: usize,
    /// Size of the Lottie JSON inside.
    pub json_bytes: usize,
    /// What normalising found and did.
    pub report: Report,
    /// Reductions applied, in order; empty when lossless.
    pub steps: Vec<Step>,
    /// Lottie layers.
    pub layers: usize,
    /// Paint groups in all layers, each a colour.
    pub groups: usize,
    /// Rectangles in all groups.
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

/// Asked between steps of the work: when it says yes, the work stops, as
/// [`make`] and the painter do. It may be asked from several threads at
/// once.
pub type Cancelled<'a> = &'a (dyn Fn() -> bool + Sync);

/// An animation encoded and laid out: its scene, its Lottie JSON, and
/// that packed by `gzip -9`.
struct Rendered {
    scene: Scene,
    json: String,
    packed: Vec<u8>,
}

/// Encodes and lays out `anim`.
fn render(
    anim: &PixelAnim,
    settings: &Settings,
    options: &Options,
    cancelled: Cancelled,
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
    let scene = painter(anim, settings, Some(&score), cancelled)?;
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

/// Makes a sticker of `animation`, reporting `progress`. `cancelled` is
/// asked between steps, and while encoding between colours; when it says
/// yes, this stops with [`Error::Cancelled`](crate::Error::Cancelled).
///
/// With the `threads` feature, fitting tries each round's candidates in
/// threads of their own, and zopfli packs in one, which has two effects
/// past this call:
///
/// - zopfli can't be stopped, so a stop while it packs returns at once
///   and leaves its thread to finish alone, at work for as long as packing
///   the sticker takes;
/// - at most two such threads run in the whole process, counting those
///   left behind, so that stopping and starting again doesn't pile them
///   up. However many stickers are made at once, two are packed at a
///   time, and after two stops the next waits for one to end, still
///   answering to `cancelled`.
///
/// On wasm, which has no threads to start, the feature does nothing.
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
                fit(&original, current, known, &mut steps, &mut calibration, &fitting, progress)?;
        }
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
                rendered = render(&current, &settings, options, cancelled)?;
                &rendered
            }
        };
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
                None => *current_quick.insert(quick(&current, options, cancelled)?),
            };
            Ok(known.is_some())
        };
        if hopeless && retarget()? {
            target = ((target as f64) * RETARGET) as usize;
            continue;
        }
        // too large as it is, kept so or with nothing for fitting to do;
        // short of that, only packing tells
        if hopeless && steps.is_empty() {
            let quick = Quick { packed: quick_packed.len(), share: share_of_limits(scene, json) };
            progress(Progress::TooLarge { bytes: quick.size(ZOPFLI_SHARE, options.max_bytes) });
        }
        progress(Progress::Packing);
        let tgs = match (hopeless, real) {
            (true, _) => quick_packed.clone(),
            (false, Some(real)) => real.tgs.clone(),
            (false, None) => pack(json, options.effort, cancelled)?,
        };
        check()?;
        let fits = tgs.len() <= options.max_bytes && within_limits(scene, json);
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

/// Whether work is given to threads: with the `threads` feature, but on
/// wasm, where starting one panics. Features are the same for every target
/// of a build, so one that also builds for the web can have it on there.
const THREADS: bool = cfg!(all(feature = "threads", not(target_family = "wasm")));

/// `json` packed by zopfli, given up on when `cancelled` says so. zopfli
/// can't be stopped, so with [`THREADS`] it packs in a thread of its own,
/// left to finish alone; at most [`PACKING`] of them run at once in the
/// whole process, so stopping and starting again doesn't pile them up.
fn pack(json: &str, effort: Effort, cancelled: Cancelled) -> Result<Vec<u8>> {
    let check = || if cancelled() { Err(crate::Error::Cancelled) } else { Ok(()) };
    check()?;
    let iterations = zopfli_iterations(effort);
    let tgs = if THREADS {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::mpsc::{RecvTimeoutError, channel};
        use std::time::Duration;
        // a stop is noticed within this, which the window can't tell apart
        // from at once
        const POLL: Duration = Duration::from_millis(50);
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
        let json = json.as_bytes().to_vec();
        std::thread::spawn(move || {
            let _running = running;
            send.send(file::pack(&json, iterations))
        });
        loop {
            check()?;
            match receive.recv_timeout(POLL) {
                Ok(tgs) => break tgs,
                Err(RecvTimeoutError::Timeout) => {}
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

/// `f` of each item, in threads of their own with [`THREADS`].
fn parallel<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    if THREADS {
        std::thread::scope(|scope| {
            let handles: Vec<_> = items.iter().map(|item| scope.spawn(|| f(item))).collect();
            handles.into_iter().map(|handle| handle.join().expect("no panics")).collect()
        })
    } else {
        items.iter().map(f).collect()
    }
}

internal! {
/// How many zopfli iterations stickers are packed with at `effort`.
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

    use tgradish_frames::Frame;

    use super::fit::{Calibration, normalise_likely_scale};
    use super::*;
    use crate::encode::EncodeError;

    /// A quick estimate scaled by `calibration`.
    fn estimate(anim: &PixelAnim, options: &Options, calibration: f64) -> Option<usize> {
        let quick = quick(anim, options, &|| false).unwrap();
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
        let kept = make(&input, &lossless, &mut |_| {}, &|| false).unwrap();
        let once = started.elapsed();
        let options = Options { fit: Fit::Auto, reductions: Vec::new(), ..lossless };
        let started = std::time::Instant::now();
        let fitted = make(&input, &options, &mut |_| {}, &|| false).unwrap();
        let took = started.elapsed();
        assert!(fitted.tgs == kept.tgs && fitted.steps.is_empty());
        assert!(took < once * 4 + Duration::from_secs(1), "{took:?} against {once:?}");
    }

    #[test]
    fn packs_art_that_cant_fit_quickly() {
        // noise in many colours, kept as it is: far over the limit
        let input = noise();
        let options = Options { fit: Fit::Exact, effort: Effort::Fast, ..Options::default() };
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
        let lossless = Options { fit: Fit::Exact, effort: Effort::Fast, ..Options::default() };
        let made = make(&input, &lossless, &mut |_| {}, &|| false).unwrap();
        let json = file::unpack(&made.tgs).unwrap();
        let gzip = file::pack_quick(&json).len();
        assert!((made.bytes as f64) < gzip as f64 * 0.85, "{} of {gzip}", made.bytes);
        // at a limit only zopfli's size fits, it fits as it is either way
        for fit in [Fit::Exact, Fit::Auto] {
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
        let input =
            tgradish_frames::decode(&walker.bytes, &Default::default(), &Default::default())
                .unwrap();
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
    fn takes_the_likely_scale_from_normalising_when_it_is_the_same() {
        let crop = normalise::Options { square: normalise::Square::Crop, ..Default::default() };
        let canvas = normalise::Options { keep_canvas: true, ..Default::default() };
        let forced = normalise::Options { pixel_scale: Some(2), ..Default::default() };
        for file in tgradish_corpus::files() {
            let input =
                tgradish_frames::decode(&file.bytes, &Default::default(), &Default::default())
                    .unwrap();
            for options in [&normalise::Options::default(), &crop, &canvas, &forced] {
                let (anim, report) = normalise(&input, options).unwrap();
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
                likely: normalise_likely_scale(&anim),
                settings: &settings,
                options: &options,
                cancelled: &|| false,
            };
            let mut calibration = Some(Calibration { factor: 1.0, real: None });
            let (current, progress) = (anim.clone(), &mut |_| {});
            fit(&anim, current, None, &mut steps, &mut calibration, &fitting, progress).unwrap();
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
        let Rendered { scene, json, .. } = render(&anim, &quick, &options, &|| false).unwrap();
        assert!(layout::cost(&scene) > telegram::MAX_COST, "{}", layout::cost(&scene));
        assert!(!within_limits(&scene, &json));
        // with no weight on the packed size, what is left is the cost
        assert!(estimate(&anim, &options, 0.0).unwrap() > telegram::MAX_BYTES);
    }
}
