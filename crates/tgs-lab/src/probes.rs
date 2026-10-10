//! Stickers that find out what Telegram accepts and how its apps draw it
//! (T9 in `docs/tgs.md`), and a checklist for the results. Each probe
//! changes one thing about what the encoder writes, so a rejection says
//! which.

use std::fmt::Write as _;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use tgradish_tgs::check::check;
use tgradish_tgs::frames::{Animation as Frames, Frame};
use tgradish_tgs::layout::lay_out;
use tgradish_tgs::lottie::{Animation, Item, Layer, Style, Transform};
use tgradish_tgs::normalise::{Options, normalise};
use tgradish_tgs::scene::FillRule;
use tgradish_tgs::{encode, file};

const CANVAS: f64 = 512.0;
/// Raw JSON of the large probe, just under Telegram Desktop's 2 MiB.
const NEAR_2_MIB: usize = 2_000_000;
const MAX_TGS: usize = 64 * 1024;

struct Probe {
    file: String,
    /// Where to upload it: a sticker pack or a custom emoji pack.
    pack: &'static str,
    tests: String,
    expect: &'static str,
    json: String,
}

/// The test sprite: 32x32 art pixels, 10 frames of 18 ticks. A bordered
/// square of diagonal stripes (seams everywhere) on the left of a
/// transparent canvas, with a square moving through it, a translucent bar
/// and a blinking pixel.
fn sprite() -> Result<Frames> {
    const SIZE: u32 = 32;
    let border = [40, 40, 60, 255];
    let stripes = [[230, 90, 60, 255], [250, 210, 80, 255]];
    let square = [60, 140, 230, 255];
    let translucent = [120, 220, 120, 128];
    let blink = [255, 255, 255, 255];
    let frames = (0..10)
        .map(|frame| {
            let rgba = (0..SIZE * SIZE)
                .flat_map(|index| {
                    let (x, y) = (index % SIZE, index / SIZE);
                    let moving = 2 + (frame % 6) * 3;
                    match (x, y) {
                        (24.., _) | (_, ..4) | (_, 28..) => [0; 4],
                        (0 | 23, _) | (_, 4 | 27) => border,
                        _ if (moving..moving + 4).contains(&x) && (8..12).contains(&y) => square,
                        (4..10, 20..23) => translucent,
                        (18, 22) if frame % 2 == 0 => blink,
                        _ => stripes[((x + y) / 3 % 2) as usize],
                    }
                })
                .collect();
            Frame { rgba, duration: Duration::from_millis(300) }
        })
        .collect();
    Ok(Frames::new(SIZE, SIZE, frames)?)
}

/// The sprite as the encoder writes it, on a canvas with room to move.
fn sprite_json(style: Style) -> Result<String> {
    let options = Options { keep_canvas: true, ..Options::default() };
    let (anim, _) = normalise(&sprite()?, &options)?;
    let scene = encode::painter(&anim, &Default::default(), None)?;
    Ok(lay_out(&scene, &anim, Some("tgradish probe".into())).to_json(style))
}

fn group(rects: impl IntoIterator<Item = (f64, f64)>, colour: [u8; 4]) -> Item {
    let mut items: Vec<Item> =
        rects.into_iter().map(|(x, y)| Item::Rect { centre: [x, y], size: [1.0, 1.0] }).collect();
    items.push(Item::Fill { colour, rule: FillRule::NonZero });
    items.push(Item::GroupTransform);
    Item::Group(items)
}

/// Layers of a `units`-wide grid filling the canvas's width, centred.
fn grid_transform(units: f64, rows: f64) -> Transform {
    let scale = CANVAS / units;
    Transform { position: [0.0, (CANVAS - rows * scale) / 2.0], scale }
}

/// A checkerboard drawn by many layers in turn, each moved a little,
/// until the JSON is just under 2 MiB. The layers differ only in position
/// and timing, so it packs small.
fn near_2_mib() -> String {
    let checker = || {
        group(
            (0..24u32)
                .flat_map(|y| (0..24u32).map(move |x| (x, y)))
                .filter(|(x, y)| (x + y) % 2 == 0)
                .map(|(x, y)| (f64::from(x) + 0.5, f64::from(y) + 0.5)),
            [60, 140, 230, 255],
        )
    };
    let layers_json = |count: u32| {
        let layers = (0..count)
            .map(|index| Layer {
                from: index * 180 / count,
                to: (index + 1) * 180 / count,
                transform: Transform { position: [f64::from(index % 16) * 8.0, 64.0], scale: 16.0 },
                items: vec![checker()],
            })
            .collect();
        Animation { name: Some("tgradish probe".into()), ticks: 180, layers }
            .to_json(Style::default())
    };
    let one = layers_json(1).len();
    let two = layers_json(2).len();
    let count = ((NEAR_2_MIB - one) / (two - one)) as u32;
    layers_json(count)
}

