//! Background worker scheduling for probing/decoding inputs and results and
//! running conversions without blocking the UI thread.
//!
//! [`App::pump`] runs on every frame to poll finished tasks, start queued
//! work, and evict decoded full-resolution frames (as they take much memory)
//! for items that are not currently viewed.

use eframe::egui;
use tgradish_core::backend::Backend;
use tgradish_core::presets::{Format, FormatOptions};
use tgradish_core::webm::Options;
use tgradish_core::{tgs, webp};

use crate::app::App;
use crate::files::THUMB;
use crate::item::{Conversion, Item, Kind, Recipe, Show};
use crate::jobs::{Done, Setup, Status};
use crate::l10n::fl;
use crate::media::{self, Art, Clip, Load, Task, Video};
use crate::outputs::Written;

/// Maximum concurrent background input loads besides the selected item.
const READING: usize = 2;

impl App {
    /// Builds a conversion [`Setup`] for `recipe` using `backend` (which
    /// `.tgs` conversions only require when decoding video inputs) and the
    /// configured worker thread limit.
    pub(crate) fn setup(
        &self,
        recipe: Recipe,
        backend: &Result<Backend, String>,
    ) -> Result<Setup, String> {
        let options = match recipe {
            Recipe::Webm(options) => FormatOptions::Webm(options),
            Recipe::Tgs(options) => FormatOptions::Tgs(options),
            Recipe::Webp(options) => FormatOptions::Webp(options),
        };
        let threads = self.config.threads;
        match backend {
            Ok(backend) => Ok(Setup { options, backend: Some(backend.clone()), threads }),
            Err(_) if options.format() == Format::Tgs => {
                Ok(Setup { options, backend: None, threads })
            }
            Err(why) => Err(why.clone()),
        }
    }

    /// Polls active loads and conversions, evicts unused frame buffers, and
    /// starts due background tasks.
    pub(crate) fn pump(&mut self, ctx: &egui::Context) {
        let backend = self.backend();
        // Retry failed or in-progress loads whenever the ffmpeg backend
        // changes.
        if self.read_with.as_ref() != Some(&backend) {
            self.items.iter_mut().for_each(read_again);
            self.read_with = Some(backend.clone());
        }
        let selected = self.selected;
        let mut reading = self
            .items
            .iter()
            .filter(|item| {
                item.sniffed.is_loading()
                    || item.video.is_loading()
                    || item.art.is_loading()
                    || item.result_clip.is_loading()
            })
            .count();
        for item in &mut self.items {
            // A `.tgs` that keeps the crop's shape (`Resize::Contain`) keeps
            // the crop square, shown or not.
            crate::file_settings::keep_square(item, &self.presets, &self.config);
            item.recipe = item.choices.recipe(item.format, &self.presets, &self.config).ok();
            check_earlier(ctx, item);
            let is_selected = selected == Some(item.id);
            // Retain decoded frames for items waiting in the conversion queue.
            let queued = item.job.as_ref().is_some_and(|conversion| conversion.job.is_waiting());
            let kept = is_selected || self.recent.contains(&item.id) || queued;
            if item.sniffed.poll() {
                // Fall back to the extension-derived kind if magic-byte sniffing fails.
                let kind = item.sniffed.ready().copied().unwrap_or(item.kind);
                item.take_kind(kind);
            }
            let finished = item.video.poll() | item.art.poll();
            if let Some((_, part)) = &mut item.part {
                part.poll();
            }
            if let Some((_, still)) = &mut item.still {
                still.poll();
            }
            item.result_clip.poll();
            let tgs = item.format == Format::Tgs;
            if finished || item.facts.as_ref().is_none_or(|facts| facts.art() != tgs) {
                item.take_facts();
            }
            if finished {
                settle(ctx, item);
                if !kept {
                    release(item);
                }
            }
            item.find_blockers(backend.is_ok());
            if item.result_thumb.is_none()
                && let Some(clip) = item.result_clip.ready()
            {
                item.result_thumb = Some(ctx.load_texture(
                    format!("result-{}", item.id),
                    clip.thumbnail(THUMB as u32 * 2),
                    texture_options(clip),
                ));
                if !kept {
                    item.result_clip = Load::Idle;
                }
            }
            // Decode the result clip when selected or when its thumbnail is not yet built.
            let result_wanted = is_selected || item.result_thumb.is_none();
            if result_wanted
                && item.result_clip.is_idle()
                && item.made.is_some()
                && (is_selected || reading < READING)
            {
                load_result_clip(ctx, item, &backend);
                reading += usize::from(item.result_clip.is_loading());
            }
            let wanted = is_selected || item.input_thumb.is_none() || queued;
            if !wanted || (!is_selected && reading >= READING) {
                continue;
            }
            // Sniff file content first to determine which loader to run.
            if item.sniffed.is_idle() {
                let path = item.inputs[0].clone();
                item.sniffed = Load::Loading(Task::spawn(ctx, move |_| Ok(Kind::of_file(&path))));
                reading += 1;
            }
            if item.sniffed.is_loading() {
                continue;
            }
            let reads_art =
                item.format == Format::Tgs || (item.format_guessed && item.kind == Kind::Image);
            let read_with = item.choices.reading(&self.presets, &self.config);
            if reads_art
                && (item.art.is_idle() || (item.art_reading != read_with && !item.art.is_loading()))
            {
                item.art_reading = read_with;
                item.art.reload(Art::load(
                    ctx,
                    item.input(),
                    item.art_reading.clone(),
                    backend.as_ref().ok().cloned(),
                ));
                reading += 1;
            } else if matches!(item.format, Format::Webm | Format::Webp)
                && !(item.format_guessed && item.art.is_loading())
            {
                // Re-render Aseprite or frame-sequence inputs when tag, layer,
                // or fps choices change.
                let drawing = match item.kind {
                    Kind::Aseprite | Kind::Frames => {
                        item.choices.drawing(item.format, &self.presets, &self.config)
                    }
                    Kind::Video | Kind::Image => Options::default(),
                };
                let redraw = item.video_drawing != drawing && !item.video.is_loading();
                if item.video.is_idle() || redraw {
                    if redraw {
                        (item.part, item.still) = (None, None);
                    }
                    item.video_drawing = drawing.clone();
                    let input = item.input();
                    match &backend {
                        Ok(backend) => {
                            item.video.reload(Video::load(ctx, backend.clone(), input, drawing))
                        }
                        Err(message) => item.video = Load::Failed(message.clone()),
                    }
                    reading += 1;
                }
            }
        }
        self.load_detail(ctx, &backend);
        self.convert_next(ctx, &backend);
    }

