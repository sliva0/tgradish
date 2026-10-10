//! What the terminal can do beyond plain text: colours, links on file
//! names, progress in the tab or taskbar, and pictures. Found once from the
//! environment, since terminals can't be asked without waiting for answers.
//!
//! - Colours follow `--color`, then `NO_COLOR` and `CLICOLOR_FORCE`.
//! - Links (OSC 8) are on in terminals known to show them; `FORCE_HYPERLINK`
//!   set to 1 or 0 decides instead.
//! - Progress (OSC 9;4) shows in Windows Terminal's tab and taskbar,
//!   ConEmu's and Ghostty's. Elsewhere OSC 9 can be a notification, so it
//!   is only sent where it is known to mean progress.
//! - Pictures use kitty's graphics protocol (kitty, Ghostty) or iTerm2's
//!   inline images (iTerm2, WezTerm).

use std::io::{IsTerminal, Write};
use std::sync::OnceLock;

use clap::ValueEnum;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ColorChoice {
    Auto,
    Always,
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Images {
    Kitty,
    Iterm,
}

#[derive(Debug, Clone, Copy, Default)]
struct Features {
    links: bool,
    progress: bool,
    images: Option<Images>,
}

static FEATURES: OnceLock<Features> = OnceLock::new();

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

/// Finds out what the terminal does. Plain text when `plain`, as for JSON.
pub fn init(color: ColorChoice, plain: bool) {
    let colors = match color {
        _ if plain => false,
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto if env("NO_COLOR").is_some() => false,
        ColorChoice::Auto if env("CLICOLOR_FORCE").is_some_and(|v| v != "0") => true,
        ColorChoice::Auto => console::colors_enabled_stderr() && console::colors_enabled(),
    };
    console::set_colors_enabled(colors);
    console::set_colors_enabled_stderr(colors);
    let features = if plain || color == ColorChoice::Never || !std::io::stderr().is_terminal() {
        Features::default()
    } else {
        detect(colors)
    };
    let _ = FEATURES.set(features);
}

fn detect(colors: bool) -> Features {
    let term = env("TERM").unwrap_or_default();
    if term == "dumb" {
        return Features::default();
    }
    let program = env("TERM_PROGRAM").unwrap_or_default();
    let kitty = env("KITTY_WINDOW_ID").is_some() || term == "xterm-kitty";
    let ghostty = program == "ghostty" || term == "xterm-ghostty";
    let wezterm = program == "WezTerm" || env("WEZTERM_PANE").is_some();
    let iterm = program == "iTerm.app";
    let windows_terminal = env("WT_SESSION").is_some();
    let vte = env("VTE_VERSION").and_then(|v| v.parse::<u32>().ok()).is_some_and(|v| v >= 5000);
    let links = match env("FORCE_HYPERLINK") {
        Some(force) => force != "0",
        None => {
            colors
                && (kitty
                    || ghostty
                    || wezterm
                    || iterm
                    || windows_terminal
                    || vte
                    || env("KONSOLE_VERSION").is_some()
                    || matches!(program.as_str(), "vscode" | "Hyper")
                    || matches!(term.as_str(), "foot" | "foot-extra" | "alacritty"))
        }
    };
    let progress = windows_terminal || env("ConEmuPID").is_some() || ghostty;
    let images = if kitty || ghostty {
        Some(Images::Kitty)
    } else if iterm || wezterm {
        Some(Images::Iterm)
    } else {
        None
    };
    Features { links, progress, images }
}

fn features() -> Features {
    FEATURES.get().copied().unwrap_or_default()
}

/// `text` linking to the file at `path`, for stderr, where links show.
pub fn link(text: impl std::fmt::Display, path: &std::path::Path) -> String {
    if !features().links {
        return text.to_string();
    }
    linked(text, path)
}

/// [`link`] for stdout, which may go to a file even when stderr shows.
pub fn link_out(text: impl std::fmt::Display, path: &std::path::Path) -> String {
    if !features().links || !std::io::stdout().is_terminal() {
        return text.to_string();
    }
    linked(text, path)
}

fn linked(text: impl std::fmt::Display, path: &std::path::Path) -> String {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let url = url_of(&absolute);
    format!("\x1b]8;;{url}\x1b\\{text}\x1b]8;;\x1b\\")
}

/// A `file://` URL, with what URLs can't hold escaped.
fn url_of(path: &std::path::Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let mut url = String::from("file://");
    if !text.starts_with('/') {
        url.push('/');
    }
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' | b':' => {
                url.push(byte as char);
            }
            _ => url.push_str(&format!("%{byte:02X}")),
        }
    }
    url
}

/// Shows how far work is in the tab or taskbar, or clears it with `None`.
pub fn progress(fraction: Option<f64>) {
    if !features().progress {
        return;
    }
    let code = match fraction {
        Some(fraction) => format!("\x1b]9;4;1;{}\x07", (fraction.clamp(0.0, 1.0) * 100.0).round()),
        None => "\x1b]9;4;0;0\x07".to_owned(),
    };
    let mut stderr = std::io::stderr();
    let _ = stderr.write_all(code.as_bytes());
    let _ = stderr.flush();
}

/// Whether [`picture`] can show anything on stdout.
pub fn shows_pictures() -> bool {
    features().images.is_some() && std::io::stdout().is_terminal()
}

/// Shows an RGBA picture in the terminal, `rows` lines high, where it can.
pub fn picture(rgba: &[u8], width: u32, height: u32, rows: u32) {
    let Some(images) = features().images.filter(|_| std::io::stdout().is_terminal()) else {
        return;
    };
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let Ok(mut writer) = encoder.write_header() else { return };
        if writer.write_image_data(rgba).is_err() {
            return;
        }
    }
    let data = base64(&png);
    let mut out = String::new();
    match images {
        Images::Kitty => {
            // in chunks of at most 4096 bytes, as the protocol asks
            let chunks: Vec<&str> = data
                .as_bytes()
                .chunks(4096)
                .map(|chunk| std::str::from_utf8(chunk).expect("base64 is ASCII"))
                .collect();
            for (index, chunk) in chunks.iter().enumerate() {
                let more = u8::from(index + 1 < chunks.len());
                if index == 0 {
                    out.push_str(&format!("\x1b_Gf=100,a=T,r={rows},m={more};{chunk}\x1b\\"));
                } else {
                    out.push_str(&format!("\x1b_Gm={more};{chunk}\x1b\\"));
                }
            }
        }
        Images::Iterm => {
            out.push_str(&format!(
                "\x1b]1337;File=inline=1;height={rows};preserveAspectRatio=1;size={}:{data}\x07",
                png.len()
            ));
        }
    }
    out.push('\n');
    let mut stdout = std::io::stdout();
    let _ = stdout.write_all(out.as_bytes());
    let _ = stdout.flush();
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, &b)| n | u32::from(b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_base64() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
    }

    #[test]
    fn makes_file_urls() {
        assert_eq!(url_of(std::path::Path::new("/a b/ü.webm")), "file:///a%20b/%C3%BC.webm");
        assert_eq!(url_of(std::path::Path::new("C:\\x\\y.tgs")), "file:///C:/x/y.tgs");
    }
}
