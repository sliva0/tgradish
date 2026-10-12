use std::path::PathBuf;

use anyhow::{Context as _, Result, bail};
use console::style;
use serde_json::json;
use tgradish_core::backend::BackendInfo;
use tgradish_core::config::Config;
use tgradish_core::presets::{self, Format, Presets};
use tgradish_core::telegram::{self, Target};
use tgradish_core::webm::{self, Patch, WebmInfo};
use tgradish_core::{TOOL_ID, protocol};
use tgradish_core::{still, tgs};

use crate::term;

use crate::Context;
use crate::args::{ConfigCommand, FfmpegCommand, InspectArgs, PresetCommand, SpoofArgs};
use crate::ui;

fn print_json(value: &impl serde::Serialize) {
    println!("{}", serde_json::to_string_pretty(value).expect("output serialises"));
}

pub fn spoof(ctx: &Context, args: SpoofArgs) -> Result<()> {
    let output = match (&args.output, args.in_place) {
        (_, true) => args.input.clone(),
        (Some(output), false) => output.clone(),
        (None, false) => args.input.with_extension("spoofed.webm"),
    };
    if !args.in_place && !args.overwrite && output != args.input && output.exists() {
        bail!("{} already exists, use --overwrite to replace it", output.display());
    }
    let changes = Patch {
        duration: Some(args.duration),
        title: args.title.clone(),
        muxing_app: args.watermark.then(|| TOOL_ID.to_string()),
        signature: args.watermark.then(tgradish_core::signature),
        ..Default::default()
    };
    let replace = args.overwrite || output == args.input;
    let report = webm::patch_file(&args.input, &output, &changes, replace)
        .with_context(|| format!("could not spoof {}", args.input.display()))?;

    if ctx.global.json {
        print_json(&json!({ "output": output, "report": report }));
        return Ok(());
    }
    if args.duration > telegram::MAX_SECONDS {
        eprintln!(
            "{} {} is longer than {} s, Telegram will still reject it",
            ui::warning_label(),
            ui::seconds(args.duration),
            telegram::MAX_SECONDS
        );
    }
    if report.duration_tags_skipped > 0 {
        eprintln!("{} some DURATION tags had no room for the new value", ui::warning_label());
    }
    let old = report.old_duration.map_or("unknown".into(), ui::seconds);
    println!(
        "{}",
        ui::status(
            "Spoofed",
            format!(
                "{}: header duration {old} → {}",
                term::link_out(style(output.display()).bold(), &output),
                ui::seconds(args.duration)
            )
        )
    );
    Ok(())
}

fn print_image(
    path: &std::path::Path,
    info: &still::Info,
    target: Target,
    issues: &[telegram::Issue],
) {
    println!("{}", style(term::link_out(path.display(), path)).bold());
    println!("{}", ui::field("size", ui::size_within(info.bytes, telegram::MAX_STILL_BYTES)));
    let kind = if info.lossy { "lossy" } else { "lossless" };
    let alpha = if info.alpha { ", transparent" } else { "" };
    println!(
        "{}",
        ui::field("picture", format!("WebP {}x{}, {kind}{alpha}", info.width, info.height))
    );
    if let Some(tool) = &info.made_with {
        println!("{}", ui::field("made", format!("with {tool}")));
    }
    print_verdict(issues, target);
}

/// What Telegram would say about a sticker.
fn print_verdict(issues: &[telegram::Issue], target: Target) {
    if issues.is_empty() {
        let accepted = format!("{} as {}", style("✔ accepts it").green(), target.name());
        println!("{}", ui::field("telegram", accepted));
    } else {
        let refused = format!("{} as {}:", style("✖ refuses it").red(), target.name());
        println!("{}", ui::field("telegram", refused));
        for issue in issues {
            println!("              {} {issue}", style("-").red());
        }
    }
}

fn print_info(path: &std::path::Path, info: &WebmInfo, target: Target) {
    println!("{}", style(term::link_out(path.display(), path)).bold());
    println!("{}", ui::field("size", ui::size_within(info.file_size, target.max_bytes())));
    let video = match &info.video {
        Some(video) => {
            let fps = info.fps().map(ui::fps).unwrap_or_else(|| "unknown fps".into());
            let alpha = if video.alpha { ", transparent" } else { "" };
            format!(
                "{} {}x{}, {fps}, {}{alpha}",
                video.codec_id,
                video.width,
                video.height,
                ui::count(info.video_frames, "frame")
            )
        }
        None => "none".to_owned(),
    };
    println!("{}", ui::field("video", video));
    if info.audio_tracks > 0 {
        println!("{}", ui::field("audio", ui::count(info.audio_tracks, "track")));
    }
    let header = info.header_duration.map_or("none".into(), ui::seconds);
    let content = info.content_duration.map_or("unknown".into(), ui::seconds);
    let spoofed = if info.spoofed() { " (spoofed)" } else { "" };
    let duration = format!("{header} in header, {content} of video{spoofed}");
    println!("{}", ui::field("duration", duration));
    if let Some(title) = &info.title {
        println!("{}", ui::field("title", title));
    }
    let apps: Vec<_> = [("written by", &info.writing_app), ("muxed by", &info.muxing_app)]
        .into_iter()
        .filter_map(|(label, app)| app.as_ref().map(|app| format!("{label} {app}")))
        .collect();
    if !apps.is_empty() {
        println!("{}", ui::field("made", apps.join(", ")));
    }
    if let Some(signature) = &info.signature {
        println!("{}", ui::field("signature", signature));
    }
    if info.truncated {
        println!("  {}", style("the file is truncated").red());
    }

    print_verdict(&telegram::check(info, target), target);
}

