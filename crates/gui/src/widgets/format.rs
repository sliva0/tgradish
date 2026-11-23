//! Formatting and parsing of the values the GUI shows: sizes, times,
//! speeds and frame rates.

pub(crate) fn kib(bytes: u64) -> String {
    format!("{:.1} KiB", bytes as f64 / 1024.0)
}

/// Formats seconds as `1:02.50` or `2.50 s` to the nearest hundredth, rounding
/// first so 119.996 becomes `2:00.00` rather than `1:60.00`.
pub(crate) fn seconds(value: f64) -> String {
    let hundredths = (value * 100.0).round();
    if hundredths >= 6000.0 {
        let hundredths = hundredths as u64;
        let (minutes, rest) = (hundredths / 6000, hundredths % 6000);
        format!("{minutes}:{:02}.{:02}", rest / 100, rest % 100)
    } else {
        format!("{value:.2} s")
    }
}

/// Parses seconds typed in [`seconds`] format (`1:02.50` or `2.50 s`) or as a
/// bare number.
pub(crate) fn parse_seconds(text: &str) -> Option<f64> {
    let text = text.trim();
    let text = text.strip_suffix('s').unwrap_or(text).trim();
    let value = match text.split_once(':') {
        Some((minutes, rest)) => {
            let minutes: u32 = minutes.trim().parse().ok()?;
            let rest: f64 = rest.trim().parse().ok()?;
            (0.0..60.0).contains(&rest).then(|| f64::from(minutes) * 60.0 + rest)?
        }
        None => text.parse().ok()?,
    };
    value.is_finite().then_some(value)
}

// Live numbers (while dragging, playing, or converting) use a fixed number of
// decimal places so their width stays constant: speeds `×1.50`, seconds
// `2.50 s`, frame rates `29.97 fps`. Ubuntu's digits are tabular with no
// inter-digit kerning, so widths only change when crossing a power of ten
// (9.99 or 99.99); `steady_width` and `steady_hint` reserve room for the
// widest expected value. Static or finished numbers use `trimmed` for cleaner
// reading.

/// Decimal places for speeds and frame rates.
pub(crate) const SPEED_DECIMALS: usize = 2;
pub(crate) const FPS_DECIMALS: usize = 2;

/// Formats `value` with `decimals` fixed decimal places.
pub(crate) fn fixed(value: f64, decimals: usize) -> String {
    format!("{value:.decimals$}")
}

/// Formats a speed factor without the `×` prefix (`1.50`).
pub(crate) fn speed(value: f64) -> String {
    fixed(value, SPEED_DECIMALS)
}

/// Formats a frame rate without its unit (`29.97`, `30.00`).
pub(crate) fn fps(value: f64) -> String {
    fixed(value, FPS_DECIMALS)
}

/// Replaces every ASCII digit in `text` with `0` to measure the width of any
/// number with the same digit count.
pub(crate) fn zeros(text: &str) -> String {
    text.chars().map(|c| if c.is_ascii_digit() { '0' } else { c }).collect()
}

/// Returns zero-masked templates for the widest [`seconds`] representations
/// between `0` and `most` (`00.00 s` and, if `most >= 59.995`, `0:00.00`).
pub(crate) fn widest_seconds(most: f64) -> Vec<String> {
    let mut widest = vec![zeros(&seconds(most.clamp(0.0, 59.99)))];
    if most >= 59.995 {
        widest.push(zeros(&seconds(most)));
    }
    widest
}

/// Formats `value` with up to two decimal places, stripping trailing zeros
/// (`60`, `29.97`).
pub(crate) fn trimmed(value: f64) -> String {
    trimmed_to(value, 2)
}

/// Formats `value` with up to `decimals` decimal places, stripping trailing
/// zeros.
pub(crate) fn trimmed_to(value: f64, decimals: usize) -> String {
    let text = format!("{value:.decimals$}");
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_times() {
        assert_eq!(seconds(2.5), "2.50 s");
        assert_eq!(seconds(62.5), "1:02.50");
        assert_eq!(seconds(119.996), "2:00.00");
        // Formatted times round-trip through `parse_seconds` to the hundredth.
        for (time, written) in [(59.996, "1:00.00"), (60.0, "1:00.00"), (119.996, "2:00.00")] {
            assert_eq!(seconds(time), written);
            let read = parse_seconds(written).unwrap();
            assert!((read - (time * 100.0_f64).round() / 100.0).abs() < 1e-9, "{written}");
        }
        assert_eq!(parse_seconds("2.50 s"), Some(2.5));
        assert_eq!(parse_seconds(" 4 "), Some(4.0));
        assert_eq!(parse_seconds("1:75"), None);
        assert_eq!(parse_seconds("x"), None);
    }
}
