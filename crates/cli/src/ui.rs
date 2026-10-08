//! Text output helpers. Progress and diagnostics go to stderr, results to
//! stdout.

use console::style;
use tgradish_core::events::{Params, Rate};
use tgradish_core::telegram;

pub fn error_label() -> console::StyledObject<&'static str> {
    style("error:").red().bold()
}

pub fn warning_label() -> console::StyledObject<&'static str> {
    style("warning:").yellow().bold()
}

/// Size in KiB with the share of Telegram's limit, like `252.4 KiB (98.6%)`.
pub fn size(bytes: u64) -> String {
    format!(
        "{:.1} KiB ({:.1}%)",
        bytes as f64 / 1024.0,
        bytes as f64 / telegram::MAX_BYTES as f64 * 100.0
    )
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

/// Name of a value as used in options and JSON, like `auto` for `Fit::Auto`.
pub fn name(value: &impl serde::Serialize) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(name)) => name,
        _ => "?".into(),
    }
}
