//! Text output helpers. Progress and diagnostics go to stderr, results to
//! stdout.

use console::style;
use tgradish_core::events::{Params, Rate};

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

pub fn fps(value: f64) -> String {
    let text = format!("{value:.2}");
    format!("{} fps", text.trim_end_matches('0').trim_end_matches('.'))
}

pub fn params(params: &Params) -> String {
    let rate = match params.rate {
        Rate::Bitrate(kbps) => format!("{kbps} kbit/s"),
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
