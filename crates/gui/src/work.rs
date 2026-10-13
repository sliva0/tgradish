//! Reading and decoding inputs and results, and converting, each on a
//! worker thread so the window keeps drawing meanwhile. The window starts
//! what is due and takes in what is done each time it draws, and lets go
//! of decoded frames where an item isn't looked at, as they take much
//! memory.

use eframe::egui;
use tgradish_core::backend::Backend;
use tgradish_core::options::Options;
use tgradish_core::presets::Format;
use tgradish_core::still;

use crate::app::App;
use crate::files::THUMB;
use crate::item::{Item, Kind, Made, Show};
use crate::jobs::{Plan, Status};
use crate::media::{self, Art, Clip, Load, Video};
use crate::outputs::Written;

/// How many items read their inputs at once, besides the selected one.
const READING: usize = 2;

impl App {
    /// Takes in loads and conversions, and starts what is due.
    pub(crate) fn pump(&mut self, ctx: &egui::Context) {
        let backend = self.backend();
        let selected = self.selected;
        let mut reading = self
            .items
            .iter()
            .filter(|item| item.video.is_loading() || item.art.is_loading())
            .count();
        for item in &mut self.items {
            let is_selected = selected == Some(item.id);
            let kept = is_selected || self.recent.contains(&item.id);
            let finished = item.video.poll() | item.art.poll();
            if let Some((_, part)) = &mut item.part {
                part.poll();
            }
            if let Some((_, still)) = &mut item.still {
                still.poll();
            }
            item.result_clip.poll();
            if finished {
                settle(ctx, item);
                if !kept {
                    release(item);
                }
            }
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
            // read what the item needs
            let wanted = is_selected || item.input_thumb.is_none();
            if !wanted || (!is_selected && reading >= READING) {
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
                    item.inputs.clone(),
                    item.sequence,
                    item.art_reading.clone(),
                    backend.as_ref().ok().cloned(),
                ));
                reading += 1;
            } else if matches!(item.format, Format::Webm | Format::Image)
                && !(item.format_guessed && item.art.is_loading())
            {
                // pixel art drawn again with another tag, layers or rate
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
                    let (inputs, sequence) = (item.inputs.clone(), item.sequence);
                    match &backend {
                        Ok(backend) => item.video.reload(Video::load(
                            ctx,
                            backend.clone(),
                            inputs,
                            sequence,
                            drawing,
                        )),
                        Err(message) => item.video = Load::Failed(message.clone()),
                    }
                    reading += 1;
                }
            }
            if is_selected && item.result_clip.is_idle() && item.made.is_some() {
                load_result_clip(ctx, item, &backend);
            }
        }
        self.load_detail(ctx, &backend);
        self.convert_next(ctx, &backend);
    }

    /// Reads more of the selected video once what is looked at stays put
    /// for a moment: the used part at full rate, and the frame shown while
    /// paused at full size.
    fn load_detail(&mut self, ctx: &egui::Context, backend: &Result<Backend, String>) {
        let Some(id) = self.selected else { return };
        let Ok(backend) = backend else { return };
        let Some(item) = self.item_mut(id) else { return };
        if item.format == Format::Tgs {
            return;
        }
        let range = item.range().map(|(start, end)| (start, end - start));
        // drawn pixel art has every frame at its full size already
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
        // finished jobs become results
        let mut finished = Vec::new();
        for item in &mut self.items {
            if let Some((job, ..)) = &mut item.job
                && (job.poll() || job.is_finished())
                && !job.is_waiting()
            {
                finished.push(item.id);
            }
        }
        for id in finished {
            let selected = self.selected == Some(id);
            let item = self.item_mut(id).expect("just found");
            let (job, format, choices) = item.job.take().expect("just found");
            let made = Made { job, format, choices };
            let Status::Done(done) = &made.job.status else {
                item.failed = Some(made);
                continue;
            };
            let output = done.output.clone();
            item.result_thumb = None;
            item.result_clip = match &done.preview {
                Some(preview) => Load::Ready(Clip::from_preview(preview.clone(), true)),
                None => Load::Idle,
            };
            if selected {
                item.view.show = Show::Result;
            }
            item.made = Some(made);
            item.failed = None;
            if item.result_clip.is_idle() {
                load_result_clip(ctx, item, backend);
            }
            if let Some(written) = Written::of(id, &output) {
                self.written.insert(output, written);
            }
        }
        if self.items.iter().any(|item| item.job.as_ref().is_some_and(|(job, ..)| job.is_running()))
        {
            return;
        }
        while let Some(id) = self.queue.pop_front() {
            let Some(item) = self.item(id) else { continue };
            let Some((job, format, choices)) = &item.job else { continue };
            let plan = match format {
                Format::Webm => {
                    choices.webm_options(&self.presets, &self.config).and_then(|options| {
                        backend.clone().map(|backend| Plan::Webm { options, backend })
                    })
                }
                Format::Tgs => choices.tgs_options(&self.presets, &self.config).map(|options| {
                    // ffmpeg, for videos
                    Plan::Tgs { options, backend: backend.clone().ok() }
                }),
                Format::Image => {
                    choices.image_options(&self.presets, &self.config).and_then(|options| {
                        backend.clone().map(|backend| Plan::Image { options, backend })
                    })
                }
            };
            let overwrite = self.may_replace(id, &job.output);
            // a result of another time, rather than this item's own
            let replaces = job.output.exists() && !self.wrote(id, &job.output);
            let (inputs, sequence) = (item.inputs.clone(), item.sequence);
            let item = self.item_mut(id).expect("found above");
            let (job, ..) = item.job.as_mut().expect("found above");
            match plan {
                Ok(plan) => {
                    job.replaces = replaces;
                    job.start(plan, inputs, sequence, overwrite, ctx);
                    return;
                }
                Err(message) => {
                    job.status = Status::Failed(message);
                }
            }
        }
    }
}

