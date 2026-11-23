//! Lottie subset written by tgradish and its compact JSON serialiser.
//!
//! Default output contains only what pixelart2tgs 1.x stickers (accepted by
//! Telegram) used, plus rectangles without a corner radius (drawn properly by
//! both renderers and accepted in Telegram upload probes) and paths with zero
//! tangents (`[0,0]`) because Telegram now rejects empty tangents (`[]`).
//! [`Style`] restores optional fields for upload probes.

use std::fmt::Write;

use crate::scene::FillRule;

/// Sticker frame rate in frames per second.
const FPS: u32 = crate::limits::telegram::FPS;
const CANVAS: u32 = crate::limits::telegram::CANVAS;

/// Complete Lottie animation produced by [`crate::layout`].
#[derive(Debug, Clone, PartialEq)]
pub struct Animation {
    /// Top-level `nm` field, where tgradish writes its tool signature.
    pub name: Option<String>,
    /// Duration in 60 fps ticks (`op`).
    pub ticks: u32,
    /// Shape layers, ordered top to bottom as required by Lottie.
    pub layers: Vec<Layer>,
}

/// Optional Lottie fields omitted by default. Renderers do not require them,
/// though Telegram's validator might; omitting `"st"` and `"r"` saves about 6%
/// of file size.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Style {
    /// Include `"tgs":1` (written by Telegram's exporter).
    pub tgs_key: bool,
    /// Include `"st":0` on layers.
    pub layer_start: bool,
    /// Include `"r":{"k":0}` (zero corner radius) on rectangles.
    pub rect_roundness: bool,
}

/// A Lottie shape layer.
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    /// `ln` layer name, ignored by renderers; carries [`crate::mark`] bytes
    /// when a sticker has too few rectangles to encode them by order.
    pub id: Option<String>,
    /// Start frame (`ip`, inclusive).
    pub from: u32,
    /// End frame (`op`, exclusive).
    pub to: u32,
    /// Sorted frame spans within `from..to` where the layer is hidden via
    /// opacity hold keyframes.
    pub hidden: Vec<(u32, u32)>,
    /// Transform mapping layer coordinates to the canvas.
    pub transform: Transform,
    /// Shape items, ordered top to bottom.
    pub items: Vec<Item>,
}

/// Maps layer coordinates to canvas pixels: `position + scale * point`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    /// Canvas pixel coordinates of the layer origin `(x, y)`.
    pub position: [f64; 2],
    /// Canvas pixels per layer unit along `(x, y)`.
    pub scale: [f64; 2],
}

/// A Lottie shape item.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// Shape group drawn top to bottom, ending with a fill and an
    /// [`Item::GroupTransform`] (which has opacity keyframes only when the
    /// group is hidden for part of its layer's lifetime, such as a reused shape
    /// shown across non-contiguous frame spans).
    Group(Vec<Item>),
    /// Axis-aligned rectangle in layer coordinates.
    Rect {
        /// Rectangle centre `(x, y)`.
        centre: [f64; 2],
        /// Rectangle `(width, height)`.
        size: [f64; 2],
    },
    /// Closed polygon with straight edges.
    Path(Vec<[f64; 2]>),
    /// Solid fill colour in straight RGBA (alpha becomes fill opacity).
    Fill {
        /// Fill colour `[r, g, b, a]`.
        colour: [u8; 4],
        /// Winding rule for preceding shapes.
        rule: FillRule,
    },
    /// Group transform required at the end of every group; carries hold
    /// keyframes on opacity `(frame, percent)` when visibility changes.
    GroupTransform {
        /// Opacity hold keyframes `(frame, percent)`.
        opacity: Vec<(u32, u8)>,
    },
}

impl Animation {
    /// Serialises to compact JSON with a fixed key order so deflate finds
    /// repeated substrings.
    pub fn to_json(&self, style: Style) -> String {
        let mut out = String::new();
        out.push('{');
        if style.tgs_key {
            out.push_str("\"tgs\":1,");
        }
        // Bodymovin version; everything emitted here fits 5.7 and passed
        // upload probes.
        write!(
            out,
            "\"v\":\"5.7.2\",\"fr\":{FPS},\"ip\":0,\"op\":{},\"w\":{CANVAS},\"h\":{CANVAS},",
            self.ticks
        )
        .unwrap();
        if let Some(name) = &self.name {
            write!(out, "\"nm\":{},", serde_json::Value::from(name.as_str())).unwrap();
        }
        out.push_str("\"layers\":[");
        for (index, layer) in self.layers.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            layer.write(&mut out, style);
        }
        out.push_str("]}");
        out
    }
}

