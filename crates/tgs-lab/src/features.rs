//! Probes for Lottie features. Telegram's rules forbid some
//! that its own stickers use, and others are on no list but could help the
//! encoder. Each probe is a plain square with one feature added, so a
//! refusal names the feature, and a render shows whether tlottie draws it.

use std::path::Path;

use anyhow::Result;
use serde_json::{Value, json};

use crate::probes::{Probe, save};

const BLUE: [f64; 3] = [0.24, 0.55, 0.9];
const RED: [f64; 3] = [0.9, 0.24, 0.24];

fn value(k: Value) -> Value {
    json!({ "a": 0, "k": k })
}

/// Hold keyframes, one per `(tick, value)`.
fn keys(steps: &[(u32, Value)]) -> Value {
    let keys: Vec<Value> = steps.iter().map(|(t, s)| json!({ "t": t, "s": s, "h": 1 })).collect();
    json!({ "a": 1, "k": keys })
}

fn rect(size: f64) -> Value {
    json!({ "ty": "rc", "d": 1, "p": value(json!([0, 0])), "s": value(json!([size, size])), "r": value(json!(0)) })
}

fn ellipse(size: f64) -> Value {
    json!({ "ty": "el", "d": 1, "p": value(json!([0, 0])), "s": value(json!([size, size])) })
}

fn fill(rgb: [f64; 3]) -> Value {
    json!({ "ty": "fl", "c": value(json!([rgb[0], rgb[1], rgb[2], 1])), "o": value(json!(100)), "r": 1 })
}

fn stroke(rgb: [f64; 3], width: f64) -> Value {
    json!({
        "ty": "st", "c": value(json!([rgb[0], rgb[1], rgb[2], 1])), "o": value(json!(100)),
        "w": value(json!(width)), "lc": 2, "lj": 2, "ml": 4,
    })
}

fn transform() -> Value {
    json!({
        "ty": "tr", "p": value(json!([0, 0])), "a": value(json!([0, 0])),
        "s": value(json!([100, 100])), "r": value(json!(0)), "o": value(json!(100)),
    })
}

fn group(mut items: Vec<Value>) -> Value {
    items.push(transform());
    json!({ "ty": "gr", "it": items })
}

/// A closed outline through `points`, with zero tangents.
fn outline(points: &[[f64; 2]]) -> Value {
    let zero = vec![json!([0, 0]); points.len()];
    json!({ "c": true, "i": zero, "o": zero, "v": points })
}

fn diamond() -> Value {
    outline(&[[0.0, -110.0], [110.0, 0.0], [0.0, 110.0], [-110.0, 0.0]])
}

/// The transform of a layer centred on the canvas.
fn layer_transform() -> Value {
    json!({
        "o": value(json!(100)), "r": value(json!(0)), "p": value(json!([256, 256, 0])),
        "a": value(json!([0, 0, 0])), "s": value(json!([100, 100, 100])),
    })
}

fn shape_layer(index: u32, shapes: Vec<Value>) -> Value {
    json!({
        "ddd": 0, "ind": index, "ty": 4, "nm": format!("layer {index}"), "sr": 1,
        "ks": layer_transform(), "ao": 0, "shapes": shapes, "ip": 0, "op": 60, "st": 0, "bm": 0,
    })
}

/// The plain sticker every probe changes: a blue square in a layer.
fn square_layer(index: u32) -> Value {
    shape_layer(index, vec![group(vec![rect(220.0), fill(BLUE)])])
}

fn document(layers: Vec<Value>) -> Value {
    json!({
        "v": "5.7.2", "fr": 60, "ip": 0, "op": 60, "w": 512, "h": 512,
        "nm": "tgradish feature probe", "ddd": 0, "assets": [], "layers": layers,
    })
}

/// The square with `change` made to its layer.
fn changed_layer(change: impl FnOnce(&mut Value)) -> Value {
    let mut layer = square_layer(1);
    change(&mut layer);
    document(vec![layer])
}

