//! Renders `.tgs` previews either directly from the normalised pixel art or by
//! reading back a `.tgs` file written by tgradish.

use std::path::Path;
use std::sync::Arc;

use tgradish_tgs::PixelAnim;
use tgradish_tgs::picture::{self, DrawError, Picture};

use crate::error::{Error, Result};
use crate::preview::Preview;

/// Maximum side length of a `.tgs` preview (large enough for the GUI and safe
/// for any GPU texture limit).
pub(crate) const PREVIEW_MAX_SIDE: u32 = 1024;

fn preview(picture: Picture) -> Preview {
    Preview { width: picture.width, height: picture.height, frames: picture.frames }
}

/// Renders `anim` into a preview, stretched square when `stretch` is true.
fn draw(anim: &PixelAnim, stretch: bool) -> Preview {
    preview(picture::of_art(anim, stretch, PREVIEW_MAX_SIDE))
}

/// Deferred `.tgs` preview storing the compact [`PixelAnim`] instead of full
/// RGBA frames. Calling [`Drawing::draw`] produces the same frames as
/// [`preview_file`].
#[derive(Clone)]
pub struct Drawing {
    anim: Arc<PixelAnim>,
    /// True when the sticker stretches non-square art to fill the square
    /// canvas.
    stretch: bool,
}

impl Drawing {
    /// Stores `anim` for deferred preview rendering.
    pub(crate) fn new(anim: PixelAnim, stretch: bool) -> Drawing {
        Drawing { anim: Arc::new(anim), stretch }
    }

    /// Renders straight-RGBA frames (one pixel per art pixel) with tick
    /// durations.
    pub fn draw(&self) -> Preview {
        draw(&self.anim, self.stretch)
    }
}

impl PartialEq for Drawing {
    fn eq(&self, other: &Drawing) -> bool {
        self.stretch == other.stretch
            && (Arc::ptr_eq(&self.anim, &other.anim) || self.anim == other.anim)
    }
}

/// Omits frame pixels so `Debug` output stays short.
impl std::fmt::Debug for Drawing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Drawing")
            .field("width", &self.anim.width())
            .field("height", &self.anim.height())
            .field("stretch", &self.stretch)
            .finish_non_exhaustive()
    }
}

