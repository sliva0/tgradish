//! Image stickers and emoji: one frame of a video, or a picture, as a
//! lossless WebP, which @Stickers takes for its static sets as it takes
//! PNG.
//!
//! The picture is planned like a WebM sticker's, from the options a still
//! picture has: target, crop, resize, scaling and exact scale, and `start`,
//! where the frame is.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::Serialize;

use crate::backend::Backend;
use crate::convert::{self, Plan, Request};
use crate::error::{Error, Result};
use crate::ffmpeg::CancelToken;
use crate::options::{Options, Scaling};
use crate::telegram::{self, Issue, Target};

/// The input with a `.sticker.webp` or `.emoji.webp` extension.
pub fn default_output(input: &Path, target: Target) -> PathBuf {
    input.with_extension(format!("{}.webp", target.name()))
}

/// What `convert --json` prints for an image: once it is made, or why it
/// couldn't be.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum StillEvent {
    Finished(Outcome),
    /// A conversion failed. Only printed by the CLI; the library returns
    /// errors instead.
    Error {
        message: String,
        /// The input that failed when converting several, `None` when the
        /// whole command failed.
        input: Option<PathBuf>,
    },
}

/// A finished image sticker or emoji.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Outcome {
    pub output: PathBuf,
    pub bytes: u64,
    pub width: u32,
    pub height: u32,
    pub encoding: Encoding,
    pub issues: Vec<Issue>,
    pub warnings: Vec<String>,
}

/// The options of `options` a still picture has.
pub fn picture_options(options: &Options) -> Options {
    Options {
        target: options.target,
        crop: options.crop,
        resize: options.resize,
        scaling: options.scaling,
        exact_scale: options.exact_scale,
        start: options.start,
        title: options.title.clone(),
        watermark: options.watermark,
        ..Options::default()
    }
}

/// Plans the picture: its frame, a sticker's or emoji's size, and the
/// output, `.webp` unless `request` names one.
pub fn plan(request: &Request, source: crate::ffmpeg::Probe) -> Result<(Plan, Vec<String>)> {
    let options = picture_options(&request.options);
    let target = options.target.unwrap_or_default();
    let output = request.output.clone().unwrap_or_else(|| default_output(&request.input, target));
    let request = Request { output: Some(output), options, ..request.clone() };
    let (mut plan, warnings) = convert::plan(&request, source)?;
    // one frame
    plan.length = 1.0 / plan.fps;
    Ok((plan, warnings))
}

/// Makes an image sticker or emoji from the frame at `start`.
pub fn convert(backend: &Backend, request: &Request, cancel: &CancelToken) -> Result<Outcome> {
    let source = backend.probe(&request.input, cancel)?;
    let (mut plan, warnings) = plan(request, source)?;
    let options = &request.options;
    if options.scaling.unwrap_or(Scaling::Auto) == Scaling::Auto
        && options.exact_scale.is_none()
        && plan.enlarges() >= 2.0
        && convert::looks_like_pixel_art(backend, &plan, cancel)?
    {
        plan.scaling = Scaling::Sharp;
    }
    let rgba = backend.still(&plan, cancel)?;
    cancel.check()?;
    let (mut webp, encoding) = fit(&rgba, plan.width, plan.height)?;
    if plan.watermark || plan.title.is_some() {
        let alpha = rgba.as_chunks::<4>().0.iter().any(|pixel| pixel[3] < 255);
        let xmp = xmp(plan.title.as_deref(), plan.watermark);
        webp = with_xmp(&webp, &xmp, (plan.width, plan.height), alpha);
    }
    crate::fsutil::write_file(&plan.output, &webp, request.overwrite).map_err(|err| {
        match err.kind() {
            std::io::ErrorKind::AlreadyExists => Error::OutputExists(plan.output.clone()),
            _ => err.into(),
        }
    })?;
    let bytes = webp.len() as u64;
    Ok(Outcome {
        output: plan.output.clone(),
        bytes,
        width: plan.width,
        height: plan.height,
        encoding,
        issues: telegram::check_still(plan.width, plan.height, bytes, plan.target),
        warnings,
    })
}

