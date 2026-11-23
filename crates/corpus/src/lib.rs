//! Synthetic test data that produces identical results on every machine with
//! no third-party assets. It contains pixel art drawn in code and saved in
//! every format tgradish reads (GIF, WebP, APNG, Aseprite, image sequences and
//! sprite sheets), plus video clips generated with ffmpeg.
//!
//! Each sprite covers a specific case found in real pixel art; see [`sprites`].
//! The sprites are small enough to generate quickly, large enough for
//! meaningful encoding tests, and use whole-centisecond delays so GIF preserves
//! their timing.

mod canvas;
mod clip;
pub mod encode;
mod sprites;

pub use canvas::{Canvas, Colour};
pub use clip::{Clip, clip, ffmpeg_on_path, ffmpeg_required, skip};

/// A test animation and its metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sprite {
    pub name: &'static str,
    pub about: &'static str,
    pub frames: Vec<Canvas>,
    /// Duration of each frame in milliseconds.
    pub delays: Vec<u32>,
    /// Input pixels per art pixel when the art sits on an exact grid.
    pub scale: u32,
    /// Dominant pixel grid when a few pixels sit off-grid.
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

    /// Total duration in milliseconds.
    pub fn length(&self) -> u32 {
        self.delays.iter().sum()
    }
}

/// Returns all animated test sprites.
pub fn sprites() -> Vec<Sprite> {
    let walker = sprites::walker();
    let doubled = (walker.0.iter().map(|frame| frame.scaled(2)).collect(), walker.1.clone());
    let (hammer, hammer_delays) = sprites::hammer();
    // Cut through the last column of art pixels, as sprite rips often do.
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

/// Returns the sprite with the given `name`, or panics if not found.
pub fn sprite(name: &str) -> Sprite {
    sprites()
        .into_iter()
        .find(|sprite| sprite.name == name)
        .unwrap_or_else(|| panic!("no sprite {name}"))
}

/// An encoded corpus file and its source sprite.
pub struct File {
    pub name: String,
    pub bytes: Vec<u8>,
    pub sprite: Sprite,
}

/// Encodes every animated sprite in a suitable format: WebP or APNG for sprites
/// with partial transparency, GIF for the rest, plus an extra Aseprite file for
/// `walker_2x` containing an `"idle"` tag on its first two frames.
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

/// Returns the image-sequence test cases: `steps` (4 frames at 27x31, with
/// frames 2 and 4 identical) and `wave` (8 frames at 28x42).
pub fn sequences() -> [(&'static str, Vec<Canvas>); 2] {
    [("steps", sprites::steps()), ("wave", sprites::wave())]
}

/// Writes all corpus files and PNG image sequences (one folder per sequence)
/// into `dir`.
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
