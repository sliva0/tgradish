//! Conversion progress and result summary shown in the side column beside the
//! preview: output size against Telegram's limit, per-stage or per-encode
//! progress bars, and fitting steps or encode tables.

use eframe::egui::{self, Color32, RichText, vec2};

use crate::item::Item;
use crate::jobs::{self, Job, Status};
use crate::l10n::fl;
use crate::{theme, widgets};

/// Draws the conversion progress or last result summary for `item` in the side
/// column (size gauge, Telegram validation, and a paste hint when `copyable`).
pub(crate) fn summary(ui: &mut egui::Ui, item: &Item, queued: Option<usize>, copyable: bool) {
    if let Some(conversion) = &item.job {
        progress_column(ui, &conversion.job, queued);
    }
    if let Some(failed) = &item.failed {
        widgets::section(ui, &fl!("last-conversion"));
        match &failed.job.status {
            Status::Failed(message) => {
                let said = ui.colored_label(theme::bad(ui.visuals()), message.say());
                // Scroll to the full error message when clicked in the bottom bar.
                if crate::actions::pointed_at_failure(ui.ctx()) {
                    said.scroll_to_me(Some(egui::Align::Center));
                }
            }
            Status::Exists(path) => {
                let path = path.display().to_string();
                ui.colored_label(theme::warn(ui.visuals()), fl!("path-in-the-way", path = path));
            }
            Status::Cancelled => widgets::note(ui, fl!("stopped-early")),
            Status::Done(_) | Status::Waiting | Status::Running => {}
        }
    }
    let Some(made) = &item.made else { return };
    widgets::section(
        ui,
        &if item.result_is_current() { fl!("result") } else { fl!("result", "stale") },
    );
    // `item.made` only stores completed conversions.
    let Status::Done(done) = &made.job.status else { return };
    widgets::gauge(
        ui,
        done.bytes,
        done.limit,
        fl!("of-limit", used = widgets::kib(done.bytes), limit = widgets::kib(done.limit)),
    );
    ui.add_space(4.0);
    widgets::problems(ui, &done.problems);
    // Hint how to copy and paste into Telegram when the result is current and valid.
    let refused = done.problems.iter().any(|problem| problem.refused);
    if copyable && item.job.is_none() && item.result_is_current() && !refused {
        let keys = crate::keys::keys_text(crate::keys::Command::CopyResult);
        widgets::note(ui, fl!("result-copy", keys = keys));
    }
    if let Some(plan) = &made.job.progress.webm {
        // Use the kept attempt's actual frame rate and duration (fitting may
        // have lowered fps or trimmed duration compared to the initial plan).
        let progress = &made.job.progress;
        let kept = progress.attempts.iter().find(|attempt| Some(attempt.number) == done.attempt);
        let (fps, length) =
            kept.map_or((plan.fps, plan.length), |kept| (kept.params.fps, kept.params.length));
        let (width, height, fps) = (plan.width, plan.height, widgets::trimmed(fps));
        let length = widgets::seconds(length);
        widgets::note(
            ui,
            if done.spoofed {
                fl!(
                    "result-webm",
                    "spoofed",
                    width = width,
                    height = height,
                    fps = fps,
                    length = length
                )
            } else {
                fl!("result-webm", width = width, height = height, fps = fps, length = length)
            },
        );
    }
    // When stopped early by Keep best so far, note which encode was kept
    // (ticked in the table below).
    if let (true, Some(kept)) = (done.settled, done.attempt) {
        widgets::note(ui, decision_text(&tgradish_core::webm::Decision::Settled { attempt: kept }));
    }
    if done.changed {
        widgets::note(ui, fl!("changed-to-fit"));
    }
    if made.job.replaces {
        ui.colored_label(theme::warn(ui.visuals()), format!("⚠ {}", fl!("replaced-earlier")));
    }
}

/// State of a single fitting attempt or `.tgs` compression pass for
/// [`tried_bar`]: still running, fits under the limit, or oversized (`from` is
/// the byte offset in the label where the warning-coloured size starts).
enum Tried {
    Working(widgets::Fill),
    Fits,
    TooLarge { from: usize },
}

/// Draws a progress bar for one fitting attempt (a WebM encode or a `.tgs`
/// compression/retry pass), sized to fit `widest`.
fn tried_bar(ui: &mut egui::Ui, tried: Tried, text: &str, widest: &str) {
    match tried {
        Tried::Working(fill) => widgets::steady_bar(ui, fill, text, widest, running_colour(ui)),
        Tried::Fits => {
            let done = theme::POSITIVE.gamma_multiply(0.45);
            widgets::steady_bar(ui, widgets::Fill::Share(1.0), text, widest, done);
        }
        // Oversized attempts are normal search steps rather than errors: draw
        // a neutral spent bar with only the size/verdict suffix in warning colour.
        Tried::TooLarge { from } => widgets::bar_spent(ui, text, widest, from),
    }
    ui.add_space(2.0);
}