/// Says a file of several couldn't be inspected, and goes on to the rest.
fn print_failure(ctx: &Context, path: &std::path::Path, err: &impl std::fmt::Display) {
    if ctx.global.json {
        crate::json_line(&json!({ "file": path, "error": err.to_string() }));
    } else {
        eprintln!("{} {}: {err}", ui::error_label(), path.display());
    }
}

pub fn inspect(ctx: &Context, args: InspectArgs) -> Result<()> {
    let mut failed = 0;
    for (i, path) in args.files.iter().enumerate() {
        if still::is_image(path) {
            match still::inspect_file(path) {
                Ok(info) => {
                    let target = args.target.map_or_else(|| info.target(), Into::into);
                    let issues = info.issues(target);
                    if ctx.global.json {
                        let line = json!({
                            "file": path, "format": "image", "target": target,
                            "info": info, "issues": issues,
                        });
                        crate::json_line(&line);
                    } else {
                        if i > 0 {
                            println!();
                        }
                        print_image(path, &info, target, &issues);
                    }
                }
                Err(err) if args.files.len() > 1 => {
                    failed += 1;
                    print_failure(ctx, path, &err);
                }
                Err(err) => return Err(err).with_context(|| format!("{}", path.display())),
            }
            continue;
        }
        if tgs::is_sticker(path) {
            match tgs::inspect_file(path) {
                Ok((stats, issues)) if ctx.global.json => {
                    let line =
                        json!({ "file": path, "format": "tgs", "stats": stats, "issues": issues });
                    crate::json_line(&line);
                }
                Ok((stats, issues)) => {
                    if i > 0 {
                        println!();
                    }
                    print_sticker(path, &stats, &issues);
                }
                Err(err) if args.files.len() > 1 => {
                    failed += 1;
                    print_failure(ctx, path, &err);
                }
                Err(err) => return Err(err).with_context(|| format!("{}", path.display())),
            }
            continue;
        }
        let info = match webm::inspect_file(path) {
            Ok(info) => info,
            Err(err) if args.files.len() > 1 => {
                failed += 1;
                print_failure(ctx, path, &err);
                continue;
            }
            Err(err) => return Err(err).with_context(|| format!("{}", path.display())),
        };
        let target = args.target.map_or_else(|| info.target(), Into::into);
        if ctx.global.json {
            #[derive(serde::Serialize)]
            struct Line<'a> {
                file: &'a std::path::Path,
                format: &'static str,
                target: Target,
                info: &'a WebmInfo,
                issues: Vec<telegram::Issue>,
            }
            let issues = telegram::check(&info, target);
            let line = Line { file: path, format: "webm", target, info: &info, issues };
            crate::json_line(&line);
        } else {
            if i > 0 {
                println!();
            }
            print_info(path, &info, target);
        }
    }
    if failed > 0 {
        if ctx.global.json {
            return Err(crate::Exit(1).into());
        }
        bail!("{failed} of {} could not be read", ui::count(args.files.len(), "file"));
    }
    Ok(())
}

/// Prints every licence notice this build carries.
pub fn licenses() {
    #[cfg(feature = "gui")]
    let notices = tgradish_gui::notices();
    #[cfg(not(feature = "gui"))]
    let notices = tgradish_core::licenses::notices();
    for (index, notice) in notices.iter().enumerate() {
        if index > 0 {
            println!();
        }
        println!("{}", style(&notice.title).bold());
        println!("{}\n", "=".repeat(notice.title.chars().count()));
        println!("{}", notice.text.trim_end());
    }
}

/// The presets used when none is given, for WebM and `.tgs`.
fn default_presets(ctx: &Context) -> [&str; 2] {
    [ctx.config.preset_for(Format::Webm), ctx.config.preset_for(Format::Tgs)]
}

pub fn describe(ctx: &Context) -> Result<()> {
    let presets = Presets::load_user()?;
    print_json(&protocol::describe(&presets, default_presets(ctx)));
    Ok(())
}