/// The square with `items` instead of its own shape.
fn shapes(items: Vec<Value>) -> Value {
    document(vec![shape_layer(1, vec![group(items)])])
}

fn mask(mode: &str, inverted: bool) -> Value {
    changed_layer(|layer| {
        layer["hasMask"] = json!(true);
        layer["masksProperties"] = json!([{
            "inv": inverted, "mode": mode, "pt": value(diamond()), "o": value(json!(100)),
            "x": value(json!(0)), "nm": "Mask 1",
        }]);
    })
}

/// A diamond as the matte of the square, of kind `tt`: 1 alpha, 2 inverted
/// alpha, 3 luma, 4 inverted luma.
fn matte(kind: u32) -> Value {
    let mut source = shape_layer(
        1,
        vec![group(vec![json!({ "ty": "sh", "ks": value(diamond()) }), fill([1.0, 1.0, 1.0])])],
    );
    source["td"] = json!(1);
    let mut target = square_layer(2);
    target["tt"] = json!(kind);
    document(vec![source, target])
}

/// A 2x2 PNG, red and blue, as a data URL.
fn png_data_url() -> String {
    let pixels: [u8; 16] =
        [230, 60, 60, 255, 60, 140, 230, 255, 60, 140, 230, 255, 230, 60, 60, 255];
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, 2, 2);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("writing to memory");
        writer.write_image_data(&pixels).expect("writing to memory");
    }
    format!("data:image/png;base64,{}", base64(&png))
}

fn base64(bytes: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, &b)| n | u32::from(b) << (16 - 8 * i));
        for i in 0..4 {
            out.push(if i <= chunk.len() {
                CHARS[(n >> (18 - 6 * i) & 63) as usize] as char
            } else {
                '='
            });
        }
    }
    out
}

/// A precomp of the square moving right, for time remapping.
fn moving_precomp(time_remap: Option<Value>) -> Value {
    let mut inner = square_layer(1);
    inner["ks"]["p"] = keys(&[(0, json!([156, 256, 0])), (30, json!([356, 256, 0]))]);
    let mut layer = json!({
        "ddd": 0, "ind": 1, "ty": 0, "nm": "precomp", "refId": "moving", "sr": 1,
        "ks": layer_transform(), "ao": 0, "w": 512, "h": 512, "ip": 0, "op": 60, "st": 0, "bm": 0,
    });
    layer["ks"]["a"] = value(json!([256, 256, 0]));
    if let Some(tm) = time_remap {
        layer["tm"] = tm;
    }
    let mut lottie = document(vec![layer]);
    lottie["assets"] = json!([{ "id": "moving", "layers": [inner] }]);
    lottie
}

/// Position keyframes along a curve, with spatial tangents.
fn curved_motion() -> Value {
    let ease = json!({ "x": [0.5], "y": [0.5] });
    json!({ "a": 1, "k": [
        { "t": 0, "s": [156, 356, 0], "to": [60, -120, 0], "ti": [-60, 0, 0], "i": ease, "o": ease },
        { "t": 30, "s": [356, 156, 0], "to": [60, 0, 0], "ti": [60, -120, 0], "i": ease, "o": ease },
        { "t": 60, "s": [156, 356, 0] },
    ] })
}