/// Straight RGBA as a WebP: lossless when `quality` is `None`, otherwise
/// lossy at that quality, 0 to 100, with exact alpha.
#[allow(unsafe_code)]
pub fn encode(rgba: &[u8], width: u32, height: u32, quality: Option<f32>) -> Result<Vec<u8>> {
    use libwebp_sys as webp;
    use std::mem::MaybeUninit;
    assert_eq!(rgba.len(), width as usize * height as usize * 4, "a whole picture");
    let failed = |what: &str| Error::Webp(what.to_owned());
    let abi = webp::WEBP_ENCODER_ABI_VERSION as i32;
    // SAFETY: libwebp initialises the config, picture and writer before
    // they are read; the picture reads `rgba` only while importing it, and
    // is freed; the writer's memory is copied out, then freed
    unsafe {
        let mut config = MaybeUninit::<webp::WebPConfig>::uninit();
        let preset = webp::WebPPreset::WEBP_PRESET_DEFAULT;
        if webp::WebPConfigInitInternal(config.as_mut_ptr(), preset, 100.0, abi) == 0 {
            return Err(failed("libwebp's version doesn't match"));
        }
        let mut config = config.assume_init();
        match quality {
            // the smallest libwebp makes, which may change invisible pixels
            None => {
                webp::WebPConfigLosslessPreset(&mut config, LOSSLESS_EFFORT);
                config.exact = 0;
            }
            Some(quality) => {
                (config.quality, config.method, config.alpha_quality) = (quality, 6, 100);
            }
        }
        if webp::WebPValidateConfig(&config) == 0 {
            return Err(failed("invalid settings"));
        }
        let mut picture = MaybeUninit::<webp::WebPPicture>::uninit();
        if webp::WebPPictureInitInternal(picture.as_mut_ptr(), abi) == 0 {
            return Err(failed("libwebp's version doesn't match"));
        }
        let mut picture = picture.assume_init();
        picture.use_argb = i32::from(quality.is_none());
        (picture.width, picture.height) = (width as i32, height as i32);
        let mut writer = MaybeUninit::<webp::WebPMemoryWriter>::uninit();
        webp::WebPMemoryWriterInit(writer.as_mut_ptr());
        let mut writer = writer.assume_init();
        picture.writer = Some(webp::WebPMemoryWrite);
        picture.custom_ptr = (&raw mut writer).cast();
        let encoded = webp::WebPPictureImportRGBA(&mut picture, rgba.as_ptr(), width as i32 * 4)
            != 0
            && webp::WebPEncode(&config, &mut picture) != 0;
        webp::WebPPictureFree(&mut picture);
        let bytes = if encoded && !writer.mem.is_null() {
            std::slice::from_raw_parts(writer.mem, writer.size).to_vec()
        } else {
            Vec::new()
        };
        webp::WebPMemoryWriterClear(&mut writer);
        if bytes.is_empty() {
            return Err(failed("libwebp couldn't encode the picture"));
        }
        Ok(bytes)
    }
}

/// libwebp's lossless effort, 0 to 9: 6 is close to the smallest at a
/// fraction of 9's time.
const LOSSLESS_EFFORT: i32 = 6;

/// How a picture was made to fit the size limit.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, JsonSchema)]
#[serde(tag = "encoding", rename_all = "snake_case")]
pub enum Encoding {
    Lossless,
    /// Lossy at this quality, from 0 to 100, as lossless didn't fit.
    Lossy {
        quality: u8,
    },
}

/// [`encode`]d within the size limit: lossless, or lossy at the highest
/// quality that fits, which photos need. Returns the smallest try if none
/// fits.
fn fit(rgba: &[u8], width: u32, height: u32) -> Result<(Vec<u8>, Encoding)> {
    let lossless = encode(rgba, width, height, None)?;
    let fits = |webp: &[u8]| webp.len() as u64 <= telegram::MAX_STILL_BYTES;
    if fits(&lossless) {
        return Ok((lossless, Encoding::Lossless));
    }
    // the highest quality that fits, searched among whole numbers
    let (mut low, mut high) = (0u8, 100u8);
    let mut best = None;
    while low <= high {
        let quality = low + (high - low) / 2;
        let lossy = encode(rgba, width, height, Some(f32::from(quality)))?;
        if fits(&lossy) {
            best = Some((lossy, Encoding::Lossy { quality }));
            low = quality + 1;
        } else if quality == 0 {
            return Ok(best.unwrap_or((lossy, Encoding::Lossy { quality })));
        } else {
            high = quality - 1;
        }
    }
    let smallest = || {
        encode(rgba, width, height, Some(0.0)).map(|webp| (webp, Encoding::Lossy { quality: 0 }))
    };
    best.map_or_else(smallest, Ok)
}