/// Draws per-stage or per-encode progress bars for an active conversion job.
fn progress_column(ui: &mut egui::Ui, job: &Job, queued: Option<usize>) {
    widgets::section(ui, &fl!("converting"));
    if job.is_waiting() {
        widgets::note(
            ui,
            match queued {
                Some(0) | None => fl!("next-to-convert"),
                Some(ahead) => fl!("waiting-for", ahead = ahead),
            },
        );
        return;
    }
    let progress = &job.progress;
    let running = running_colour(ui);
    let done = theme::POSITIVE.gamma_multiply(0.45);
    if let Some(stage) = progress.tgs {
        for (each, text) in stages(progress) {
            use widgets::Fill;
            // If the initial compression pass exceeded the limit, render it as
            // TooLarge just like a missed retry.
            if let (jobs::TgsStage::Compressing, Some(&bytes)) = (each, progress.missed.first()) {
                let from = missed_from(&text, bytes);
                tried_bar(ui, Tried::TooLarge { from }, &text, &widest_stage(each));
                continue;
            }
            let (fill, colour) = match each.cmp(&stage) {
                std::cmp::Ordering::Less => (Fill::Share(1.0), done),
                std::cmp::Ordering::Equal => (stage_fill(progress, stage), running),
                std::cmp::Ordering::Greater => (Fill::Share(0.0), running),
            };
            widgets::steady_bar(ui, fill, &text, &widest_stage(each), colour);
            ui.add_space(2.0);
        }
        for (index, &aim) in progress.retries.iter().enumerate() {
            let (text, widest) = retry_texts(index, aim, progress.missed.get(index + 1).copied());
            let tried = match progress.missed.get(index + 1) {
                Some(&bytes) => Tried::TooLarge { from: missed_from(&text, bytes) },
                None => Tried::Working(widgets::Fill::Working(progress.stage_done)),
            };
            tried_bar(ui, tried, &text, &widest);
        }
        return;
    }
    if progress.attempts.is_empty() {
        widgets::bar(ui, widgets::Fill::Unknown, &fl!("starting"), running);
    }
    for attempt in &progress.attempts {
        let rate = rate_text(attempt);
        let (number, fps) = (attempt.number, widgets::trimmed(attempt.params.fps));
        let text = fl!("encode", number = number, fps = fps.as_str(), rate = rate.as_str());
        // Reserve height for the longest completion string ("too-large").
        let widest = widgets::zeros(&fl!(
            "encode-done",
            "too-large",
            number = number,
            fps = fps.as_str(),
            rate = rate.as_str(),
            size = if progress.webm.is_some() { percent(1.0) } else { widgets::kib(1000 * 1024) }
        ));
        match attempt.bytes {
            Some(bytes) => {
                let size = match &progress.webm {
                    Some(plan) => share(bytes, plan.limit),
                    None => widgets::kib(bytes),
                };
                if attempt.fits {
                    let text =
                        fl!("encode-done", number = number, fps = fps, rate = rate, size = size);
                    tried_bar(ui, Tried::Fits, &text, &widest);
                } else {
                    let text = fl!(
                        "encode-done",
                        "too-large",
                        number = number,
                        fps = fps,
                        rate = rate,
                        size = size.as_str()
                    );
                    let from = text.rfind(&size).unwrap_or(text.len());
                    tried_bar(ui, Tried::TooLarge { from }, &text, &widest);
                }
            }
            None => {
                let fill = widgets::Fill::working(progress.fraction);
                tried_bar(ui, Tried::Working(fill), &text, &widest);
            }
        }
    }
    if let Some(plan) = &progress.webm
        && plan.attempts > 1
    {
        widgets::note(ui, fl!("encodes-up-to", count = plan.attempts));
    }
}

/// Returns the accent fill at 50% opacity (or lower if needed to keep text
/// readable).
fn running_colour(ui: &egui::Ui) -> Color32 {
    theme::fill(ui.visuals(), 0.5)
}