fn features() -> Vec<(&'static str, &'static str, Value)> {
    let red = || fill(RED);
    vec![
        ("plain", "the plain square every other probe changes", changed_layer(|_| {})),
        // forbidden by the rules
        (
            "expression",
            "position from an expression, wiggle(4, 40)",
            changed_layer(|layer| {
                layer["ks"]["p"]["x"] = json!("var $bm_rt;\n$bm_rt = wiggle(4, 40);");
            }),
        ),
        ("mask-add", "a diamond mask, add", mask("a", false)),
        ("mask-subtract", "a diamond mask, subtract", mask("s", false)),
        ("mask-intersect", "a diamond mask, intersect", mask("i", false)),
        ("mask-inverted", "a diamond mask, add, inverted", mask("a", true)),
        ("matte-alpha", "a diamond alpha matte", matte(1)),
        ("matte-alpha-inverted", "a diamond inverted alpha matte", matte(2)),
        ("matte-luma", "a white diamond luma matte", matte(3)),
        (
            "effect-slider",
            "a slider control effect, as expressions use",
            changed_layer(|layer| {
                layer["ef"] = json!([{
                    "ty": 5, "nm": "Slider Control", "np": 3, "mn": "ADBE Slider Control", "ix": 1, "en": 1,
                    "ef": [{ "ty": 0, "nm": "Slider", "mn": "ADBE Slider Control-0001", "ix": 1, "v": value(json!(50)) }],
                }]);
            }),
        ),
        (
            "effect-fill",
            "a fill effect turning the square red",
            changed_layer(|layer| {
                let effect = |ty: u32, nm: &str, ix: u32, v: Value| json!({ "ty": ty, "nm": nm, "mn": format!("ADBE Fill-000{ix}"), "ix": ix, "v": value(v) });
                layer["ef"] = json!([{
                    "ty": 21, "nm": "Fill", "np": 9, "mn": "ADBE Fill", "ix": 1, "en": 1,
                    "ef": [
                        effect(10, "Fill Mask", 1, json!(0)), effect(7, "All Masks", 2, json!(0)),
                        effect(2, "Color", 3, json!([RED[0], RED[1], RED[2], 1])), effect(7, "Invert", 4, json!(0)),
                        effect(0, "Horizontal Feather", 5, json!(0)), effect(0, "Vertical Feather", 6, json!(0)),
                        effect(0, "Opacity", 7, json!(1)),
                    ],
                }]);
            }),
        ),
        ("image", "an image layer: a 2x2 PNG, scaled up", {
            let mut layer = json!({
                "ddd": 0, "ind": 1, "ty": 2, "nm": "image", "refId": "image", "sr": 1,
                "ks": layer_transform(), "ao": 0, "ip": 0, "op": 60, "st": 0, "bm": 0,
            });
            layer["ks"]["a"] = value(json!([1, 1, 0]));
            layer["ks"]["s"] = value(json!([11000, 11000, 100]));
            let mut lottie = document(vec![layer]);
            lottie["assets"] =
                json!([{ "id": "image", "w": 2, "h": 2, "u": "", "p": png_data_url(), "e": 1 }]);
            lottie
        }),
        ("solid", "a solid layer instead of the square", {
            let mut layer = json!({
                "ddd": 0, "ind": 1, "ty": 1, "nm": "solid", "sr": 1, "ks": layer_transform(), "ao": 0,
                "sw": 220, "sh": 220, "sc": "#3c8ce6", "ip": 0, "op": 60, "st": 0, "bm": 0,
            });
            layer["ks"]["a"] = value(json!([110, 110, 0]));
            document(vec![layer])
        }),
        ("text", "a text layer, \"Hi\" in Arial, without glyphs", {
            let layer = json!({
                "ddd": 0, "ind": 1, "ty": 5, "nm": "text", "sr": 1, "ks": layer_transform(), "ao": 0,
                "t": {
                    "d": { "k": [{ "s": { "s": 120, "f": "Arial", "t": "Hi", "j": 2, "tr": 0, "lh": 144, "ls": 0, "fc": BLUE }, "t": 0 }] },
                    "p": {}, "m": { "g": 1, "a": value(json!([0, 0])) }, "a": [],
                },
                "ip": 0, "op": 60, "st": 0, "bm": 0,
            });
            let mut lottie = document(vec![layer]);
            lottie["fonts"] = json!({ "list": [{ "fName": "Arial", "fFamily": "Arial", "fStyle": "Regular", "ascent": 72 }] });
            lottie
        }),
        (
            "3d-layer",
            "the square's layer marked 3D, turned 40° about y",
            changed_layer(|layer| {
                layer["ddd"] = json!(1);
                layer["ks"]["ry"] = value(json!(40));
                layer["ks"]["rx"] = value(json!(0));
                layer["ks"]["rz"] = value(json!(0));
                layer["ks"]["or"] = value(json!([0, 0, 0]));
            }),
        ),
        (
            "merge-paths",
            "a square minus a circle by merge paths (subtract)",
            shapes(vec![rect(220.0), ellipse(140.0), json!({ "ty": "mm", "mm": 3 }), fill(BLUE)]),
        ),
        (
            "star",
            "a five-pointed star",
            shapes(vec![
                json!({
                    "ty": "sr", "sy": 1, "d": 1, "pt": value(json!(5)), "p": value(json!([0, 0])), "r": value(json!(0)),
                    "ir": value(json!(50)), "is": value(json!(0)), "or": value(json!(120)), "os": value(json!(0)),
                }),
                fill(BLUE),
            ]),
        ),
        (
            "polygon",
            "a hexagon (a star shape of the polygon kind)",
            shapes(vec![
                json!({
                    "ty": "sr", "sy": 2, "d": 1, "pt": value(json!(6)), "p": value(json!([0, 0])), "r": value(json!(0)),
                    "or": value(json!(120)), "os": value(json!(0)),
                }),
                fill(BLUE),
            ]),
        ),
        (
            "gradient-stroke",
            "the square outlined by a red to blue gradient stroke",
            shapes(vec![
                rect(220.0),
                json!({
                    "ty": "gs", "o": value(json!(100)), "w": value(json!(24)), "lc": 2, "lj": 2, "ml": 4, "t": 1,
                    "g": { "p": 2, "k": value(json!([0, RED[0], RED[1], RED[2], 1, BLUE[0], BLUE[1], BLUE[2]])) },
                    "s": value(json!([-110, 0])), "e": value(json!([110, 0])),
                }),
            ]),
        ),
        (
            "repeater",
            "a small square repeated 4 times by a repeater",
            shapes(vec![
                json!({ "ty": "rc", "d": 1, "p": value(json!([-150, 0])), "s": value(json!([60, 60])), "r": value(json!(0)) }),
                fill(BLUE),
                json!({
                    "ty": "rp", "c": value(json!(4)), "o": value(json!(0)), "m": 1,
                    "tr": {
                        "ty": "tr", "p": value(json!([100, 0])), "a": value(json!([0, 0])), "s": value(json!([100, 100])),
                        "r": value(json!(0)), "so": value(json!(100)), "eo": value(json!(100)),
                    },
                }),
            ]),
        ),
        (
            "time-stretch",
            "the square stepping right, its layer stretched to half speed",
            changed_layer(|layer| {
                layer["ks"]["p"] = keys(&[(0, json!([156, 256, 0])), (15, json!([356, 256, 0]))]);
                layer["sr"] = json!(2);
            }),
        ),
        (
            "time-remap",
            "a precomp of the square stepping right, time remapped to play backwards",
            moving_precomp(Some(json!({
                "a": 1, "k": [
                    { "t": 0, "s": [0.99], "i": { "x": [1], "y": [1] }, "o": { "x": [0], "y": [0] } },
                    { "t": 59, "s": [0] },
                ],
            }))),
        ),
        (
            "auto-orient",
            "the square moving along a curve, auto-oriented",
            changed_layer(|layer| {
                layer["ks"]["p"] = curved_motion();
                layer["ao"] = json!(1);
            }),
        ),
        (
            "spatial-bezier",
            "the square moving along a curve: spatial tangents, as auto-bezier keys export",
            changed_layer(|layer| {
                layer["ks"]["p"] = curved_motion();
            }),
        ),
        // on no list, and some could help the encoder
        ("ellipse", "a circle", shapes(vec![ellipse(220.0), fill(BLUE)])),
        (
            "rounded-corners",
            "the square with rounded corners",
            shapes(vec![rect(220.0), json!({ "ty": "rd", "r": value(json!(40)) }), fill(BLUE)]),
        ),
        (
            "trim-paths",
            "a circle's outline drawn in by trim paths",
            shapes(vec![
                ellipse(200.0),
                stroke(BLUE, 24.0),
                json!({ "ty": "tm", "s": value(json!(0)), "e": { "a": 1, "k": [
                { "t": 0, "s": [0], "i": { "x": [1], "y": [1] }, "o": { "x": [0], "y": [0] } },
                { "t": 59, "s": [100] },
            ] }, "o": value(json!(0)), "m": 1 }),
            ]),
        ),
        (
            "dashed-stroke",
            "the square's outline, dashed",
            shapes(vec![rect(220.0), {
                let mut dashed = stroke(BLUE, 16.0);
                dashed["d"] = json!([
                    { "n": "d", "nm": "dash", "v": value(json!(30)) },
                    { "n": "g", "nm": "gap", "v": value(json!(15)) },
                    { "n": "o", "nm": "offset", "v": value(json!(0)) },
                ]);
                dashed
            }]),
        ),
        (
            "even-odd",
            "a square and a smaller one in it under an even-odd fill: a frame",
            shapes(vec![
                rect(220.0),
                rect(120.0),
                json!({ "ty": "fl", "c": value(json!([BLUE[0], BLUE[1], BLUE[2], 1])), "o": value(json!(100)), "r": 2 }),
            ]),
        ),
        (
            "gradient-fill",
            "the square with a red to blue gradient fill",
            shapes(vec![
                rect(220.0),
                json!({
                    "ty": "gf", "o": value(json!(100)), "r": 1, "t": 1,
                    "g": { "p": 2, "k": value(json!([0, RED[0], RED[1], RED[2], 1, BLUE[0], BLUE[1], BLUE[2]])) },
                    "s": value(json!([-110, 0])), "e": value(json!([110, 0])),
                }),
            ]),
        ),
        (
            "fill-colour-keys",
            "the square's colour switching blue and red: hold keyframes on the fill",
            shapes(vec![
                rect(220.0),
                json!({ "ty": "fl", "o": value(json!(100)), "r": 1, "c": keys(&[
                (0, json!([BLUE[0], BLUE[1], BLUE[2], 1])), (30, json!([RED[0], RED[1], RED[2], 1])),
            ]) }),
            ]),
        ),
        (
            "opacity-keys",
            "the square blinking: hold keyframes on the layer's opacity",
            changed_layer(|layer| {
                layer["ks"]["o"] = keys(&[(0, json!([100])), (30, json!([0]))]);
            }),
        ),
        ("null-parent", "the square parented to a null layer that steps right", {
            let null = json!({
                "ddd": 0, "ind": 1, "ty": 3, "nm": "null", "sr": 1, "ao": 0, "ip": 0, "op": 60, "st": 0, "bm": 0,
                "ks": {
                    "o": value(json!(0)), "r": value(json!(0)), "a": value(json!([0, 0, 0])), "s": value(json!([100, 100, 100])),
                    "p": keys(&[(0, json!([-80, 0, 0])), (30, json!([80, 0, 0]))]),
                },
            });
            let mut child = square_layer(2);
            child["parent"] = json!(1);
            document(vec![null, child])
        }),
        ("blend-multiply", "a red square multiplied over the blue one", {
            let mut top = shape_layer(
                1,
                vec![group(vec![
                    json!({ "ty": "rc", "d": 1, "p": value(json!([60, 60])), "s": value(json!([220, 220])), "r": value(json!(0)) }),
                    red(),
                ])],
            );
            top["bm"] = json!(1);
            document(vec![top, square_layer(2)])
        }),
        ("skew-rotation", "the square rotated 20° and skewed 20° by its group transform", {
            let mut item = group(vec![rect(220.0), fill(BLUE)]);
            let transform = item["it"].as_array_mut().unwrap().last_mut().unwrap();
            transform["r"] = value(json!(20));
            transform["sk"] = value(json!(20));
            transform["sa"] = value(json!(0));
            document(vec![shape_layer(1, vec![item])])
        }),
        (
            "offset-path",
            "the square grown 30 by an offset path",
            shapes(vec![
                rect(160.0),
                json!({ "ty": "op", "a": value(json!(30)), "lj": 1, "ml": value(json!(4)) }),
                fill(BLUE),
            ]),
        ),
        (
            "zig-zag",
            "the circle with a zig-zag edge",
            shapes(vec![
                ellipse(200.0),
                json!({ "ty": "zz", "r": value(json!(12)), "s": value(json!(16)), "pt": value(json!(1)) }),
                fill(BLUE),
            ]),
        ),
        (
            "pucker-bloat",
            "the square bloated",
            shapes(vec![rect(200.0), json!({ "ty": "pb", "a": value(json!(40)) }), fill(BLUE)]),
        ),
        (
            "twist",
            "the square twisted",
            shapes(vec![
                rect(200.0),
                json!({ "ty": "tw", "a": value(json!(90)), "c": value(json!([0, 0])) }),
                fill(BLUE),
            ]),
        ),
        ("hidden-layer", "a red square in a hidden layer over the blue one", {
            let mut hidden = shape_layer(1, vec![group(vec![rect(120.0), red()])]);
            hidden["hd"] = json!(true);
            document(vec![hidden, square_layer(2)])
        }),
        ("markers", "the square, with a marker", {
            let mut lottie = changed_layer(|_| {});
            lottie["markers"] = json!([{ "tm": 0, "cm": "loop", "dr": 60 }]);
            lottie
        }),
    ]
}

