//! What a `.tgs` shows, for previews: drawn from the art it was made of,
//! or read back from a file tgradish wrote, which shows the same.

use std::path::Path;

use tgradish_tgs::picture::{self, DrawError, Picture};

use crate::error::{Error, Result};
use crate::preview::Preview;

/// The longest side of a `.tgs` preview: plenty for the GUI, and within
/// what any GPU takes as a texture.
pub(crate) const PREVIEW_MAX_SIDE: u32 = 1024;

fn preview(picture: Picture) -> Preview {
    Preview { width: picture.width, height: picture.height, frames: picture.frames }
}

/// What `anim` shows, `stretch`ed square as the sticker is, or not.
pub(super) fn draw(anim: &tgradish_tgs::PixelAnim, stretch: bool) -> Preview {
    preview(picture::of_art(anim, stretch, PREVIEW_MAX_SIDE))
}

/// What the `.tgs` (or plain Lottie JSON) at `path` shows, drawn as
/// converting drew it when tgradish made it: a pixel per art pixel, each
/// frame with how long it shows. A sticker with more than tgradish's own
/// have, as paths or gradients, fails as [`Error::Undrawable`], and one
/// that can't be read as [`Error::Unreadable`]; neither is a picture that
/// isn't what it shows.
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
        use crate::options::{Crop, Effort, Resize};
        use crate::tgs::{Options, Request, convert};
        let dir = tempfile::tempdir().unwrap();
        let mut read = 0;
        for file in tgradish_corpus::files() {
            let input = dir.path().join(&file.name);
            std::fs::write(&input, &file.bytes).unwrap();
            let (width, height) = file.sprite.size();
            // whole, square three ways, and cut through art pixels
            let cut = Crop { x: 1, y: 2, width: width - 2, height: height - 3 };
            let options = [
                Options::default(),
                Options { resize: Some(Resize::Crop), ..Options::default() },
                Options { resize: Some(Resize::Stretch), ..Options::default() },
                Options { crop: Some(cut), ..Options::default() },
                Options { crop: Some(cut), resize: Some(Resize::Crop), ..Options::default() },
            ];
            for (index, options) in options.into_iter().enumerate() {
                let request = Request {
                    output: Some(dir.path().join(format!("{}.{index}.tgs", file.name))),
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
