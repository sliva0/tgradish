use std::path::PathBuf;

use anyhow::{Context as _, Result, bail};
use console::style;
use serde_json::json;
use tgradish_core::backend::BackendInfo;
use tgradish_core::config::Config;
use tgradish_core::inspect::{self, Inspected};
use tgradish_core::presets::{self, Format, Presets};
use tgradish_core::telegram::{self, Target};
use tgradish_core::tgradish_tgs::{check, limits};
use tgradish_core::tgs;
use tgradish_core::webm::Options;
use tgradish_core::webm::container::{self, Patch, WebmInfo};
use tgradish_core::webp;
use tgradish_core::{TOOL_ID, protocol};

use crate::term;

use crate::Context;
use crate::args::{ConfigCommand, FfmpegCommand, InspectArgs, PresetCommand, SpoofArgs};
use crate::ui;

/// What `spoof` puts after the name of a WebM it writes beside the input.
pub const SPOOFED: &str = ".spoofed.webm";

pub fn spoof(ctx: &Context, args: SpoofArgs) -> Result<()> {
    let output = match (&args.output, args.in_place) {
        (_, true) => args.input.clone(),
        (Some(output), false) => output.clone(),
        (None, false) => args.input.with_extension(&SPOOFED[1..]),
    };
    if output.is_dir() {
        bail!("{} is a directory: -o names the spoofed file", output.display());
    }
    if !args.in_place && !args.overwrite && output != args.input && output.exists() {
        bail!("{} already exists, use --overwrite to replace it", output.display());
    }
    let watermark = !args.no_watermark;
    let changes = Patch {
        duration: Some(args.fake_duration),
        title: args.title.clone(),
        muxing_app: watermark.then(|| TOOL_ID.to_string()),
        signature: watermark.then(tgradish_core::signature),
        ..Default::default()
    };
    let replace = args.overwrite || output == args.input;
    let report = container::patch_file(&args.input, &output, &changes, replace)
        .with_context(|| format!("couldn't spoof {}", args.input.display()))?;

    // a duration Telegram doesn't take makes a file it refuses
    let refused = args.fake_duration > telegram::MAX_SECONDS;
    let done = || if refused { Err(crate::Exit(crate::REFUSED).into()) } else { Ok(()) };
    if ctx.global.json {
        crate::json_line(&json!({ "output": output.to_string_lossy(), "report": report }));
        return done();
    }
    if refused {
        eprintln!(
            "{} {} is longer than {} s, Telegram will still refuse it",
            ui::warning_label(),
            ui::seconds(args.fake_duration),
            telegram::MAX_SECONDS
        );
    }
    if report.duration_tags_skipped > 0 {
        eprintln!("{} some DURATION tags had no room for the new value", ui::warning_label());
    }
    let old = report.old_duration.map_or("unknown".into(), ui::seconds);
    outln!(
        "{}",
        ui::status(
            "Spoofed",
            format!(
                "{}: metadata duration {old} → {}",
                term::link_out(style(output.display()).bold(), &output),
                ui::seconds(args.fake_duration)
            )
        )
    );
    done()
}

fn print_image(
    path: &std::path::Path,
    info: &webp::Info,
    target: Target,
    issues: &[telegram::Issue],
) {
    outln!("{}", style(term::link_out(path.display(), path)).bold());
    outln!("{}", ui::field("size", ui::size_within(info.bytes, telegram::MAX_STILL_BYTES)));
    let kind = if info.lossy { "lossy" } else { "lossless" };
    let alpha = if info.alpha { ", transparent" } else { "" };
    outln!(
        "{}",
        ui::field("picture", format!("WebP {}x{}, {kind}{alpha}", info.width, info.height))
    );
    if let Some(tool) = &info.made_with {
        outln!("{}", ui::field("made", format!("with {tool}")));
    }
    print_verdict(issues, target);
}

/// What Telegram would say about a sticker.
fn print_verdict(issues: &[telegram::Issue], target: Target) {
    if issues.is_empty() {
        let accepted = format!("{} as {}", style("✔ accepts it").green(), target.name());
        outln!("{}", ui::field("telegram", accepted));
    } else {
        let refused = format!("{} as {}:", style("✖ refuses it").red(), target.name());
        outln!("{}", ui::field("telegram", refused));
        for issue in issues {
            outln!("              {} {issue}", style("-").red());
        }
    }
}

