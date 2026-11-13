//! Encoding pictures as WebP within the size limit, with metadata.

use schemars::JsonSchema;
use serde::Serialize;

use crate::error::{Error, Result};
use crate::telegram;

/// Straight RGBA as a WebP: lossless when `quality` is `None`, otherwise
/// lossy at that quality, 0 to 100, with exact alpha.
#[allow(unsafe_code)]
pub(crate) fn encode(
    rgba: &[u8],
    width: u32,
    height: u32,
    quality: Option<f32>,
) -> Result<Vec<u8>> {
    use libwebp_sys as webp;
    use std::mem::MaybeUninit;
    assert_eq!(rgba.len(), width as usize * height as usize * 4, "a whole picture");
    let failed = |what: &str| Error::Webp(what.to_owned());
    let abi = webp::WEBP_ENCODER_ABI_VERSION as i32;
    // SAFETY: the config, picture and writer start zeroed, which suits
    // their integers, pointers and optional functions, padding fields
    // included, which libwebp's initialisers leave as they are; the picture
    // reads `rgba` only while importing it, and is freed; the writer's
    // memory is copied out, then freed
    unsafe {
        let mut config = MaybeUninit::<webp::WebPConfig>::zeroed();
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
        let mut picture = MaybeUninit::<webp::WebPPicture>::zeroed();
        if webp::WebPPictureInitInternal(picture.as_mut_ptr(), abi) == 0 {
            return Err(failed("libwebp's version doesn't match"));
        }
        let mut picture = picture.assume_init();
        picture.use_argb = i32::from(quality.is_none());
        (picture.width, picture.height) = (width as i32, height as i32);
        let mut writer = MaybeUninit::<webp::WebPMemoryWriter>::zeroed();
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
#[serde(tag = "kind", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Encoding {
    Lossless,
    /// Lossy at this quality, from 0 to 100, as lossless didn't fit.
    Lossy {
        quality: u8,
    },
}

/// [`encode`]d within the size limit, as `finish` makes the file of it:
/// lossless, or lossy at the highest quality that fits, which photos need.
/// Returns the smallest try if none fits.
pub(super) fn fit(
    rgba: &[u8],
    width: u32,
    height: u32,
    finish: impl Fn(Vec<u8>) -> Vec<u8>,
) -> Result<(Vec<u8>, Encoding)> {
    let encode = |quality| encode(rgba, width, height, quality).map(&finish);
    let lossless = encode(None)?;
    let fits = |webp: &[u8]| webp.len() as u64 <= telegram::MAX_STILL_BYTES;
    if fits(&lossless) {
        return Ok((lossless, Encoding::Lossless));
    }
    // the highest quality that fits, searched among whole numbers
    let (mut low, mut high) = (0u8, 100u8);
    let mut best = None;
    while low <= high {
        let quality = low + (high - low) / 2;
        let lossy = encode(Some(f32::from(quality)))?;
        if fits(&lossy) {
            best = Some((lossy, Encoding::Lossy { quality }));
            low = quality + 1;
        } else if quality == 0 {
            return Ok(best.unwrap_or((lossy, Encoding::Lossy { quality })));
        } else {
            high = quality - 1;
        }
    }
    // the search only ends without a fit at quality 0, which returns above
    Ok(best.expect("a quality that fits"))
}

/// `webp` with `xmp` metadata: in the extended format, which says what
/// chunks follow and whether the picture has alpha.
pub(super) fn with_xmp(
    webp: &[u8],
    xmp: &[u8],
    (width, height): (u32, u32),
    alpha: bool,
) -> Vec<u8> {
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
pub(super) fn xmp(title: Option<&str>, mark: bool) -> Vec<u8> {
    let escape = |text: &str| {
        text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
    };
    let tool =
        if mark { format!(" xmp:CreatorTool=\"{}\"", crate::TOOL_ID) } else { String::new() };
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telegram::{Issue, Target};
    use crate::webp::{creator_tool, inspect_file, preview};

    fn read_back(webp: &[u8]) -> (u32, u32, Vec<u8>) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.webp");
        std::fs::write(&path, webp).unwrap();
        let preview = preview(&path).unwrap();
        let info = inspect_file(&path).unwrap();
        assert_eq!((info.width, info.height), (preview.width, preview.height));
        (preview.width, preview.height, preview.frames[0].0.clone())
    }

    #[test]
    fn encodes_losslessly_with_metadata() {
        // half transparent, half a gradient
        let rgba: Vec<u8> = (0..64u32 * 32)
            .flat_map(|i| if i % 64 < 32 { [0; 4] } else { [i as u8, 0, 200, 255] })
            .collect();
        let (webp, encoding) = fit(&rgba, 64, 32, |webp| webp).unwrap();
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
        // whole headers on a cut picture aren't a picture
        let dir = tempfile::tempdir().unwrap();
        let (whole, cut) = (dir.path().join("whole.webp"), dir.path().join("cut.webp"));
        std::fs::write(&whole, &marked).unwrap();
        std::fs::write(&cut, &webp[..webp.len() / 2]).unwrap();
        let info = inspect_file(&whole).unwrap();
        assert_eq!((info.width, info.animated, info.made_with.is_some()), (64, false, true));
        assert!(info.issues(Target::Sticker).iter().any(|i| matches!(i, Issue::WrongSize { .. })));
        assert!(inspect_file(&cut).is_err());
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
        // with metadata, which counts too
        let xmp = vec![b'x'; 20_000];
        let finish = |webp: Vec<u8>| with_xmp(&webp, &xmp, (512, 512), false);
        let (webp, encoding) = fit(&noise, 512, 512, finish).unwrap();
        assert!(matches!(encoding, Encoding::Lossy { quality } if quality > 50), "{encoding:?}");
        assert!(webp.len() as u64 <= telegram::MAX_STILL_BYTES);
        assert_eq!(read_back(&webp).0, 512);
    }
}
