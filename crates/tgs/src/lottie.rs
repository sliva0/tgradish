//! The subset of Lottie tgradish writes, and a compact JSON writer for it.
//!
//! The default output has only fields that the stickers 1.x made (accepted
//! by Telegram) or the planning prototypes (checked in rlottie and tlottie)
//! had. [`Style`] switches the rest, for the T9 probes.

use std::fmt::Write;

use crate::scene::FillRule;

/// Lottie's frame rate for stickers.
const FPS: u32 = crate::limits::telegram::FPS;
const CANVAS: u32 = crate::limits::telegram::CANVAS;

#[derive(Debug, Clone, PartialEq)]
pub struct Animation {
    /// The top-level `nm`, where tgradish signs its output.
    pub name: Option<String>,
    /// Length in 60 fps frames: `op`.
    pub ticks: u32,
    /// Top first, as Lottie lists them.
    pub layers: Vec<Layer>,
}

/// Optional fields; renderers don't need them, Telegram's checks might.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Style {
    /// `"tgs":1`, which Telegram's exporter writes.
    pub tgs_key: bool,
    /// `"st":0` on layers.
    pub layer_start: bool,
    /// `"r":{"k":0}` (no rounding) on rectangles.
    pub rect_roundness: bool,
}

impl Default for Style {
    fn default() -> Style {
        Style { tgs_key: false, layer_start: true, rect_roundness: true }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    /// Shown for frames `from..to`: `ip` and `op`.
    pub from: u32,
    pub to: u32,
    pub transform: Transform,
    /// Top first.
    pub items: Vec<Item>,
}

/// Maps layer coordinates onto the canvas: `position + scale * point`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    pub position: [f64; 2],
    /// Canvas pixels per layer unit.
    pub scale: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// Drawn top first; ends with its fill and [`Item::GroupTransform`].
    Group(Vec<Item>),
    Rect {
        centre: [f64; 2],
        size: [f64; 2],
    },
    /// A closed outline with straight edges.
    Path(Vec<[f64; 2]>),
    /// Straight RGBA; translucent colours become fill opacity.
    Fill {
        colour: [u8; 4],
        rule: FillRule,
    },
    /// The empty transform every group needs.
    GroupTransform,
}

impl Animation {
    /// Compact JSON with a stable key order, so equal content gives equal
    /// bytes for deflate to find.
    pub fn to_json(&self, style: Style) -> String {
        let mut out = String::new();
        out.push('{');
        if style.tgs_key {
            out.push_str("\"tgs\":1,");
        }
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
        let Transform { position: [x, y], scale } = self.transform;
        let scale = number(scale * 100.0, 4);
        write!(
            out,
            "{{\"ty\":4,\"ks\":{{\"p\":{{\"k\":[{},{}]}},\"s\":{{\"k\":[{scale},{scale}]}}}},\"ip\":{},\"op\":{},",
            number(x, 4),
            number(y, 4),
            self.from,
            self.to
        )
        .unwrap();
        if style.layer_start {
            out.push_str("\"st\":0,");
        }
        out.push_str("\"shapes\":");
        write_items(out, &self.items, style);
        out.push('}');
    }
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
                // the 1.x format: no "c", the first point repeated to close
                // the outline, empty tangents (rlottie misdraws paths
                // without them)
                let mut vertices = String::new();
                for &[x, y] in points.iter().chain(points.first()) {
                    if !vertices.is_empty() {
                        vertices.push(',');
                    }
                    write!(vertices, "[{},{}]", number(x, 6), number(y, 6)).unwrap();
                }
                let tangents = vec!["[]"; points.len() + 1].join(",");
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
            Item::GroupTransform => out.push_str("{\"ty\":\"tr\"}"),
        }
    }
    out.push(']');
}

/// A number with at most `decimals` decimals, without trailing zeros,
/// exponents or `-0`.
pub fn number(value: f64, decimals: usize) -> String {
    let mut text = format!("{value:.decimals$}");
    if text.contains('.') {
        text.truncate(text.trim_end_matches('0').trim_end_matches('.').len());
    }
    if text == "-0" { "0".to_owned() } else { text }
}

/// The shortest decimal in `low..=high`, which must be at most 1 apart.
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

/// A colour channel as a fraction. Renderers truncate `255 * fraction`, so
/// this aims inside `[value, value + 1)` with a margin for float maths.
pub fn channel(value: u8) -> String {
    match value {
        0 => "0".to_owned(),
        255 => "1".to_owned(),
        v => shortest((f64::from(v) + 0.02) / 255.0, (f64::from(v) + 0.98) / 255.0),
    }
}

/// Alpha as fill opacity in percent; truncated like colour channels.
pub fn opacity(alpha: u8) -> String {
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
    fn writes_compact_json() {
        let animation = Animation {
            name: Some("made with \"tgradish\"".into()),
            ticks: 30,
            layers: vec![Layer {
                from: 0,
                to: 30,
                transform: Transform { position: [6.0, 0.0], scale: 10.0 },
                items: vec![Item::Group(vec![
                    Item::Rect { centre: [1.5, 2.0], size: [3.0, 4.0] },
                    Item::Path(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]]),
                    Item::Fill { colour: [255, 0, 51, 128], rule: FillRule::EvenOdd },
                    Item::GroupTransform,
                ])],
            }],
        };
        let json = animation.to_json(Style::default());
        assert_eq!(
            json,
            r#"{"v":"5.7.2","fr":60,"ip":0,"op":30,"w":512,"h":512,"nm":"made with \"tgradish\"","layers":[{"ty":4,"ks":{"p":{"k":[6,0]},"s":{"k":[1000,1000]}},"ip":0,"op":30,"st":0,"shapes":[{"ty":"gr","it":[{"ty":"rc","p":{"k":[1.5,2]},"s":{"k":[3,4]},"r":{"k":0}},{"ty":"sh","ks":{"k":{"i":[[],[],[],[]],"o":[[],[],[],[]],"v":[[0,0],[1,0],[1,1],[0,0]]}}},{"ty":"fl","c":{"k":[1,0,0.201]},"o":{"k":50.3},"r":2},{"ty":"tr"}]}]}]}"#
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
