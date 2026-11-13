//! What the last conversion of an item made, or what the one under way is
//! doing, beside the preview: its size against Telegram's limit, the
//! progress of each part, and how it was made to fit.

use eframe::egui::{self, Color32, RichText, vec2};

use crate::item::Item;
use crate::jobs::{self, Job, Status};
use crate::l10n::fl;
use crate::{theme, widgets};

/// The last result of an item in short: its size and what Telegram would
/// say, beside the preview.
pub(crate) fn summary(ui: &mut egui::Ui, item: &Item, queued: Option<usize>) {
    if let Some(conversion) = &item.job {
        progress_column(ui, &conversion.job, queued);
    }
    if let Some(failed) = &item.failed {
        widgets::section(ui, &fl!("last-conversion"));
        match &failed.job.status {
            Status::Failed(message) => {
                ui.colored_label(widgets::BAD, message);
            }
            Status::Exists(path) => {
                let path = path.display().to_string();
                ui.colored_label(widgets::WARN, fl!("path-in-the-way", path = path));
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
    if let Some(plan) = &made.job.progress.webm {
        // the encode kept, which fitting may have made at a lower
        // frame rate or shorter than planned
        let progress = &made.job.progress;
        let kept = progress.attempts.iter().find(|attempt| Some(attempt.number) == done.kept);
        let (fps, length) =
            kept.map_or((plan.fps, plan.length), |kept| (kept.params.fps, kept.params.length));
        let (width, height, fps) = (plan.width, plan.height, format!("{fps:.2}"));
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
    if done.lossy {
        widgets::note(ui, fl!("changed-to-fit"));
    }
    if made.job.replaces {
        ui.colored_label(widgets::WARN, format!("⚠ {}", fl!("replaced-earlier")));
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
    let done = widgets::GOOD.gamma_multiply(0.45);
    if let Some(stage) = progress.tgs {
        use jobs::TgsStage;
        // too large, and never to be changed: packed quickly, in red
        let hopeless = progress.tgs_exact && progress.unchanged_bytes.is_some();
        for (each, text) in stages(progress, stage) {
            use widgets::Fill;
            let (fill, colour) = match each.cmp(&stage) {
                std::cmp::Ordering::Less if hopeless && each == TgsStage::Fitting => {
                    (Fill::Share(1.0), widgets::BAD.gamma_multiply(0.45))
                }
                std::cmp::Ordering::Less => (Fill::Share(1.0), done),
                std::cmp::Ordering::Equal => (stage_fill(progress, stage), running),
                std::cmp::Ordering::Greater => (Fill::Share(0.0), running),
            };
            widgets::bar(ui, fill, &text, colour);
            ui.add_space(2.0);
        }
        return;
    }
    if progress.attempts.is_empty() {
        widgets::bar(ui, widgets::Fill::Unknown, &fl!("starting"), running);
    }
    for attempt in &progress.attempts {
        let rate = rate_text(attempt);
        let (number, fps) = (attempt.number, format!("{:.0}", attempt.params.fps));
        let text = fl!("encode", number = number, fps = fps.as_str(), rate = rate.as_str());
        match attempt.bytes {
            Some(bytes) => {
                let size = match &progress.webm {
                    Some(plan) => share(bytes, plan.limit),
                    None => widgets::kib(bytes),
                };
                let (text, colour) = if attempt.fits {
                    (fl!("encode-done", number = number, fps = fps, rate = rate, size = size), done)
                } else {
                    let text = fl!(
                        "encode-done",
                        "too-large",
                        number = number,
                        fps = fps,
                        rate = rate,
                        size = size
                    );
                    (text, widgets::BAD.gamma_multiply(0.45))
                };
                widgets::bar(ui, widgets::Fill::Share(1.0), &text, colour);
            }
            None => widgets::bar(ui, widgets::Fill::working(progress.fraction), &text, running),
        }
        ui.add_space(2.0);
    }
    if let Some(plan) = &progress.webm
        && plan.attempts > 1
    {
        widgets::note(ui, fl!("encodes-up-to", count = plan.attempts));
    }
}

/// The accent, half let through, so the text over it stays readable.
fn running_colour(ui: &egui::Ui) -> Color32 {
    theme::tint(theme::accent(ui.visuals()), 0.5)
}

/// What each stage of a `.tgs` conversion says, once it is at `stage`.
fn stages(progress: &jobs::Progress, stage: jobs::TgsStage) -> [(jobs::TgsStage, String); 4] {
    use jobs::TgsStage;
    let fits = progress.unchanged_bytes.is_none() && stage > TgsStage::Fitting;
    let hopeless = progress.tgs_exact && progress.unchanged_bytes.is_some();
    [
        (TgsStage::Reading, fl!("stage-reading")),
        (TgsStage::Drawing, fl!("stage-drawing")),
        (
            TgsStage::Fitting,
            match (progress.unchanged_bytes, progress.steps.last()) {
                // never changed: nothing is fitted
                (Some(bytes), _) if progress.tgs_exact => {
                    fl!("stage-kept-too-large", size = widgets::kib(bytes as u64))
                }
                (None, _) if progress.tgs_exact => fl!("stage-kept"),
                (Some(_), Some(step)) => {
                    let count = progress.steps.len();
                    let size = widgets::kib(step.bytes as u64);
                    fl!("stage-fitting-steps", count = count, size = size)
                }
                (Some(bytes), None) => {
                    fl!("stage-fitting-from", size = widgets::kib(bytes as u64))
                }
                (None, _) if fits => fl!("stage-fits"),
                (None, _) => fl!("stage-fitting"),
            },
        ),
        (
            TgsStage::Compressing,
            if hopeless { fl!("stage-compressing", "hopeless") } else { fl!("stage-compressing") },
        ),
    ]
}

/// How far the stage at work has got. It moves even while its share stays
/// put, as fitting's does through a round of trying reductions.
fn stage_fill(progress: &jobs::Progress, stage: jobs::TgsStage) -> widgets::Fill {
    match stage {
        jobs::TgsStage::Fitting => widgets::Fill::working(progress.tgs_fitting()),
        _ => widgets::Fill::Unknown,
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

/// The step a conversion is at and how far it has got. Only that step: how
/// many encodes fitting needs isn't known until it has them, and a `.tgs`
/// conversion's stages take very different times, so a share of all of it
/// would stall and jump.
fn current_step(progress: &jobs::Progress) -> (widgets::Fill, String) {
    use widgets::Fill;
    if let Some(stage) = progress.tgs {
        let text = stages(progress, stage).into_iter().find(|(each, _)| *each == stage);
        return (stage_fill(progress, stage), text.map(|(_, text)| text).unwrap_or_default());
    }
    let Some(attempt) = progress.attempts.last() else {
        return (Fill::Unknown, fl!("starting"));
    };
    let (number, rate) = (attempt.number, rate_text(attempt));
    let fps = format!("{:.0}", attempt.params.fps);
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
            ui.label(fl!("art-tempo", tempo = format!("{:.2}", report.tempo)));
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
    if let Some(bytes) = progress.unchanged_bytes {
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
            let columns = if scored { 5 } else { 4 };
            // the kept attempt's tick in a column of its own, so the
            // numbers stay in line
            let grid = egui::Grid::new(("attempts", item.id))
                .num_columns(columns + 1)
                .spacing([10.0, 4.0]);
            grid.striped(true).show(ui, |ui| {
                // as wide as the tick from the start, so nothing moves when
                // one is kept
                let font = egui::TextStyle::Body.resolve(ui.style());
                let tick = ui.painter().layout_no_wrap("✔".into(), font, Color32::PLACEHOLDER);
                ui.allocate_exact_size(vec2(tick.size().x, 0.0), egui::Sense::hover());
                let headers = [
                    ("#".to_owned(), fl!("column-number")),
                    ("fps".to_owned(), fl!("column-fps")),
                    ("kbit/s".to_owned(), fl!("column-rate")),
                    (fl!("column-size"), fl!("column-size", "hint")),
                    (fl!("column-alike"), fl!("column-alike", "hint")),
                ];
                for (header, hint) in &headers[..columns] {
                    ui.label(RichText::new(header).weak()).on_hover_text(hint);
                }
                ui.end_row();
                for attempt in &progress.attempts {
                    let kept = done.kept == Some(attempt.number);
                    ui.label(if kept { "✔" } else { "" });
                    let number = RichText::new(attempt.number.to_string());
                    ui.label(if kept { number.strong() } else { number });
                    ui.label(widgets::trimmed(attempt.params.fps));
                    ui.label(match attempt.params.rate {
                        tgradish_core::webm::Rate::Bitrate(kbps) => format!("{kbps:.0}"),
                        tgradish_core::webm::Rate::Crf(crf) => {
                            fl!("rate-quality-short", crf = crf)
                        }
                        tgradish_core::webm::Rate::Lossless => fl!("rate-lossless"),
                        rate => format!("{rate:?}"),
                    });
                    match attempt.bytes {
                        Some(bytes) => {
                            let colour = if attempt.fits { widgets::GOOD } else { widgets::BAD };
                            ui.colored_label(colour, share(bytes, done.limit))
                                .on_hover_text(widgets::kib(bytes))
                        }
                        None => ui.label("—"),
                    };
                    if scored {
                        let alike = attempt.ssim.map(|ssim| format!("{:.1}%", ssim * 100.0));
                        ui.label(alike.unwrap_or_default());
                    }
                    ui.end_row();
                }
            });
        });
    }
    for warning in &progress.warnings {
        ui.colored_label(widgets::WARN, format!("⚠ {warning}"));
    }
}

/// `bytes` as a share of `limit`, as the attempts' sizes show.
fn share(bytes: u64, limit: u64) -> String {
    format!("{:.1}%", bytes as f64 / limit.max(1) as f64 * 100.0)
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
        // one added to tgradish-tgs since, which the window has no words for
        other => format!("{other:?}"),
    }
}

#[cfg(test)]
mod tests {
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

        // a .tgs says the stage it is at, not the others
        let compressing = Progress { tgs: Some(TgsStage::Compressing), ..Progress::default() };
        let (fill, text) = current_step(&compressing);
        assert!(matches!(fill, Fill::Unknown));
        assert_eq!(text, fl!("stage-compressing"));
    }
}
