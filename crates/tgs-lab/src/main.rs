//! Development tool for tgradish's `.tgs` output: renders stickers with
//! tlottie, the renderer in Telegram's current clients, and shows what
//! normalising does to inputs. Verification and benchmarks come with later
//! milestones (see `docs/tgs.md`).
//!
//! ```console
//! cargo run -p tgs-lab -- info sticker.tgs
//! cargo run -p tgs-lab -- render sticker.tgs --frame 10 --size 512 out.png
//! cargo run -p tgs-lab -- normalise art.gif
//! ```

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use tlottie::{CPURenderer, Composition, Limits, RenderOptions};

/// Lottie JSON from a `.tgs` (gzipped) or plain `.json` file.
fn read_lottie(path: &Path) -> Result<Vec<u8>> {
    let bytes = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut json = Vec::new();
        flate2::read::GzDecoder::new(&bytes[..])
            .read_to_end(&mut json)
            .with_context(|| format!("{} is not valid gzip", path.display()))?;
        Ok(json)
    } else {
        Ok(bytes)
    }
}

fn parse(json: &[u8]) -> Result<Composition> {
    Composition::parse(json, &Limits::default()).map_err(|err| anyhow::anyhow!("{err}"))
}

fn info(paths: &[PathBuf]) -> Result<()> {
    for path in paths {
        let json = read_lottie(path)?;
        let composition = parse(&json).with_context(|| format!("{}", path.display()))?;
        println!(
            "{}: {} frames, {} bytes of JSON, {} bytes on disk",
            path.display(),
            composition.frame_count(),
            json.len(),
            std::fs::metadata(path)?.len(),
        );
    }
    Ok(())
}

/// Writes premultiplied ARGB pixels as an RGBA PNG.
fn write_png(path: &Path, size: u32, pixels: &[u32]) -> Result<()> {
    let mut rgba = Vec::with_capacity(pixels.len() * 4);
    for &pixel in pixels {
        let [a, r, g, b] = pixel.to_be_bytes();
        let unpremultiply =
            |c: u8| if a == 0 { 0 } else { (u32::from(c) * 255 / u32::from(a)) as u8 };
        rgba.extend([unpremultiply(r), unpremultiply(g), unpremultiply(b), a]);
    }
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut encoder = png::Encoder::new(file, size, size);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&rgba)?;
    writer.finish()?;
    Ok(())
}

fn render(args: &[String]) -> Result<()> {
    let mut frame = 0.0f32;
    let mut size = 512u32;
    let mut positional = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--frame" => frame = iter.next().context("--frame needs a number")?.parse()?,
            "--size" => size = iter.next().context("--size needs a number")?.parse()?,
            _ => positional.push(arg),
        }
    }
    let [input, output] = positional[..] else {
        bail!("usage: tgs-lab render INPUT [--frame N] [--size PX] OUTPUT.png");
    };
    let composition = parse(&read_lottie(Path::new(input))?)?;
    let mut pixels = vec![0u32; (size as usize).pow(2)];
    let mut renderer = CPURenderer::new(composition);
    renderer
        .render(frame, &mut pixels, size, size, RenderOptions::default())
        .map_err(|err| anyhow::anyhow!("{err}"))?;
    write_png(Path::new(output), size, &pixels)
}

/// Prints the normalisation report of each input as a JSON line.
fn normalise(paths: &[PathBuf]) -> Result<()> {
    use tgradish_tgs::frames::{DecodeOptions, decode};
    use tgradish_tgs::normalise::{Options, normalise};
    for path in paths {
        let bytes =
            std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
        let animation = decode(&bytes, &DecodeOptions::default())
            .with_context(|| format!("{}", path.display()))?;
        let (_, report) = normalise(&animation, &Options::default())
            .with_context(|| format!("{}", path.display()))?;
        let mut line = serde_json::json!({ "file": path });
        line.as_object_mut()
            .unwrap()
            .extend(serde_json::to_value(&report)?.as_object().unwrap().clone());
        println!("{line}");
    }
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("info") if args.len() > 1 => {
            info(&args[1..].iter().map(PathBuf::from).collect::<Vec<_>>())
        }
        Some("render") => render(&args[1..]),
        Some("normalise") if args.len() > 1 => {
            normalise(&args[1..].iter().map(PathBuf::from).collect::<Vec<_>>())
        }
        _ => bail!(
            "usage: tgs-lab info FILE...\n       \
             tgs-lab render INPUT [--frame N] [--size PX] OUTPUT.png\n       \
             tgs-lab normalise FILE..."
        ),
    }
}
