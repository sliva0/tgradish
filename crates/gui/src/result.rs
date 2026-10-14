//! What the last conversion of an item made, or what the one under way is
//! doing, beside the preview: its size against Telegram's limit, the
//! progress of each part, and how it was made to fit.

use eframe::egui::{self, RichText};

use crate::item::Item;
use crate::jobs::{self, Job, Status};
use crate::{theme, widgets};

/// The last result of an item in short: its size and what Telegram would
/// say, beside the preview.
pub(crate) fn summary(ui: &mut egui::Ui, item: &Item, queued: Option<usize>) {
    if let Some((job, ..)) = &item.job {
        progress_column(ui, job, queued);
    }
    if let Some(failed) = &item.failed {
        widgets::section(ui, "Last conversion");
        match &failed.job.status {
            Status::Failed(message) => {
                ui.colored_label(widgets::BAD, message);
            }
            Status::Exists(path) => {
                ui.colored_label(widgets::WARN, format!("{} is in the way", path.display()));
            }
            Status::Cancelled => widgets::note(ui, "Stopped before it was done"),
            Status::Done(_) | Status::Waiting | Status::Running => {}
        }
    }
    let Some(made) = &item.made else { return };
    widgets::section(
        ui,
        if item.result_is_current() { "Result" } else { "Result, from earlier settings" },
    );
    // a result is only kept once it is done
    let Status::Done(done) = &made.job.status else { return };
    widgets::gauge(
        ui,
        done.bytes,
        done.limit,
        format!("{} of {}", widgets::kib(done.bytes), widgets::kib(done.limit)),
    );
    ui.add_space(4.0);
    widgets::problems(ui, &done.problems);
    if let Some(plan) = &made.job.progress.webm {
        let spoofed = if done.spoofed { ", duration spoofed" } else { "" };
        // the encode kept, which fitting may have made at a lower
        // frame rate or shorter than planned
        let progress = &made.job.progress;
        let kept = progress.attempts.iter().find(|attempt| Some(attempt.number) == done.kept);
        let (fps, length) =
            kept.map_or((plan.fps, plan.length), |kept| (kept.params.fps, kept.params.length));
        widgets::note(
            ui,
            format!(
                "{} × {} px, {fps:.2} fps, {}{spoofed}",
                plan.width,
                plan.height,
                widgets::seconds(length)
            ),
        );
    }
    if done.lossy {
        widgets::note(ui, "Changed to fit: see Details below");
    }
    if made.job.replaces {
        ui.colored_label(widgets::WARN, "⚠ Replaced an earlier result made by tgradish");
    }
}

