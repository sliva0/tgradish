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
use crate::reduce::{Kind, Reduction, error};
use crate::scene::Scene;
use crate::{Result, file};

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
    /// The largest `.tgs` to make.
    pub max_bytes: usize,
    /// Written into the sticker as its name.
    pub name: Option<String>,
    pub style: Style,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            normalise: normalise::Options::default(),
            effort: Effort::default(),
            fit: Fit::default(),
            reductions: Kind::ALL.to_vec(),
            max_bytes: telegram::MAX_BYTES,
            name: None,
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
    let score = |scene: &Scene| {
        file::quick_size(
            lay_out(scene, anim, options.name.clone()).to_json(options.style).as_bytes(),
        )
    };
    let scene = painter(anim, settings, Some(&score))?;
    let json = lay_out(&scene, anim, options.name.clone()).to_json(options.style);
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
        match (render(&original, &quick, options), render(&original, &settings, options)) {
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

fn zopfli_iterations(effort: Effort) -> u64 {
    match effort {
        Effort::Fast => 5,
        Effort::Balanced => 15,
        Effort::Best => 40,
    }
}

/// Reduces `current` until its estimate is at most `target`: each round
/// takes the reduction that saves the most bytes for the least change,
/// then the last one is weakened as far as it still fits.
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
        // (value, kind, candidate, size, error)
        let mut best: Option<(f64, usize, PixelAnim, usize, f64)> = None;
        let mut tried = false;
        for (k, ladder) in ladders.iter().enumerate() {
            let Some(reduction) = ladder.get(next[k]) else { continue };
            tried = true;
            let candidate = reduction.apply(&current)?;
            let candidate_size = size_of(&candidate);
            if candidate == current || candidate_size >= size {
                // no help at this strength; the next round tries stronger
                next[k] += 1;
                continue;
            }
            let candidate_error = error(original, &candidate);
            let value = (size - candidate_size) as f64 / (candidate_error - error_now).max(1e-9);
            if best.as_ref().is_none_or(|b| value > b.0) {
                best = Some((value, k, candidate, candidate_size, candidate_error));
            }
        }
        if !tried {
            // every reduction is used up: as small as it gets
            break;
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
    fn estimates_shapes_over_the_servers_limit_as_too_large() {
        // isolated pixels, on even cells in even frames and odd cells in odd
        // ones: 3969 rectangles a frame that nothing can merge
        const SIDE: u32 = 126;
        let frames = (0..7)
            .map(|frame| {
                let rgba = (0..SIDE * SIDE)
                    .flat_map(|index| {
                        let (x, y) = (index % SIDE, index / SIDE);
                        let on = x % 2 == frame % 2 && y % 2 == frame % 2;
                        if on { [230, 40, 40, 255] } else { [0; 4] }
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