    /// Loads higher-detail video frames for the selected item once playback or
    /// trim controls settle: the trimmed segment at full frame rate, and the
    /// current paused frame at full resolution.
    fn load_detail(&mut self, ctx: &egui::Context, backend: &Result<Backend, String>) {
        let Some(id) = self.selected else { return };
        let Ok(backend) = backend else { return };
        let Some(item) = self.item_mut(id) else { return };
        if item.format == Format::Tgs {
            return;
        }
        let range = item.range().map(|(start, end)| (start, end - start));
        // Rendered pixel-art clips already contain every frame at full resolution.
        let Some(video) = item.video.ready().filter(|video| !video.complete) else { return };
        let still = video.probe.still_image;
        if !item.view.playing || still {
            let time = if still { 0.0 } else { item.view.time };
            let loaded = item.still.as_ref().is_some_and(|(at, _)| *at == time);
            if !loaded && settled(ctx, ("still", id), (time, 0.0), 0.25) {
                let task = video.load_still(ctx, backend.clone(), time);
                item.still = Some((time, Load::Loading(task)));
            }
        } else {
            item.still = None;
        }
        let Some(range) = range else { return };
        if !item.part.as_ref().is_some_and(|(loaded, _)| *loaded == range)
            && settled(ctx, ("range", id), range, 0.4)
        {
            let task = video.load_part(ctx, backend.clone(), range);
            item.part = Some((range, Load::Loading(task)));
        }
    }

    fn convert_next(&mut self, ctx: &egui::Context, backend: &Result<Backend, String>) {
        let mut finished = Vec::new();
        for item in &mut self.items {
            if let Some(Conversion { job, .. }) = &mut item.job
                && (job.poll() || job.is_finished())
            {
                finished.push(item.id);
            }
        }
        for id in finished {
            let selected = self.selected == Some(id);
            let item = self.item_mut(id).expect("just found");
            let mut made = item.job.take().expect("just found");
            let Status::Done(done) = &mut made.job.status else {
                item.failed = Some(made);
                continue;
            };
            let output = done.output.clone();
            item.result_thumb = None;
            item.result_clip = Load::Idle;
            if selected {
                item.view.show = Show::Result;
            }
            item.made = Some(made);
            item.failed = None;
            load_result_clip(ctx, item, backend);
            if let Some(written) = Written::of(id, &output) {
                self.written.insert(output, written);
            }
        }
        if self
            .items
            .iter()
            .any(|item| item.job.as_ref().is_some_and(|conversion| conversion.job.is_running()))
        {
            return;
        }
        while let Some(id) = self.queue.pop_front() {
            let Some(item) = self.item(id) else { continue };
            // Fail queued jobs that developed a blocker after being enqueued.
            if let Some(blocker) = item.blockers.first() {
                let why = fl!("blocked", reason = blocker.reason());
                let item = self.item_mut(id).expect("found above");
                if let Some(conversion) = &mut item.job {
                    conversion.job.status = Status::Failed(why);
                }
                ctx.request_repaint();
                continue;
            }
            let Some(Conversion { format, choices: Some(choices), .. }) = &item.job else {
                continue;
            };
            // When a `.tgs` job keeps the crop's shape (`Resize::Contain`),
            // wait for `item.art` so its size is known before squaring the
            // crop.
            if *format == Format::Tgs
                && crate::file_settings::keeps_crop_shape(choices, &self.presets, &self.config)
            {
                let size = item.art.ready().map(|art| (art.clip.width, art.clip.height));
                match size {
                    Some(size) => {
                        let (presets, config) = (&self.presets, &self.config);
                        let item = self.items.iter_mut().find(|item| item.id == id);
                        if let Some(Conversion { choices: Some(choices), .. }) =
                            item.and_then(|item| item.job.as_mut())
                        {
                            crate::file_settings::square_choices(choices, size, presets, config);
                        }
                    }
                    None if !matches!(item.art, crate::media::Load::Failed(_)) => {
                        self.queue.push_front(id);
                        return;
                    }
                    None => {}
                }
            }
            let item = self.item(id).expect("found above");
            let Some(Conversion { job, format, choices: Some(choices), .. }) = &item.job else {
                continue;
            };
            let recipe = choices.recipe(*format, &self.presets, &self.config);
            let setup = recipe.clone().and_then(|recipe| self.setup(recipe, backend));
            let overwrite = self.may_replace(id, &job.output);
            // Warn when overwriting an existing file not written for this item.
            let replaces = job.output.exists() && !self.wrote(id, &job.output);
            let input = item.input();
            let item = self.item_mut(id).expect("found above");
            let conversion = item.job.as_mut().expect("found above");
            conversion.recipe = recipe.ok();
            let job = &mut conversion.job;
            match setup {
                Ok(setup) => {
                    job.replaces = replaces;
                    job.start(setup, input, overwrite, ctx);
                    return;
                }
                Err(message) => {
                    job.status = Status::Failed(message);
                    // Taken in next frame, which nothing else may request.
                    ctx.request_repaint();
                }
            }
        }
    }
}

