//! What the last conversion of an item made, or what the one under way is
//! doing, beside the preview: its size against Telegram's limit, the
//! progress of each part, and how it was made to fit.

use eframe::egui::{self, Color32, RichText, vec2};

use crate::item::Item;
use crate::jobs::{self, Job, Status};
use crate::l10n::fl;
use crate::{theme, widgets};

/// The last result of an item in short: its size and what Telegram would
/// say, beside the preview, and, where it is `copyable` as the bar's Copy
/// copies it, that it is pasted into Telegram so.
pub(crate) fn summary(ui: &mut egui::Ui, item: &Item, queued: Option<usize>, copyable: bool) {
    if let Some(conversion) = &item.job {
        progress_column(ui, &conversion.job, queued);
    }
    if let Some(failed) = &item.failed {
        widgets::section(ui, &fl!("last-conversion"));
        match &failed.job.status {
            Status::Failed(message) => {
                let said = ui.colored_label(theme::bad(ui.visuals()), message);
                // as the bar, which says it in short, asked
                let id = egui::Id::new(crate::actions::POINT_AT_FAILURE);
                if ui.data_mut(|data| data.remove_temp::<bool>(id)).unwrap_or(false) {
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
    // a result is only kept once it is done
    let Status::Done(done) = &made.job.status else { return };
    widgets::gauge(
        ui,
        done.bytes,
        done.limit,
        fl!("of-limit", used = widgets::kib(done.bytes), limit = widgets::kib(done.limit)),
    );
    ui.add_space(4.0);
    widgets::problems(ui, &done.problems);
    // the way on, once converting is done, with a result Telegram takes
    let refused = done.problems.iter().any(|problem| problem.refused);
    if copyable && item.job.is_none() && item.result_is_current() && !refused {
        let keys = crate::keys::keys_text(crate::keys::Command::CopyResult);
        widgets::note(ui, fl!("result-copy", keys = keys));
    }
    if let Some(plan) = &made.job.progress.webm {
        // the encode kept, which fitting may have made at a lower
        // frame rate or shorter than planned
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
    // stopped early, as asked: which encode is kept, as the table ticks it
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

/// What a conversion is doing, a bar for each part of it: the stages of a
/// `.tgs`, or each encode of a WebM.
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
            let (fill, colour) = match each.cmp(&stage) {
                std::cmp::Ordering::Less => (Fill::Share(1.0), done),
                std::cmp::Ordering::Equal => (stage_fill(progress, stage), running),
                std::cmp::Ordering::Greater => (Fill::Share(0.0), running),
            };
            widgets::steady_bar(ui, fill, &text, &widest_stage(each), colour);
            ui.add_space(2.0);
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
        // as it says once done, too large, the most it says
        let widest = widgets::zeros(&fl!(
            "encode-done",
            "too-large",
            number = number,
            fps = fps.as_str(),
            rate = rate.as_str(),
            size = if progress.webm.is_some() { "100.0%" } else { "1000.0 KiB" }
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
                    widgets::steady_bar(ui, widgets::Fill::Share(1.0), &text, &widest, done);
                } else {
                    // too large is how fitting goes on, not a failure: a
                    // plain bar, only its size and verdict a warning
                    let text = fl!(
                        "encode-done",
                        "too-large",
                        number = number,
                        fps = fps,
                        rate = rate,
                        size = size.as_str()
                    );
                    let from = text.rfind(&size).unwrap_or(text.len());
                    widgets::bar_spent(ui, &text, from);
                }
            }
            None => {
                let fill = widgets::Fill::working(progress.fraction);
                widgets::steady_bar(ui, fill, &text, &widest, running);
            }
        }
        ui.add_space(2.0);
    }
    if let Some(plan) = &progress.webm
        && plan.attempts > 1
    {
        widgets::note(ui, fl!("encodes-up-to", count = plan.attempts));
    }
}

/// The accent, half let through, or less where the text over it wouldn't
/// read on that.
fn running_colour(ui: &egui::Ui) -> Color32 {
    theme::fill(ui.visuals(), 0.5)
}

/// What each stage of a `.tgs` conversion says, a bar each. Fitting has
/// one only once the sticker is found too large and fitting starts: one
/// that fits, or one kept exactly as it is, is never fitted, and a bar for
/// it would stand for nothing.
fn stages(progress: &jobs::Progress) -> Vec<(jobs::TgsStage, String)> {
    use jobs::TgsStage;
    let hopeless = progress.tgs_exact && progress.unchanged_bytes.is_some();
    let fitting = match (progress.unchanged_bytes, progress.steps.last()) {
        _ if progress.tgs_exact => None,
        (_, Some(step)) => {
            let (count, size) = (progress.steps.len(), widgets::kib(step.bytes as u64));
            Some(fl!("stage-fitting-steps", count = count, size = size))
        }
        (Some(bytes), None) => Some(fl!("stage-fitting-from", size = widgets::kib(bytes as u64))),
        (None, None) => None,
    };
    let compressing =
        if hopeless { fl!("stage-compressing", "hopeless") } else { fl!("stage-compressing") };
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

/// The most the bar of `stage` of a `.tgs` conversion says as it goes on.
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
        TgsStage::Compressing => {
            vec![fl!("stage-compressing"), fl!("stage-compressing", "hopeless")]
        }
    };
    texts.into_iter().max_by_key(String::len).unwrap_or_default()
}

/// How far the stage at work has got, as converting says: reading the art
/// says nothing until it is read.
fn stage_fill(progress: &jobs::Progress, stage: jobs::TgsStage) -> widgets::Fill {
    match stage {
        jobs::TgsStage::Reading => widgets::Fill::Unknown,
        _ => widgets::Fill::Working(progress.stage_done),
    }
}

/// The rate an encode runs at.
fn rate_text(attempt: &jobs::Attempt) -> String {
    match attempt.params.rate {
        tgradish_core::webm::Rate::Bitrate(kbps) => format!("{kbps:.0} kbit/s"),
        tgradish_core::webm::Rate::Crf(crf) => fl!("rate-quality", crf = crf),
        tgradish_core::webm::Rate::Lossless => fl!("rate-lossless"),
        rate => format!("{rate:?}"),
    }
}

/// The step a conversion is at and how far it has got. Only that step,
/// as the column of every step beside the preview shows them: how many
/// encodes fitting a WebM needs isn't known until it has them, and the
/// list's bar shows how far all of a `.tgs` got.
fn current_step(progress: &jobs::Progress) -> (widgets::Fill, String) {
    use widgets::Fill;
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
    // done, it is being scored or the next is about to start
    let fill =
        if attempt.bytes.is_some() { Fill::Unknown } else { Fill::working(progress.fraction) };
    (fill, text)
}

/// A bar of `size` showing the step a conversion is at, for the bar beside
/// Stop, which never scrolls away.
pub(crate) fn step_bar(ui: &mut egui::Ui, job: &Job, size: egui::Vec2) {
    let (fill, text) = current_step(&job.progress);
    widgets::bar_sized(ui, size, fill, &text, running_colour(ui));
}

/// How the last result was made, under its size beside the preview.
pub(crate) fn details(ui: &mut egui::Ui, item: &Item) {
    let Some(made) = &item.made else { return };
    let Status::Done(done) = &made.job.status else { return };
    let progress = &made.job.progress;
    ui.add_space(6.0);
    // as plain as the result's size above it
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
    // kept as it is, or with nothing fitting could take off, it wasn't
    // changed to fit, however large it was
    if let (Some(bytes), false) = (progress.unchanged_bytes, progress.steps.is_empty()) {
        ui.label(fl!("art-before-fitting", size = widgets::kib(bytes as u64)));
    }
    for step in &progress.steps {
        let (change, size) = (reduction(&step.reduction), widgets::kib(step.bytes as u64));
        ui.label(format!("  • {}", fl!("art-step", change = change, size = size)));
    }
    if !progress.attempts.is_empty() {
        // narrow, for the column beside the preview: units in the headers,
        // and sideways where it still doesn't fit
        // how like the input each looks, where any was scored
        let scored = progress.attempts.iter().any(|attempt| attempt.ssim.is_some());
        egui::ScrollArea::horizontal().id_salt(("attempts", item.id)).show(ui, |ui| {
            let columns = if scored { 6 } else { 5 };
            // the tick has a column of its own, headed by nothing, so the
            // numbers stay in line under theirs
            let headers = [
                (String::new(), String::new()),
                ("#".to_owned(), fl!("column-number")),
                ("fps".to_owned(), fl!("column-fps")),
                ("kbit/s".to_owned(), fl!("column-rate")),
                (fl!("column-size"), fl!("column-size", "hint")),
                (fl!("column-alike"), fl!("column-alike", "hint")),
            ];
            // each cell's text, and what it says on hover
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
                    let alike = attempt.ssim.map(|ssim| format!("{:.1}%", ssim * 100.0));
                    row.push((RichText::new(alike.unwrap_or_default()), String::new()));
                }
                rows.push(row);
            }
            // numbers lined up by their last digit, in columns as wide as
            // their widest
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
            // no wider than its columns, as a grid makes them at least
            // the width of a field otherwise, and rows no taller than a
            // line, so the headings sit right over what they head
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
    // settling is said with the result, above
    let said = |decision: &&tgradish_core::webm::Decision| {
        !matches!(decision, tgradish_core::webm::Decision::Settled { .. })
    };
    for decision in progress.decisions.iter().filter(said) {
        widgets::note(ui, decision_text(decision));
    }
    for warning in &progress.warnings {
        ui.colored_label(theme::warn(ui.visuals()), format!("⚠ {warning}"));
    }
}

/// `bytes` as a share of `limit`, as the attempts' sizes show.
fn share(bytes: u64, limit: u64) -> String {
    percent(bytes as f64 / limit.max(1) as f64)
}

/// A share, or SSIM, from 0 to 1, as the table shows it.
fn percent(share: f64) -> String {
    format!("{:.1}%", share * 100.0)
}

/// Why fitting stopped where it did, or tried a lower frame rate, in the
/// GUI's words.
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
        // one added to core since, which the GUI has no words for
        decision => decision.to_string(),
    }
}

fn reduction(reduction: &tgradish_core::tgradish_tgs::reduce::Reduction) -> String {
    use tgradish_core::tgradish_tgs::reduce::Reduction;
    match *reduction {
        Reduction::SnapToGrid { scale } => fl!("step-snapped", scale = scale),
        Reduction::MergeColours { distance } => {
            fl!("step-colours", distance = format!("{:.0}", distance * 100.0))
        }
        Reduction::MergeFrames { changed } => {
            fl!("step-frames", changed = format!("{:.1}", changed * 100.0))
        }
        Reduction::DropFrames { share } => {
            fl!("step-dropped", share = format!("{:.0}", share * 100.0))
        }
        Reduction::Despeckle => fl!("step-specks"),
        Reduction::Downscale { factor } => {
            fl!("step-resolution", factor = format!("{:.0}", factor * 100.0))
        }
        // one added to tgradish-tgs since, which the GUI has no words for
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
        let progresses = [
            tgs(TgsStage::Fitting, false),
            tgs(TgsStage::Compressing, true),
            webm(false),
            webm(true),
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
