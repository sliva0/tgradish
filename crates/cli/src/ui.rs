//! Text output helpers. Progress and diagnostics go to stderr, results to
//! stdout.

use console::style;
use tgradish_core::telegram::{Issue, Severity, Target};
use tgradish_core::webm::{Params, Rate};

/// Formats a cargo-style status line: `verb` right-aligned in 12 columns and
/// coloured green, followed by `text`.
pub fn status(verb: &str, text: impl std::fmt::Display) -> String {
    format!("{} {text}", style(format!("{verb:>12}")).green().bold())
}

/// Like [`status`], coloured red for a non-fatal failure.
pub fn status_bad(verb: &str, text: impl std::fmt::Display) -> String {
    format!("{} {text}", style(format!("{verb:>12}")).red().bold())
}

/// Formats an aligned `key  value` line with a coloured key.
pub fn field(key: &str, value: impl std::fmt::Display) -> String {
    format!("  {} {value}", style(format!("{key:<11}")).cyan())
}

/// Clap help styles matching cargo's colour scheme.
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

/// Dims secondary detail text.
pub fn dim(text: impl std::fmt::Display) -> String {
    style(text).dim().to_string()
}

/// Prints the GUI notice banner and top-level CLI help.
///
/// Writes help to stderr when `to_stderr` is true (for example when launched
/// without a display).
pub fn introduce(to_stderr: bool) {
    use clap::CommandFactory;
    if cfg!(feature = "gui") {
        // as wide as the note, or the terminal where that is narrower
        let width = console::Term::stderr().size_checked().map_or(66, |(_, columns)| columns);
        let line = "─".repeat(usize::from(width).min(66));
        eprintln!("{}", style(&line).cyan());
        eprintln!(
            "  {} {} {}",
            style("tgradish has a GUI:").cyan().bold(),
            style("tgradish gui").bold().underlined(),
            style("opens it.").cyan().bold()
        );
        eprintln!(
            "  {}",
            style("It also opens when tgradish starts from a file manager or menu.").cyan()
        );
        eprintln!("{}", style(&line).cyan());
        eprintln!();
    }
    if to_stderr {
        let help = crate::args::Cli::command().render_help();
        if console::colors_enabled_stderr() {
            eprint!("{}", help.ansi());
        } else {
            eprint!("{help}");
        }
    } else {
        let _ = crate::args::Cli::command().print_help();
    }
}

/// Prefixes `message` with a clickable link to `path`, avoiding duplication if
/// `message` already starts with `path`.
pub fn about(path: &std::path::Path, message: &str) -> String {
    let name = path.display().to_string();
    let link = crate::term::link(&name, path);
    match message.strip_prefix(&name) {
        Some(rest) if rest.starts_with([' ', ':']) => format!("{link}{rest}"),
        _ => format!("{link}: {message}"),
    }
}

/// Formats the message printed when `tgradish gui` hands files to an already
/// running GUI window.
#[cfg_attr(not(feature = "gui"), allow(dead_code))]
pub fn handed_over(files: usize, first: Option<&std::path::Path>) -> String {
    let what = match (files, first) {
        (0, _) => "brought window to the front".to_owned(),
        (1, Some(file)) => format!("added {} to it", crate::term::link(file.display(), file)),
        (files, _) => format!("added {} to it", count(files, "file")),
    };
    format!("tgradish is already running: {what}")
}

pub fn error_label() -> console::StyledObject<&'static str> {
    style("error:").red().bold()
}

pub fn warning_label() -> console::StyledObject<&'static str> {
    style("warning:").yellow().bold()
}

/// Returns the styled `error:` or `warning:` label for `severity`.
fn severity_label(severity: Severity) -> console::StyledObject<&'static str> {
    match severity {
        Severity::Error => error_label(),
        _ => warning_label(),
    }
}

/// Formats a result issue for stderr: `.tgs`-specific issues use their
/// severity label; shared limit issues are warned as Telegram rejections.
pub fn issue(issue: &Issue) -> String {
    match issue {
        Issue::Lottie(_) => format!("{} {issue}", severity_label(issue.severity())),
        _ => format!("{} Telegram will reject it: {issue}", warning_label()),
    }
}

/// Formats Telegram's verdict on a file with `issues` for `inspect`: either
/// accepted (naming `target` when checked as one), or rejected with shared
/// limit failures listed under the rejection and `.tgs`-only issues labelled
/// by their severity.
pub fn verdict(issues: &[Issue], target: Option<Target>) -> Vec<String> {
    let named = target.map_or(String::new(), |target| format!(" as {}", target.name()));
    if issues.is_empty() {
        return vec![field("telegram", format!("{}{named}", style("✔ accepted").green()))];
    }
    let (lottie, shared): (Vec<&Issue>, Vec<&Issue>) =
        issues.iter().partition(|issue| matches!(issue, Issue::Lottie(_)));
    let mut lines = Vec::new();
    if !shared.is_empty() {
        lines.push(field("telegram", format!("{}{named}:", style("✖ rejected").red())));
        lines.extend(
            shared.iter().map(|issue| format!("              {} {issue}", style("-").red())),
        );
    }
    lines.extend(
        lottie.iter().map(|issue| format!("  {} {issue}", severity_label(issue.severity()))),
    );
    lines
}

/// Formats a byte size and percentage of `limit`, e.g. `252.4 KiB (98.6%)`.
pub fn size_within(bytes: u64, limit: u64) -> String {
    format!("{} ({:.1}%)", kib(bytes), bytes as f64 / limit as f64 * 100.0)
}

/// Formats `n` with a singular or plural noun (appending `"s"` unless
/// `n == 1`).
pub fn count(n: impl std::fmt::Display, what: &str) -> String {
    let n = n.to_string();
    format!("{n} {what}{}", if n == "1" { "" } else { "s" })
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
        rate => format!("{rate:?}"),
    };
    format!("{}, {}, {rate}", fps(params.fps), seconds(params.length))
}

/// Name of a value as used in options and JSON, like `auto` for `Fit::Auto`.
pub fn name(value: &impl serde::Serialize) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(name)) => name,
        _ => "?".into(),
    }
}