/// Returns true if `value` (tracked under `key`) has remained unchanged for
/// `wait` seconds, scheduling a repaint while waiting.
fn settled(
    ctx: &egui::Context,
    key: impl std::hash::Hash + std::fmt::Debug,
    value: (f64, f64),
    wait: f64,
) -> bool {
    let now = ctx.input(|input| input.time);
    let since = ctx.data_mut(|data| {
        let entry = data.get_temp_mut_or_insert_with(egui::Id::new(key), || (value, now));
        if entry.0 != value {
            *entry = (value, now);
        }
        entry.1
    });
    let settled = now - since >= wait;
    if !settled {
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait / 4.0));
    }
    settled
}

/// Builds the input thumbnail texture and finalises auto-detected output format
/// once an input finishes loading. Out-of-bounds crops (e.g. from a replaced
/// smaller file) are left in place so `find_blockers` flags them.
fn settle(ctx: &egui::Context, item: &mut Item) {
    if item.format_guessed
        && let Some(art) = item.art.ready()
    {
        item.format_guessed = false;
        if art.clip.looks_like_pixel_art() {
            item.format = Format::Tgs;
        } else if !art.clip.is_animated() {
            // Non-pixel-art still images default to static WebP.
            item.format = Format::Webp;
        }
        item.history.follow_guess(item.format, false);
    }
    if item.input_thumb.is_none()
        && let Some(clip) = item.input_clip()
    {
        let image = clip.thumbnail(THUMB as u32 * 2);
        item.input_thumb =
            Some(ctx.load_texture(format!("input-{}", item.id), image, texture_options(clip)));
    }
}

/// Verifies a restored session's previous output file on a background thread
/// and promotes it to `item.made` if still valid on disk (dropped if the file
/// is gone or the item was converted meanwhile).
fn check_earlier(ctx: &egui::Context, item: &mut Item) {
    let Some((earlier, load)) = &mut item.earlier else { return };
    if load.is_idle() {
        let (output, format) = (earlier.job.output.clone(), earlier.format);
        let target = earlier.recipe.as_ref().map(Recipe::target).unwrap_or_default();
        load.reload(media::Task::spawn(ctx, move |_| {
            Done::of_file(&output, format, target).ok_or_else(String::new)
        }));
    }
    load.poll();
    if load.is_loading() {
        return;
    }
    let (mut earlier, load) = item.earlier.take().expect("there");
    if let Load::Ready(done) = load
        && item.made.is_none()
    {
        earlier.job.status = Status::Done(Box::new(done));
        item.made = Some(earlier);
    }
}

/// Resets failed or in-flight media loads on `item` when the ffmpeg backend
/// changes (an in-flight load using the old backend could still fail where the
/// new backend succeeds).
fn read_again(item: &mut Item) {
    item.video.restart();
    item.art.restart();
    item.result_clip.restart();
    if !item.part.as_ref().is_some_and(|(_, part)| part.is_ready()) {
        item.part = None;
    }
    if !item.still.as_ref().is_some_and(|(_, still)| still.is_ready()) {
        item.still = None;
    }
}

/// Drops full-resolution decoded frame buffers for `item` while keeping its
/// list thumbnails; frames are reloaded when `item` is selected again.
pub(crate) fn release(item: &mut Item) {
    if item.video.ready().is_some() {
        item.video = Load::Idle;
    }
    if item.art.ready().is_some() {
        item.art = Load::Idle;
    }
    item.part = None;
    item.still = None;
    if item.result_clip.ready().is_some() {
        item.result_clip = Load::Idle;
    }
}

fn texture_options(clip: &Clip) -> egui::TextureOptions {
    if clip.pixelated { egui::TextureOptions::NEAREST } else { egui::TextureOptions::LINEAR }
}