impl Layer {
    fn write(&self, out: &mut String, style: Style) {
        let Transform { position: [x, y], scale: [across, down] } = self.transform;
        // Lottie scales are percentages. Layer positions are in canvas pixels,
        // so 4 decimal places are well below visible precision; shape
        // coordinates use 6 because layer scale magnifies them.
        let (across, down) = (number(across * 100.0, 4), number(down * 100.0, 4));
        if let Some(id) = &self.id {
            write!(out, "{{\"ln\":{},", serde_json::Value::from(id.as_str())).unwrap();
        } else {
            out.push('{');
        }
        write!(
            out,
            "\"ty\":4,\"ks\":{{\"p\":{{\"k\":[{},{}]}},\"s\":{{\"k\":[{across},{down}]}}",
            number(x, 4),
            number(y, 4),
        )
        .unwrap();
        if !self.hidden.is_empty() {
            // Visible from `from`, toggling off and on around each hidden span.
            let mut keys = vec![(self.from, 100)];
            for &(start, end) in &self.hidden {
                keys.extend([(start, 0), (end, 100)]);
            }
            out.push(',');
            write_opacity(out, &keys);
        }
        write!(out, "}},\"ip\":{},\"op\":{},", self.from, self.to).unwrap();
        if style.layer_start {
            out.push_str("\"st\":0,");
        }
        out.push_str("\"shapes\":");
        write_items(out, &self.items, style);
        out.push('}');
    }
}

/// Writes opacity hold keyframes `(frame, percent)`.
fn write_opacity(out: &mut String, keys: &[(u32, u8)]) {
    out.push_str("\"o\":{\"a\":1,\"k\":[");
    for (index, (tick, opacity)) in keys.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        write!(out, "{{\"t\":{tick},\"s\":[{opacity}],\"h\":1}}").unwrap();
    }
    out.push_str("]}");
}

fn write_items(out: &mut String, items: &[Item], style: Style) {
    out.push('[');
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        match item {
            Item::Group(items) => {
                out.push_str("{\"ty\":\"gr\",\"it\":");
                write_items(out, items, style);
                out.push('}');
            }
            Item::Rect { centre: [x, y], size: [w, h] } => {
                write!(
                    out,
                    "{{\"ty\":\"rc\",\"p\":{{\"k\":[{},{}]}},\"s\":{{\"k\":[{},{}]}}",
                    number(*x, 6),
                    number(*y, 6),
                    number(*w, 6),
                    number(*h, 6)
                )
                .unwrap();
                if style.rect_roundness {
                    out.push_str(",\"r\":{\"k\":0}");
                }
                out.push('}');
            }
            Item::Path(points) => {
                // like 1.x: no "c", the first point repeated to close the
                // outline; but zero tangents, since Telegram's server now
                // refuses 1.x's empty ones (and rlottie misdraws paths
                // without any)
                let mut vertices = String::new();
                for &[x, y] in points.iter().chain(points.first()) {
                    if !vertices.is_empty() {
                        vertices.push(',');
                    }
                    write!(vertices, "[{},{}]", number(x, 6), number(y, 6)).unwrap();
                }
                let tangents = vec!["[0,0]"; points.len() + 1].join(",");
                write!(
                    out,
                    "{{\"ty\":\"sh\",\"ks\":{{\"k\":{{\"i\":[{tangents}],\"o\":[{tangents}],\"v\":[{vertices}]}}}}}}"
                )
                .unwrap();
            }
            Item::Fill { colour: [r, g, b, a], rule } => {
                write!(
                    out,
                    "{{\"ty\":\"fl\",\"c\":{{\"k\":[{},{},{}]}}",
                    channel(*r),
                    channel(*g),
                    channel(*b)
                )
                .unwrap();
                if *a < 255 {
                    write!(out, ",\"o\":{{\"k\":{}}}", opacity(*a)).unwrap();
                }
                if *rule == FillRule::EvenOdd {
                    out.push_str(",\"r\":2");
                }
                out.push('}');
            }
            Item::GroupTransform { opacity } if opacity.is_empty() => {
                out.push_str("{\"ty\":\"tr\"}");
            }
            Item::GroupTransform { opacity } => {
                out.push_str("{\"ty\":\"tr\",");
                write_opacity(out, opacity);
                out.push('}');
            }
        }
    }
    out.push(']');
}

/// Formats `value` with at most `decimals` decimals, without trailing zeros,
/// exponents or `-0`.
fn number(value: f64, decimals: usize) -> String {
    let mut text = format!("{value:.decimals$}");
    if text.contains('.') {
        text.truncate(text.trim_end_matches('0').trim_end_matches('.').len());
    }
    if text == "-0" { "0".to_owned() } else { text }
}