/// `webp` with `xmp` metadata: in the extended format, which says what
/// chunks follow and whether the picture has alpha.
fn with_xmp(webp: &[u8], xmp: &[u8], (width, height): (u32, u32), alpha: bool) -> Vec<u8> {
    let chunk = |name: &[u8], data: &[u8]| {
        let mut chunk = name.to_vec();
        chunk.extend((data.len() as u32).to_le_bytes());
        chunk.extend(data);
        if data.len() % 2 == 1 {
            chunk.push(0);
        }
        chunk
    };
    // RIFF, its size and WEBP, then chunks
    let mut chunks = webp.get(12..).unwrap_or_default().to_vec();
    if chunks.starts_with(b"VP8X") {
        // XMP's flag
        chunks[8] |= 0x04;
    } else {
        let mut header = vec![0x04 | if alpha { 0x10 } else { 0 }, 0, 0, 0];
        header.extend(&(width - 1).to_le_bytes()[..3]);
        header.extend(&(height - 1).to_le_bytes()[..3]);
        chunks.splice(0..0, chunk(b"VP8X", &header));
    }
    chunks.extend(chunk(b"XMP ", xmp));
    let mut out = b"RIFF".to_vec();
    out.extend((chunks.len() as u32 + 4).to_le_bytes());
    out.extend(b"WEBP");
    out.extend(chunks);
    out
}

/// XMP with the picture's title, and naming tgradish as the tool that made
/// it when `mark` says so.
fn xmp(title: Option<&str>, mark: bool) -> Vec<u8> {
    let escape = |text: &str| {
        text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
    };
    let tool = if mark {
        format!(" xmp:CreatorTool=\"tgradish {}\"", env!("CARGO_PKG_VERSION"))
    } else {
        String::new()
    };
    let title = title.map_or(String::new(), |title| {
        format!(
            "<dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt></dc:title>",
            escape(title)
        )
    });
    format!(
        "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF \
         xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:Description \
         rdf:about=\"\" xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\" \
         xmlns:dc=\"http://purl.org/dc/elements/1.1/\"{tool}>{title}</rdf:Description>\
         </rdf:RDF></x:xmpmeta>"
    )
    .into_bytes()
}

/// The tool a WebP's XMP names as its maker, if it has one.
pub fn creator_tool(webp: &[u8]) -> Option<String> {
    if webp.get(..4)? != b"RIFF" || webp.get(8..12)? != b"WEBP" {
        return None;
    }
    let mut chunks = webp.get(12..)?;
    while chunks.len() >= 8 {
        let size = u32::from_le_bytes(chunks[4..8].try_into().ok()?) as usize;
        let data = chunks.get(8..8 + size)?;
        if &chunks[..4] == b"XMP " {
            let text = std::str::from_utf8(data).ok()?;
            let key = "xmp:CreatorTool=\"";
            let start = text.find(key)? + key.len();
            let end = start + text[start..].find('"')?;
            return Some(text[start..end].to_owned());
        }
        // chunks are padded to even sizes
        chunks = chunks.get(8 + size + size % 2..)?;
    }
    None
}

/// What an image sticker is, for inspecting it.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Info {
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
    pub alpha: bool,
    pub lossy: bool,
    /// The tool its metadata names as its maker.
    pub made_with: Option<String>,
}

impl Info {
    /// What it would be: emoji are the only square 100 px stickers.
    pub fn target(&self) -> Target {
        if (self.width, self.height) == (100, 100) { Target::Emoji } else { Target::Sticker }
    }
}

/// Whether `path` names a WebP image, by its extension.
pub fn is_image(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("webp"))
}

/// What the WebP at `path` is.
pub fn inspect_file(path: &Path) -> Result<Info> {
    let bytes = std::fs::read(path)?;
    let invalid = |message: String| Error::Probe { path: path.to_path_buf(), message };
    let mut decoder = image_webp::WebPDecoder::new(Cursor::new(&bytes))
        .map_err(|err| invalid(err.to_string()))?;
    let (width, height) = decoder.dimensions();
    Ok(Info {
        width,
        height,
        bytes: bytes.len() as u64,
        alpha: decoder.has_alpha(),
        lossy: decoder.is_lossy(),
        made_with: creator_tool(&bytes),
    })
}