/// 2700 layers of one art pixel each, appearing 15 per frame until all of
/// them show; tlottie allows 2715 painted layers.
fn many_layers() -> String {
    let (columns, rows) = (60u32, 45u32);
    let layers = (0..columns * rows)
        .map(|index| {
            let (x, y) = (index % columns, index / columns);
            let colour = [(x * 4) as u8, (y * 5) as u8, 255 - (x * 4) as u8, 255];
            Layer {
                from: index / 15,
                to: 180,
                transform: grid_transform(f64::from(columns), f64::from(rows)),
                items: vec![group([(f64::from(x) + 0.5, f64::from(y) + 0.5)], colour)],
            }
        })
        .collect();
    Animation { name: Some("tgradish probe".into()), ticks: 180, layers }.to_json(Style::default())
}

/// One layer with a 5100-rectangle checkerboard under one fill; tlottie
/// allows 5120 rectangles per layer.
fn many_rects() -> String {
    let (columns, rows) = (102u32, 100u32);
    let cells = (0..rows)
        .flat_map(|y| (0..columns).map(move |x| (x, y)))
        .filter(|(x, y)| (x + y) % 2 == 0)
        .map(|(x, y)| (f64::from(x) + 0.5, f64::from(y) + 0.5));
    let layer = Layer {
        from: 0,
        to: 180,
        transform: grid_transform(f64::from(columns), f64::from(rows)),
        items: vec![group(cells, [40, 40, 60, 255])],
    };
    Animation { name: Some("tgradish probe".into()), ticks: 180, layers: vec![layer] }
        .to_json(Style::default())
}

fn parse(json: &str) -> Value {
    serde_json::from_str(json).expect("the writer writes JSON")
}

fn layers(lottie: &mut Value) -> &mut Vec<Value> {
    lottie["layers"].as_array_mut().expect("Lottie has layers")
}

/// The sprite drawn twice at half size, from one precomp asset.
fn precomp(sprite: &str) -> String {
    let mut lottie = parse(sprite);
    let inner = std::mem::take(layers(&mut lottie));
    let placed = |x: f64| {
        json!({
            "ty": 0, "refId": "sprite", "w": 512, "h": 512,
            "ks": { "p": { "k": [x, 128] }, "s": { "k": [50, 50] } },
            "ip": 0, "op": 180,
        })
    };
    lottie["assets"] = json!([{ "id": "sprite", "layers": inner }]);
    lottie["layers"] = json!([placed(0.0), placed(256.0)]);
    lottie.to_string()
}

/// The sprite stepping right and back by whole art pixels, with hold
/// position keyframes on every layer.
fn keyframes(sprite: &str) -> String {
    // the canvas is 32 art pixels wide
    let step = CANVAS / 32.0;
    let mut lottie = parse(sprite);
    for layer in layers(&mut lottie) {
        let start = layer["ks"]["p"]["k"].clone();
        let (x, y) = (start[0].as_f64().unwrap(), start[1].as_f64().unwrap());
        let keys: Vec<Value> = [0.0, 1.0, 2.0, 1.0]
            .iter()
            .enumerate()
            .map(|(index, steps)| json!({ "t": index * 45, "s": [x + steps * step, y], "h": 1 }))
            .collect();
        layer["ks"]["p"] = json!({ "a": 1, "k": keys });
    }
    lottie.to_string()
}

/// The sprite at 30 fps. With `same_frames`, the same 180 frames last 6
/// seconds; otherwise every time is halved, keeping 3 seconds.
fn half_rate(sprite: &str, same_frames: bool) -> String {
    let mut lottie = parse(sprite);
    lottie["fr"] = json!(30);
    if !same_frames {
        lottie["op"] = json!(90);
        for layer in layers(&mut lottie) {
            for key in ["ip", "op"] {
                // frames are 18 ticks, so halves are whole
                layer[key] = json!(layer[key].as_u64().unwrap() / 2);
            }
        }
    }
    lottie.to_string()
}

/// The sprite twice in a row at 60 fps: 360 frames, 6 seconds.
fn twice(sprite: &str) -> String {
    let mut lottie = parse(sprite);
    let first = layers(&mut lottie).clone();
    for mut layer in first {
        for key in ["ip", "op"] {
            layer[key] = json!(layer[key].as_u64().unwrap() + 180);
        }
        layers(&mut lottie).push(layer);
    }
    lottie["op"] = json!(360);
    lottie.to_string()
}

/// The sprite on a 100x100 canvas, the size of video emoji.
fn small_canvas(sprite: &str) -> String {
    let factor = 100.0 / CANVAS;
    let mut lottie = parse(sprite);
    lottie["w"] = json!(100);
    lottie["h"] = json!(100);
    for layer in layers(&mut lottie) {
        for key in ["p", "s"] {
            let value = &mut layer["ks"][key]["k"];
            for number in value.as_array_mut().unwrap() {
                *number = json!(number.as_f64().unwrap() * factor);
            }
        }
    }
    lottie.to_string()
}

/// The first `count` dark squares of a checkerboard `columns` wide.
fn checker_cells(count: u32, columns: u32) -> impl Iterator<Item = (f64, f64)> {
    (0u32..)
        .map(move |index| (index % columns, index / columns))
        .filter(|(x, y)| (x + y) % 2 == 0)
        .take(count as usize)
        .map(|(x, y)| (f64::from(x) + 0.5, f64::from(y) + 0.5))
}

fn animation(layers: Vec<Layer>) -> String {
    Animation { name: Some("tgradish probe".into()), ticks: 180, layers }.to_json(Style::default())
}