/// Returns the shortest decimal string in `low..=high` (`high - low <= 1`).
fn shortest(low: f64, high: f64) -> String {
    for decimals in 0..=6 {
        let step = 10f64.powi(decimals);
        let candidate = (low * step).ceil() / step;
        if candidate <= high {
            return number(candidate, decimals as usize);
        }
    }
    number(low, 6)
}

/// Formats an 8-bit colour channel as a `0..=1` fraction. Renderers truncate
/// `255 * fraction`, so this targets `[value + 0.02, value + 0.98]` for float
/// safety.
fn channel(value: u8) -> String {
    match value {
        0 => "0".to_owned(),
        255 => "1".to_owned(),
        v => shortest((f64::from(v) + 0.02) / 255.0, (f64::from(v) + 0.98) / 255.0),
    }
}

/// Formats alpha as a percentage, using the same truncation margin as
/// [`channel`].
fn opacity(alpha: u8) -> String {
    shortest((f64::from(alpha) + 0.02) / 2.55, (f64::from(alpha) + 0.98) / 2.55)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_numbers() {
        assert_eq!(number(3.0, 4), "3");
        assert_eq!(number(3.5, 4), "3.5");
        assert_eq!(number(-0.00001, 4), "0");
        assert_eq!(number(1089.361702, 4), "1089.3617");
        assert_eq!(number(1e-7, 6), "0");
    }

    #[test]
    fn colours_survive_truncation() {
        for value in 0..=255u8 {
            let fraction: f32 = channel(value).parse().unwrap();
            assert_eq!((fraction * 255.0) as u32, u32::from(value), "{value}: {}", channel(value));
            assert!(channel(value).len() <= 5, "{}", channel(value));
            let percent: f32 = opacity(value).parse().unwrap();
            assert_eq!(
                (percent / 100.0 * 255.0) as u32,
                u32::from(value),
                "{value}: {}",
                opacity(value)
            );
        }
        assert_eq!(channel(128), "0.503");
        // exact boundaries like 51 / 255 = 0.2 are avoided for float safety
        assert_eq!(channel(51), "0.201");
    }

    #[test]
    fn hides_layers_with_opacity_keys() {
        let layer = Layer {
            id: None,
            from: 10,
            to: 90,
            transform: Transform { position: [0.0, 0.0], scale: [1.0, 1.0] },
            items: Vec::new(),
            hidden: vec![(30, 50), (60, 70)],
        };
        let json =
            Animation { name: None, ticks: 90, layers: vec![layer] }.to_json(Style::default());
        assert!(json.contains(
            r#""ks":{"p":{"k":[0,0]},"s":{"k":[100,100]},"o":{"a":1,"k":[{"t":10,"s":[100],"h":1},{"t":30,"s":[0],"h":1},{"t":50,"s":[100],"h":1},{"t":60,"s":[0],"h":1},{"t":70,"s":[100],"h":1}]}},"ip":10,"op":90"#
        ), "{json}");
    }

    #[test]
    fn writes_compact_json() {
        let animation = Animation {
            name: Some("made with \"tgradish\"".into()),
            ticks: 30,
            layers: vec![Layer {
                id: None,
                from: 0,
                to: 30,
                transform: Transform { position: [6.0, 0.0], scale: [10.0, 10.0] },
                items: vec![Item::Group(vec![
                    Item::Rect { centre: [1.5, 2.0], size: [3.0, 4.0] },
                    Item::Path(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]]),
                    Item::Fill { colour: [255, 0, 51, 128], rule: FillRule::EvenOdd },
                    Item::GroupTransform { opacity: Vec::new() },
                ])],
                hidden: Vec::new(),
            }],
        };
        let json = animation.to_json(Style::default());
        assert_eq!(
            json,
            r#"{"v":"5.7.2","fr":60,"ip":0,"op":30,"w":512,"h":512,"nm":"made with \"tgradish\"","layers":[{"ty":4,"ks":{"p":{"k":[6,0]},"s":{"k":[1000,1000]}},"ip":0,"op":30,"shapes":[{"ty":"gr","it":[{"ty":"rc","p":{"k":[1.5,2]},"s":{"k":[3,4]}},{"ty":"sh","ks":{"k":{"i":[[0,0],[0,0],[0,0],[0,0]],"o":[[0,0],[0,0],[0,0],[0,0]],"v":[[0,0],[1,0],[1,1],[0,0]]}}},{"ty":"fl","c":{"k":[1,0,0.201]},"o":{"k":50.3},"r":2},{"ty":"tr"}]}]}]}"#
        );
        serde_json::from_str::<serde_json::Value>(&json).unwrap();
        let bare = Style { tgs_key: true, layer_start: false, rect_roundness: false };
        let json = animation.to_json(bare);
        assert!(
            json.starts_with(r#"{"tgs":1,"v""#)
                && !json.contains("\"st\"")
                && !json.contains("\"r\":{")
        );
    }
}