/// A WebP or PNG image sticker's size, for inspecting it.
pub fn dimensions(path: &Path) -> Result<(u32, u32)> {
    let bytes = std::fs::read(path)?;
    let invalid = |message: String| Error::Probe { path: path.to_path_buf(), message };
    let decoder = image_webp::WebPDecoder::new(Cursor::new(&bytes))
        .map_err(|err| invalid(err.to_string()))?;
    Ok(decoder.dimensions())
}

/// A WebP, decoded for a preview: one frame, as long as a `.tgs` one.
pub fn preview(path: &Path) -> Result<crate::tgs::Preview> {
    let bytes = std::fs::read(path)?;
    let invalid = |message: String| Error::Probe { path: path.to_path_buf(), message };
    let mut decoder = image_webp::WebPDecoder::new(Cursor::new(&bytes))
        .map_err(|err| invalid(err.to_string()))?;
    let (width, height) = decoder.dimensions();
    let size = decoder.output_buffer_size().ok_or_else(|| invalid("too large".into()))?;
    let mut pixels = vec![0; size];
    decoder.read_image(&mut pixels).map_err(|err| invalid(err.to_string()))?;
    let rgba = if decoder.has_alpha() {
        pixels
    } else {
        pixels.as_chunks::<3>().0.iter().flat_map(|&[r, g, b]| [r, g, b, 255]).collect()
    };
    Ok(crate::tgs::Preview { width, height, frames: vec![(rgba, 1)] })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_back(webp: &[u8]) -> (u32, u32, Vec<u8>) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.webp");
        std::fs::write(&path, webp).unwrap();
        let preview = preview(&path).unwrap();
        assert_eq!(dimensions(&path).unwrap(), (preview.width, preview.height));
        (preview.width, preview.height, preview.frames[0].0.clone())
    }

    #[test]
    fn encodes_losslessly_with_metadata() {
        // half transparent, half a gradient
        let rgba: Vec<u8> = (0..64u32 * 32)
            .flat_map(|i| if i % 64 < 32 { [0; 4] } else { [i as u8, 0, 200, 255] })
            .collect();
        let (webp, encoding) = fit(&rgba, 64, 32).unwrap();
        assert_eq!(encoding, Encoding::Lossless);
        let marked = with_xmp(&webp, &xmp(Some("a <pig>"), true), (64, 32), true);
        for webp in [&webp, &marked] {
            let (width, height, pixels) = read_back(webp);
            assert_eq!((width, height), (64, 32));
            // invisible pixels may change; visible ones don't
            let visible = |pixels: &[u8]| -> Vec<[u8; 4]> {
                pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|&p| if p[3] == 0 { [0; 4] } else { p })
                    .collect()
            };
            assert_eq!(visible(&pixels), visible(&rgba));
        }
        let text = String::from_utf8_lossy(&marked);
        assert!(text.contains("a &lt;pig&gt;"));
        let tool = format!("tgradish {}", env!("CARGO_PKG_VERSION"));
        assert_eq!(creator_tool(&marked), Some(tool));
        assert_eq!(creator_tool(&webp), None);
        assert_eq!(&marked[12..16], b"VP8X");
        assert_eq!(u32::from_le_bytes(marked[4..8].try_into().unwrap()) as usize + 8, marked.len());
        // no title, no mark: no tool named
        assert!(!String::from_utf8_lossy(&xmp(None, false)).contains("tgradish"));
    }

    #[test]
    fn goes_lossy_where_lossless_would_not_fit() {
        // noise doesn't fit 512 KiB losslessly at 512 x 512
        let mut seed = 1u32;
        let noise: Vec<u8> = (0..512 * 512 * 4)
            .map(|i| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                if i % 4 == 3 { 255 } else { (seed >> 24) as u8 }
            })
            .collect();
        let (webp, encoding) = fit(&noise, 512, 512).unwrap();
        assert!(matches!(encoding, Encoding::Lossy { quality } if quality > 50), "{encoding:?}");
        assert!(webp.len() as u64 <= telegram::MAX_STILL_BYTES);
        assert_eq!(read_back(&webp).0, 512);
    }
}
