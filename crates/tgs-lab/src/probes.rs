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
    file: &'static str,
    /// Where to upload it: a sticker pack or a custom emoji pack.
    pack: &'static str,
    tests: String,
    expect: &'static str,
    json: String,
}

/// The test sprite: 32x32 art pixels, 12 frames of 15 ticks. A bordered
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
    let frames = (0..12)
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
            Frame { rgba, duration: Duration::from_millis(250) }
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

/// The sprite at 30 fps: the same 180 frames last 6 seconds.
fn half_rate(sprite: &str) -> String {
    let mut lottie = parse(sprite);
    lottie["fr"] = json!(30);
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

/// The probes; `art` adds a real sticker encoded as tgradish does by
/// default.
fn probes(art: Option<(String, String)>) -> Result<Vec<Probe>> {
    let sprite = sprite_json(Style::default())?;
    let full = sprite_json(Style { tgs_key: true, layer_start: true, rect_roundness: true })?;
    let mut probes = vec![
        Probe {
            file: "01-default.tgs",
            pack: "sticker",
            tests: "what tgradish writes: no `tgs`, `st` or rectangle `r` fields".into(),
            expect: "accepted; sharp pixels, no seams between colours, the bar half see-through",
            json: sprite.clone(),
        },
        Probe {
            file: "02-all-fields.tgs",
            pack: "sticker",
            tests: "the same with `\"tgs\":1`, `\"st\":0` and `\"r\":{\"k\":0}`".into(),
            expect: "accepted, looks like 01; if only this one is accepted, tgradish needs the fields",
            json: full,
        },
        Probe {
            file: "03-near-2mib.tgs",
            pack: "sticker",
            tests: "JSON just under 2 MiB, Telegram Desktop's limit".into(),
            expect: "accepted; a moving checkerboard, shown on Desktop too",
            json: near_2_mib(),
        },
        Probe {
            file: "04-many-layers.tgs",
            pack: "sticker",
            tests: "2700 layers; tlottie (Android, Desktop, web) allows 2715".into(),
            expect: "accepted; a colour grid filling in, smooth on phones",
            json: many_layers(),
        },
        Probe {
            file: "05-many-rects.tgs",
            pack: "sticker",
            tests: "5100 rectangles in one layer; tlottie allows 5120".into(),
            expect: "accepted; a fine checkerboard",
            json: many_rects(),
        },
        Probe {
            file: "06-precomp.tgs",
            pack: "sticker",
            tests: "a precomp: one asset drawn by two layers".into(),
            expect: "accepted; the sprite twice, side by side, half size",
            json: precomp(&sprite),
        },
        Probe {
            file: "07-keyframes.tgs",
            pack: "sticker",
            tests: "position hold keyframes moving the sprite by whole pixels".into(),
            expect: "accepted; the sprite steps right and back, staying sharp",
            json: keyframes(&sprite),
        },
        Probe {
            file: "08-30fps.tgs",
            pack: "sticker",
            tests: "30 fps instead of 60: 180 frames lasting 6 s".into(),
            expect: "rejected (stickers must be 60 fps); if accepted, note how long it plays",
            json: half_rate(&sprite),
        },
        Probe {
            file: "09-6s.tgs",
            pack: "sticker",
            tests: "360 frames at 60 fps: 6 s".into(),
            expect: "rejected (at most 3 s); if accepted, note whether it plays 6 s",
            json: twice(&sprite),
        },
        Probe {
            file: "10-emoji.tgs",
            pack: "emoji",
            tests: "01 as a custom emoji, 512x512 like stickers".into(),
            expect: "accepted; sharp at emoji size",
            json: sprite.clone(),
        },
        Probe {
            file: "11-emoji-100.tgs",
            pack: "emoji",
            tests: "the same on a 100x100 canvas, the size of video emoji".into(),
            expect: "one of 10 and 11 is accepted, or both: tells which size emoji use",
            json: small_canvas(&sprite),
        },
    ];
    if let Some((name, json)) = art {
        probes.push(Probe {
            file: "12-real-art.tgs",
            pack: "sticker",
            tests: format!("real art ({name}), encoded as tgradish does"),
            expect: "accepted; looks like the input, no seams or fringes along edges",
            json,
        });
    }

    Ok(probes)
}

/// Writes the probes and `CHECKLIST.md` into `dir`.
pub fn write(dir: &Path, art: Option<(String, String)>) -> Result<()> {
    let probes = probes(art)?;
    std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let mut checklist = String::from(
        "# Telegram probes\n\n\
         Made by `tgs-lab probes`; `docs/probes.md` says how to use them. Upload each \
         file through @Stickers into a test pack of the kind in \"Pack\", then look at \
         it in each app. Write down for each: accepted or the bot's error, and how it \
         looks (sharp, seams, wrong colours, not shown).\n\n\
         | File | Pack | Tests | Expected | JSON | .tgs | Accepted | Android | Desktop | iOS | Web |\n\
         | --- | --- | --- | --- | ---: | ---: | --- | --- | --- | --- | --- |\n",
    );
    for probe in &probes {
        let tgs = file::pack(probe.json.as_bytes(), 15);
        let (stats, issues) = check(probe.json.as_bytes(), Some(tgs.len()))
            .map_err(anyhow::Error::msg)
            .context(probe.file)?;
        ensure!(tgs.len() <= MAX_TGS, "{} is {} bytes, over 64 KiB", probe.file, tgs.len());
        std::fs::write(dir.join(probe.file), &tgs)?;
        writeln!(
            checklist,
            "| {} | {} | {} | {} | {:.0} KiB | {:.1} KiB |  |  |  |  |  |",
            probe.file,
            probe.pack,
            probe.tests,
            probe.expect,
            stats.json_bytes as f64 / 1024.0,
            tgs.len() as f64 / 1024.0,
        )?;
        let notes: Vec<String> = issues.iter().map(|issue| issue.message.clone()).collect();
        println!(
            "{:<20} {:>8} bytes JSON {:>6} bytes .tgs {:>5} layers{}",
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
        assert_eq!(probes.len(), 11);
        for probe in &probes {
            // gzip -9 packs worse than zopfli
            assert!(file::quick_size(probe.json.as_bytes()) <= MAX_TGS, "{}", probe.file);
            let pixels = Renderer::new(probe.json.as_bytes()).unwrap().render(0, 64).unwrap();
            assert!(pixels.iter().any(|pixel| pixel[3] > 0), "{} draws nothing", probe.file);
        }
        let near = probes.iter().find(|probe| probe.file == "03-near-2mib.tgs").unwrap();
        assert!((NEAR_2_MIB - 100_000..=NEAR_2_MIB).contains(&near.json.len()));
    }
}