/// What a conversion is doing, a bar for each part of it: the stages of a
/// `.tgs`, or each encode of a WebM.
fn progress_column(ui: &mut egui::Ui, job: &Job, queued: Option<usize>) {
    widgets::section(ui, "Converting");
    if job.is_waiting() {
        widgets::note(
            ui,
            match queued {
                Some(0) | None => "Next to convert".to_owned(),
                Some(ahead) => format!("Waiting for {ahead} more"),
            },
        );
        return;
    }
    let progress = &job.progress;
    // the accent, half let through, so the text over it stays readable
    let running = theme::tint(theme::accent(ui.visuals()), 0.5);
    let done = widgets::GOOD.gamma_multiply(0.45);
    if let Some(stage) = progress.tgs {
        use jobs::TgsStage;
        let fits = progress.lossless_bytes.is_none() && stage > TgsStage::Fitting;
        // too large, and never to be changed: packed quickly, in red
        let hopeless = progress.tgs_lossless && progress.lossless_bytes.is_some();
        let stages = [
            (TgsStage::Reading, "Reading the art".to_owned()),
            (TgsStage::Drawing, "Drawing it as shapes".to_owned()),
            (
                TgsStage::Fitting,
                match (progress.lossless_bytes, progress.steps.last()) {
                    // never changed: nothing is fitted
                    (Some(bytes), _) if progress.tgs_lossless => {
                        format!("Kept as it is: about {}, too large", widgets::kib(bytes as u64))
                    }
                    (None, _) if progress.tgs_lossless => "Kept as it is".to_owned(),
                    (Some(_), Some(step)) => {
                        let count = progress.steps.len();
                        let steps = if count == 1 { "step" } else { "steps" };
                        format!(
                            "Fitting into 64 KiB: {count} {steps}, about {}",
                            widgets::kib(step.bytes as u64)
                        )
                    }
                    (Some(bytes), None) => {
                        format!("Fitting into 64 KiB from {}", widgets::kib(bytes as u64))
                    }
                    (None, _) if fits => "Fits as it is".to_owned(),
                    (None, _) => "Fitting into 64 KiB, if it has to".to_owned(),
                },
            ),
            (
                TgsStage::Compressing,
                if hopeless { "Compressing quickly: it can't fit" } else { "Compressing" }
                    .to_owned(),
            ),
        ];
        for (each, text) in stages {
            use widgets::Fill;
            // the stage at work moves even while its share stays put, as
            // fitting's does through a round of trying reductions
            let (fill, colour) = match each.cmp(&stage) {
                std::cmp::Ordering::Less if hopeless && each == TgsStage::Fitting => {
                    (Fill::Share(1.0), widgets::BAD.gamma_multiply(0.45))
                }
                std::cmp::Ordering::Less => (Fill::Share(1.0), done),
                std::cmp::Ordering::Equal if each == TgsStage::Fitting => {
                    (Fill::working(progress.tgs_fitting()), running)
                }
                std::cmp::Ordering::Equal => (Fill::Unknown, running),
                std::cmp::Ordering::Greater => (Fill::Share(0.0), running),
            };
            widgets::bar(ui, fill, &text, colour);
            ui.add_space(2.0);
        }
        return;
    }
    if progress.attempts.is_empty() {
        widgets::bar(ui, widgets::Fill::Unknown, "Starting", running);
    }
    for attempt in &progress.attempts {
        let rate = match attempt.params.rate {
            tgradish_core::events::Rate::Bitrate(kbps) => format!("{kbps:.0} kbit/s"),
            tgradish_core::events::Rate::Crf(crf) => format!("quality {crf}"),
            tgradish_core::events::Rate::Lossless => "lossless".into(),
        };
        let text = format!("Encode {}: {:.0} fps, {rate}", attempt.number, attempt.params.fps);
        match attempt.bytes {
            Some(bytes) => {
                let (mark, colour) = if attempt.fits {
                    ("fits", done)
                } else {
                    ("too large", widgets::BAD.gamma_multiply(0.45))
                };
                let text = format!("{text} → {}, {mark}", widgets::kib(bytes));
                widgets::bar(ui, widgets::Fill::Share(1.0), &text, colour);
            }
            None => widgets::bar(ui, widgets::Fill::working(progress.fraction), &text, running),
        }
        ui.add_space(2.0);
    }
    if let Some(plan) = &progress.webm
        && plan.attempts > 1
    {
        widgets::note(
            ui,
            format!(
                "Up to {}, until one lands just under the limit",
                widgets::count(plan.attempts, "encode")
            ),
        );
    }
}

