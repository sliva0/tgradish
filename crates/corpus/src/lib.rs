//! Synthetic test data, so every machine has the same and nothing in it
//! belongs to anyone: pixel art drawn in code, written in each format
//! tgradish reads (GIF, WebP, APNG, Aseprite, image sequences and sprite
//! sheets), and video clips made with ffmpeg.
//!
//! Each sprite stands for a case real pixel art has; [`sprites`] says
//! which. They are small, but not so small that encoding them is trivial,
//! and their timings are whole centiseconds, which GIF keeps.

mod canvas;
mod clip;
pub mod encode;
mod sprites;

pub use canvas::{Canvas, Colour};
pub use clip::{Clip, clip};

/// An animation, and what it stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sprite {
    pub name: &'static str,
    pub about: &'static str,
    pub frames: Vec<Canvas>,
    /// How long each frame shows, in milliseconds.
    pub delays: Vec<u32>,
    /// Input pixels to an art pixel, where the art is on a whole grid.
    pub scale: u32,
    /// The grid most of the art is on, where some pixels are off it.
    pub likely_scale: Option<u32>,
}

impl Sprite {
    fn new(
        name: &'static str,
        about: &'static str,
        (frames, delays): (Vec<Canvas>, Vec<u32>),
    ) -> Sprite {
        Sprite { name, about, frames, delays, scale: 1, likely_scale: None }
    }

    pub fn size(&self) -> (u32, u32) {
        (self.frames[0].width, self.frames[0].height)
    }

    /// How long it plays, in milliseconds.
    pub fn length(&self) -> u32 {
        self.delays.iter().sum()
    }
}

/// The animated sprites.
pub fn sprites() -> Vec<Sprite> {
    let walker = sprites::walker();
    let doubled = (walker.0.iter().map(|frame| frame.scaled(2)).collect(), walker.1.clone());
    let (hammer, hammer_delays) = sprites::hammer();
    // cut through the last column of art pixels, as rips often are
    let hammer = hammer.iter().map(|frame| frame.scaled(4).cropped(0, 0, 415, 276)).collect();
    vec![
        Sprite::new("walker", "a person walking: outlined and shaded, 1x", walker),
        Sprite {
            scale: 2,
            ..Sprite::new("walker_2x", "the walker scaled up twice, exactly", doubled)
        },
        Sprite {
            scale: 4,
            ..Sprite::new(
                "hammer_4x",
                "a large canvas at 4x, cut through an art pixel on the right",
                (hammer, hammer_delays),
            )
        },
        Sprite {
            likely_scale: Some(2),
            ..Sprite::new(
                "slime_jittered",
                "2x art with a few pixels off its grid, as touched up by hand",
                sprites::slime_jittered(),
            )
        },
        Sprite::new("bow", "longer than 3 seconds: 43 frames over 9.85 s", sprites::bow()),
        Sprite::new("glow", "partial alpha, as only WebP and PNG keep", sprites::glow()),
        Sprite::new("dance", "busy: much of it changes every frame", sprites::dance()),
        Sprite::new("sunrise", "heavy ordered dithering", sprites::sunrise()),
        Sprite::new(
            "waterfall",
            "palette cycling: colours move, shapes stay",
            sprites::waterfall(),
        ),
        Sprite::new("bounce", "an APNG with a partly transparent shadow", sprites::bounce()),
    ]
}

/// The sprite named `name`.
pub fn sprite(name: &str) -> Sprite {
    sprites()
        .into_iter()
        .find(|sprite| sprite.name == name)
        .unwrap_or_else(|| panic!("no sprite {name}"))
}

/// A file of the corpus: its name with the extension of its format, and
/// the sprite it holds.
pub struct File {
    pub name: String,
    pub bytes: Vec<u8>,
    pub sprite: Sprite,
}

/// Every animated sprite as a file, in the format that suits it: partial
/// alpha in WebP and APNG, the rest in GIF, and the walker also as an
/// Aseprite file with an "idle" tag of its first two frames.
pub fn files() -> Vec<File> {
    let mut files = Vec::new();
    for sprite in sprites() {
        let (extension, bytes) = match sprite.name {
            "glow" => ("webp", encode::webp(&sprite)),
            "bounce" => ("png", encode::apng(&sprite)),
            _ => ("gif", encode::gif(&sprite)),
        };
        if sprite.name == "walker_2x" {
            let ase = encode::aseprite(&sprite, &[("idle", 0, 1)]);
            files.push(File { name: "walker_2x.ase".into(), bytes: ase, sprite: sprite.clone() });
        }
        files.push(File { name: format!("{}.{extension}", sprite.name), bytes, sprite });
    }
    files
}

/// The frames of image sequences: four steps of 27 by 31, the second and
/// fourth the same, and eight frames of a wave, 28 by 42.
pub fn sequences() -> [(&'static str, Vec<Canvas>); 2] {
    [("steps", sprites::steps()), ("wave", sprites::wave())]
}

/// Writes the files and the sequences' frames, as PNG files in a folder
/// each, to `dir`.
pub fn write(dir: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    for file in files() {
        std::fs::write(dir.join(&file.name), &file.bytes)?;
    }
    for (name, frames) in sequences() {
        let folder = dir.join("sequences").join(name);
        std::fs::create_dir_all(&folder)?;
        for (index, frame) in frames.iter().enumerate() {
            std::fs::write(folder.join(format!("{}.png", index + 1)), encode::png(frame))?;
        }
    }
    Ok(())
}