const FEATURES: &str = "# Telegram probes: Lottie features\n\n\
    Made by `tgs-lab features`; `docs/probes.md` says how to use them. Upload them \
    one at a time through @Stickers after `/newanimated`, write down which are \
    accepted, and how the accepted ones look in each app (each should look like \
    what \"Tests\" says).\n\n\
    | File | Tests | JSON | .tgs | Accepted | Looks right |\n\
    | --- | --- | ---: | ---: | --- | --- |\n";

/// Writes the feature probes into `dir`.
pub fn write(dir: &Path) -> Result<()> {
    let probes: Vec<Probe> = features()
        .into_iter()
        .enumerate()
        .map(|(index, (name, tests, json))| Probe {
            file: format!("{:02}-{name}.tgs", index + 1),
            pack: "sticker",
            tests: tests.into(),
            expect: "",
            json: json.to_string(),
        })
        .collect();
    save(dir, &probes, FEATURES, |probe, sizes| {
        format!("| {} | {} | {sizes} |  |  |", probe.file, probe.tests)
    })
}

#[cfg(test)]
mod tests {
    use tgradish_tgs::check::{Severity, check};

    use super::*;
    use crate::verify::Renderer;

    #[test]
    fn encodes_base64() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
    }

    #[test]
    fn features_are_unique_and_parse() {
        let features = features();
        let mut names: Vec<&str> = features.iter().map(|(name, ..)| *name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), features.len());
        for (name, _, json) in &features {
            // Telegram accepted every one, so check finds no error in them
            let (_, issues) = check(json.to_string().as_bytes(), None).unwrap();
            assert!(
                issues.iter().all(|issue| issue.severity == Severity::Warning),
                "{name}: {issues:?}"
            );
            // and tlottie reads them all
            Renderer::new(json.to_string().as_bytes()).unwrap().render(0, 64).unwrap();
        }
    }
}
