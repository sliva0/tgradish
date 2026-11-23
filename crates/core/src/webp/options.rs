//! Conversion options for static WebP: those of [`crate::webm::Options`] a
//! still picture has.

use serde::{Deserialize, Serialize};

use crate::options::{IntegerScale, Resize, Scaling, layered};
use crate::webm;

/// Options for one static WebP conversion: how the picture is fitted and
/// scaled, and whether it is watermarked; what is one file's own is its
/// [`Selection`](crate::options::Selection). The rest of WebM's concern time
/// and video encoding. `None` uses the default, as WebM's does; presets set
/// these in their `[webm]` table.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Options {
    /// How to fit the picture into the target dimensions. Default: contain
    /// for stickers, pad for emoji.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resize: Option<Resize>,
    /// Scaling algorithm: smooth, sharp or integer. Default: auto (sharp for
    /// pixel art upscaled 2x or more).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scaling: Option<Scaling>,
    /// Scale by an exact integer N or 1/N so source and output pixels align,
    /// as WebM's `integer-scale` does. Overrides `scaling`, and requires
    /// `resize` to be `contain` or `pad`.
    // read as `exact-scale` too, as it was once named
    #[serde(alias = "exact-scale", skip_serializing_if = "Option::is_none")]
    pub integer_scale: Option<IntegerScale>,
    /// Write tgradish metadata into the output. Default: true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub watermark: Option<bool>,
}

layered!(Options { resize, scaling, integer_scale, watermark });

impl Options {
    /// Overlays set fields from `overlay` onto `self`, clearing
    /// `integer_scale` where `overlay` sets [`IntegerScale::NONE`], as
    /// [`webm::Options::merged`] does.
    pub fn merged(self, overlay: &Options) -> Options {
        let mut merged = self.overlaid(overlay);
        if merged.integer_scale == Some(IntegerScale::NONE) {
            merged.integer_scale = None;
        }
        merged
    }
}

/// What of WebM's options a picture has.
impl From<&webm::Options> for Options {
    fn from(options: &webm::Options) -> Options {
        Options {
            resize: options.resize,
            scaling: options.scaling,
            integer_scale: options.integer_scale,
            watermark: options.watermark,
        }
    }
}

/// WebM's options with a picture's set, as planning a picture takes them.
impl From<Options> for webm::Options {
    fn from(options: Options) -> webm::Options {
        let Options { resize, scaling, integer_scale, watermark } = options;
        webm::Options { resize, scaling, integer_scale, watermark, ..webm::Options::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_what_of_webms_options_a_picture_has() {
        let webm = webm::Options {
            crf: Some(30),
            tempo: Some(2.0),
            watermark: Some(false),
            integer_scale: Some(IntegerScale::NONE),
            ..webm::Options::default()
        };
        let picture = Options::from(&webm);
        assert_eq!(crate::options::set_keys(&picture), ["integer-scale", "watermark"]);
        let back = webm::Options::from(picture.clone());
        assert_eq!(crate::options::set_keys(&back), ["integer-scale", "watermark"]);
        assert_eq!(Options::from(&back), picture);
        // every key of a picture's is WebM's, in WebM's order
        let webm_keys: Vec<String> = webm::Options::keys().collect();
        let keys: Vec<String> = Options::keys().collect();
        let in_webm: Vec<&String> = webm_keys.iter().filter(|key| keys.contains(key)).collect();
        assert_eq!(keys.iter().collect::<Vec<_>>(), in_webm);
    }

    #[test]
    fn clears_an_integer_scale_merged_over() {
        let scaled = Options { integer_scale: "2".parse().ok(), ..Options::default() };
        let none = Options { integer_scale: Some(IntegerScale::NONE), ..Options::default() };
        assert_eq!(scaled.merged(&none).integer_scale, None);
        let read: Options = serde_json::from_str(r#"{"exact-scale":"2"}"#).unwrap();
        assert_eq!(read.integer_scale, "2".parse().ok());
    }
}
