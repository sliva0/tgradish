//! Text output helpers. Progress and diagnostics go to stderr, results to
//! stdout.

use console::style;
use tgradish_core::events::{Params, Rate};

/// A line like cargo's: `verb`, coloured and right-aligned in 12 columns,
/// then `text`.
pub fn status(verb: &str, text: impl std::fmt::Display) -> String {
    format!("{} {text}", style(format!("{verb:>12}")).green().bold())
}

/// [`status`] for something that went wrong without stopping everything.
pub fn status_bad(verb: &str, text: impl std::fmt::Display) -> String {
    format!("{} {text}", style(format!("{verb:>12}")).red().bold())
}

/// A line of `key: value` facts, the key coloured.
pub fn field(key: &str, value: impl std::fmt::Display) -> String {
    format!("  {} {value}", style(format!("{key:<11}")).cyan())
}

/// Help in cargo's colours.
pub const fn help_styles() -> clap::builder::Styles {
    use clap::builder::styling::AnsiColor;
    clap::builder::Styles::styled()
        .header(AnsiColor::Green.on_default().bold())
        .usage(AnsiColor::Green.on_default().bold())
        .literal(AnsiColor::Cyan.on_default().bold())
        .placeholder(AnsiColor::Cyan.on_default())
        .error(AnsiColor::Red.on_default().bold())
        .valid(AnsiColor::Cyan.on_default().bold())
        .invalid(AnsiColor::Yellow.on_default().bold())
}

/// Details, weaker than what they explain.
pub fn dim(text: impl std::fmt::Display) -> String {
    style(text).dim().to_string()
}

/// What `tgradish` alone prints: a note on the window above the help.
pub fn introduce() {
    use clap::CommandFactory;
    let window = cfg!(feature = "gui");
    if window {
        let line = "─".repeat(66);
        eprintln!("{}", style(&line).cyan());
        eprintln!(
            "  {} {} {}",
            style("tgradish has a window:").cyan().bold(),
            style("tgradish gui").bold().underlined(),
            style("opens it.").cyan().bold()
        );
        eprintln!(
            "  {}",
            style("It also opens when tgradish starts from a file manager or a menu.").cyan()
        );
        eprintln!("{}", style(&line).cyan());
        eprintln!();
    }
    let _ = crate::args::Cli::command().print_help();
}

pub fn error_label() -> console::StyledObject<&'static str> {
    style("error:").red().bold()
}

pub fn warning_label() -> console::StyledObject<&'static str> {
    style("warning:").yellow().bold()
}

/// A size, and how much of `limit` it uses, like `252.4 KiB (98.6%)`.
pub fn size_within(bytes: u64, limit: u64) -> String {
    format!("{} ({:.1}%)", kib(bytes), bytes as f64 / limit as f64 * 100.0)
}

pub fn kib(bytes: u64) -> String {
    format!("{:.1} KiB", bytes as f64 / 1024.0)
}

pub fn seconds(value: f64) -> String {
    format!("{value:.2} s")
}

/// A number with at most one decimal, without a trailing `.0`.
fn decimal(value: f64) -> String {
    let text = format!("{value:.1}");
    text.strip_suffix(".0").map_or_else(|| text.clone(), str::to_owned)
}

pub fn fps(value: f64) -> String {
    let text = format!("{value:.2}");
    format!("{} fps", text.trim_end_matches('0').trim_end_matches('.'))
}

pub fn params(params: &Params) -> String {
    let rate = match params.rate {
        Rate::Bitrate(kbps) => format!("{} kbit/s", decimal(kbps)),
        Rate::Crf(crf) => format!("crf {crf}"),
        Rate::Lossless => "lossless".into(),
    };
    format!("{}, {}, {rate}", fps(params.fps), seconds(params.length))
}

/// What a hidden mark says, like `tgradish 2.0.0 from the command line,
/// user #3fa9c1`.
pub fn mark(mark: &tgradish_core::mark::Mark) -> String {
    use tgradish_core::mark::Client;
    let client = match mark.client {
        Client::Library => "",
        Client::Cli => " from the command line",
        Client::Window => " in its window",
        Client::Bot => " through the bot",
    };
    let user = if mark.user == 0 { String::new() } else { format!(", user {}", mark.user_text()) };
    format!("tgradish {}{client}{user}", mark.version_text())
}

/// Name of a value as used in options and JSON, like `auto` for `Fit::Auto`.
pub fn name(value: &impl serde::Serialize) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(name)) => name,
        _ => "?".into(),
    }
}