/// `count` layers of one square each, in a grid, appearing in the first
/// half.
fn one_square_layers(count: u32) -> String {
    let columns = f64::from(count).sqrt().ceil() as u32;
    let rows = count.div_ceil(columns);
    let layers = (0..count)
        .map(|index| {
            let (x, y) = (index % columns, index / columns);
            let colour = [(x * 255 / columns) as u8, (y * 255 / rows) as u8, 160, 255];
            Layer {
                from: index * 90 / count,
                to: 180,
                transform: grid_transform(f64::from(columns), f64::from(rows)),
                items: vec![group([(f64::from(x) + 0.5, f64::from(y) + 0.5)], colour)],
            }
        })
        .collect();
    animation(layers)
}

/// One layer with a checkerboard of `count` squares, shown throughout,
/// and a blinking square in a layer of its own.
fn squares_in_one_layer(count: u32) -> String {
    let columns = f64::from(2 * count).sqrt().ceil() as u32;
    let rows = (2 * count).div_ceil(columns);
    let transform = grid_transform(f64::from(columns), f64::from(rows));
    let board = Layer {
        from: 0,
        to: 180,
        transform,
        items: vec![group(checker_cells(count, columns), [40, 40, 60, 255])],
    };
    let blink =
        Layer { from: 0, to: 90, transform, items: vec![group([(1.5, 0.5)], [230, 90, 60, 255])] };
    animation(vec![board, blink])
}

/// `count` layers of a 200-square checkerboard each, tiled. With
/// `in_turn`, each is shown for its share of the 3 seconds; otherwise
/// they appear in the first half and stay.
fn checkerboard_layers(count: u32, in_turn: bool) -> String {
    const SIDE: u32 = 20;
    let tiles = f64::from(count).sqrt().ceil() as u32;
    let tile_rows = count.div_ceil(tiles);
    let units = f64::from(tiles * SIDE);
    let scale = CANVAS / units;
    let layers = (0..count)
        .map(|index| {
            let (x, y) = (index % tiles, index / tiles);
            let (from, to) = if in_turn {
                (index * 180 / count, (index + 1) * 180 / count)
            } else {
                (index * 90 / count, 180)
            };
            let colour = [(x * 255 / tiles) as u8, (y * 255 / tile_rows) as u8, 160, 255];
            Layer {
                from,
                to,
                transform: Transform {
                    position: [
                        f64::from(x * SIDE) * scale,
                        (CANVAS - f64::from(tile_rows * SIDE) * scale) / 2.0
                            + f64::from(y * SIDE) * scale,
                    ],
                    scale,
                },
                items: vec![group(checker_cells(SIDE * SIDE / 2, SIDE), colour)],
            }
        })
        .collect();
    animation(layers)
}

/// `count` paths in one layer, each going round its own square until it
/// has `points` points; with 4000, much JSON from few shapes, and it packs
/// small. Their tangents are empty, as pixelart2tgs 1.x wrote them and as
/// rounds 2 and 3 uploaded them; Telegram refuses that (see
/// [`path_form`]).
fn long_paths(count: u32, points: usize) -> String {
    let columns = f64::from(count).sqrt().ceil() as u32;
    let units = f64::from(columns * 2);
    let mut items: Vec<Item> = (0..count)
        .map(|path| {
            let (x, y) = (f64::from(path % columns * 2), f64::from(path / columns * 2));
            let square = [[x, y], [x + 1.0, y], [x + 1.0, y + 1.0], [x, y + 1.0]];
            Item::Path(square.iter().copied().cycle().take(points).collect())
        })
        .collect();
    items.push(Item::Fill { colour: [60, 140, 230, 255], rule: FillRule::NonZero });
    items.push(Item::GroupTransform);
    let transform = grid_transform(units, units);
    let layer = Layer { from: 0, to: 180, transform, items: vec![Item::Group(items)] };
    let blink =
        Layer { from: 0, to: 90, transform, items: vec![group([(1.5, 1.5)], [230, 90, 60, 255])] };
    // the writer repeats the first point
    let list = |tangent| vec![tangent; points + 1].join(",");
    let mut json = animation(vec![layer, blink]);
    for key in ["i", "o"] {
        let written = format!("\"{key}\":[{}]", list("[0,0]"));
        json = json.replace(&written, &format!("\"{key}\":[{}]", list("[]")));
    }
    json
}

/// The sprite with its name padded until the JSON is `bytes` long: size
/// alone, nothing more to draw.
fn padded(sprite: &str, bytes: usize) -> String {
    let mut lottie = parse(sprite);
    let unpadded = lottie.to_string().len();
    let name = lottie["nm"].as_str().unwrap_or_default().to_owned();
    lottie["nm"] = json!(format!("{name} {}", ".".repeat(bytes.saturating_sub(unpadded + 1))));
    lottie.to_string()
}