fn load_result_clip(ctx: &egui::Context, item: &mut Item, backend: &Result<Backend, String>) {
    let Some(Conversion { job, format, .. }) = &mut item.made else { return };
    let Status::Done(done) = &mut job.status else { return };
    item.result_clip = match (format, backend) {
        (Format::Webm, Ok(backend)) => {
            Load::Loading(media::load_result(ctx, backend.clone(), &done.output))
        }
        // Static WebP results are small enough to decode synchronously.
        (Format::Webp, _) => match webp::preview(&done.output) {
            Ok(preview) => Load::Ready(Clip::from_preview(preview, true)),
            Err(err) => Load::Failed(crate::jobs::failure(&err)),
        },
        // Render `.tgs` from the in-memory `Drawing` when just converted, or
        // parse and render from disk if reloaded later.
        (Format::Tgs, _) => {
            let (output, drawing) = (done.output.clone(), done.drawing.take());
            Load::Loading(Task::spawn(ctx, move |_| {
                let preview = match drawing {
                    Some(drawing) => drawing.draw(),
                    None => tgs::preview_file(&output).map_err(|err| crate::jobs::failure(&err))?,
                };
                Ok(Clip::from_preview(preview, true))
            }))
        }
        (Format::Webm, Err(_)) => return,
    };
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::config::Config;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;
    use tgradish_core::ffmpeg::FfmpegChoice;
    use tgradish_core::options::{Crop, Fit};
    use tgradish_core::telegram;

    use super::*;
    use crate::item::Blocker;
    use crate::testing::{
        convert, convert_all, corpus, finish, harness, has_ffmpeg, made_job, square, video,
        video_of, walking_video,
    };

    #[test]
    fn converts_pixel_art() {
        let dir = tempfile::tempdir().unwrap();
        let frames = dir.path().join("frames");
        std::fs::create_dir(&frames).unwrap();
        square(&frames.join("1.png"), [255, 0, 0, 255]);
        square(&frames.join("2.png"), [0, 0, 255, 255]);
        let mut config = Config::default();
        config.gui.output_dir = Some(dir.path().join("out"));
        let mut harness = harness(config);
        harness.run_steps(2);
        harness.get_by_label(&fl!("drop-here"));

        harness.state_mut().add(vec![frames]);
        finish(&mut harness);
        let item = &harness.state().items[0];
        assert_eq!((item.kind, item.format), (Kind::Frames, Format::Tgs));
        assert!(item.input_thumb.is_some());
        // a folder can't be a video
        harness.get_by_label(&fl!("format-animation"));
        convert(&harness);
        harness.run_steps(2);
        finish(&mut harness);
        let item = &harness.state().items[0];
        assert!(matches!(made_job(item).status, Status::Done(_)));
        assert!(item.result_is_current() && item.result_thumb.is_some());
        assert!(dir.path().join("out").join("frames.sticker.tgs").exists());
        harness.get_by_label_contains(&fl!("telegram-accepts"));

        // changing a setting makes the result old, and converting again
        // replaces the file it wrote before
        harness.state_mut().items[0].choices.target = telegram::Target::Emoji;
        harness.run_steps(2);
        assert!(!harness.state().items[0].result_is_current());
        convert(&harness);
        harness.run_steps(2);
        finish(&mut harness);
        assert!(dir.path().join("out").join("frames.emoji.tgs").exists());
        assert!(harness.state().items[0].result_is_current());
    }

    #[test]
    fn tells_a_result_old_by_what_converting_would_make() {
        let dir = tempfile::tempdir().unwrap();
        let art = dir.path().join("art.png");
        square(&art, [255, 0, 0, 255]);
        let mut harness = harness(Config::default());
        harness.state_mut().add(vec![art]);
        finish(&mut harness);
        convert(&harness);
        harness.run_steps(2);
        finish(&mut harness);
        let current = |harness: &mut Harness<App>| {
            harness.run_steps(2);
            harness.state().items[0].result_is_current()
        };
        assert!(current(&mut harness));
        // what only other formats are made with changes nothing
        harness.state_mut().items[0].choices.frame = Some(1.0);
        harness.state_mut().items[0].choices.webm.crf = Some(20);
        assert!(current(&mut harness));
        // nor does naming the preset it was made with
        harness.state_mut().items[0].choices.preset = Some(Format::Tgs.default_preset().into());
        assert!(current(&mut harness));
        // but another default preset does, where the item has none of its own
        harness.state_mut().config.tgs_preset = Some("fast".into());
        assert!(current(&mut harness));
        harness.state_mut().items[0].choices.preset = None;
        assert!(!current(&mut harness));
    }

    #[test]
    fn makes_static_stickers_from_a_frame() {
        let dir = tempfile::tempdir().unwrap();
        let Some(video) = video_of(dir.path(), "320x240", 2) else { return };
        let mut config = Config::default();
        config.gui.output_dir = Some(dir.path().join("out"));
        config.ffmpeg.choice = FfmpegChoice::System;
        let mut harness = harness(config);
        harness.state_mut().add(vec![video]);
        finish(&mut harness);
        harness.get_by_label(&fl!("format-image")).click();
        harness.run_steps(2);
        // the frame at a second, which the preview stops at
        harness.state_mut().items[0].choices.frame = Some(1.0);
        harness.run_steps(2);
        let item = &harness.state().items[0];
        assert_eq!((item.format, item.view.time, item.view.playing), (Format::Webp, 1.0, false));
        convert(&harness);
        harness.run_steps(2);
        finish(&mut harness);
        let item = &harness.state().items[0];
        assert!(matches!(made_job(item).status, Status::Done(_)));
        let out = dir.path().join("out").join("clip.sticker.webp");
        let info = webp::inspect_file(&out).unwrap();
        assert_eq!((info.width, info.height), (512, 384));
        assert!(item.result_clip.ready().is_some());
        harness.get_by_label_contains(&fl!("telegram-accepts"));
    }

    #[test]
    fn converts_a_cropped_video() {
        let dir = tempfile::tempdir().unwrap();
        let Some(video) = video(dir.path(), "320x240") else { return };
        let mut config = Config::default();
        config.ffmpeg.choice = FfmpegChoice::System;
        let mut harness = harness(config);
        harness.state_mut().add(vec![video]);
        finish(&mut harness);
        {
            let item = &mut harness.state_mut().items[0];
            assert_eq!(
                (item.kind, item.format, item.input_size()),
                (Kind::Video, Format::Webm, Some((320, 240)))
            );
            item.choices.preset = Some("fast".into());
            item.choices.crop = Some(Crop { x: 40, y: 20, width: 200, height: 100 });
        }
        harness.run_steps(2);
        convert(&harness);
        harness.run_steps(2);
        finish(&mut harness);
        let item = &harness.state().items[0];
        let Status::Done(done) = &made_job(item).status else {
            panic!("{:?}", made_job(item).status)
        };
        let info = tgradish_core::webm::container::inspect_file(&done.output).unwrap();
        let picture = info.video.unwrap();
        assert_eq!((picture.width, picture.height), (512, 256));
        assert!(item.result_clip.ready().is_some());
    }

    #[test]
    fn shows_a_video_made_once_it_is_read() {
        let dir = tempfile::tempdir().unwrap();
        let Some(video) = video(dir.path(), "64x48") else { return };
        let mut config = Config::default();
        config.ffmpeg.choice = FfmpegChoice::System;
        config.gui.output_dir = Some(dir.path().join("out"));
        let mut harness = harness(config);
        harness.state_mut().add(vec![video]);
        finish(&mut harness);
        let item = &mut harness.state_mut().items[0];
        (item.choices.preset, item.choices.webm.fit) = (Some("fast".into()), Some(Fit::Off));
        harness.run_steps(2);
        convert(&harness);
        harness.run_steps(2);
        // made, its result read from its file afterwards, the input shown
        // meanwhile
        for _ in 0..3000 {
            harness.step();
            if harness.state().items[0].made.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let item = &harness.state().items[0];
        assert_eq!(item.view.show, Show::Result);
        if item.result_clip.is_loading() {
            assert_eq!(item.shown(), Show::Input);
        }
        // and the result once it is read, as chosen
        finish(&mut harness);
        let item = &harness.state().items[0];
        assert_eq!((item.view.show, item.shown()), (Show::Result, Show::Result));
        let clip = item.result_clip.ready().map(|clip| clip.id);
        assert_eq!(harness.state().screen.showing(), clip);
    }

    #[test]
    fn shows_a_sharp_frame_while_paused() {
        let dir = tempfile::tempdir().unwrap();
        let Some(video) = video(dir.path(), "1280x720") else { return };
        let mut config = Config::default();
        config.ffmpeg.choice = FfmpegChoice::System;
        let mut harness = harness(config);
        harness.state_mut().add(vec![video]);
        finish(&mut harness);
        assert_eq!(harness.state().items[0].input_clip().unwrap().width, 640);
        harness.state_mut().items[0].view.playing = false;
        for _ in 0..500 {
            harness.step();
            if harness.state().items[0]
                .still
                .as_ref()
                .is_some_and(|(_, still)| still.ready().is_some())
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let (_, still) = harness.state().items[0].still.as_ref().expect("asked for");
        assert_eq!(still.ready().map(|clip| (clip.width, clip.height)), Some((1280, 720)));
    }

    #[test]
    fn keeps_a_result_old_when_converting_again_fails() {
        let dir = tempfile::tempdir().unwrap();
        let art = dir.path().join("art.png");
        square(&art, [255, 0, 0, 255]);
        let mut harness = harness(Config::default());
        harness.state_mut().add(vec![art]);
        finish(&mut harness);
        convert(&harness);
        harness.run_steps(2);
        finish(&mut harness);
        assert!(harness.state().items[0].result_is_current());
        // a preset that isn't there fails
        harness.state_mut().items[0].choices.preset = Some("no such preset".into());
        harness.run_steps(2);
        convert(&harness);
        harness.run_steps(2);
        finish(&mut harness);
        let item = &harness.state().items[0];
        assert!(matches!(item.failed.as_ref().unwrap().job.status, Status::Failed(_)));
        assert!(item.made.is_some() && !item.result_is_current());
    }

    #[test]
    fn reads_inputs_again_once_ffmpeg_is_found() {
        let dir = tempfile::tempdir().unwrap();
        let Some(video) = video(dir.path(), "64x48") else { return };
        let mut config = Config::default();
        config.ffmpeg.path = Some(dir.path().join("no-ffmpeg-here"));
        let mut harness = harness(config);
        harness.state_mut().add(vec![video]);
        finish(&mut harness);
        assert!(harness.state().items[0].input_error().is_some());
        // as Settings would change it
        let ffmpeg = &mut harness.state_mut().config.ffmpeg;
        (ffmpeg.choice, ffmpeg.path) = (FfmpegChoice::System, None);
        harness.run_steps(2);
        finish(&mut harness);
        let item = &harness.state().items[0];
        assert_eq!((item.input_error(), item.input_size()), (None, Some((64, 48))));
        assert!(item.input_thumb.is_some());
    }

    #[test]
    fn says_where_ffmpeg_is_needed_while_there_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let clip = dir.path().join("clip.mp4");
        std::fs::write(&clip, b"a video, as far as its name says").unwrap();
        // pixel art, made into a .tgs without ffmpeg
        let art = dir.path().join("art.png");
        square(&art, [255, 0, 0, 255]);
        let mut config = Config::default();
        config.ffmpeg.path = Some(dir.path().join("no-ffmpeg-here"));
        let mut harness = harness(config);
        harness.state_mut().add(vec![clip, art]);
        finish(&mut harness);
        let ids: Vec<u64> = harness.state().items.iter().map(|item| item.id).collect();
        assert_eq!(harness.state().items[0].blockers, [Blocker::NeedsFfmpeg]);
        assert!(harness.state().items[1].blockers.is_empty());
        let needs = format!("⚠ {}", fl!("needs-ffmpeg"));
        let row = format!("⚠ {}", fl!("row-needs-ffmpeg"));
        // the video's row says why, while the pixel art is selected
        harness.state_mut().select(ids[1]);
        harness.run_steps(2);
        harness.get_by_label(&row);
        assert!(harness.query_by_label(&needs).is_none());
        // and its settings, with a way to choose one, once it is
        harness.state_mut().select(ids[0]);
        harness.run_steps(2);
        harness.get_by_label(&needs);
        harness.get_by_label(&fl!("choose-ffmpeg"));
        // said there, and not again over the settings
        assert!(harness.state().banner.rows.is_empty());
        // the file waits for it, rather than failing
        convert(&harness);
        harness.run_steps(2);
        let item = &harness.state().items[0];
        assert!(item.job.is_none() && item.failed.is_none());
        // once there is one, it is said no more
        if !has_ffmpeg() {
            return;
        }
        let ffmpeg = &mut harness.state_mut().config.ffmpeg;
        (ffmpeg.choice, ffmpeg.path) = (FfmpegChoice::System, None);
        harness.run_steps(2);
        assert!(!harness.state().items[0].blockers.contains(&Blocker::NeedsFfmpeg));
        harness.run_steps(2);
        assert!(harness.query_by_label(&needs).is_none());
    }

    #[test]
    fn keeps_a_crop_the_input_has_no_room_for_until_it_is_put_right() {
        let dir = tempfile::tempdir().unwrap();
        let art = dir.path().join("art.png");
        square(&art, [255, 0, 0, 255]);
        let mut inputs = vec![(art, Crop { x: 10, y: 10, width: 4, height: 4 })];
        if let Some(video) = video(dir.path(), "64x48") {
            inputs.push((video, Crop { x: 0, y: 0, width: 64, height: 64 }));
        }
        let mut config = Config::default();
        config.ffmpeg.choice = FfmpegChoice::System;
        config.gui.output_dir = Some(dir.path().join("out"));
        for (input, outside) in inputs {
            // as a session keeps it, for a file since replaced by a smaller one
            let mut harness = harness(config.clone());
            harness.state_mut().add(vec![input.clone()]);
            harness.state_mut().items[0].choices.crop = Some(outside);
            finish(&mut harness);
            let item = &harness.state().items[0];
            assert!(item.input_size().is_some(), "{input:?}");
            assert_eq!(item.choices.crop, Some(outside), "{input:?}");
            assert_eq!(item.blockers, [Blocker::CropOutside], "{input:?}");
            // said before converting, which can't start
            let reason = fl!("blocked", reason = fl!("blocked-crop"));
            harness.get_all_by_label_contains(&reason).next().expect("said");
            harness.get_all_by_label_contains(&fl!("row-attention")).next().expect("said");
            convert(&harness);
            harness.run_steps(2);
            let item = &harness.state().items[0];
            assert!(item.job.is_none() && harness.state().queue.is_empty(), "{input:?}");
            // the way out, under the crop
            harness.get_by_label(&fl!("use-whole")).click_accesskit();
            harness.run_steps(2);
            let item = &harness.state().items[0];
            assert_eq!((item.choices.crop, item.blocked()), (None, false), "{input:?}");
        }
    }

    #[test]
    fn says_what_an_aseprite_file_lacks_and_shows_what_it_has() {
        let dir = tempfile::tempdir().unwrap();
        let ase = corpus(dir.path(), "walker_2x.ase", "walker.ase");
        let mut harness = harness(Config::default());
        harness.state_mut().add(vec![ase]);
        let choices = &mut harness.state_mut().items[0].choices;
        choices.tag = Some("gone".into());
        choices.hide_layers = Some(vec!["art".into(), "lost".into()]);
        finish(&mut harness);
        let item = &harness.state().items[0];
        assert_eq!(
            item.blockers,
            [Blocker::TagMissing("gone".into()), Blocker::LayerMissing(vec!["lost".into()])]
        );
        // read as the file has it, its tags and layers there to pick
        assert!(item.input_error().is_none() && item.input_clip().is_some());
        harness.get_by_label("idle");
        harness.get_all_by_label_contains(&fl!("blocked", "heading")).next().expect("said");
        // the way out under the tags, after theirs
        harness.get_all_by_label(&fl!("all-frames")).last().unwrap().click_accesskit();
        harness.run_steps(2);
        assert_eq!(harness.state().items[0].choices.tag, None);
        harness.get_by_label(&fl!("layers-as-file")).click_accesskit();
        finish(&mut harness);
        let item = &harness.state().items[0];
        assert_eq!((&item.choices.hide_layers, item.blocked()), (&None, false));
    }

    #[test]
    fn offers_the_nearest_sheet_that_divides_the_picture() {
        assert_eq!(crate::item::dividing_sheet(4, 4, 3, 1), "4x1".parse().unwrap());
        assert_eq!(crate::item::dividing_sheet(96, 32, 5, 2), "6x2".parse().unwrap());
        let dir = tempfile::tempdir().unwrap();
        let art = dir.path().join("sheet.png");
        square(&art, [255, 0, 0, 255]);
        let mut harness = harness(Config::default());
        harness.state_mut().add(vec![art]);
        let item = &mut harness.state_mut().items[0];
        (item.format, item.format_guessed) = (Format::Tgs, false);
        item.choices.tgs.sheet = "3x1".parse().ok();
        finish(&mut harness);
        let uneven = Blocker::SheetUneven { width: 4, height: 4, columns: 3, rows: 1 };
        assert_eq!(harness.state().items[0].blockers, [uneven]);
        let nearest = fl!("sheet-nearest", columns = 4, rows = 1);
        harness.get_by_label(&nearest).click_accesskit();
        finish(&mut harness);
        let item = &harness.state().items[0];
        assert_eq!((item.choices.tgs.sheet, item.blocked()), ("4x1".parse().ok(), false));
        assert_eq!(item.input_size(), Some((1, 4)));
    }

    #[test]
    fn gives_back_the_frames_of_items_not_looked_at() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = harness(Config::default());
        let arts: Vec<_> = (0..5)
            .map(|n| {
                let art = dir.path().join(format!("{n}.png"));
                square(&art, [255, 0, 0, 255]);
                art
            })
            .collect();
        harness.state_mut().add(arts);
        finish(&mut harness);
        convert_all(&harness).click();
        harness.run_steps(2);
        finish(&mut harness);
        let ids: Vec<u64> = harness.state().items.iter().map(|item| item.id).collect();
        for &id in &ids {
            harness.state_mut().select(id);
            finish(&mut harness);
        }
        // the first, looked at longest ago, keeps only its thumbnails
        let first = &harness.state().items[0];
        assert!(first.art.is_idle() && first.result_clip.is_idle());
        assert!(first.input_thumb.is_some() && first.result_thumb.is_some());
        // and reads it all again when looked at
        harness.state_mut().select(ids[0]);
        finish(&mut harness);
        let first = &harness.state().items[0];
        assert!(first.art.ready().is_some() && first.result_clip.ready().is_some());
    }

    #[test]
    fn shows_the_frame_paused_on_at_full_size() {
        if !has_ffmpeg() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let video = dir.path().join("red-blue.mp4");
        // red until 0.48 s, then blue
        let graph = "color=c=red:s=1280x720:r=25:d=0.48[a];color=c=blue:s=1280x720:r=25:d=0.52[b];[a][b]concat=n=2:v=1:a=0";
        let made = std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-filter_complex", graph, "-pix_fmt", "yuv420p"])
            .arg(&video)
            .status();
        assert!(made.expect("ffmpeg runs").success(), "ffmpeg could not make the input");
        let mut config = Config::default();
        config.ffmpeg.choice = FfmpegChoice::System;
        let mut harness = harness(config);
        harness.state_mut().add(vec![video]);
        finish(&mut harness);
        let item = &mut harness.state_mut().items[0];
        (item.view.playing, item.view.time) = (false, 0.45);
        for _ in 0..500 {
            harness.step();
            let ready = |item: &Item| {
                item.still.as_ref().and_then(|(_, still)| still.ready().map(|clip| clip.id))
            };
            if let Some(id) = ready(&harness.state().items[0]) {
                harness.run_steps(2);
                assert_eq!(harness.state().screen.showing(), Some(id));
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let (_, still) = harness.state().items[0].still.as_ref().expect("asked for");
        let clip = still.ready().expect("read");
        assert_eq!(clip.width, 1280);
        let pixel = &clip.frames[0][..4];
        assert!(pixel[0] > 200 && pixel[2] < 60, "the frame at 0.45 s is red, not {pixel:?}");
    }

    #[test]
    fn shows_still_images_at_full_size() {
        if !has_ffmpeg() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let photo = dir.path().join("photo.jpg");
        let made = std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-f", "lavfi", "-i", "testsrc2=s=1280x720", "-frames:v", "1"])
            .arg(&photo)
            .status();
        assert!(made.expect("ffmpeg runs").success(), "ffmpeg could not make the input");
        let mut config = Config::default();
        config.ffmpeg.choice = FfmpegChoice::System;
        let mut harness = harness(config);
        harness.state_mut().add(vec![photo]);
        finish(&mut harness);
        for _ in 0..500 {
            harness.step();
            if harness.state().items[0]
                .still
                .as_ref()
                .is_some_and(|(_, still)| still.ready().is_some())
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        harness.run_steps(2);
        let (_, still) = harness.state().items[0].still.as_ref().expect("asked for");
        let clip = still.ready().expect("read");
        assert_eq!((clip.width, harness.state().screen.showing()), (1280, Some(clip.id)));
    }

    #[test]
    fn guesses_pixel_art_and_photos_apart() {
        let dir = tempfile::tempdir().unwrap();
        let art = dir.path().join("art.png");
        square(&art, [255, 0, 0, 255]);
        let mut harness = harness(Config::default());
        harness.state_mut().add(vec![art]);
        finish(&mut harness);
        let item = &harness.state().items[0];
        assert_eq!(
            (item.kind, item.format, item.format_guessed),
            (Kind::Image, Format::Tgs, false)
        );
        // a picture of more colours than pixel art has stays a picture
        let photo = dir.path().join("photo.png");
        let file = std::io::BufWriter::new(std::fs::File::create(&photo).unwrap());
        let mut encoder = png::Encoder::new(file, 32, 32);
        encoder.set_color(png::ColorType::Rgba);
        let pixels: Vec<u8> =
            (0..32u32 * 32).flat_map(|i| [i as u8, (i >> 2) as u8, 99, 255]).collect();
        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
        harness.state_mut().add(vec![photo]);
        finish(&mut harness);
        let item = &harness.state().items[1];
        assert_eq!((item.kind, item.format), (Kind::Image, Format::Webp));
    }

    #[test]
    fn tells_files_apart_by_their_content_before_their_names() {
        let dir = tempfile::tempdir().unwrap();
        let art = dir.path().join("art.mp4");
        square(&art, [255, 0, 0, 255]);
        let ase = corpus(dir.path(), "walker_2x.ase", "walker.dat");
        let mut harness = harness(Config::default());
        harness.state_mut().add(vec![art, ase]);
        // taken by their names until they are read
        let kinds = |harness: &Harness<App>| -> Vec<_> {
            harness.state().items.iter().map(|item| (item.kind, item.format)).collect()
        };
        assert_eq!(kinds(&harness), [(Kind::Video, Format::Webm), (Kind::Video, Format::Webm)]);
        for index in [0, 1] {
            let id = harness.state().items[index].id;
            harness.state_mut().select(id);
            finish(&mut harness);
        }
        assert_eq!(kinds(&harness), [(Kind::Image, Format::Tgs), (Kind::Aseprite, Format::Tgs)]);
    }

    #[test]
    fn makes_aseprite_files_into_every_format() {
        let dir = tempfile::tempdir().unwrap();
        let ase = corpus(dir.path(), "walker_2x.ase", "walker.ase");
        let mut config = Config::default();
        config.gui.output_dir = Some(dir.path().join("out"));
        let mut harness = harness(config);
        harness.state_mut().add(vec![ase]);
        finish(&mut harness);
        let item = &harness.state().items[0];
        assert_eq!((item.kind, item.format), (Kind::Aseprite, Format::Tgs));
        let (tags, layers, durations) = item.aseprite().expect("read");
        assert_eq!(durations, [0.1; 8]);
        let tags: Vec<_> = tags.iter().map(|tag| tag.name.clone()).collect();
        let layers: Vec<_> = layers.iter().map(|l| (l.name.as_str(), l.visible)).collect();
        assert_eq!(
            (tags, layers),
            (vec!["idle".to_owned()], vec![("sketch", false), ("art", true)])
        );
        // a tag played forward picks where the input starts and ends, and
        // nothing is read again
        harness.get_by_label("idle").click();
        harness.run_steps(2);
        let item = &harness.state().items[0];
        assert!(!item.input_reading());
        let (start, length) = (item.choices.start, item.choices.length.unwrap_or_default());
        // as the timeline cuts pixel art: from the start, a hair inside
        assert_eq!((item.choices.tag.as_deref(), start), (None, None));
        assert!((length - 0.2).abs() < 1e-5, "{length}");
        // picked again, all of it
        harness.get_by_label("idle").click();
        harness.run_steps(2);
        let choices = &harness.state().items[0].choices;
        assert_eq!((choices.start, choices.length), (None, None));
        // the idle tag read as its own animation, its sketch shown
        let choices = &mut harness.state_mut().items[0].choices;
        choices.tag = Some("idle".into());
        choices.show_layers = Some(vec!["sketch".into()]);
        // read again, the section stays where it was meanwhile: the read
        // starts in this frame and is taken in only in the next
        harness.step();
        assert!(harness.state().items[0].input_reading());
        assert!(harness.query_by_label(&fl!("aseprite-file")).is_some());
        finish(&mut harness);
        assert_eq!(harness.state().items[0].input_clip().unwrap().frames.len(), 2);
        // drawn the same for ffmpeg, as a video and as a picture
        if !has_ffmpeg() {
            return;
        }
        for format in [Format::Webm, Format::Webp] {
            harness.state_mut().items[0].format = format;
            finish(&mut harness);
            let item = &harness.state().items[0];
            assert_eq!(item.aseprite().map(|(tags, ..)| tags.len()), Some(1), "{format:?}");
            let length = item.input_length().unwrap_or_default();
            assert!((length - 0.2).abs() < 1e-3, "{format:?}: {length}");
            convert(&harness);
            harness.run_steps(2);
            finish(&mut harness);
            let done = made_job(&harness.state().items[0]);
            assert!(matches!(&done.status, Status::Done(_)), "{format:?}: {:?}", done.status);
        }
    }

    #[test]
    fn reads_again_what_an_ffmpeg_changed_from_was_reading() {
        let dir = tempfile::tempdir().unwrap();
        let Some(video) = video(dir.path(), "320x240") else { return };
        let mut config = Config::default();
        config.ffmpeg.choice = FfmpegChoice::System;
        let mut harness = harness(config);
        harness.state_mut().add(vec![video]);
        finish(&mut harness);
        // as if read with an ffmpeg that is slow to fail, and another chosen
        // before it does
        let (fail, failing) = std::sync::mpsc::channel::<()>();
        let ctx = harness.ctx.clone();
        let app = harness.state_mut();
        app.items[0].video = Load::Loading(Task::spawn(&ctx, move |_| {
            let _ = failing.recv();
            Err("the old ffmpeg failed".into())
        }));
        app.read_with = Some(Err("the old ffmpeg".into()));
        harness.step();
        fail.send(()).unwrap();
        finish(&mut harness);
        let video = &harness.state().items[0].video;
        assert!(video.ready().is_some(), "{:?}", harness.state().items[0].input_error());
    }

    #[test]
    fn makes_videos_into_animations() {
        let dir = tempfile::tempdir().unwrap();
        let Some(video) = walking_video(dir.path()) else { return };
        let mut config = Config::default();
        config.gui.output_dir = Some(dir.path().join("out"));
        let mut harness = harness(config);
        harness.state_mut().add(vec![video]);
        finish(&mut harness);
        assert_eq!(harness.state().items[0].kind, Kind::Video);
        harness.state_mut().items[0].format = Format::Tgs;
        finish(&mut harness);
        // its frames, read through ffmpeg
        let item = &harness.state().items[0];
        assert_eq!(item.input_clip().map(|clip| clip.frames.len()), Some(8));
        convert(&harness);
        harness.run_steps(2);
        finish(&mut harness);
        let done = made_job(&harness.state().items[0]);
        let Status::Done(done) = &done.status else { panic!("{:?}", done.status) };
        assert!(done.output.extension().is_some_and(|ext| ext == "tgs"));
    }

    #[test]
    fn makes_sequences_into_videos_and_pictures() {
        if !has_ffmpeg() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let frames = dir.path().join("walk");
        std::fs::create_dir(&frames).unwrap();
        square(&frames.join("1.png"), [255, 0, 0, 255]);
        square(&frames.join("2.png"), [0, 0, 255, 255]);
        let mut config = Config::default();
        config.gui.output_dir = Some(dir.path().join("out"));
        let mut harness = harness(config);
        harness.state_mut().add(vec![frames]);
        finish(&mut harness);
        assert_eq!(harness.state().items[0].kind, Kind::Frames);
        // two frames, at the default rate unless said
        harness.state_mut().items[0].format = Format::Webm;
        finish(&mut harness);
        let length = harness.state().items[0].input_length().unwrap_or_default();
        let expected = 2.0 / tgradish_core::tgs::DEFAULT_FPS;
        assert!((length - expected).abs() < 1e-3, "{length}");
        harness.state_mut().items[0].choices.webm.fps = Some(4.0);
        finish(&mut harness);
        let length = harness.state().items[0].input_length().unwrap_or_default();
        assert!((length - 0.5).abs() < 1e-3, "{length}");
        for format in [Format::Webm, Format::Webp] {
            harness.state_mut().items[0].format = format;
            finish(&mut harness);
            convert(&harness);
            harness.run_steps(2);
            finish(&mut harness);
            let done = made_job(&harness.state().items[0]);
            assert!(matches!(&done.status, Status::Done(_)), "{format:?}: {:?}", done.status);
        }
    }
}