/// Returns the `.tgs` stages to display as progress bars. The `Fitting` stage
/// is included only when the initial sticker exceeded the limit and reduction
/// actually ran (stickers that already fit or use exact mode skip fitting).
fn stages(progress: &jobs::Progress) -> Vec<(jobs::TgsStage, String)> {
    use jobs::TgsStage;
    let hopeless = progress.tgs_exact && progress.unchanged_bytes.is_some();
    // Show only the initial fitting pass's steps on the Fitting bar (later
    // retries have their own bars).
    let first = progress.first_steps.unwrap_or(usize::MAX).min(progress.steps.len());
    let steps = &progress.steps[..first];
    let fitting = match (progress.unchanged_bytes, steps.last()) {
        _ if progress.tgs_exact => None,
        (_, Some(step)) => {
            let (count, size) = (steps.len(), widgets::kib(step.bytes as u64));
            Some(fl!("stage-fitting-steps", count = count, size = size))
        }
        (Some(bytes), None) => Some(fl!("stage-fitting-from", size = widgets::kib(bytes as u64))),
        (None, None) => None,
    };
    let compressing = match progress.missed.first() {
        Some(&bytes) => missed_text(bytes),
        None if hopeless => fl!("stage-compressing", "hopeless"),
        None => fl!("stage-compressing"),
    };
    [
        Some((TgsStage::Reading, fl!("stage-reading"))),
        Some((TgsStage::Drawing, fl!("stage-drawing"))),
        fitting.map(|text| (TgsStage::Fitting, text)),
        Some((TgsStage::Compressing, compressing)),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Returns the longest text that `stage` of a `.tgs` conversion can display.
fn widest_stage(stage: jobs::TgsStage) -> String {
    use jobs::TgsStage;
    let size = "1000.0 KiB";
    let texts = match stage {
        TgsStage::Reading => vec![fl!("stage-reading")],
        TgsStage::Drawing => vec![fl!("stage-drawing")],
        TgsStage::Fitting => vec![
            fl!("stage-fitting-steps", count = 100, size = size),
            fl!("stage-fitting-from", size = size),
        ],
        TgsStage::Compressing => vec![
            fl!("stage-compressing"),
            fl!("stage-compressing", "hopeless"),
            missed_text(Some(999_999)),
            missed_text(None),
        ],
    };
    texts.into_iter().max_by_key(String::len).unwrap_or_default()
}

/// Formats the `Compressing` bar label when the first pass missed the size
/// limit (either compressed to `bytes` or skipped as far too large).
fn missed_text(bytes: Option<u64>) -> String {
    match bytes {
        Some(bytes) => fl!("stage-compressing", "missed", size = widgets::kib(bytes)),
        None => fl!("stage-compressing", "missed-far"),
    }
}

/// Returns the byte offset in `text` where the warning-coloured size/verdict
/// suffix begins for a missed attempt.
fn missed_from(text: &str, bytes: Option<u64>) -> usize {
    match bytes {
        Some(bytes) => text.rfind(&widgets::kib(bytes)).unwrap_or(text.len()),
        None => 0,
    }
}

/// Returns the current and widest label strings for the `index`-th `.tgs`
/// retry aiming at `aim` bytes.
fn retry_texts(index: usize, aim: u64, missed: Option<Option<u64>>) -> (String, String) {
    let (number, aim) = (index + 2, widgets::kib(aim));
    let came_to = |bytes: Option<u64>| match bytes {
        Some(bytes) => {
            let size = widgets::kib(bytes);
            fl!("retry-done", number = number, aim = aim.as_str(), size = size)
        }
        None => fl!("retry-done", "far", number = number, aim = aim.as_str()),
    };
    let text = match missed {
        Some(bytes) => came_to(bytes),
        None => fl!("retry", number = number, aim = aim.as_str()),
    };
    let widest = [came_to(Some(999_999)), came_to(None)].into_iter().max_by_key(String::len);
    (text, widgets::zeros(&widest.unwrap_or_default()))
}

/// Returns the fill state for the active `.tgs` `stage` (`Reading` has no
/// incremental progress).
fn stage_fill(progress: &jobs::Progress, stage: jobs::TgsStage) -> widgets::Fill {
    match stage {
        jobs::TgsStage::Reading => widgets::Fill::Unknown,
        _ => widgets::Fill::Working(progress.stage_done),
    }
}

/// Formats an encode attempt's rate control mode.
fn rate_text(attempt: &jobs::Attempt) -> String {
    match attempt.params.rate {
        tgradish_core::webm::Rate::Bitrate(kbps) => format!("{kbps:.0} kbit/s"),
        tgradish_core::webm::Rate::Crf(crf) => fl!("rate-quality", crf = crf),
        tgradish_core::webm::Rate::Lossless => fl!("rate-lossless"),
        rate => format!("{rate:?}"),
    }
}

/// Returns the fill and label for the single active step of a conversion
/// (shown in the bottom action bar next to Stop, while the side column shows
/// all steps): how many encodes fitting a WebM needs is unknown in advance,
/// and the list's bar already shows a `.tgs`'s overall progress.
fn current_step(progress: &jobs::Progress) -> (widgets::Fill, String) {
    use widgets::Fill;
    if let Some(&aim) = progress.retries.last() {
        let (text, _) = retry_texts(progress.retries.len() - 1, aim, None);
        return (Fill::Working(progress.stage_done), text);
    }
    if let Some(stage) = progress.tgs {
        let text = stages(progress).into_iter().find(|(each, _)| *each == stage);
        return (stage_fill(progress, stage), text.map(|(_, text)| text).unwrap_or_default());
    }
    let Some(attempt) = progress.attempts.last() else {
        return (Fill::Unknown, fl!("starting"));
    };
    let (number, rate) = (attempt.number, rate_text(attempt));
    let fps = widgets::trimmed(attempt.params.fps);
    let text = match progress.webm.as_ref().map(|plan| plan.attempts) {
        Some(most) if most > 1 => {
            fl!("encode-of", number = number, most = most, fps = fps, rate = rate)
        }
        _ => fl!("encode", number = number, fps = fps, rate = rate),
    };
    // Once an encode finishes, it is either being scored for SSIM or about to
    // start the next attempt.
    let fill =
        if attempt.bytes.is_some() { Fill::Unknown } else { Fill::working(progress.fraction) };
    (fill, text)
}

/// Draws a progress bar of `size` for the current conversion step (used in the
/// sticky bottom action bar beside Stop).
pub(crate) fn step_bar(ui: &mut egui::Ui, job: &Job, size: egui::Vec2) {
    let (fill, text) = current_step(&job.progress);
    widgets::bar_sized(ui, size, fill, &text, running_colour(ui));
}

/// Draws detailed conversion breakdown under the result gauge: `.tgs` sprite
/// stats, shape counts, and reduction steps, or the WebM encode attempts table
/// and fitting decisions.
pub(crate) fn details(ui: &mut egui::Ui, item: &Item) {
    let Some(made) = &item.made else { return };
    let Status::Done(done) = &made.job.status else { return };
    let progress = &made.job.progress;
    ui.add_space(6.0);
    // As plain as the result's size above it.
    if let Some(report) = &progress.report {
        let (width, height) = (report.width, report.height);
        let size = match report.scale {
            1 => fl!("art-size", width = width, height = height),
            scale => fl!("art-size", "scaled", width = width, height = height, scale = scale),
        };
        let colours = fl!("count-colours", count = report.colours);
        let frames = fl!("count-frames", count = report.frames);
        ui.label(fl!("art-summary", size = size, colours = colours, frames = frames));
        if report.tempo != 1.0 {
            ui.label(fl!("art-tempo", tempo = widgets::trimmed(report.tempo)));
        }
        if let Some(likely) = report.likely_scale {
            ui.label(fl!("art-off-grid", scale = likely.scale));
        }
    }
    if let (Some(json), Some((layers, rectangles))) = (done.json_bytes, done.shapes) {
        let layers = fl!("count-layers", count = layers);
        let rectangles = fl!("count-rectangles", count = rectangles);
        let size = widgets::kib(json);
        ui.label(fl!("art-shapes", layers = layers, rectangles = rectangles, size = size));
    }
    // Show the pre-fitting size only when reduction steps actually ran.
    if let (Some(bytes), false) = (progress.unchanged_bytes, progress.steps.is_empty()) {
        ui.label(fl!("art-before-fitting", size = widgets::kib(bytes as u64)));
    }
    for step in &progress.steps {
        let (change, size) = (reduction(&step.reduction), widgets::kib(step.bytes as u64));
        ui.label(format!("  • {}", fl!("art-step", change = change, size = size)));
    }
    if !progress.attempts.is_empty() {
        // Keep the table compact for the side column: put units in column
        // headers and scroll horizontally if needed. Include the similarity
        // column whenever any attempt was scored with SSIM.
        let scored = progress.attempts.iter().any(|attempt| attempt.ssim.is_some());
        egui::ScrollArea::horizontal().id_salt(("attempts", item.id)).show(ui, |ui| {
            let columns = if scored { 6 } else { 5 };
            // Place the kept checkmark in its own unlabelled column so numeric
            // columns stay right-aligned beneath their headers.
            let headers = [
                (String::new(), String::new()),
                ("#".to_owned(), fl!("column-number")),
                ("fps".to_owned(), fl!("column-fps")),
                ("kbit/s".to_owned(), fl!("column-rate")),
                (fl!("column-size"), fl!("column-size", "hint")),
                (fl!("column-alike"), fl!("column-alike", "hint")),
            ];
            // Each cell's text, and what it says on hover.
            let mut rows: Vec<Vec<(RichText, String)>> = vec![
                headers[..columns]
                    .iter()
                    .map(|(header, hint)| (RichText::new(header).weak(), hint.clone()))
                    .collect(),
            ];
            for attempt in &progress.attempts {
                let kept = done.attempt == Some(attempt.number);
                let (tick, number) = match kept {
                    true => (
                        RichText::new("✔").strong(),
                        RichText::new(attempt.number.to_string()).strong(),
                    ),
                    false => (RichText::new(""), RichText::new(attempt.number.to_string())),
                };
                let rate = match attempt.params.rate {
                    tgradish_core::webm::Rate::Bitrate(kbps) => format!("{kbps:.0}"),
                    tgradish_core::webm::Rate::Crf(crf) => fl!("rate-quality-short", crf = crf),
                    tgradish_core::webm::Rate::Lossless => fl!("rate-lossless"),
                    rate => format!("{rate:?}"),
                };
                let size = match attempt.bytes {
                    Some(bytes) => {
                        let colour = match attempt.fits {
                            true => theme::good(ui.visuals()),
                            false => theme::bad(ui.visuals()),
                        };
                        (RichText::new(share(bytes, done.limit)).color(colour), widgets::kib(bytes))
                    }
                    None => (RichText::new("—"), String::new()),
                };
                let mut row = vec![
                    (tick, String::new()),
                    (number, String::new()),
                    (RichText::new(widgets::trimmed(attempt.params.fps)), String::new()),
                    (RichText::new(rate), String::new()),
                    size,
                ];
                if scored {
                    let alike = attempt.ssim.map(percent);
                    row.push((RichText::new(alike.unwrap_or_default()), String::new()));
                }
                rows.push(row);
            }
            // Right-align numbers within columns sized to their widest cell.
            let width = |text: &RichText| {
                let text = egui::WidgetText::from(text.clone());
                let wrap = Some(egui::TextWrapMode::Extend);
                text.into_galley(ui, wrap, f32::INFINITY, egui::TextStyle::Body).size().x
            };
            let mut widths = vec![0.0_f32; columns];
            for row in &rows {
                for (column, (text, _)) in row.iter().enumerate() {
                    widths[column] = widths[column].max(width(text));
                }
            }
            let height = ui.text_style_height(&egui::TextStyle::Body);
            // Override egui::Grid's default minimum column width and row height
            // so columns hug their contents and headers sit close above the first row.
            let grid = egui::Grid::new(("attempts", item.id))
                .num_columns(columns)
                .min_col_width(0.0)
                .min_row_height(height)
                .spacing([8.0, 4.0]);
            grid.striped(true).show(ui, |ui| {
                let layout = egui::Layout::right_to_left(egui::Align::Center);
                for row in rows {
                    for (column, (text, hint)) in row.into_iter().enumerate() {
                        let size = vec2(widths[column], height);
                        if text.is_empty() {
                            ui.allocate_exact_size(size, egui::Sense::hover());
                            continue;
                        }
                        let cell = ui.allocate_ui_with_layout(size, layout, |ui| ui.label(text));
                        if !hint.is_empty() {
                            cell.inner.on_hover_text(hint);
                        }
                    }
                    ui.end_row();
                }
            });
        });
    }
    // `Decision::Settled` is already shown in the result summary above.
    let said = |decision: &&tgradish_core::webm::Decision| {
        !matches!(decision, tgradish_core::webm::Decision::Settled { .. })
    };
    for decision in progress.decisions.iter().filter(said) {
        widgets::note(ui, decision_text(decision));
    }
    for warning in &progress.warnings {
        ui.colored_label(theme::warn(ui.visuals()), format!("⚠ {}", warning.text()));
    }
}

/// Formats `bytes` as a percentage of `limit`.
fn share(bytes: u64, limit: u64) -> String {
    percent(bytes as f64 / limit.max(1) as f64)
}

/// Formats a `0.0..=1.0` ratio or SSIM score as a percentage with one decimal place.
fn percent(share: f64) -> String {
    widgets::percent(share * 100.0, 1)
}

/// Formats a WebM fitting [`Decision`](tgradish_core::webm::Decision) for the GUI.
fn decision_text(decision: &tgradish_core::webm::Decision) -> String {
    use tgradish_core::webm::Decision;
    let fps = widgets::trimmed;
    match *decision {
        Decision::InBand { attempt, share, least } => {
            fl!("decided-in-band", number = attempt, share = percent(share), least = percent(least))
        }
        Decision::AlikeEnough { attempt, ssim } => {
            fl!("decided-alike-enough", number = attempt, alike = percent(ssim))
        }
        Decision::AlikeUnchanged { attempt, than, ssim } => {
            fl!("decided-alike-unchanged", number = attempt, than = than, alike = percent(ssim))
        }
        Decision::LittleToGain { attempt, share, predicted } => fl!(
            "decided-little-to-gain",
            number = attempt,
            share = percent(share),
            predicted = percent(predicted)
        ),
        Decision::TooLargeAtLeast { attempt, share } => {
            fl!("decided-too-large-at-least", number = attempt, share = percent(share))
        }
        Decision::NothingLeft => fl!("decided-nothing-left"),
        Decision::OutOfAttempts { attempts } => fl!("decided-out-of-attempts", count = attempts),
        Decision::LowersFrameRateToFit { from, fps: rate } => {
            fl!("decided-lowers-to-fit", fps = fps(rate), from = fps(from))
        }
        Decision::LowersFrameRateForLooks { from, fps: rate, attempt, ssim } => fl!(
            "decided-lowers-for-looks",
            fps = fps(rate),
            from = fps(from),
            number = attempt,
            alike = percent(ssim)
        ),
        Decision::LowerLooksNoBetter { fps: rate, ssim, kept, kept_fps, kept_ssim } => fl!(
            "decided-lower-no-better",
            fps = fps(rate),
            alike = percent(ssim),
            kept = kept,
            kept_fps = fps(kept_fps),
            kept_alike = percent(kept_ssim)
        ),
        Decision::Settled { attempt } => fl!("decided-settled", number = attempt),
        // Fallback for any future `#[non_exhaustive]` Decision variant.
        decision => decision.to_string(),
    }
}

fn reduction(reduction: &tgradish_core::tgradish_tgs::reduce::Reduction) -> String {
    use tgradish_core::tgradish_tgs::reduce::Reduction;
    match *reduction {
        Reduction::SnapToGrid { scale } => fl!("step-snapped", scale = scale),
        Reduction::MergeColours { distance } => {
            fl!("step-colours", distance = widgets::percent(distance * 100.0, 0))
        }
        Reduction::MergeFrames { changed } => {
            fl!("step-frames", changed = widgets::percent(changed * 100.0, 1))
        }
        Reduction::DropFrames { share } => {
            fl!("step-dropped", share = widgets::percent(share * 100.0, 0))
        }
        Reduction::Despeckle => fl!("step-specks"),
        Reduction::Downscale { factor } => {
            fl!("step-resolution", factor = widgets::percent(factor * 100.0, 0))
        }
        // Fallback for any future `#[non_exhaustive]` Reduction variant.
        other => format!("{other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use tgradish_core::presets::Format;
    use tgradish_core::webm::{Params, Rate};

    use super::*;
    use crate::jobs::{Attempt, Progress, TgsStage, WebmPlan};
    use crate::widgets::Fill;

    fn encode(number: u32, bytes: Option<u64>) -> Attempt {
        let params = Params { fps: 30.0, length: 1.0, rate: Rate::Bitrate(81.0) };
        Attempt { number, params, bytes, fits: false, ssim: None }
    }

    fn plan(attempts: u32) -> WebmPlan {
        WebmPlan { width: 512, height: 512, fps: 30.0, length: 1.0, attempts, limit: 262_144 }
    }

    /// Every text drawn last, and whether it was cut short.
    fn drawn(output: &egui::FullOutput) -> Vec<(String, bool)> {
        fn walk(shape: &egui::Shape, found: &mut Vec<(String, bool)>) {
            match shape {
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, found)),
                egui::Shape::Text(text) => {
                    found.push((text.galley.text().to_owned(), text.galley.elided));
                }
                _ => {}
            }
        }
        let mut found = Vec::new();
        output.shapes.iter().for_each(|shape| walk(&shape.shape, &mut found));
        found
    }

    /// In British English the bars of a conversion say all they say,
    /// their longest too, in the column beside the preview at its
    /// narrowest, in the smallest window, and at its usual width, and
    /// the bar beside Stop as narrow as it gets there; each bar keeping its
    /// height as what it says changes.
    #[test]
    fn says_every_step_of_converting_whole_and_keeps_each_bars_height() {
        use tgradish_core::tgradish_tgs::{reduce::Reduction, sticker::Step};
        crate::l10n::english();
        // the most each says: many steps of a large sticker, one that can't
        // fit, and encodes with long numbers, too large
        let step = Step::new(Reduction::Despeckle, 999_999, 0.0);
        let tgs = |stage, hopeless: bool| Progress {
            tgs: Some(stage),
            unchanged_bytes: Some(999_999),
            tgs_exact: hopeless,
            steps: vec![step.clone(); 100],
            ..Progress::default()
        };
        let long = |number, bytes| Attempt {
            number,
            params: Params { fps: 29.97, length: 1.0, rate: Rate::Bitrate(12_345.0) },
            bytes,
            fits: false,
            ssim: None,
        };
        let webm = |done: bool| Progress {
            webm: Some(plan(12)),
            attempts: vec![long(10, Some(999_999)), long(12, done.then_some(999_999))],
            fraction: Some(0.5),
            ..Progress::default()
        };
        // tried again after it missed, twice: once far too large
        let retried = Progress {
            missed: vec![Some(999_999), None],
            retries: vec![999_999, 999_999],
            ..tgs(TgsStage::Compressing, false)
        };
        let progresses = [
            tgs(TgsStage::Fitting, false),
            tgs(TgsStage::Compressing, true),
            webm(false),
            webm(true),
            retried,
        ];
        for width in [216.0, 288.0] {
            let mut heights = Vec::new();
            for progress in &progresses {
                let mut job = Job::waiting("clip.webm".into());
                job.status = Status::Running;
                job.progress = progress.clone();
                let mut harness =
                    egui_kittest::Harness::builder().with_size([400.0, 600.0]).build_ui(|ui| {
                        ui.set_max_width(width);
                        progress_column(ui, &job, None);
                        step_bar(ui, &job, egui::vec2(320.0, crate::actions::BAR_ROW));
                    });
                widgets::test_fonts(&harness);
                harness.run_steps(2);
                let output = harness.output().clone();
                for (text, elided) in drawn(&output) {
                    assert!(!elided, "{width}: {text:?} cut short");
                }
                heights.push(harness.ctx.globally_used_rect().height());
            }
            // an encode done, too large, as tall as it was at work
            assert_eq!(heights[2], heights[3], "{width}");
        }
    }

    #[test]
    fn keeps_an_encodes_height_as_it_comes_out_too_large_by_fewer_digits() {
        crate::l10n::english();
        // too large by fewer digits than the most it may say, 76.3%, at
        // the widths of the column beside the preview, from its narrowest
        let webm = |done: bool| Progress {
            webm: Some(plan(4)),
            attempts: vec![encode(1, done.then_some(200_000))],
            fraction: Some(0.5),
            ..Progress::default()
        };
        for width in (216..=288).step_by(2) {
            let height = |progress: Progress| {
                let mut job = Job::waiting("clip.webm".into());
                job.status = Status::Running;
                job.progress = progress;
                let mut harness =
                    egui_kittest::Harness::builder().with_size([400.0, 600.0]).build_ui(|ui| {
                        ui.set_max_width(width as f32);
                        progress_column(ui, &job, None);
                    });
                widgets::test_fonts(&harness);
                harness.run_steps(2);
                harness.ctx.globally_used_rect().height()
            };
            assert_eq!(height(webm(false)), height(webm(true)), "{width}");
        }
    }

    #[test]
    fn heads_each_column_of_the_encodes_over_it() {
        use egui_kittest::kittest::Queryable;
        let mut item = Item::new(1, vec!["clip.mp4".into()], false, Default::default());
        let mut job = Job::waiting("clip.webm".into());
        let kept = |number, fps, kbps, bytes, fits, ssim| Attempt {
            number,
            params: Params { fps, length: 1.0, rate: Rate::Bitrate(kbps) },
            bytes: Some(bytes),
            fits,
            ssim: Some(ssim),
        };
        job.progress.webm = Some(plan(8));
        job.progress.attempts = vec![
            kept(1, 30.0, 2400.0, 300_000, false, 0.97),
            kept(12, 25.0, 950.0, 158_600, true, 0.999),
        ];
        job.status = Status::Done(Box::new(jobs::Done {
            output: "clip.webm".into(),
            bytes: 158_600,
            limit: 262_144,
            changed: false,
            attempt: Some(12),
            spoofed: false,
            settled: false,
            json_bytes: None,
            shapes: None,
            problems: Vec::new(),
            drawing: None,
        }));
        let choices = item.choices.clone();
        item.made = Some(crate::item::Conversion::of(job, Format::Webm, choices));
        let mut harness = egui_kittest::Harness::new_ui(move |ui| details(ui, &item));
        harness.run();

        let rect = |label: &str| harness.get_by_label(label).rect();
        let size = fl!("column-size");
        let alike = fl!("column-alike");
        let columns: [(&str, [&str; 2]); 5] = [
            ("#", ["1", "12"]),
            ("fps", ["30", "25"]),
            ("kbit/s", ["2400", "950"]),
            (&size, ["114.4%", "60.5%"]),
            (&alike, ["97.0%", "99.9%"]),
        ];
        for (heading, cells) in columns {
            let heading = rect(heading);
            for cell in cells {
                let cell = rect(cell);
                // numbers lined up by their last digit, the heading's too
                let off = (heading.right() - cell.right()).abs();
                assert!(off < 0.5, "{heading:?} over {cell:?}");
                assert!(heading.bottom() <= cell.top(), "{heading:?} over {cell:?}");
            }
            // and close over the first row, not a field's height above it
            let gap = rect(cells[0]).top() - heading.bottom();
            assert!(gap < 8.0, "{gap} between {heading:?} and its first row");
        }
        // the tick in its own column, before the numbers, on the kept row
        let (tick, number) = (rect("✔"), rect("12"));
        assert!(tick.right() < rect("1").left(), "{tick:?}");
        assert!((tick.center().y - number.center().y).abs() < 0.5, "{tick:?}");
    }

    #[test]
    fn says_why_fitting_stopped_in_the_tables_terms() {
        use tgradish_core::webm::Decision;
        let stopped = decision_text(&Decision::InBand { attempt: 2, share: 0.99, least: 0.96 });
        assert_eq!(stopped, fl!("decided-in-band", number = 2, share = "99.0%", least = "96.0%"));
        let lowered = Decision::LowersFrameRateToFit { from: 29.97, fps: 20.0 };
        let lowered = decision_text(&lowered);
        assert_eq!(lowered, fl!("decided-lowers-to-fit", fps = "20", from = "29.97"));
    }

    #[test]
    fn the_current_step_is_only_the_one_at_work() {
        let (fill, text) = current_step(&Progress::default());
        assert!(matches!(fill, Fill::Unknown), "{text}");
        assert_eq!(text, fl!("starting"));

        // the encode at work, by its own progress, with how many there may be
        let mut progress = Progress {
            webm: Some(plan(8)),
            attempts: vec![encode(1, Some(300_000)), encode(2, None)],
            fraction: Some(0.4),
            ..Progress::default()
        };
        let (fill, text) = current_step(&progress);
        assert!(matches!(fill, Fill::Working(share) if share == 0.4), "{text}");
        assert!(text.contains('2') && text.contains('8'), "{text}");
        // between encodes it hasn't stopped
        progress.attempts.pop();
        assert!(matches!(current_step(&progress).0, Fill::Unknown));
        // only one is allowed: no ceiling to tell of
        progress.webm = Some(plan(1));
        progress.attempts = vec![encode(1, None)];
        let number = fl!("encode", number = 1, fps = "30", rate = "81 kbit/s");
        assert_eq!(current_step(&progress).1, number);

        // a .tgs says the stage it is at, not the others, and how far it
        // got, but while the art is read
        let compressing = Progress {
            tgs: Some(TgsStage::Compressing),
            tgs_done: 0.9,
            stage_done: 0.75,
            ..Progress::default()
        };
        let (fill, text) = current_step(&compressing);
        assert!(matches!(fill, Fill::Working(share) if share == 0.75), "{text}");
        assert_eq!(text, fl!("stage-compressing"));
        let reading = Progress { tgs: Some(TgsStage::Reading), ..Progress::default() };
        assert!(matches!(current_step(&reading).0, Fill::Unknown));
        // tried again once the result missed: that try, by its own progress
        let retried = Progress {
            missed: vec![Some(66_000)],
            retries: vec![62_000],
            stage_done: 0.25,
            ..compressing
        };
        let (fill, text) = current_step(&retried);
        assert!(matches!(fill, Fill::Working(share) if share == 0.25), "{text}");
        assert_eq!(text, fl!("retry", number = 2, aim = widgets::kib(62_000)));
    }

    #[test]
    fn says_what_each_try_at_fitting_a_tgs_came_to() {
        let progress = Progress {
            tgs: Some(TgsStage::Compressing),
            missed: vec![Some(66_000), None],
            retries: vec![62_000, 59_000],
            ..Progress::default()
        };
        // the first try's compressing, then each try again's bar
        let compressing = stages(&progress).pop().unwrap().1;
        // its fitting as it ended, whatever the tries again take off
        use tgradish_core::tgradish_tgs::{reduce::Reduction, sticker::Step};
        let step = |bytes| Step::new(Reduction::Despeckle, bytes, 0.0);
        let fitted = Progress {
            unchanged_bytes: Some(200_000),
            steps: vec![step(110_000), step(64_000), step(63_000)],
            first_steps: Some(2),
            ..progress.clone()
        };
        let fitting = stages(&fitted).into_iter().find(|(each, _)| *each == TgsStage::Fitting);
        let size = widgets::kib(64_000);
        assert_eq!(fitting.unwrap().1, fl!("stage-fitting-steps", count = 2, size = size));
        assert_eq!(compressing, fl!("stage-compressing", "missed", size = widgets::kib(66_000)));
        let aim = widgets::kib(62_000);
        let (second, widest) = retry_texts(0, 62_000, progress.missed.get(1).copied());
        assert_eq!(second, fl!("retry-done", "far", number = 2, aim = aim.as_str()));
        assert!(widest.len() >= second.len(), "{widest} {second}");
        let (third, _) = retry_texts(1, 59_000, progress.missed.get(2).copied());
        assert_eq!(third, fl!("retry", number = 3, aim = widgets::kib(59_000)));
        // what it came to said as a warning, from its size
        let done = fl!("retry-done", number = 2, aim = aim.as_str(), size = widgets::kib(66_000));
        assert!(done[missed_from(&done, Some(66_000))..].starts_with(&widgets::kib(66_000)));
    }

    #[test]
    fn a_tgs_shows_fitting_only_where_it_fits() {
        use tgradish_core::tgradish_tgs::{reduce::Reduction, sticker::Step};
        let bars = |progress: &Progress| -> Vec<TgsStage> {
            stages(progress).into_iter().map(|(stage, _)| stage).collect()
        };
        let unfitted = vec![TgsStage::Reading, TgsStage::Drawing, TgsStage::Compressing];
        // small enough as it is, and kept exactly as it is
        let mut progress = Progress { tgs: Some(TgsStage::Compressing), ..Progress::default() };
        assert_eq!(bars(&progress), unfitted);
        progress.tgs_exact = true;
        assert_eq!(bars(&progress), unfitted);
        // too large, and kept so all the same: compressed as it is
        progress.unchanged_bytes = Some(200_000);
        assert_eq!(bars(&progress), unfitted);
        assert_eq!(stages(&progress)[2].1, fl!("stage-compressing", "hopeless"));

        // too large, and fitted
        let mut progress = Progress {
            tgs: Some(TgsStage::Fitting),
            unchanged_bytes: Some(200_000),
            ..Progress::default()
        };
        let fitted =
            vec![TgsStage::Reading, TgsStage::Drawing, TgsStage::Fitting, TgsStage::Compressing];
        assert_eq!(bars(&progress), fitted);
        assert_eq!(
            current_step(&progress).1,
            fl!("stage-fitting-from", size = widgets::kib(200_000))
        );
        progress.steps.push(Step::new(Reduction::Despeckle, 60_000, 0.0));
        assert_eq!(bars(&progress), fitted);
        let fitting = fl!("stage-fitting-steps", count = 1, size = widgets::kib(60_000));
        assert_eq!(current_step(&progress).1, fitting);
    }
}
