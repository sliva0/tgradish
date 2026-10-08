//! Development tool for tgradish's `.tgs` output: renders stickers with
//! tlottie, the renderer in Telegram's current clients, shows what
//! normalising does to inputs, and verifies encodings against their art.
//! Benchmarks come with later milestones (see `docs/tgs.md`).
//!
//! ```console
//! cargo run -p tgs-lab -- info sticker.tgs
//! cargo run -p tgs-lab -- render sticker.tgs --frame 10 --size 512 out.png
//! cargo run -p tgs-lab -- normalise art.gif
//! cargo run -p tgs-lab -- encode art.gif out.tgs
//! cargo run -p tgs-lab -- verify art.gif [out.tgs]
//! ```

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

/// The baseline encoding of `anim`, as Lottie JSON.
fn baseline(anim: &PixelAnim) -> String {
    let scene = encode::runs(anim);
    lay_out(&scene, anim, Some("tgs-lab".into())).to_json(Style::default())
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
    let rgba: Vec<u8> = pixels.iter().flat_map(|&pixel| straight(pixel)).collect();
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut encoder = png::Encoder::new(file, size, size);
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

fn encode_file(input: &Path, output: &Path) -> Result<()> {
    let (anim, _) = load(input)?;
    let tgs = file::pack(baseline(&anim).as_bytes(), 15);
    std::fs::write(output, &tgs).with_context(|| format!("cannot write {}", output.display()))?;
    println!("{}: {} bytes", output.display(), tgs.len());
    Ok(())
}

fn verify_file(args: &[String]) -> Result<()> {
    let (options, positional) = options(args, &["--sizes"])?;
    let sizes: Vec<u32> = match options.first() {
        Some((_, list)) => list.split(',').map(str::parse).collect::<Result<_, _>>()?,
        None => vec![100, 160, 237, 512],
    };
    let (source, tgs) = match positional[..] {
        [source] => (source, None),
        [source, tgs] => (source, Some(tgs)),
        _ => bail!("usage: tgs-lab verify SOURCE [STICKER.tgs] [--sizes 100,160,237,512]"),
    };
    let (anim, _) = load(Path::new(source))?;
    let json = match tgs {
        Some(path) => read_lottie(Path::new(path))?,
        None => baseline(&anim).into_bytes(),
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
        let seams = verify::seams(&mut renderer, &anim, &sizes)?;
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
        Some("encode") if args.len() == 3 => encode_file(Path::new(&args[1]), Path::new(&args[2])),
        Some("verify") => verify_file(&args[1..]),
        _ => bail!(
            "usage: tgs-lab info FILE...\n       \
             tgs-lab render INPUT [--frame N] [--size PX] OUTPUT.png\n       \
             tgs-lab normalise FILE...\n       \
             tgs-lab encode SOURCE OUTPUT.tgs\n       \
             tgs-lab verify SOURCE [STICKER.tgs] [--sizes 100,160,237,512]"
        ),
    }
}