/// How the last result was made, under its size beside the preview.
pub(crate) fn details(ui: &mut egui::Ui, item: &Item) {
    let Some(made) = &item.made else { return };
    let Status::Done(done) = &made.job.status else { return };
    let progress = &made.job.progress;
    ui.add_space(6.0);
    // as plain as the result's size above it
    use widgets::count;
    if let Some(report) = &progress.report {
        let scale = match report.scale {
            1 => String::new(),
            scale => format!(", each {scale} × {scale} input pixels"),
        };
        ui.label(format!(
            "{} × {} art pixels{scale}; {}, {}",
            report.width,
            report.height,
            count(report.colours, "colour"),
            count(report.frames, "frame"),
        ));
        if report.tempo != 1.0 {
            ui.label(format!("Plays {:.2}× as fast as the input", report.tempo));
        }
        if let Some(likely) = report.likely_scale {
            ui.label(format!(
                "Most of it looks like {0}× art with some pixels off the grid; an art pixel \
                 size of {0} snaps them",
                likely.scale
            ));
        }
    }
    if let (Some(json), Some((layers, rectangles))) = (done.json_bytes, done.shapes) {
        ui.label(format!(
            "{} of {}, {} unpacked",
            count(layers, "layer"),
            count(rectangles, "rectangle"),
            widgets::kib(json)
        ));
    }
    if let Some(bytes) = progress.lossless_bytes {
        ui.label(format!("About {} as it is; changed to fit:", widgets::kib(bytes as u64)));
    }
    for step in &progress.steps {
        let after = widgets::kib(step.bytes as u64);
        ui.label(format!("  • {}, about {after} after", reduction(&step.reduction)));
    }
    if !progress.attempts.is_empty() {
        // narrow, for the column beside the preview: units in the headers,
        // and sideways where it still doesn't fit
        egui::ScrollArea::horizontal().id_salt(("attempts", item.id)).show(ui, |ui| {
            let grid = egui::Grid::new(("attempts", item.id)).num_columns(5).spacing([10.0, 4.0]);
            grid.striped(true).show(ui, |ui| {
                let headers = [
                    ("#", "Each encode in turn; ✔ the one kept"),
                    ("fps", "Frames a second"),
                    ("kbit/s", "Bitrate, or the quality it was encoded at"),
                    ("KiB", "Size, green where it fits the limit"),
                    ("alike", "How like the input it looks (SSIM)"),
                ];
                for (header, hint) in headers {
                    ui.label(RichText::new(header).weak()).on_hover_text(hint);
                }
                ui.end_row();
                for attempt in &progress.attempts {
                    let kept = done.kept == Some(attempt.number);
                    let number = attempt.number.to_string();
                    let number = if kept { format!("✔ {number}") } else { number };
                    let number = RichText::new(number);
                    ui.label(if kept { number.strong() } else { number });
                    ui.label(widgets::trimmed(attempt.params.fps));
                    ui.label(match attempt.params.rate {
                        tgradish_core::events::Rate::Bitrate(kbps) => format!("{kbps:.0}"),
                        tgradish_core::events::Rate::Crf(crf) => format!("q {crf}"),
                        tgradish_core::events::Rate::Lossless => "lossless".into(),
                    });
                    match attempt.bytes {
                        Some(bytes) => {
                            let colour = if attempt.fits { widgets::GOOD } else { widgets::BAD };
                            ui.colored_label(colour, format!("{:.1}", bytes as f64 / 1024.0))
                        }
                        None => ui.label("—"),
                    };
                    let alike = attempt.ssim.map(|ssim| format!("{:.1}%", ssim * 100.0));
                    ui.label(alike.unwrap_or_default());
                    ui.end_row();
                }
            });
        });
    }
    for warning in &progress.warnings {
        ui.colored_label(widgets::WARN, format!("⚠ {warning}"));
    }
}

fn reduction(reduction: &tgradish_tgs::reduce::Reduction) -> String {
    use tgradish_tgs::reduce::Reduction;
    match *reduction {
        Reduction::SnapToGrid { scale } => format!("pixels snapped to a grid of {scale}"),
        Reduction::MergeColours { distance } => {
            format!("colours closer than {:.0}% merged", distance * 100.0)
        }
        Reduction::MergeFrames { changed } => {
            format!("frames that change under {:.1}% merged", changed * 100.0)
        }
        Reduction::DropFrames { share } => format!("{:.0}% of frames dropped", share * 100.0),
        Reduction::Despeckle => "single stray pixels removed".into(),
        Reduction::Downscale { factor } => format!("resolution lowered to {:.0}%", factor * 100.0),
    }
}
