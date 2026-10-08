//! Runs the corpus through the encoder and compares sizes with pixelart2tgs
//! 1.x (the "1.x now" column of the corpus README).

use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result};
use tgradish_tgs::encode::{Settings, painter};
use tgradish_tgs::file;
use tgradish_tgs::frames::{DecodeOptions, decode};
use tgradish_tgs::layout::lay_out;
use tgradish_tgs::lottie::Style;
use tgradish_tgs::normalise::{Options, normalise};

use crate::verify::{self, Renderer};

pub struct Bench {
    /// gzip -9 instead of zopfli, for quick runs.
    pub fast: bool,
    /// Render every result and count wrong cells and seams.
    pub verify: bool,
    pub settings: Settings,
    pub style: Style,
}

/// "1.x now" sizes by file name, from the README's table.
fn sizes_from_1x(readme: &str) -> Vec<(String, u64)> {
    readme
        .lines()
        .filter_map(|line| {
            let cells: Vec<&str> = line.split('|').map(str::trim).collect();
            let name = cells.get(1)?.strip_prefix('`')?.strip_suffix('`')?;
            Some((name.to_owned(), cells.get(7)?.parse().ok()?))
        })
        .collect()
}

pub fn run(dir: &Path, bench: &Bench) -> Result<()> {
    let readme = std::fs::read_to_string(dir.join("README.md")).unwrap_or_default();
    let reference = sizes_from_1x(&readme);
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .with_context(|| format!("cannot read {}", dir.display()))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<_, _>>()?;
    files.retain(|path| {
        matches!(path.extension().and_then(|e| e.to_str()), Some("gif" | "webp" | "ase"))
    });
    files.sort();

    println!(
        "{:38} {:>7} {:>7} {:>7} {:>6} {:>6} {:>6} {:>6} {:>6}",
        "file", "raw", "packed", "1.x", "ratio", "layers", "groups", "rects", "ms"
    );
    let (mut ours, mut theirs, mut over) = (0u64, 0u64, 0);
    let (mut misses, mut leaks, mut fringes) = (0, 0, 0);
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy();
        let started = Instant::now();
        let input = decode(&std::fs::read(path)?, &DecodeOptions::default())
            .with_context(|| name.to_string())?;
        let (anim, _) = normalise(&input, &Options::default())?;
        let scene = painter(&anim, &bench.settings).with_context(|| name.to_string())?;
        let json = lay_out(&scene, &anim, Some("tgradish".into())).to_json(bench.style);
        let packed = if bench.fast {
            use std::io::Write;
            let mut encoder =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
            encoder.write_all(json.as_bytes())?;
            encoder.finish()?
        } else {
            file::pack(json.as_bytes(), 15)
        };
        let ms = started.elapsed().as_millis();
        let groups: usize = scene.layers.iter().map(|layer| layer.groups.len()).sum();
        let rects: usize = scene
            .layers
            .iter()
            .flat_map(|layer| &layer.groups)
            .map(|group| group.shapes.len())
            .sum();
        let old = reference.iter().find(|(file, _)| *file == name).map(|(_, size)| *size);
        let ratio = old.map(|old| format!("{:.2}", old as f64 / packed.len() as f64));
        println!(
            "{name:38} {:>7} {:>7} {:>7} {:>6} {:>6} {:>6} {:>6} {:>6}",
            json.len(),
            packed.len(),
            old.map(|old| old.to_string()).unwrap_or_default(),
            ratio.unwrap_or_default(),
            scene.layers.len(),
            groups,
            rects,
            ms
        );
        if let Some(old) = old {
            ours += packed.len() as u64;
            theirs += old;
        }
        over += usize::from(packed.len() > tgradish_tgs::limits::telegram::MAX_BYTES);
        if bench.verify {
            for (renderer, mut instance) in Renderer::all(json.as_bytes())? {
                let centres = verify::centres(&mut instance, &anim)?;
                let seams = verify::seams(&mut instance, &anim, &[100, 160, 237, 512], None)?;
                if centres.misses + seams.leaks + seams.fringes > 0 {
                    println!(
                        "  {renderer}: {} wrong cells, {} leaks, {} fringes",
                        centres.misses, seams.leaks, seams.fringes
                    );
                }
                misses += centres.misses;
                leaks += seams.leaks;
                fringes += seams.fringes;
            }
        }
    }
    println!(
        "total: {ours} bytes against {theirs} for 1.x ({:.2}x smaller); {over} over 64 KiB",
        theirs as f64 / ours.max(1) as f64
    );
    if bench.verify {
        println!("rendered: {misses} wrong cells, {leaks} leaks, {fringes} fringes");
    }
    Ok(())
}