/// Renders a `.tgs` (or plain Lottie JSON) file at `path` for preview at one
/// pixel per art pixel (e.g. a 16×16 sticker scaled 32× into a 512×512 canvas
/// is rendered at 16×16). Returns [`Error::Undrawable`] for Lottie files using
/// features outside tgradish's rectangle subset (such as paths or gradients),
/// and [`Error::Unreadable`] for malformed or unreadable files. It never
/// returns a preview that differs from what the sticker shows.
pub fn preview_file(path: &Path) -> Result<Preview> {
    let (json, _) = super::inspect::read_lottie(path)?;
    match picture::of_lottie(&json, PREVIEW_MAX_SIDE) {
        Ok(picture) => Ok(preview(picture)),
        Err(DrawError::Unsupported(what)) => {
            Err(Error::Undrawable { path: path.to_path_buf(), what })
        }
        Err(err) => Err(Error::Unreadable { path: path.to_path_buf(), message: err.to_string() }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use tgradish_tgs::frames;

    #[test]
    fn previews_a_pixel_per_art_pixel() {
        let anim = |width: u32, height: u32, colour: &dyn Fn(u32, u32) -> [u8; 4]| {
            let rgba =
                (0..height).flat_map(|y| (0..width).flat_map(move |x| colour(x, y))).collect();
            let frame = frames::Frame { rgba, duration: Duration::from_millis(100) };
            let animation = frames::Animation::new(width, height, vec![frame]).unwrap();
            tgradish_tgs::normalise(animation.clone(), &Default::default()).unwrap().0
        };
        let (red, blue) = ([255, 0, 0, 255], [0, 0, 255, 255]);
        // 3x2 art at 4x
        let art = anim(12, 8, &|x, y| if (x / 4 + y / 4) % 2 == 0 { red } else { blue });
        let preview = draw(&art, false);
        assert_eq!((preview.width, preview.height), (3, 2));
        assert_eq!(preview.frames[0].0, [red, blue, red, blue, red, blue].concat());
        // stretched square: its rows twice as high
        let preview = draw(&art, true);
        assert_eq!((preview.width, preview.height), (3, 3));
        let rows = [[red, blue, red], [blue, red, blue], [blue, red, blue]];
        assert_eq!(preview.frames[0].0, rows.concat().concat());
        // too wide to show a pixel per art pixel
        let wide = anim(4000, 2, &|x, _| if x % 2 == 0 { red } else { blue });
        let preview = draw(&wide, false);
        assert_eq!((preview.width, preview.height), (1000, 1));
        assert_eq!(preview.frames[0].0.len(), 1000 * 4);
    }

    #[test]
    fn reads_back_what_converting_drew() {
        use crate::options::{Crop, Effort, Resize, Selection};
        use crate::tgs::{Options, Request, convert};
        let dir = tempfile::tempdir().unwrap();
        let mut read = 0;
        for file in tgradish_corpus::files() {
            let input = dir.path().join(&file.name);
            std::fs::write(&input, &file.bytes).unwrap();
            let (width, height) = file.sprite.size();
            // whole, square three ways, and cut through art pixels
            let cut = Crop { x: 1, y: 2, width: width - 2, height: height - 3 };
            let resize = |resize| Options { resize: Some(resize), ..Options::default() };
            let whole = Selection::default();
            let cropped = Selection { crop: Some(cut), ..Selection::default() };
            let options = [
                (whole.clone(), Options::default()),
                (whole.clone(), resize(Resize::Crop)),
                (whole, resize(Resize::Stretch)),
                (cropped.clone(), Options::default()),
                (cropped, resize(Resize::Crop)),
            ];
            for (index, (selection, options)) in options.into_iter().enumerate() {
                let request = Request {
                    output: Some(dir.path().join(format!("{}.{index}.tgs", file.name))),
                    selection,
                    options: Options { effort: Some(Effort::Fast), ..options },
                    overwrite: true,
                    ..Request::new(crate::input::Input::File(input.clone()))
                };
                let made =
                    convert(None, &request, &crate::CancelToken::new(), &mut |_| {}).unwrap();
                let preview = preview_file(&made.output).unwrap();
                assert_eq!(Some(preview), made.preview(), "{} {index}", file.name);
                read += 1;
            }
        }
        assert!(read >= 30, "{read}");
    }

    #[test]
    fn says_why_a_file_cant_be_shown() {
        let dir = tempfile::tempdir().unwrap();
        // another program's sticker, a circle, as plain Lottie JSON
        let circle = br#"{"v":"5.7.2","fr":60,"ip":0,"op":60,"w":512,"h":512,"layers":[{"ty":4,"ks":{},"ip":0,"op":60,"shapes":[{"ty":"gr","it":[{"ty":"el","p":{"k":[256,256]},"s":{"k":[200,200]}},{"ty":"fl","c":{"k":[1,0,0]}},{"ty":"tr"}]}]}]}"#;
        let path = dir.path().join("circle.tgs");
        std::fs::write(&path, circle).unwrap();
        let err = preview_file(&path).unwrap_err();
        assert!(matches!(&err, Error::Undrawable { what, .. } if what == "ellipses"), "{err}");
        // and what is no sticker at all
        let broken = dir.path().join("broken.tgs");
        std::fs::write(&broken, b"\x1f\x8b\x08 and no more").unwrap();
        assert!(matches!(preview_file(&broken), Err(Error::Unreadable { .. })));
        std::fs::write(&broken, b"{\"layers\":").unwrap();
        assert!(matches!(preview_file(&broken), Err(Error::Unreadable { .. })));
        assert!(preview_file(&dir.path().join("gone.tgs")).is_err());
    }
}
