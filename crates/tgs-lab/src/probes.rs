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

/// `count` paths in one layer, each going round its own square 1000
/// times: 4000 points, much JSON from few shapes, and it packs small.
fn long_paths(count: u32) -> String {
    let columns = f64::from(count).sqrt().ceil() as u32;
    let units = f64::from(columns * 2);
    let mut items: Vec<Item> = (0..count)
        .map(|path| {
            let (x, y) = (f64::from(path % columns * 2), f64::from(path / columns * 2));
            let square = [[x, y], [x + 1.0, y], [x + 1.0, y + 1.0], [x, y + 1.0]];
            Item::Path(square.iter().copied().cycle().take(4000).collect())
        })
        .collect();
    items.push(Item::Fill { colour: [60, 140, 230, 255], rule: FillRule::NonZero });
    items.push(Item::GroupTransform);
    let transform = grid_transform(units, units);
    let layer = Layer { from: 0, to: 180, transform, items: vec![Item::Group(items)] };
    let blink =
        Layer { from: 0, to: 90, transform, items: vec![group([(1.5, 1.5)], [230, 90, 60, 255])] };
    animation(vec![layer, blink])
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
            long_paths(count),
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

/// Packs the probes into `dir`, with `CHECKLIST.md`: `intro` and a `row`
/// for each, given its sizes as table cells.
fn save(
    dir: &Path,
    probes: &[Probe],
    intro: &str,
    row: impl Fn(&Probe, &str) -> String,
) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let mut checklist = String::from(intro);
    for probe in probes {
        let tgs = file::pack(probe.json.as_bytes(), 15);
        let (stats, issues) = check(probe.json.as_bytes(), Some(tgs.len()))
            .map_err(anyhow::Error::msg)
            .with_context(|| probe.file.clone())?;
        ensure!(tgs.len() <= MAX_TGS, "{} is {} bytes, over 64 KiB", probe.file, tgs.len());
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
}