fn print_info(path: &std::path::Path, info: &WebmInfo, target: Target, issues: &[telegram::Issue]) {
    outln!("{}", style(term::link_out(path.display(), path)).bold());
    outln!("{}", ui::field("size", ui::size_within(info.file_size, target.max_bytes())));
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
    outln!("{}", ui::field("video", video));
    if info.audio_tracks > 0 {
        outln!("{}", ui::field("audio", ui::count(info.audio_tracks, "track")));
    }
    let header = info.header_duration.map_or("none".into(), ui::seconds);
    let content = info.content_duration.map_or("unknown".into(), ui::seconds);
    let spoofed = if info.spoofed() { " (spoofed)" } else { "" };
    let duration = format!("{header} in metadata, {content} of video{spoofed}");
    outln!("{}", ui::field("duration", duration));
    if let Some(title) = &info.title {
        outln!("{}", ui::field("title", title));
    }
    let apps: Vec<_> = [("written by", &info.writing_app), ("muxed by", &info.muxing_app)]
        .into_iter()
        .filter_map(|(label, app)| app.as_ref().map(|app| format!("{label} {app}")))
        .collect();
    if !apps.is_empty() {
        outln!("{}", ui::field("made", apps.join(", ")));
    }
    if let Some(signature) = &info.signature {
        outln!("{}", ui::field("signature", signature));
    }
    if info.truncated {
        outln!("  {}", style("the file is truncated").red());
    }

    print_verdict(issues, target);
}

/// Says a file couldn't be inspected, and goes on to the rest.
fn print_failure(ctx: &Context, path: &std::path::Path, err: &impl std::fmt::Display) {
    if ctx.global.json {
        crate::print_json_error(&anyhow::anyhow!("{err}"), Some(path));
    } else {
        eprintln!("{} {}: {err}", ui::error_label(), path.display());
    }
}

pub fn inspect(ctx: &Context, args: InspectArgs) -> Result<()> {
    let (mut failed, mut refused) = (0, false);
    for (i, path) in args.files.iter().enumerate() {
        let inspected = match inspect::inspect(path, args.target.map(Into::into)) {
            Ok(inspected) => inspected,
            Err(err) => {
                failed += 1;
                print_failure(ctx, path, &err);
                continue;
            }
        };
        refused |= inspected.refused();
        if ctx.global.json {
            #[derive(serde::Serialize)]
            struct Line<'a> {
                #[serde(serialize_with = "protocol::serialize_path")]
                input: &'a std::path::Path,
                #[serde(flatten)]
                inspected: &'a Inspected,
            }
            crate::json_line(&Line { input: path, inspected: &inspected });
            continue;
        }
        if i > 0 {
            outln!();
        }
        match &inspected {
            Inspected::Image { target, info, issues } => print_image(path, info, *target, issues),
            Inspected::Tgs { info, issues } => print_sticker(path, info, issues),
            Inspected::Webm { target, info, issues } => print_info(path, info, *target, issues),
            // a kind core has that this command line doesn't word yet
            _ => outln!("{}: shown with --json", path.display()),
        }
    }
    if failed > 0 {
        // each failure was already reported with its input
        if ctx.global.json || args.files.len() == 1 {
            return Err(crate::Exit(1).into());
        }
        bail!("{failed} of {} couldn't be read", ui::count(args.files.len(), "file"));
    }
    if refused {
        return Err(crate::Exit(crate::REFUSED).into());
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
            outln!();
        }
        outln!("{}", style(&notice.title).bold());
        outln!("{}\n", "=".repeat(notice.title.chars().count()));
        outln!("{}", notice.text.trim_end());
    }
}

pub fn describe(ctx: &Context) -> Result<()> {
    let presets = Presets::load_user()?;
    crate::json_line(&protocol::describe(&presets, &ctx.config));
    Ok(())
}

/// What `name` is the default preset for, as `preset list` says it: of
/// every format, or the ones named.
fn default_for(ctx: &Context, name: &str) -> String {
    const FORMATS: [Format; 3] = [Format::Webm, Format::Tgs, Format::Webp];
    let formats: Vec<&str> = FORMATS
        .into_iter()
        .filter(|&format| ctx.config.preset_for(format) == name)
        .map(Format::name)
        .collect();
    match formats.as_slice() {
        [] => String::new(),
        all if all.len() == FORMATS.len() => " (default)".into(),
        [one] => format!(" (default for {one})"),
        [some @ .., last] => format!(" (default for {} and {last})", some.join(", ")),
    }
}

/// `preset`, its options with everything it extends applied, as a preset
/// file could hold it.
fn preset_file(
    preset: &presets::Preset,
    webm: &Options,
    tgs: &tgs::Options,
) -> Result<toml::Table> {
    let mut file = toml::Table::new();
    if !preset.description.is_empty() {
        file.insert("description".into(), preset.description.clone().into());
    }
    if let Some(base) = &preset.extends {
        file.insert("extends".into(), base.clone().into());
    }
    file.insert("webm".into(), toml::Table::try_from(webm)?.into());
    file.insert("tgs".into(), toml::Table::try_from(tgs)?.into());
    Ok(file)
}