/// `json` with its paths rewritten: `"c":true` or no `c`, zero or empty
/// tangents, the first point repeated at the end or not. The writer
/// leaves out `c`, writes empty tangents and repeats the first point.
fn path_form(json: &str, closed: bool, zero_tangents: bool, repeat: bool) -> String {
    fn visit(value: &mut Value, form: &dyn Fn(&mut Value)) {
        match value {
            Value::Object(map) if map.get("ty") == Some(&json!("sh")) => form(&mut map["ks"]["k"]),
            Value::Object(map) => map.values_mut().for_each(|value| visit(value, form)),
            Value::Array(items) => items.iter_mut().for_each(|value| visit(value, form)),
            _ => {}
        }
    }
    let mut lottie = parse(json);
    visit(&mut lottie, &|path| {
        let mut points = path["v"].as_array().unwrap().clone();
        if !repeat {
            points.pop();
        }
        let tangent = if zero_tangents { json!([0, 0]) } else { json!([]) };
        let tangents = json!(vec![tangent; points.len()]);
        *path = if closed {
            json!({ "c": true, "i": tangents, "o": tangents, "v": points })
        } else {
            json!({ "i": tangents, "o": tangents, "v": points })
        };
    });
    lottie.to_string()
}

/// The layers of `top` drawn over those of `bottom`.
fn merged(top: &str, bottom: &str) -> String {
    let mut lottie = parse(top);
    let under = parse(bottom)["layers"].as_array().unwrap().clone();
    layers(&mut lottie).extend(under);
    lottie.to_string()
}

/// `count` squares of a checkerboard, each in a group of its own, 1000
/// groups to a layer: four shapes for each square instead of about one.
fn single_groups(count: u32) -> String {
    let columns = f64::from(2 * count).sqrt().ceil() as u32;
    let rows = (2 * count).div_ceil(columns);
    let transform = grid_transform(f64::from(columns), f64::from(rows));
    let cells: Vec<(f64, f64)> = checker_cells(count, columns).collect();
    let layers = cells
        .chunks(1000)
        .enumerate()
        .map(|(index, chunk)| {
            let colour = [(index * 36) as u8, 140, 230, 255];
            Layer {
                from: index as u32 * 10,
                to: 180,
                transform,
                items: chunk.iter().map(|&cell| group([cell], colour)).collect(),
            }
        })
        .collect();
    animation(layers)
}

/// The sprite with a name of incompressible characters, packed by
/// `gzip -9` to exactly `bytes`.
fn packed_to(sprite: &str, bytes: usize) -> Result<Vec<u8>> {
    let mut state = 0x9e37_79b9_7f4a_7c15u64;
    let noise: String = (0..bytes * 2)
        .map(|_| {
            // xorshift: the same characters every run
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            const CHARS: &[u8] =
                b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            CHARS[(state % 64) as usize] as char
        })
        .collect();
    let mut lottie = parse(sprite);
    let mut with = |length: usize, last: char| {
        lottie["nm"] = json!(format!("tgradish probe {}{last}", &noise[..length]));
        gzip(lottie.to_string().as_bytes())
    };
    // the shortest noise reaching `bytes`, then its last character varied
    // until the size is exact
    let (mut low, mut high) = (0, noise.len() - 1);
    while low < high {
        let middle = (low + high) / 2;
        if with(middle, '.').len() < bytes { low = middle + 1 } else { high = middle }
    }
    for length in low.saturating_sub(4)..low + 4 {
        for last in ['.', '-', '_', '~', ' ', '!', '*', '\'', '(', ')'] {
            let packed = with(length, last);
            if packed.len() == bytes {
                return Ok(packed);
            }
        }
    }
    anyhow::bail!("no name packs to exactly {bytes} bytes")
}

fn gzip(json: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    encoder.write_all(json).expect("compressing into memory can't fail");
    encoder.finish().expect("compressing into memory can't fail")
}

/// `json` with every rectangle's size written with more digits: the same
/// shapes and numbers, more bytes.
fn long_sizes(json: &str) -> String {
    json.replace("\"s\":{\"k\":[1,1]}", "\"s\":{\"k\":[1.00001,1.00001]}")
}

/// `json` with zero tangents written out instead of left empty: more
/// numbers per point.
fn zero_tangents(json: &str) -> String {
    json.replace("[]", "[0,0]")
}

