//! Development tool for tgradish's `.tgs` output: renders stickers with
//! tlottie, the renderer in Telegram's current clients, shows what
//! normalising does to inputs, and verifies encodings against their art.
//! Benchmarks come with later milestones (see `docs/tgs.md`).
//!
//! ```console
//! cargo run -p tgs-lab -- info sticker.tgs
//! cargo run -p tgs-lab -- render sticker.tgs --frame 10 --size 512 out.png
//! cargo run -p tgs-lab -- normalise art.gif
//! cargo run -p tgs-lab -- encode art.gif out.tgs [--runs]
//! cargo run -p tgs-lab -- verify art.gif [out.tgs]
//! cargo run --release -p tgs-lab -- bench [--fast] [--verify] [--no-lifetimes] [--guess-order] [--full] [DIR]
//! ```

mod bench;
mod verify;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use tgradish_tgs::PixelAnim;
use tgradish_tgs::check::check;
use tgradish_tgs::frames::{DecodeOptions, decode};
use tgradish_tgs::layout::lay_out;
use tgradish_tgs::lottie::Style;
use tgradish_tgs::normalise::{Options, Report, normalise};
use tgradish_tgs::{encode, file};

use crate::verify::{Renderer, straight};

fn read(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))
}

/// Lottie JSON from a `.tgs` (gzipped) or plain `.json` file.
fn read_lottie(path: &Path) -> Result<Vec<u8>> {
    let bytes = read(path)?;
    if bytes.starts_with(&[0x1f, 0x8b]) {
        file::unpack(&bytes).with_context(|| format!("{}", path.display()))
    } else {
        Ok(bytes)
    }
}

fn load(path: &Path) -> Result<(PixelAnim, Report)> {
    let animation = decode(&read(path)?, &DecodeOptions::default())
        .with_context(|| format!("{}", path.display()))?;
    normalise(&animation, &Options::default()).with_context(|| format!("{}", path.display()))
}

/// `anim` as Lottie JSON, from the encoder or the `runs` baseline.
fn encoded(anim: &PixelAnim, runs: bool) -> Result<String> {
    let scene = if runs { encode::runs(anim) } else { encode::painter(anim, &Default::default())? };
    Ok(lay_out(&scene, anim, Some("tgs-lab".into())).to_json(Style::default()))
}

fn info(paths: &[PathBuf]) -> Result<()> {
    for path in paths {
        let bytes = read(path)?;
        let json = read_lottie(path)?;
        let packed = (bytes != json).then_some(bytes.len());
        let (stats, issues) = check(&json, packed).map_err(anyhow::Error::msg)?;
        Renderer::new(&json).with_context(|| format!("tlottie can't parse {}", path.display()))?;
        println!(
            "{}: {} frames, {} layers, {} bytes of JSON{}, features: {}",
            path.display(),
            stats.frames,
            stats.layers,
            stats.json_bytes,
            packed.map(|bytes| format!(", {bytes} bytes packed")).unwrap_or_default(),
            stats.features.iter().copied().collect::<Vec<_>>().join(", "),
        );
        for issue in issues {
            println!("  {:?}: {}", issue.severity, issue.message);
        }
    }
    Ok(())
}

/// Writes premultiplied RGBA pixels as a straight RGBA PNG.
fn write_png(path: &Path, size: u32, pixels: &[[u8; 4]]) -> Result<()> {
    let straight: Vec<[u8; 4]> = pixels.iter().map(|&pixel| straight(pixel)).collect();
    write_straight_png(path, size, size, &straight)
}

fn write_straight_png(path: &Path, width: u32, height: u32, pixels: &[[u8; 4]]) -> Result<()> {
    let rgba: Vec<u8> = pixels.iter().flatten().copied().collect();
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&rgba)?;
    writer.finish()?;
    Ok(())
}

/// `--name value` pairs and positional arguments.
type Arguments<'a> = (Vec<(&'a str, &'a str)>, Vec<&'a str>);

/// Splits `--name value` options from positional arguments.
fn options<'a>(args: &'a [String], names: &[&str]) -> Result<Arguments<'a>> {
    let (mut options, mut positional) = (Vec::new(), Vec::new());
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if names.contains(&arg.as_str()) {
            let value = iter.next().with_context(|| format!("{arg} needs a value"))?;
            options.push((arg.as_str(), value.as_str()));
        } else {
            positional.push(arg.as_str());
        }
    }
    Ok((options, positional))
}

fn render(args: &[String]) -> Result<()> {
    let (options, positional) = options(args, &["--frame", "--size"])?;
    let (mut frame, mut size) = (0, 512);
    for (name, value) in options {
        match name {
            "--frame" => frame = value.parse()?,
            _ => size = value.parse()?,
        }
    }
    let [input, output] = positional[..] else {
        bail!("usage: tgs-lab render INPUT [--frame N] [--size PX] OUTPUT.png");
    };
    let pixels = Renderer::new(&read_lottie(Path::new(input))?)?.render(frame, size)?;
    write_png(Path::new(output), size, &pixels)
}

/// Prints the normalisation report of each input as a JSON line.
fn print_reports(paths: &[PathBuf]) -> Result<()> {
    for path in paths {
        let (_, report) = load(path)?;
        let mut line = serde_json::json!({ "file": path });
        line.as_object_mut()
            .unwrap()
            .extend(serde_json::to_value(&report)?.as_object().unwrap().clone());
        println!("{line}");
    }
    Ok(())
}

