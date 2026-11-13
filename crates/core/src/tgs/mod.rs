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

pub use events::TgsEvent;
#[cfg(test)]
pub(crate) use inspect::read_json;
pub use inspect::{inspect_file, is_sticker};
pub use options::TgsOptions;
use read::load;
pub use read::{Source, needs_ffmpeg, read_source};

use std::path::{Path, PathBuf};
use std::time::Duration;

pub use tgradish_tgs::check::{Issue, Severity, Stats};
pub use tgradish_tgs::limits;
use tgradish_tgs::normalise;
pub use tgradish_tgs::reduce::{Compromise, Kind, Reduction};
use tgradish_tgs::sticker::{self, Progress, Step};

use crate::CancelToken;
use crate::backend::Backend;
use crate::error::{Error, Result};
use crate::preview::Preview;
use crate::source;
use crate::telegram::Target;

/// Frame rate of still images, sprite sheets and image sequences unless
/// `fps` says otherwise.
pub const DEFAULT_FPS: f64 = 12.0;

/// Largest `.tgs` Telegram accepts, in bytes.
pub const MAX_BYTES: u64 = tgradish_tgs::limits::telegram::MAX_BYTES as u64;

/// `NAME.sticker.tgs` or `NAME.emoji.tgs` next to the input.
pub fn default_output(input: &Path, target: Target) -> PathBuf {
    crate::fsutil::beside(input, &format!("{}.tgs", target.name()))
}

pub struct TgsRequest {
    /// One animation, image or video, or with `sequence` the frames of one
    /// animation (image files, or directories of them).
    pub inputs: Vec<PathBuf>,
    pub sequence: bool,
    pub output: PathBuf,
    pub options: TgsOptions,
    pub overwrite: bool,
    /// ffmpeg, which reads videos; pixel art doesn't need it.
    pub backend: Option<Backend>,
}

/// What converting made: all that [`TgsEvent::Finished`] says, whether or
/// not events were followed, and a preview.
#[derive(Debug, Clone)]
pub struct TgsOutcome {
    pub output: PathBuf,
    pub bytes: u64,
    /// Size of the Lottie JSON inside.
    pub json_bytes: u64,
    /// Whether anything visible was given up to fit, see
    /// [`TgsEvent::Finished`].
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

/// Fails when the output is one of the inputs, even through another path
/// or a link.
fn refuse_overwriting_inputs(request: &TgsRequest) -> Result<()> {
    let Ok(output) = request.output.canonicalize() else { return Ok(()) };
    let inputs = if request.sequence || request.inputs.len() > 1 {
        source::sequence_files(&request.inputs)?
    } else {
        request.inputs.clone()
    };
    for input in inputs {
        if input.canonicalize().is_ok_and(|input| input == output) {
            return Err(Error::OutputIsInput(request.output.clone()));
        }
    }
    Ok(())
}

/// Converts pixel art into a `.tgs` sticker.
pub fn convert(
    request: &TgsRequest,
    cancel: &CancelToken,
    on_event: &mut dyn FnMut(TgsEvent),
) -> Result<TgsOutcome> {
    if request.output.exists() && !request.overwrite {
        return Err(Error::OutputExists(request.output.clone()));
    }
    refuse_overwriting_inputs(request)?;
    let mut settings = request.options.sticker_options()?;
    let loaded = load(
        &request.inputs,
        request.sequence,
        &request.options,
        request.backend.as_ref(),
        cancel,
    )?;
    if let Some(part) = &loaded.video {
        // read from the start: as long as the part used
        settings.normalise.start = Duration::ZERO;
        settings.normalise.length = part.length;
    }
    let input = request.inputs.first().cloned().unwrap_or_default();
    let sticker = sticker::make(
        &loaded.animation,
        &settings,
        &mut |progress| match progress {
            // what reading the art found worth saying, after it
            Progress::Normalised { report } => {
                let found = report.warnings.iter().map(|w| w.to_string());
                let warnings: Vec<String> = loaded.warnings.iter().cloned().chain(found).collect();
                on_event(TgsEvent::Started { input: input.clone(), report });
                for message in warnings {
                    on_event(TgsEvent::Warning { message });
                }
            }
            Progress::TooLarge { bytes } => on_event(TgsEvent::TooLarge { bytes: bytes as u64 }),
            Progress::Reduced { step } => on_event(TgsEvent::Reduced { step }),
            Progress::Packing => on_event(TgsEvent::Packing),
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
    crate::fsutil::write_output(&request.output, &sticker.tgs, request.overwrite)?;
    let outcome = TgsOutcome {
        output: request.output.clone(),
        bytes: sticker.bytes as u64,
        json_bytes: sticker.json_bytes as u64,
        changed,
        steps: sticker.steps,
        layers: sticker.layers,
        rectangles: sticker.rectangles,
        issues: sticker.issues,
        preview: Preview::of(
            &sticker.anim,
            settings.normalise.square == normalise::Square::Stretch,
        ),
    };
    on_event(TgsEvent::Finished {
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
        let request = |output: PathBuf| TgsRequest {
            inputs: vec![dir.path().to_path_buf()],
            sequence: true,
            output,
            options: TgsOptions::default(),
            overwrite: true,
            backend: None,
        };
        let err = convert(&request(input.clone()), &CancelToken::new(), &mut |_| {}).unwrap_err();
        assert!(matches!(err, Error::OutputIsInput(_)), "{err}");
        assert_eq!(std::fs::read(&input).unwrap(), b"not even a png");
        // the same file by another path
        let other = dir.path().join(".").join("art.png");
        assert!(convert(&request(other), &CancelToken::new(), &mut |_| {}).is_err());
    }

    #[test]
    fn makes_art_square_three_ways() {
        let dir = tempfile::tempdir().unwrap();
        let files = tgradish_corpus::files();
        let walker = files.iter().find(|file| file.name == "walker_2x.gif").unwrap();
        let input = dir.path().join("walker.gif");
        std::fs::write(&input, &walker.bytes).unwrap();
        let make = |resize| {
            let options = TgsOptions { resize: Some(resize), ..TgsOptions::default() };
            let request = TgsRequest {
                inputs: vec![input.clone()],
                sequence: false,
                output: dir.path().join(format!("{resize:?}.tgs")),
                options,
                overwrite: true,
                backend: None,
            };
            convert(&request, &CancelToken::new(), &mut |_| {})
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