/// The third round: whether Telegram's limit on large stickers counts
/// bytes, numbers, arrays or points, and the limit on the packed size.
/// Each probe's JSON or file is made so the candidates disagree about it.
/// `one_x` adds stickers pixelart2tgs 1.x made, by name, with their
/// tangents fixed.
fn size_probes(one_x: &[(String, String)]) -> Result<Vec<(Probe, Option<Vec<u8>>)>> {
    let sprite = sprite_json(Style::default())?;
    let mut probes = Vec::new();
    let mut add = |name: &str, tests: String, json: String, packed: Option<Vec<u8>>| {
        let file = format!("{:02}-{name}.tgs", probes.len() + 1);
        probes.push((Probe { file, pack: "sticker", tests, expect: "", json }, packed));
    };
    for bytes in [65_000, 65_536, 65_537, 66_000, 80_000] {
        add(
            &format!("packed-{bytes}"),
            format!("01 packed to exactly {bytes} bytes by a long name"),
            sprite.clone(),
            Some(packed_to(&sprite, bytes)?),
        );
    }
    add(
        "boards-in-turn-101",
        "101 layers of 200 squares, one at a time: 80 800 points".into(),
        checkerboard_layers(101, true),
        None,
    );
    add(
        "boards-in-turn-100-long-sizes",
        "round 2's accepted boards-in-turn-100 with sizes written as 1.00001: 240 KB more".into(),
        long_sizes(&checkerboard_layers(100, true)),
        None,
    );
    for count in [8, 19] {
        add(
            &format!("paths-{count}"),
            format!("{count} paths of 4000 points in one layer"),
            long_paths(count, 4000),
            None,
        );
    }
    add(
        "paths-5-zero-tangents",
        "5 paths of 4000 points with tangents written as [0,0]: 6 numbers per point".into(),
        zero_tangents(&long_paths(5, 4000)),
        None,
    );
    add("layers-1750", "1750 layers of one square".into(), one_square_layers(1750), None);
    add("paths-1", "one path of 4000 points".into(), long_paths(1, 4000), None);
    for count in [170, 300] {
        add(
            &format!("short-paths-{count}"),
            format!("{count} paths of 100 points in one layer: {} points", count * 100),
            long_paths(count, 100),
            None,
        );
    }
    for points in [4, 100] {
        add(
            &format!("path-{points}"),
            format!("one path of {points} points"),
            long_paths(1, points),
            None,
        );
    }
    for (name, closed, zero, repeat) in [
        ("path-standard", true, true, false),
        ("path-empty-tangents", true, false, false),
        ("path-no-c", false, true, false),
        ("path-repeated-point", true, true, true),
    ] {
        add(
            name,
            format!(
                "one path of 4 points: \"c\" {}, tangents {}, first point {}",
                if closed { "true" } else { "left out" },
                if zero { "[0,0]" } else { "empty" },
                if repeat { "repeated at the end" } else { "not repeated" }
            ),
            path_form(&long_paths(1, 4), closed, zero, repeat),
            None,
        );
    }
    add(
        "groups-6500",
        "6500 groups of one square, 1000 to a layer: 26 000 shapes, 26 000 arrays".into(),
        single_groups(6500),
        None,
    );
    add(
        "path-4000-standard",
        "one path of 4000 points in the standard form".into(),
        path_form(&long_paths(1, 4000), true, true, false),
        None,
    );
    for count in [110, 115, 118, 117, 116] {
        add(
            &format!("boards-in-turn-{count}"),
            format!("{count} layers of 200 squares, one at a time: {} shapes", count * 203),
            checkerboard_layers(count, true),
            None,
        );
    }
    add(
        "layers-1000-and-boards-95",
        "1000 layers of one square and 95 of 200: 23 285 shapes in 1095 layers".into(),
        merged(&one_square_layers(1000), &checkerboard_layers(95, true)),
        None,
    );
    add(
        "layers-1300-and-boards-83",
        "1300 layers of one square and 83 of 200: 22 049 shapes in 1383 layers".into(),
        merged(&one_square_layers(1300), &checkerboard_layers(83, true)),
        None,
    );
    add(
        "groups-5000",
        "5000 groups of one square, 1000 to a layer".into(),
        single_groups(5000),
        None,
    );
    add(
        "layers-1400-and-boards-74",
        "1400 layers of one square and 74 of 200: 20 622 shapes in 1474 layers".into(),
        merged(&one_square_layers(1400), &checkerboard_layers(74, true)),
        None,
    );
    add(
        "layers-1000-and-boards-60",
        "1000 layers of one square and 60 of 200: 16 180 shapes in 1060 layers".into(),
        merged(&one_square_layers(1000), &checkerboard_layers(60, true)),
        None,
    );
    add(
        "layers-1000-and-boards-50",
        "1000 layers of one square and 50 of 200: 14 150 shapes in 1050 layers".into(),
        merged(&one_square_layers(1000), &checkerboard_layers(50, true)),
        None,
    );
    add(
        "groups-5800",
        "5800 groups of one square, 1000 to a layer: 23 200 shapes".into(),
        single_groups(5800),
        None,
    );
    add(
        "layers-1000-and-boards-55",
        "1000 layers of one square and 55 of 200: 15 165 shapes in 1055 layers".into(),
        merged(&one_square_layers(1000), &checkerboard_layers(55, true)),
        None,
    );
    for count in [10, 5, 3] {
        add(
            &format!("paths-standard-{count}"),
            format!("{count} paths of 4000 points in the standard form: {count}000 points"),
            path_form(&long_paths(count, 4000), true, true, false),
            None,
        );
    }
    add(
        "path-4000-and-boards-60",
        "one standard path of 4000 points over 60 layers of 200 squares, one at a time".into(),
        merged(&path_form(&long_paths(1, 4000), true, true, false), &checkerboard_layers(60, true)),
        None,
    );
    add(
        "paths-2-and-boards-40",
        "two standard paths of 4000 points over 40 layers of 200 squares, one at a time".into(),
        merged(&path_form(&long_paths(2, 4000), true, true, false), &checkerboard_layers(40, true)),
        None,
    );
    let path = || path_form(&long_paths(1, 4000), true, true, false);
    add(
        "path-4000-in-3-layers",
        "three layers of one standard path of 4000 points, shown together".into(),
        merged(&merged(&path(), &path()), &path()),
        None,
    );
    add(
        "path-4000-in-3-groups",
        "one layer of three groups of one standard path of 4000 points".into(),
        {
            let mut lottie = parse(&path());
            let shapes = &mut layers(&mut lottie)[0]["shapes"];
            let group = shapes[0].clone();
            *shapes = json!([group.clone(), group.clone(), group]);
            lottie.to_string()
        },
        None,
    );
    add(
        "path-4000-in-10-layers",
        "ten layers of one standard path of 4000 points: 40 000 points, 4000 under a fill".into(),
        (0..9).fold(path(), |json, _| merged(&json, &path())),
        None,
    );
    for (name, json) in one_x {
        let stem = name.rsplit_once('.').map_or(name.as_str(), |(stem, _)| stem);
        add(
            &format!("1x-{}-zero-tangents", stem.to_lowercase().replace('_', "-")),
            format!(
                "{name} from pixelart2tgs 1.x with zero tangents instead of empty ones; it \
                 keeps merge paths, strokes and a fractional end"
            ),
            path_form(json, false, true, true),
            None,
        );
    }
    Ok(probes)
}