fn encode_file(args: &[String]) -> Result<()> {
    let runs = args.iter().any(|arg| arg == "--runs");
    let paths: Vec<&String> = args.iter().filter(|arg| *arg != "--runs").collect();
    let [input, output] = paths[..] else {
        bail!("usage: tgs-lab encode SOURCE OUTPUT.tgs [--runs]");
    };
    let (input, output) = (Path::new(input), Path::new(output));
    let (anim, _) = load(input)?;
    let tgs = file::pack(encoded(&anim, runs)?.as_bytes(), 15);
    std::fs::write(output, &tgs).with_context(|| format!("cannot write {}", output.display()))?;
    println!("{}: {} bytes", output.display(), tgs.len());
    Ok(())
}

fn verify_file(args: &[String]) -> Result<()> {
    let (options, positional) = options(args, &["--sizes", "--frames", "--picture", "--explain"])?;
    let list = |name: &str| -> Result<Option<Vec<usize>>> {
        let Some((_, list)) = options.iter().find(|(option, _)| *option == name) else {
            return Ok(None);
        };
        Ok(Some(list.split(',').map(str::parse).collect::<Result<_, _>>()?))
    };
    let sizes: Vec<u32> = match list("--sizes")? {
        Some(sizes) => sizes.into_iter().map(|size| size as u32).collect(),
        None => vec![100, 160, 237, 512],
    };
    let frames = list("--frames")?;
    let runs = positional.contains(&"--runs");
    let positional: Vec<&str> = positional.into_iter().filter(|arg| *arg != "--runs").collect();
    let (source, tgs) = match positional[..] {
        [source] => (source, None),
        [source, tgs] => (source, Some(tgs)),
        _ => bail!(
            "usage: tgs-lab verify SOURCE [STICKER.tgs] [--sizes 100,160,237,512] [--frames 0,4] [--picture PREFIX] [--explain N] [--runs]"
        ),
    };
    let (anim, _) = load(Path::new(source))?;
    let json = match tgs {
        Some(path) => read_lottie(Path::new(path))?,
        None => encoded(&anim, runs)?.into_bytes(),
    };
    let (_, issues) = check(&json, None).map_err(anyhow::Error::msg)?;
    for issue in &issues {
        println!("{:?}: {}", issue.severity, issue.message);
    }
    let mut wrong = false;
    for (name, mut renderer) in Renderer::all(&json)? {
        let centres = verify::centres(&mut renderer, &anim)?;
        println!(
            "{name}: cell centres: {} of {} wrong ({} cells too small to check)",
            centres.misses, centres.checked, centres.skipped
        );
        for miss in &centres.examples {
            println!("  {miss:?}");
        }
        let seams = verify::seams(&mut renderer, &anim, &sizes, frames.as_deref())?;
        if let Some((_, count)) = options.iter().find(|(option, _)| *option == "--explain") {
            let frame = frames.as_ref().and_then(|frames| frames.first().copied()).unwrap_or(0);
            let size = sizes.iter().copied().max().unwrap_or(512);
            for line in verify::explain(&mut renderer, &anim, frame, size, count.parse()?)? {
                println!("  {line}");
            }
        }
        if let Some((_, path)) = options.iter().find(|(option, _)| *option == "--picture") {
            // the first checked frame at the largest size, per renderer
            let frame = frames.as_ref().and_then(|frames| frames.first().copied()).unwrap_or(0);
            let size = sizes.iter().copied().max().unwrap_or(512);
            let pixels = verify::picture(&mut renderer, &anim, frame, size)?;
            let path = format!("{path}-{name}.png");
            write_straight_png(Path::new(&path), 3 * size, size, &pixels)?;
        }
        println!(
            "{name}: seams at {sizes:?}: {} leaks, {} fringes, mean error {:.2}",
            seams.leaks, seams.fringes, seams.mean_error
        );
        wrong |= centres.misses > 0;
    }
    if wrong {
        bail!("the rendering doesn't match the art");
    }
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let paths = || args[1..].iter().map(PathBuf::from).collect::<Vec<_>>();
    match args.first().map(String::as_str) {
        Some("info") if args.len() > 1 => info(&paths()),
        Some("render") => render(&args[1..]),
        Some("normalise") if args.len() > 1 => print_reports(&paths()),
        Some("encode") => encode_file(&args[1..]),
        Some("verify") => verify_file(&args[1..]),
        Some("bench") => {
            let flag = |name: &str| args[1..].iter().any(|arg| arg == name);
            let dir = args[1..].iter().find(|arg| !arg.starts_with("--"));
            bench::run(
                Path::new(dir.map_or("references/pixelart", String::as_str)),
                &bench::Bench {
                    fast: flag("--fast"),
                    verify: flag("--verify"),
                    settings: encode::Settings {
                        lifetimes: !flag("--no-lifetimes"),
                        search_order: !flag("--guess-order"),
                    },
                    style: if flag("--full") {
                        Style { tgs_key: true, layer_start: true, rect_roundness: true }
                    } else {
                        Style::default()
                    },
                },
            )
        }
        _ => bail!(
            "usage: tgs-lab info FILE...\n       \
             tgs-lab render INPUT [--frame N] [--size PX] OUTPUT.png\n       \
             tgs-lab normalise FILE...\n       \
             tgs-lab encode SOURCE OUTPUT.tgs [--runs]\n       \
             tgs-lab verify SOURCE [STICKER.tgs] [--sizes 100,160,237,512] [--frames 0,4] [--picture PREFIX] [--explain N] [--runs]\n       \
             tgs-lab bench [--fast] [--verify] [--no-lifetimes] [--guess-order] [--full] [DIR]"
        ),
    }
}
