//! From frames to a finished `.tgs`: normalise, encode, lay out, check,
//! and when it is too large, reduce until it fits (see "Fit" in
//! `docs/tgs.md`).

use serde::{Deserialize, Serialize};
use tgradish_frames::Animation;

use crate::check::{self, Issue, Severity};
use crate::encode::{Effort, Settings, painter};
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
    /// pixel scale, or a long input sped up or cut.
    pub fn lossy(&self) -> bool {
        !self.steps.is_empty()
            || self.report.snapped_pixels > 0
            || self.report.trimmed
            || self.report.speed > 1.0
    }

    /// Whether Telegram takes it.
    pub fn fits(&self) -> bool {
        self.issues.iter().all(|issue| issue.severity != Severity::Error)
    }
}

/// zopfli packs about this much smaller than `gzip -9`; estimates lean
/// high.
const ZOPFLI_SHARE: f64 = 0.93;
/// When the real size misses an estimate, the next target is this much
/// lower.
const RETARGET: f64 = 0.96;

/// Encodes and lays out `anim`.
fn render(anim: &PixelAnim, settings: &Settings, options: &Options) -> Result<(Scene, String)> {
    let json = |scene: &Scene| {
        let mut lottie = lay_out(scene, anim, options.name.clone());
        if let Some(mark) = &options.mark {
            mark::embed(&mut lottie, mark);
        }
        lottie.to_json(options.style)
    };
    let score = |scene: &Scene| file::quick_size(json(scene).as_bytes());
    let scene = painter(anim, settings, Some(&score))?;
    let json = json(&scene);
    Ok((scene, json))
}

/// Whether Telegram's server and every app take a sticker this large,
/// leaving its packed size to the caller.
fn within_limits(scene: &Scene, json: &str) -> bool {
    layout::cost(scene) <= telegram::MAX_COST && json.len() <= MAX_RAW_JSON
}

/// Estimates what a `.tgs` of `anim` comes to: quickly encoded, then
/// scaled by `calibration`. A cost near the server's limit, or JSON near
/// Telegram Desktop's, counts as that share of `max_bytes` when that is
/// more, so reductions that shrink either count as progress even before
/// it fits. `None` when it can't be made at all.
fn estimate(anim: &PixelAnim, options: &Options, calibration: f64) -> Option<usize> {
    let quick = Settings { effort: Effort::Fast, ..Settings::default() };
    let (scene, json) = render(anim, &quick, options).ok()?;
    let packed = file::quick_size(json.as_bytes()) as f64 * calibration;
    let share = f64::max(
        layout::cost(&scene) as f64 / telegram::MAX_COST as f64,
        json.len() as f64 / MAX_RAW_JSON as f64,
    );
    Some(packed.max(share * options.max_bytes as f64) as usize)
}

/// Makes a sticker of `animation`, reporting `progress`. `cancelled` is
/// asked between steps; when it says yes, this stops with
/// [`Error::Cancelled`](crate::Error::Cancelled).
pub fn make(
    animation: &Animation,
    options: &Options,
    progress: &mut dyn FnMut(Progress),
    cancelled: &dyn Fn() -> bool,
) -> Result<Sticker> {
    let check = || if cancelled() { Err(crate::Error::Cancelled) } else { Ok(()) };
    let (original, report) = normalise(animation, &options.normalise)?;
    progress(Progress::Normalised { report: Box::new(report.clone()) });
    let settings = Settings { effort: options.effort, ..Settings::default() };

    // estimates are quick encodes, scaled to what the real encoder and
    // zopfli make of the original
    let calibration = {
        let quick = Settings { effort: Effort::Fast, ..Settings::default() };
        let mut renders =
            parallel(&[quick, settings.clone()], |settings| render(&original, settings, options))
                .into_iter();
        match (renders.next().unwrap(), renders.next().unwrap()) {
            (Ok((_, fast)), Ok((_, real))) => {
                let (fast, real) =
                    (file::quick_size(fast.as_bytes()), file::quick_size(real.as_bytes()));
                ZOPFLI_SHARE * real as f64 / fast.max(1) as f64
            }
            _ => ZOPFLI_SHARE,
        }
    };

    let mut current = original.clone();
    let mut steps: Vec<Step> = Vec::new();
    let mut target = options.max_bytes;
    loop {
        if options.fit == Fit::Auto {
            current = fit(
                &original,
                current,
                &mut steps,
                (target, calibration),
                options,
                progress,
                cancelled,
            )?;
        }
        check()?;
        progress(Progress::Packing);
        let (scene, json) = render(&current, &settings, options)?;
        let tgs = file::pack(json.as_bytes(), zopfli_iterations(options.effort));
        let fits = tgs.len() <= options.max_bytes && within_limits(&scene, &json);
        // the estimate was off; aim lower and reduce some more
        if !fits && options.fit == Fit::Auto && target > options.max_bytes / 4 {
            let reachable = estimate(&current, options, calibration).is_some();
            if reachable {
                target = ((target as f64) * RETARGET) as usize;
                continue;
            }
        }
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

/// Reduces `current` until its estimate is at most `target`: each round
/// takes the reduction that saves the most bytes for the least change,
/// among those the compromise puts first while any of them help, then the
/// last one is weakened as far as it still fits.
fn fit(
    original: &PixelAnim,
    mut current: PixelAnim,
    steps: &mut Vec<Step>,
    (target, calibration): (usize, f64),
    options: &Options,
    progress: &mut dyn FnMut(Progress),
    cancelled: &dyn Fn() -> bool,
) -> Result<PixelAnim> {
    let size_of = |anim: &PixelAnim| estimate(anim, options, calibration).unwrap_or(usize::MAX);
    let mut size = size_of(&current);
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
        let tries = parallel(&kinds, |&k| -> Result<Option<(PixelAnim, usize, f64)>> {
            let candidate = ladders[k][next[k]].apply(&current)?;
            let candidate_size = size_of(&candidate);
            if candidate == current || candidate_size >= size {
                return Ok(None);
            }
            let candidate_error = error(original, &candidate);
            Ok(Some((candidate, candidate_size, candidate_error)))
        });
        // (value, kind, candidate, size, error)
        let mut best: Option<(f64, usize, PixelAnim, usize, f64)> = None;
        for (&k, tried) in kinds.iter().zip(tries) {
            let Some((candidate, candidate_size, candidate_error)) = tried? else {
                // no help at this strength; the next round tries stronger
                next[k] += 1;
                continue;
            };
            let value = (size - candidate_size) as f64 / (candidate_error - error_now).max(1e-9);
            if best.as_ref().is_none_or(|b| value > b.0) {
                best = Some((value, k, candidate, candidate_size, candidate_error));
            }
        }
        let Some((_, k, candidate, candidate_size, candidate_error)) = best else { continue };
        before_last = Some((current, k));
        current = candidate;
        size = candidate_size;
        error_now = candidate_error;
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
            let candidate_size = size_of(&candidate);
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
            let target = (lossless / 20, 1.0);
            fit(&anim, anim.clone(), &mut steps, target, &options, &mut |_| {}, &|| false).unwrap();
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
        let (scene, json) = render(&anim, &quick, &options).unwrap();
        assert!(layout::cost(&scene) > telegram::MAX_COST, "{}", layout::cost(&scene));
        assert!(!within_limits(&scene, &json));
        // with no weight on the packed size, what is left is the cost
        assert!(estimate(&anim, &options, 0.0).unwrap() > telegram::MAX_BYTES);
    }
}