/// The second round: what made Telegram refuse 03, 04 and 05. Each
/// ladder raises one thing the encoder's output can have a lot of; `art`
/// adds real stickers encoded as tgradish does.
fn limit_probes(art: Vec<(String, String)>) -> Result<Vec<Probe>> {
    let sprite = sprite_json(Style::default())?;
    let mut probes = Vec::new();
    let mut add = |name: String, tests: String, json: String| {
        let file = format!("{:02}-{name}.tgs", probes.len() + 1);
        probes.push(Probe { file, pack: "sticker", tests, expect: "", json });
    };
    for count in [60, 120, 180, 250, 500, 1000, 1500, 2000] {
        add(
            format!("layers-{count}"),
            format!("{count} layers of one square"),
            one_square_layers(count),
        );
    }
    for count in [600, 1200, 2400, 3600, 4000, 4090, 4100, 4500] {
        add(
            format!("squares-{count}"),
            format!("{count} squares in one layer, all shown"),
            squares_in_one_layer(count),
        );
    }
    for count in [12, 24] {
        add(
            format!("boards-{count}"),
            format!("{count} layers of 200 squares shown together: {} squares", count * 200),
            checkerboard_layers(count, false),
        );
    }
    for count in [48, 49, 50, 60, 80, 100, 120] {
        add(
            format!("boards-in-turn-{count}"),
            format!("{count} layers of 200 squares, one at a time: {} squares", count * 200),
            checkerboard_layers(count, true),
        );
    }
    for count in [20, 24] {
        add(
            format!("paths-{count}"),
            format!("{count} paths of 4000 points in one layer: much JSON, few shapes"),
            long_paths(count, 4000),
        );
    }
    for kb in [700, 1000, 1100, 1500, 1900] {
        add(
            format!("json-{kb}k"),
            format!("{kb} KB of JSON: 01 with its name padded"),
            padded(&sprite, kb * 1000),
        );
    }
    for (name, json) in art {
        let stem = name.rsplit_once('.').map_or(name.as_str(), |(stem, _)| stem);
        add(
            stem.to_lowercase().replace('_', "-"),
            format!("real art ({name}), encoded as tgradish does"),
            json,
        );
    }
    Ok(probes)
}

/// The probes; `art` adds a real sticker encoded as tgradish does by
/// default.
fn probes(art: Option<(String, String)>) -> Result<Vec<Probe>> {
    let sprite = sprite_json(Style::default())?;
    let full = sprite_json(Style { tgs_key: true, layer_start: true, rect_roundness: true })?;
    let mut probes = vec![
        Probe {
            file: "01-default.tgs".into(),
            pack: "sticker",
            tests: "what tgradish writes: no `tgs`, `st` or rectangle `r` fields".into(),
            expect: "accepted; sharp pixels, no seams between colours, the bar half see-through",
            json: sprite.clone(),
        },
        Probe {
            file: "02-all-fields.tgs".into(),
            pack: "sticker",
            tests: "the same with `\"tgs\":1`, `\"st\":0` and `\"r\":{\"k\":0}`".into(),
            expect: "accepted, looks like 01; if only this one is accepted, tgradish needs the fields",
            json: full,
        },
        Probe {
            file: "03-near-2mib.tgs".into(),
            pack: "sticker",
            tests: "JSON just under 2 MiB, Telegram Desktop's limit".into(),
            expect: "accepted; a moving checkerboard, shown on Desktop too",
            json: near_2_mib(),
        },
        Probe {
            file: "04-many-layers.tgs".into(),
            pack: "sticker",
            tests: "2700 layers; tlottie (Android, Desktop, web) allows 2715".into(),
            expect: "accepted; a colour grid filling in, smooth on phones",
            json: many_layers(),
        },
        Probe {
            file: "05-many-rects.tgs".into(),
            pack: "sticker",
            tests: "5100 rectangles in one layer; tlottie allows 5120".into(),
            expect: "accepted; a fine checkerboard",
            json: many_rects(),
        },
        Probe {
            file: "06-precomp.tgs".into(),
            pack: "sticker",
            tests: "a precomp: one asset drawn by two layers".into(),
            expect: "accepted; the sprite twice, side by side, half size",
            json: precomp(&sprite),
        },
        Probe {
            file: "07-keyframes.tgs".into(),
            pack: "sticker",
            tests: "position hold keyframes moving the sprite by whole pixels".into(),
            expect: "accepted; the sprite steps right and back, staying sharp",
            json: keyframes(&sprite),
        },
        Probe {
            file: "08-30fps.tgs".into(),
            pack: "sticker",
            tests: "30 fps instead of 60, still 3 s (90 frames)".into(),
            expect: "rejected (stickers must be 60 fps); if accepted, it plays like 01",
            json: half_rate(&sprite, false),
        },
        Probe {
            file: "09-30fps-6s.tgs".into(),
            pack: "sticker",
            tests: "180 frames at 30 fps: 6 s, if 08 is accepted".into(),
            expect: "rejected (at most 3 s); if accepted, note whether it plays 6 s",
            json: half_rate(&sprite, true),
        },
        Probe {
            file: "10-6s.tgs".into(),
            pack: "sticker",
            tests: "360 frames at 60 fps: 6 s".into(),
            expect: "rejected (at most 3 s); if accepted, note whether it plays 6 s",
            json: twice(&sprite),
        },
        Probe {
            file: "11-emoji.tgs".into(),
            pack: "emoji",
            tests: "01 as a custom emoji, 512x512 like stickers".into(),
            expect: "accepted; sharp at emoji size",
            json: sprite.clone(),
        },
        Probe {
            file: "12-emoji-100.tgs".into(),
            pack: "emoji",
            tests: "the same on a 100x100 canvas, the size of video emoji".into(),
            expect: "one of 11 and 12 is accepted, or both: tells which size emoji use",
            json: small_canvas(&sprite),
        },
    ];
    if let Some((name, json)) = art {
        probes.push(Probe {
            file: "13-real-art.tgs".into(),
            pack: "sticker",
            tests: format!("real art ({name}), encoded as tgradish does"),
            expect: "accepted; looks like the input, no seams or fringes along edges",
            json,
        });
    }

    Ok(probes)
}