/// Whether `value`, kept under `key`, has stayed the same for `wait`
/// seconds; asks for a repaint to look again when it hasn't.
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

/// Makes the input thumbnail and settles the guessed format once an
/// input is read.
fn settle(ctx: &egui::Context, item: &mut Item) {
    if item.format_guessed
        && let Some(art) = item.art.ready()
    {
        item.format_guessed = false;
        if art.clip.looks_like_pixel_art() {
            item.format = Format::Tgs;
        } else if !art.clip.is_animated() {
            // a photo or a drawing stays a picture
            item.format = Format::Image;
        }
    }
    if item.input_thumb.is_none()
        && let Some(clip) = item.input_clip()
    {
        let image = clip.thumbnail(THUMB as u32 * 2);
        item.input_thumb =
            Some(ctx.load_texture(format!("input-{}", item.id), image, texture_options(clip)));
    }
}

/// Frees an item's big frames; its thumbnails stay.
pub(crate) fn release(item: &mut Item) {
    if item.video.ready().is_some() {
        item.video = Load::Idle;
    }
    item.part = None;
    item.still = None;
    if item.result_clip.ready().is_some_and(|clip| !clip.pixelated) {
        item.result_clip = Load::Idle;
    }
}

fn texture_options(clip: &Clip) -> egui::TextureOptions {
    if clip.pixelated { egui::TextureOptions::NEAREST } else { egui::TextureOptions::LINEAR }
}