pub fn preset(ctx: &Context, command: PresetCommand) -> Result<()> {
    let presets = Presets::load_user()?;
    match command {
        PresetCommand::List if ctx.global.json => {
            for preset in protocol::describe(&presets, &ctx.config).presets {
                crate::json_line(&preset);
            }
        }
        PresetCommand::List => {
            for (name, preset) in presets.iter() {
                let default = default_for(ctx, name);
                match preset {
                    Ok(preset) => {
                        let origin = preset
                            .path
                            .as_ref()
                            .map_or("built-in".to_string(), |p| p.display().to_string());
                        outln!("{}{default}  {}", style(name).bold(), preset.description);
                        outln!("    {}", style(origin).dim());
                    }
                    Err(err) => outln!("{}  {} {err}", style(name).bold(), ui::error_label()),
                }
            }
        }
        PresetCommand::Show { name } => {
            let preset = presets.get(&name)?;
            let (webm, tgs) = (presets.webm(&name)?, presets.tgs(&name)?);
            let file = preset_file(preset, &webm, &tgs)?;
            if ctx.global.json {
                crate::json_line(&file);
            } else {
                out!("{}", toml::to_string(&file)?);
            }
        }
        PresetCommand::Path => match presets::user_dir() {
            Some(dir) if ctx.global.json => {
                crate::json_line(&json!({ "path": dir.to_string_lossy() }))
            }
            Some(dir) => outln!("{}", dir.display()),
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
                crate::json_line(
                    &json!({ "path": path.to_string_lossy(), "exists": path.exists() }),
                );
            } else {
                outln!("{}", path.display());
            }
        }
        ConfigCommand::Show if ctx.global.json => crate::json_line(&ctx.config),
        ConfigCommand::Show => {
            if ctx.config == Config::default() {
                eprintln!("# no config file, using defaults");
            }
            out!("{}", toml::to_string(&ctx.config)?);
        }
    }
    Ok(())
}

pub fn ffmpeg(ctx: &Context, command: FfmpegCommand) -> Result<()> {
    match command {
        FfmpegCommand::Status => {
            let backend = ctx.backend().map_err(crate::explain)?;
            let caps = backend.capabilities(&ctx.cancel)?;
            if ctx.global.json {
                crate::json_line(&json!({ "ffmpeg": backend.info(), "capabilities": caps }));
                if !caps.libvpx_vp9 {
                    return Err(crate::Exit(1).into());
                }
            } else {
                match backend.info() {
                    BackendInfo::Process { ffmpeg } => {
                        outln!("ffmpeg   {}", ffmpeg.ffmpeg.display());
                        outln!("ffprobe  {}", ffmpeg.ffprobe.display());
                        outln!("source   {}", ui::name(&ffmpeg.source));
                    }
                    BackendInfo::Builtin => outln!("ffmpeg   built into tgradish"),
                    other => outln!("ffmpeg   {other:?}"),
                }
                outln!("version  {}", caps.version);
                let vp9 = if caps.libvpx_vp9 { style("yes").green() } else { style("no").red() };
                outln!("libvpx-vp9 encoder  {vp9}");
                if !caps.libvpx_vp9 {
                    bail!("this ffmpeg can't encode VP9 with libvpx, stickers need it");
                }
            }
        }
    }
    Ok(())
}

fn print_sticker(path: &std::path::Path, stats: &check::Stats, issues: &[check::Issue]) {
    outln!("{}", style(term::link_out(path.display(), path)).bold());
    if let Some(name) = &stats.name {
        outln!("{}", ui::field("name", name));
    }
    let size = match stats.tgs_bytes {
        Some(bytes) => {
            let packed = ui::size_within(bytes as u64, tgs::MAX_BYTES);
            format!("{packed}, {} of JSON", ui::kib(stats.json_bytes as u64))
        }
        None => format!("{} of JSON", ui::kib(stats.json_bytes as u64)),
    };
    outln!("{}", ui::field("size", size));
    let animation = format!(
        "{}x{}, {} fps, {} ({})",
        stats.width,
        stats.height,
        stats.fps,
        ui::count(stats.frames, "frame"),
        ui::seconds(stats.frames / stats.fps.max(1.0)),
    );
    outln!("{}", ui::field("animation", animation));
    let cost = limits::telegram::cost(stats.shapes, stats.layers);
    let drawing = format!(
        "{}, up to {} and {} in one; cost {cost} of {}",
        ui::count(stats.layers, "layer"),
        ui::count(stats.max_shapes_per_layer, "shape"),
        ui::count(stats.max_paints_per_layer, "fill"),
        limits::telegram::MAX_COST
    );
    outln!("{}", ui::field("drawing", drawing));
    if !stats.features.is_empty() {
        let features: Vec<&str> = stats.features.iter().copied().collect();
        outln!("{}", ui::field("uses", features.join(", ")));
    }
    if issues.is_empty() {
        outln!("{}", ui::field("telegram", style("✔ accepts it").green()));
    }
    for issue in issues {
        let label = match issue.severity {
            check::Severity::Error => ui::error_label(),
            _ => ui::warning_label(),
        };
        outln!("  {label} {}", issue.message);
    }
}