const ROUND_ONE: &str = "# Telegram probes\n\n\
    Made by `tgs-lab probes`; `docs/probes.md` says how to use them. Upload each \
    file through @Stickers into a test pack of the kind in \"Pack\", then look at \
    it in each app. Write down for each: accepted or the bot's error, and how it \
    looks (sharp, seams, wrong colours, not shown).\n\n\
    | File | Pack | Tests | Expected | JSON | .tgs | Accepted | Android | Desktop | iOS | Web |\n\
    | --- | --- | --- | --- | ---: | ---: | --- | --- | --- | --- | --- |\n";

const ROUND_TWO: &str = "# Telegram probes, round 2\n\n\
    Made by `tgs-lab limits`; `docs/probes.md` says how to use them. Each group \
    raises one thing until Telegram refuses it. Upload all of them through @Stickers \
    into one test sticker pack, and write down which are accepted and, for the \
    others, the bot's answer word for word. How they look doesn't matter here.\n\n\
    | File | Tests | JSON | .tgs | Accepted | Bot's answer |\n\
    | --- | --- | ---: | ---: | --- | --- |\n";

/// Writes the first round of probes and `CHECKLIST.md` into `dir`.
pub fn write(dir: &Path, art: Option<(String, String)>) -> Result<()> {
    save(dir, &probes(art)?, ROUND_ONE, |probe, sizes| {
        format!(
            "| {} | {} | {} | {} | {sizes} |  |  |  |  |  |",
            probe.file, probe.pack, probe.tests, probe.expect
        )
    })
}

/// Writes the second round, finding Telegram's limits, into `dir`.
pub fn write_limits(dir: &Path, art: Vec<(String, String)>) -> Result<()> {
    save(dir, &limit_probes(art)?, ROUND_TWO, |probe, sizes| {
        format!("| {} | {} | {sizes} |  |  |", probe.file, probe.tests)
    })
}

const ROUND_THREE: &str = "# Telegram probes, round 3\n\n\
    Made by `tgs-lab sizes`; `docs/probes.md` says how to use them. Upload them one \
    at a time through @Stickers after `/newanimated`, and write down which are \
    accepted.\n\n\
    | File | Tests | JSON | .tgs | Accepted | Bot's answer |\n\
    | --- | --- | ---: | ---: | --- | --- |\n";

/// Writes the third round, finding what Telegram's limits count, into `dir`;
/// `one_x` are stickers pixelart2tgs 1.x made: file names and JSON.
pub fn write_sizes(dir: &Path, one_x: &[(String, String)]) -> Result<()> {
    let (probes, packed): (Vec<Probe>, Vec<Option<Vec<u8>>>) =
        size_probes(one_x)?.into_iter().unzip();
    save_packed(dir, &probes, &packed, ROUND_THREE, |probe, sizes| {
        format!("| {} | {} | {sizes} |  |  |", probe.file, probe.tests)
    })
}

/// Packs the probes into `dir`, with `CHECKLIST.md`: `intro` and a `row`
/// for each, given its sizes as table cells.
fn save(
    dir: &Path,
    probes: &[Probe],
    intro: &str,
    row: impl Fn(&Probe, &str) -> String,
) -> Result<()> {
    save_packed(dir, probes, &vec![None; probes.len()], intro, row)
}