fn load_result_clip(ctx: &egui::Context, item: &mut Item, backend: &Result<Backend, String>) {
    let Some(Made { job, format, .. }) = &item.made else { return };
    let Status::Done(done) = &job.status else { return };
    item.result_clip = match (format, backend) {
        (Format::Webm, Ok(backend)) => {
            Load::Loading(media::load_result(ctx, backend.clone(), &done.output))
        }
        // small enough to read at once
        (Format::Image, _) => match still::preview(&done.output) {
            Ok(preview) => Load::Ready(Clip::from_preview(preview, true)),
            Err(err) => Load::Failed(err.to_string()),
        },
        _ => return,
    };
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use egui_kittest::kittest::Queryable;
    use tgradish_core::config::Config;
    use tgradish_core::ffmpeg::FfmpegChoice;
    use tgradish_core::options::Crop;
    use tgradish_core::telegram;

    use super::*;
    use crate::testing::{
        convert, corpus, finish, harness, has_ffmpeg, made_job, square, video, video_of,
        walking_video,
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
        harness.get_by_label_contains("Drop videos");

        harness.state_mut().add(vec![frames]);
        finish(&mut harness);
        let item = &harness.state().items[0];
        assert_eq!((item.kind, item.format), (Kind::Frames, Format::Tgs));
        assert!(item.input_thumb.is_some());
        // a folder can't be a video
        harness.get_by_label("Animation");
        convert(&harness);
        harness.run_steps(2);
        finish(&mut harness);
        let item = &harness.state().items[0];
        assert!(matches!(made_job(item).status, Status::Done(_)));
        assert!(item.result_is_current() && item.result_thumb.is_some());
        assert!(dir.path().join("out").join("frames.sticker.tgs").exists());
        harness.get_by_label_contains("Telegram should accept it");

        // changing a setting makes the result old, and converting again
        // replaces the file this window wrote
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
    fn makes_image_stickers_from_a_frame() {
        let dir = tempfile::tempdir().unwrap();
        let Some(video) = video_of(dir.path(), "320x240", 2) else { return };
        let mut config = Config::default();
        config.gui.output_dir = Some(dir.path().join("out"));
        config.ffmpeg.choice = FfmpegChoice::System;
        let mut harness = harness(config);
        harness.state_mut().add(vec![video]);
        finish(&mut harness);
        harness.get_by_label("Image").click();
        harness.run_steps(2);
        // the frame at a second, which the preview stops at
        harness.state_mut().items[0].choices.frame = Some(1.0);
        harness.run_steps(2);
        let item = &harness.state().items[0];
        assert_eq!((item.format, item.view.time, item.view.playing), (Format::Image, 1.0, false));
        convert(&harness);
        harness.run_steps(2);
        finish(&mut harness);
        let item = &harness.state().items[0];
        assert!(matches!(made_job(item).status, Status::Done(_)));
        let out = dir.path().join("out").join("clip.sticker.webp");
        assert_eq!(still::dimensions(&out).unwrap(), (512, 384));
        assert!(item.result_clip.ready().is_some());
        harness.get_by_label_contains("Telegram should accept it");
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
        let info = tgradish_core::webm::inspect_file(&done.output).unwrap();
        let picture = info.video.unwrap();
        assert_eq!((picture.width, picture.height), (512, 256));
        assert!(item.result_clip.ready().is_some());
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
        // a crop outside the picture fails
        harness.state_mut().items[0].choices.crop =
            Some(Crop { x: 10, y: 10, width: 4, height: 4 });
        harness.run_steps(2);
        convert(&harness);
        harness.run_steps(2);
        finish(&mut harness);
        let item = &harness.state().items[0];
        assert!(matches!(item.failed.as_ref().unwrap().job.status, Status::Failed(_)));
        assert!(item.made.is_some() && !item.result_is_current());
    }

    #[test]
    fn shows_the_frame_paused_on_at_full_size() {
        let dir = tempfile::tempdir().unwrap();
        let video = dir.path().join("red-blue.mp4");
        // red until 0.48 s, then blue
        let graph = "color=c=red:s=1280x720:r=25:d=0.48[a];color=c=blue:s=1280x720:r=25:d=0.52[b];[a][b]concat=n=2:v=1:a=0";
        let made = std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-filter_complex", graph, "-pix_fmt", "yuv420p"])
            .arg(&video)
            .status();
        if !made.is_ok_and(|status| status.success()) {
            eprintln!("skipped: needs ffmpeg on PATH");
            return;
        }
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
        let dir = tempfile::tempdir().unwrap();
        let photo = dir.path().join("photo.jpg");
        let made = std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-f", "lavfi", "-i", "testsrc2=s=1280x720", "-frames:v", "1"])
            .arg(&photo)
            .status();
        if !made.is_ok_and(|status| status.success()) {
            eprintln!("skipped: needs ffmpeg on PATH");
            return;
        }
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
        assert_eq!((item.kind, item.format), (Kind::Image, Format::Image));
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
        assert_eq!((item.choices.tag.as_deref(), start), (None, Some(0.0)));
        assert!((length - 0.2).abs() < 1e-9, "{length}");
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
        assert!(harness.query_by_label("Aseprite file").is_some());
        finish(&mut harness);
        assert_eq!(harness.state().items[0].input_clip().unwrap().frames.len(), 2);
        // drawn the same for ffmpeg, as a video and as a picture
        if !has_ffmpeg() {
            return;
        }
        for format in [Format::Webm, Format::Image] {
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
        // two frames, ten a second unless said
        harness.state_mut().items[0].format = Format::Webm;
        finish(&mut harness);
        let length = harness.state().items[0].input_length().unwrap_or_default();
        assert!((length - 0.2).abs() < 1e-3, "{length}");
        harness.state_mut().items[0].choices.webm.fps = Some(4.0);
        finish(&mut harness);
        let length = harness.state().items[0].input_length().unwrap_or_default();
        assert!((length - 0.5).abs() < 1e-3, "{length}");
        for format in [Format::Webm, Format::Image] {
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
