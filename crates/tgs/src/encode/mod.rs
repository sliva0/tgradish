//! Encoders: [`PixelAnim`] in, [`Scene`] out.

mod cover;
mod mask;
mod painter;

pub use painter::{Effort, EncodeError};
#[cfg(feature = "internals")]
pub use painter::{Settings, painter};
#[cfg(not(feature = "internals"))]
pub(crate) use painter::{Settings, painter};

#[cfg(any(test, feature = "internals"))]
use crate::normalise::PixelAnim;
#[cfg(any(test, feature = "internals"))]
use crate::scene::{Group, Layer, Scene, Shape};

/// The simplest correct encoding, a baseline for tests and benchmarks:
/// every frame is its own layer, with one group per colour made of the
/// colour's horizontal runs. It breaks the seam invariant wherever two
/// colours meet.
#[cfg(any(test, feature = "internals"))]
pub fn runs(anim: &PixelAnim) -> Scene {
    let width = anim.width();
    let mut layers = Vec::new();
    let mut start = 0;
    for frame in anim.frames() {
        // palette indices fit u16, the palette's length may not
        let mut groups: Vec<Group> = (0..anim.palette().len())
            .map(|colour| Group { colour: colour as u16, ..Group::default() })
            .collect();
        for (y, row) in frame.pixels.chunks_exact(width as usize).enumerate() {
            let mut x = 0;
            while x < row.len() {
                let colour = row[x];
                let run = row[x..].iter().take_while(|&&c| c == colour).count();
                if colour != 0 {
                    groups[colour as usize].shapes.push(Shape::Rect {
                        x: x as u32,
                        y: y as u32,
                        width: run as u32,
                        height: 1,
                    });
                }
                x += run;
            }
        }
        groups.retain(|group| !group.shapes.is_empty());
        let end = start + frame.ticks;
        layers.push(Layer { from: start, to: end, groups, hidden: Vec::new() });
        start = end;
    }
    Scene { width, height: anim.height(), ticks: start, layers }
}

/// The painter on the synthetic pixel art of `tgradish-corpus`, which
/// stands for the cases real art has. Here rather than with the crate's
/// other tests of it, which can't see the painter without `internals`.
#[cfg(test)]
mod tests {
    use tgradish_frames::{DecodeOptions, Limits, decode};

    use super::*;
    use crate::check::{Severity, check};
    use crate::layout::lay_out;
    use crate::lottie::Style;
    use crate::normalise::{Options, normalise};

    /// Each file of the corpus normalised, with its name.
    fn corpus() -> Vec<(String, PixelAnim)> {
        let options = (DecodeOptions::default(), Limits::default());
        tgradish_corpus::files()
            .into_iter()
            .map(|file| {
                let input = decode(&file.bytes, &options.0, &options.1)
                    .unwrap_or_else(|err| panic!("{}: {err}", file.name));
                (file.name, normalise(&input, &Options::default()).unwrap().0)
            })
            .collect()
    }

    #[test]
    fn encodes_valid_lottie() {
        for (name, anim) in corpus() {
            let fast = Settings { effort: Effort::Fast, ..Default::default() };
            let scene = painter(&anim, &fast, None, &|| false).unwrap();
            assert_eq!(scene.compare(&anim), None, "{name}");
            let json =
                lay_out(&scene, &anim, Some("tgradish".into()), false).to_json(Style::default());
            let (stats, issues) = check(json.as_bytes(), None).unwrap();
            let errors: Vec<_> = issues.iter().filter(|i| i.severity == Severity::Error).collect();
            assert!(errors.is_empty(), "{name}: {errors:?}");
            assert_eq!(stats.frames, f64::from(anim.ticks()));
        }
    }

    #[test]
    fn painter_is_exact_without_seams() {
        for (name, anim) in corpus() {
            for lifetimes in [true, false] {
                let settings = Settings { lifetimes, ..Settings::default() };
                let scene = painter(&anim, &settings, None, &|| false).unwrap();
                assert_eq!(scene.compare(&anim), None, "{name} ({lifetimes})");
                let seams = scene.seams(anim.palette());
                assert!(
                    seams.is_empty(),
                    "{name} ({lifetimes}): {:?}",
                    &seams[..seams.len().min(5)]
                );
            }
        }
    }
}
