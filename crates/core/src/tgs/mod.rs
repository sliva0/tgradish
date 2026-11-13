//! Animated stickers (`.tgs`): options, events and the reading and
//! writing of files around `tgradish-tgs`, which works on frames in
//! memory only. Pixel art is read by tgradish itself; videos, read
//! through ffmpeg, are taken as pixel art too, their frames at the rate
//! a sticker can play. Previews of what was made are drawn here as well.

mod events;
mod inspect;
mod options;
mod preview;
mod read;

pub use events::Event;
pub use inspect::inspect_file;
#[cfg(test)]
pub(crate) use inspect::read_json;
pub use options::Options;
use read::load;
pub use read::{Art, needs_ffmpeg, read_art};

use std::path::PathBuf;
use std::time::Duration;

use tgradish_tgs::check::Issue;
use tgradish_tgs::normalise;
use tgradish_tgs::sticker::{self, Progress, Step};

use crate::CancelToken;
use crate::backend::Backend;
use crate::error::{Error, Result};
use crate::input::Input;
use crate::presets::Format;
use crate::preview::Preview;

/// Frame rate of still images, sprite sheets and image sequences unless
/// `fps` says otherwise.
pub const DEFAULT_FPS: f64 = 12.0;

/// Largest `.tgs` Telegram accepts, in bytes.
pub const MAX_BYTES: u64 = tgradish_tgs::limits::telegram::MAX_BYTES as u64;

/// A sticker to make.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub input: Input,
    /// Where the result goes: by default [`Input::default_output`], as
    /// [`Request::output`] gives it.
    pub output: Option<PathBuf>,
    pub options: Options,
    /// Replace the output if it exists.
    pub overwrite: bool,
}

impl Request {
    pub fn new(input: Input) -> Self {
        Self { input, output: None, options: Options::default(), overwrite: false }
    }

    /// Where the result goes, as asked or by default.
    pub fn output(&self) -> PathBuf {
        let target = self.options.target.unwrap_or_default();
        self.output.clone().unwrap_or_else(|| self.input.default_output(Format::Tgs, target))
    }
}

/// What converting made: all that [`Event::Finished`] says, whether or
/// not events were followed, and a preview.
#[derive(Debug, Clone)]
pub struct Outcome {
    pub output: PathBuf,
    pub bytes: u64,
    /// Size of the Lottie JSON inside.
    pub json_bytes: u64,
    /// Whether anything visible was given up to fit, see
    /// [`Event::Finished`].
    pub changed: bool,
    /// Reductions fitting applied, in order.
    pub steps: Vec<Step>,
    pub layers: usize,
    pub rectangles: usize,
    /// Problems Telegram would still have with the result.
    pub issues: Vec<Issue>,
    /// What the sticker shows, for previews.
    pub preview: Preview,
}

/// Converts pixel art into a `.tgs` sticker. `backend` is ffmpeg, which
/// only a video needs, as [`needs_ffmpeg`] tells: pixel art is read
/// without it.
pub fn convert(
    backend: Option<&Backend>,
    request: &Request,
    cancel: &CancelToken,
    on_event: &mut dyn FnMut(Event),
) -> Result<Outcome> {
    let output = request.output();
    request.input.check_output(&output, request.overwrite)?;
    let mut settings = request.options.sticker_options()?;
    let loaded = load(backend, &request.input, &request.options, cancel)?;
    if let Some(part) = &loaded.video {
        // read from the start: as long as the part used
        settings.normalise.start = Duration::ZERO;
        settings.normalise.length = part.length;
    }
    let input = request.input.first().to_path_buf();
    let sticker = sticker::make(
        &loaded.animation,
        &settings,
        &mut |progress| match progress {
            // what reading the art found worth saying, after it
            Progress::Normalised { report } => {
                let found = report.warnings.iter().map(|w| w.to_string());
                let warnings: Vec<String> = loaded.warnings.iter().cloned().chain(found).collect();
                on_event(Event::Started { input: input.clone(), report });
                for message in warnings {
                    on_event(Event::Warning { message });
                }
            }
            Progress::TooLarge { bytes } => on_event(Event::TooLarge { bytes: bytes as u64 }),
            Progress::Reduced { step } => on_event(Event::Reduced { step }),
            Progress::Packing => on_event(Event::Packing),
            // what core has no event for yet isn't said
            _ => {}
        },
        &|| cancel.is_cancelled(),
    )
    .map_err(|err| match err {
        tgradish_tgs::Error::Cancelled => Error::Cancelled,
        err => err.into(),
    })?;
    let changed = sticker.lossy();
    // stopped as the last of it was packed: nothing is written
    if cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }
    crate::fsutil::write_output(&output, &sticker.tgs, request.overwrite)?;
    let outcome = Outcome {
        output,
        bytes: sticker.bytes as u64,
        json_bytes: sticker.json_bytes as u64,
        changed,
        steps: sticker.steps,
        layers: sticker.layers,
        rectangles: sticker.rectangles,
        issues: sticker.issues,
        preview: preview::draw(
            &sticker.anim,
            settings.normalise.square == normalise::Square::Stretch,
        ),
    };
    on_event(Event::Finished {
        output: outcome.output.clone(),
        bytes: outcome.bytes,
        json_bytes: outcome.json_bytes,
        changed,
        steps: outcome.steps.clone(),
        layers: outcome.layers as u64,
        rectangles: outcome.rectangles as u64,
        issues: outcome.issues.clone(),
    });
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::Resize;

    #[test]
    fn refuses_to_overwrite_inputs() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("art.png");
        std::fs::write(&input, b"not even a png").unwrap();
        let request = |output: PathBuf| Request {
            output: Some(output),
            overwrite: true,
            ..Request::new(Input::Sequence(vec![dir.path().to_path_buf()]))
        };
        let convert = |request: &Request| convert(None, request, &CancelToken::new(), &mut |_| {});
        let err = convert(&request(input.clone())).unwrap_err();
        assert!(matches!(err, Error::OutputIsInput(_)), "{err}");
        assert_eq!(std::fs::read(&input).unwrap(), b"not even a png");
        // the same file by another path
        let other = dir.path().join(".").join("art.png");
        assert!(convert(&request(other)).is_err());
    }

    #[test]
    fn makes_art_square_three_ways() {
        let dir = tempfile::tempdir().unwrap();
        let files = tgradish_corpus::files();
        let walker = files.iter().find(|file| file.name == "walker_2x.gif").unwrap();
        let input = dir.path().join("walker.gif");
        std::fs::write(&input, &walker.bytes).unwrap();
        let make = |resize| {
            let options = Options { resize: Some(resize), ..Options::default() };
            let request = Request {
                output: Some(dir.path().join(format!("{resize:?}.tgs"))),
                options,
                overwrite: true,
                ..Request::new(Input::File(input.clone()))
            };
            convert(None, &request, &CancelToken::new(), &mut |_| {})
        };
        // taller than wide: padded keeps its shape, the others are square
        let padded = make(Resize::Pad).unwrap();
        assert!(padded.preview.width < padded.preview.height);
        for resize in [Resize::Crop, Resize::Stretch] {
            let made = make(resize).unwrap();
            assert!(made.issues.is_empty(), "{resize:?}: {:?}", made.issues);
            assert_eq!(made.preview.width, made.preview.height, "{resize:?}");
        }
        assert!(matches!(
            make(Resize::Contain),
            Err(Error::InvalidOption { option: "resize", .. })
        ));
    }
}