pub fn preset(ctx: &Context, command: PresetCommand) -> Result<()> {
    let presets = Presets::load_user()?;
    match command {
        PresetCommand::List if ctx.global.json => {
            print_json(&protocol::describe(&presets, default_presets(ctx)).presets);
        }
        PresetCommand::List => {
            let [webm, tgs] = default_presets(ctx);
            for (name, preset) in presets.iter() {
                let default = match (name == webm, name == tgs) {
                    (true, true) => " (default)",
                    (true, false) => " (default for webm)",
                    (false, true) => " (default for tgs)",
                    (false, false) => "",
                };
                match preset {
                    Ok(preset) => {
                        let origin = preset
                            .path
                            .as_ref()
                            .map_or("built-in".to_string(), |p| p.display().to_string());
                        println!("{}{default}  {}", style(name).bold(), preset.description);
                        println!("    {}", style(origin).dim());
                    }
                    Err(err) => println!("{}  {} {err}", style(name).bold(), ui::error_label()),
                }
            }
        }
        PresetCommand::Show { name } => {
            let (webm, tgs) = (presets.webm(&name)?, presets.tgs(&name)?);
            if ctx.global.json {
                print_json(&json!({ "webm": webm, "tgs": tgs }));
            } else {
                println!("[webm]\n{}\n[tgs]\n{}", toml::to_string(&webm)?, toml::to_string(&tgs)?);
            }
        }
        PresetCommand::Path => match presets::user_dir() {
            Some(dir) if ctx.global.json => print_json(&json!({ "path": dir })),
            Some(dir) => println!("{}", dir.display()),
            None => bail!("no home directory found"),
        },
    }
    Ok(())
}

pub fn config(ctx: &Context, command: ConfigCommand, path: Option<PathBuf>) -> Result<()> {
    match command {
        ConfigCommand::Path => {
            let path = path.context("no home directory found")?;
            if ctx.global.json {
                print_json(&json!({ "path": path, "exists": path.exists() }));
            } else {
                println!("{}", path.display());
            }
        }
        ConfigCommand::Show if ctx.global.json => print_json(&ctx.config),
        ConfigCommand::Show => {
            if ctx.config == Config::default() {
                eprintln!("# no config file, using defaults");
            }
            print!("{}", toml::to_string(&ctx.config)?);
        }
    }
    Ok(())
}

pub fn ffmpeg(ctx: &Context, command: FfmpegCommand) -> Result<()> {
    match command {
        FfmpegCommand::Status => {
            let backend = ctx.backend()?;
            let caps = backend.capabilities(&ctx.cancel)?;
            if ctx.global.json {
                print_json(&json!({ "ffmpeg": backend.info(), "capabilities": caps }));
                if !caps.libvpx_vp9 {
                    return Err(crate::Exit(1).into());
                }
            } else {
                match backend.info() {
                    BackendInfo::Process { ffmpeg } => {
                        println!("ffmpeg   {}", ffmpeg.ffmpeg.display());
                        println!("ffprobe  {}", ffmpeg.ffprobe.display());
                        println!("source   {}", ui::name(&ffmpeg.source));
                    }
                    BackendInfo::Builtin => println!("ffmpeg   built into tgradish"),
                }
                println!("version  {}", caps.version);
                let vp9 = if caps.libvpx_vp9 { style("yes").green() } else { style("no").red() };
                println!("libvpx-vp9 encoder  {vp9}");
                if !caps.libvpx_vp9 {
                    bail!("this ffmpeg cannot encode VP9 with libvpx, stickers need it");
                }
            }
        }
    }
    Ok(())
}

fn print_sticker(path: &std::path::Path, stats: &tgs::Stats, issues: &[tgs::Issue]) {
    println!("{}", style(term::link_out(path.display(), path)).bold());
    if let Some(name) = &stats.name {
        println!("{}", ui::field("name", name));
    }
    let size = match stats.tgs_bytes {
        Some(bytes) => {
            let packed = ui::size_within(bytes as u64, tgs::MAX_BYTES);
            format!("{packed}, {} of JSON", ui::kib(stats.json_bytes as u64))
        }
        None => format!("{} of JSON", ui::kib(stats.json_bytes as u64)),
    };
    println!("{}", ui::field("size", size));
    let animation = format!(
        "{}x{}, {} fps, {} ({})",
        stats.width,
        stats.height,
        stats.fps,
        ui::count(stats.frames, "frame"),
        ui::seconds(stats.frames / stats.fps.max(1.0)),
    );
    println!("{}", ui::field("animation", animation));
    let cost = tgs::limits::telegram::cost(stats.shapes, stats.layers);
    let drawing = format!(
        "{}, up to {} and {} in one; cost {cost} of {}",
        ui::count(stats.layers, "layer"),
        ui::count(stats.max_shapes_per_layer, "shape"),
        ui::count(stats.max_paints_per_layer, "fill"),
        tgs::limits::telegram::MAX_COST
    );
    println!("{}", ui::field("drawing", drawing));
    if !stats.features.is_empty() {
        let features: Vec<&str> = stats.features.iter().copied().collect();
        println!("{}", ui::field("uses", features.join(", ")));
    }
    if issues.is_empty() {
        println!("{}", ui::field("telegram", style("✔ accepts it").green()));
    }
    for issue in issues {
        let label = match issue.severity {
            tgs::Severity::Error => ui::error_label(),
            tgs::Severity::Warning => ui::warning_label(),
        };
        println!("  {label} {}", issue.message);
    }
}