/// [`save`], with some probes already packed; only those may be over 64
/// KiB.
fn save_packed(
    dir: &Path,
    probes: &[Probe],
    packed: &[Option<Vec<u8>>],
    intro: &str,
    row: impl Fn(&Probe, &str) -> String,
) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let mut checklist = String::from(intro);
    for (probe, packed) in probes.iter().zip(packed) {
        let json = match packed {
            Some(packed) => file::unpack(packed).map_err(anyhow::Error::msg)?,
            None => probe.json.clone().into_bytes(),
        };
        let tgs = packed.clone().unwrap_or_else(|| file::pack(&json, 15));
        let (stats, issues) = check(&json, Some(tgs.len()))
            .map_err(anyhow::Error::msg)
            .with_context(|| probe.file.clone())?;
        ensure!(
            packed.is_some() || tgs.len() <= MAX_TGS,
            "{} is {} bytes, over 64 KiB",
            probe.file,
            tgs.len()
        );
        std::fs::write(dir.join(&probe.file), &tgs)?;
        let sizes = format!(
            "{:.0} KiB | {:.1} KiB",
            stats.json_bytes as f64 / 1024.0,
            tgs.len() as f64 / 1024.0
        );
        writeln!(checklist, "{}", row(probe, &sizes))?;
        let notes: Vec<String> = issues.iter().map(|issue| issue.message.clone()).collect();
        println!(
            "{:<40} {:>8} bytes JSON {:>6} bytes .tgs {:>5} layers{}",
            probe.file,
            stats.json_bytes,
            tgs.len(),
            stats.layers,
            if notes.is_empty() { String::new() } else { format!("  ({})", notes.join("; ")) }
        );
    }
    std::fs::write(dir.join("CHECKLIST.md"), checklist)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verify::Renderer;

    #[test]
    fn probes_render_within_limits() {
        let probes = probes(None).unwrap();
        assert_eq!(probes.len(), 12);
        for probe in &probes {
            // gzip -9 packs worse than zopfli
            assert!(file::quick_size(probe.json.as_bytes()) <= MAX_TGS, "{}", probe.file);
            let pixels = Renderer::new(probe.json.as_bytes()).unwrap().render(0, 64).unwrap();
            assert!(pixels.iter().any(|pixel| pixel[3] > 0), "{} draws nothing", probe.file);
        }
        let near = probes.iter().find(|probe| probe.file == "03-near-2mib.tgs").unwrap();
        assert!((NEAR_2_MIB - 100_000..=NEAR_2_MIB).contains(&near.json.len()));
    }

    #[test]
    fn limit_probes_raise_one_thing_each() {
        let probes = limit_probes(Vec::new()).unwrap();
        assert_eq!(probes.len(), 32);
        for probe in &probes {
            assert!(file::quick_size(probe.json.as_bytes()) <= MAX_TGS, "{}", probe.file);
            let (stats, _) = check(probe.json.as_bytes(), None).unwrap();
            assert_eq!((stats.width, stats.height, stats.frames), (CANVAS, CANVAS, 180.0));
            let number = |prefix: &str| -> Option<usize> {
                probe.file.split_once(prefix)?.1.trim_end_matches(".tgs").parse().ok()
            };
            if let Some(count) = number("-layers-") {
                assert_eq!((stats.layers, stats.max_paint_sources_per_layer), (count, 1));
            } else if let Some(count) = number("-squares-") {
                assert_eq!((stats.layers, stats.max_paint_sources_per_layer), (2, count));
            } else if let Some(count) = number("-paths-") {
                // the writer repeats the first point at the end
                assert_eq!(
                    (stats.layers, stats.max_paint_sources_per_layer, stats.max_path_points),
                    (2, count, 4001)
                );
            } else if let Some(count) = number("-boards-").or_else(|| number("-in-turn-")) {
                assert_eq!((stats.layers, stats.max_paint_sources_per_layer), (count, 200));
            } else {
                let kb: usize = probe
                    .file
                    .split('-')
                    .nth(2)
                    .unwrap()
                    .trim_end_matches("k.tgs")
                    .parse()
                    .unwrap();
                assert_eq!(probe.json.len(), kb * 1000, "{}", probe.file);
            }
        }
        // the ladders draw something that moves
        let pixels = |probe: &Probe, frame| {
            Renderer::new(probe.json.as_bytes()).unwrap().render(frame, 64).unwrap()
        };
        for probe in &probes {
            assert_ne!(pixels(probe, 0), pixels(probe, 100), "{} doesn't move", probe.file);
        }
    }

    #[test]
    fn size_probes_hit_their_numbers() {
        let sprite = sprite_json(Style::default()).unwrap();
        assert_eq!(packed_to(&sprite, 65_536).unwrap().len(), 65_536);
        let stats = |json: &str| check(json.as_bytes(), None).unwrap().0;
        let groups = stats(&single_groups(5800));
        assert_eq!((groups.shapes, groups.layers), (23_200, 6));
        let mixed = stats(&merged(&one_square_layers(1000), &checkerboard_layers(55, true)));
        assert_eq!((mixed.shapes, mixed.layers), (15_165, 1055));
        // as uploaded: empty tangents, and the standard form
        assert_eq!(stats(&long_paths(1, 4)).empty_tangent_paths, 1);
        let standard = stats(&path_form(&long_paths(1, 4), true, true, false));
        assert_eq!((standard.empty_tangent_paths, standard.max_path_points), (0, 4));
    }
}
