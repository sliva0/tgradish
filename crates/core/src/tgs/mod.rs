//! Animated vector stickers (`.tgs`): file I/O, option resolution, preview
//! rendering and conversion wrapping `tgradish-tgs` (which operates in memory).
//! Pixel art is decoded directly; videos are read via ffmpeg at sticker frame
//! rates and treated as pixel art.

mod inspect;
mod options;
mod preview;
mod read;

pub use inspect::inspect_file;
#[cfg(test)]
pub(crate) use inspect::read_json;
pub use options::Options;
pub use preview::{Drawing, preview_file};
use read::load;
pub use read::{Art, needs_ffmpeg, read_art};

use std::path::PathBuf;
use std::time::Duration;

use tgradish_tgs::normalise;
use tgradish_tgs::sticker::{self, Progress};

use crate::CancelToken;
use crate::backend::Backend;
use crate::config::Threads;
use crate::error::{Error, Result};
use crate::events::{Details, Event, Outcome, StartDetails, Started, Warning};
use crate::input::Input;
use crate::options::{Resize, Selection};
use crate::presets::Format;
use crate::telegram;

/// Default frame rate for still images, sprite sheets and image sequences when
/// `fps` is unset.
pub const DEFAULT_FPS: f64 = 12.0;

/// Maximum `.tgs` file size accepted by Telegram, in bytes.
pub const MAX_BYTES: u64 = tgradish_tgs::limits::telegram::MAX_BYTES as u64;

/// Parameters for a `.tgs` conversion.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    /// Input file or image sequence.
    pub input: Input,
    /// Destination path; defaults to [`Input::default_output`] (see
    /// [`Request::output`]).
    pub output: Option<PathBuf>,
    /// What of the input is read, and the target and title.
    pub selection: Selection,
    /// `.tgs` conversion options.
    pub options: Options,
    /// Replace an existing output file if present.
    pub overwrite: bool,
    /// Maximum threads used for ffmpeg video decoding and `.tgs` fitting.
    pub threads: Threads,
}

impl Request {
    /// Creates a request with default options writing beside `input`, without
    /// overwriting existing files.
    pub fn new(input: Input) -> Self {
        let threads = Threads::Auto;
        let (selection, options) = (Selection::default(), Options::default());
        Self { input, output: None, selection, options, overwrite: false, threads }
    }

    /// Returns the explicit output path, or the default path beside the input.
    pub fn output(&self) -> PathBuf {
        let target = self.selection.target.unwrap_or_default();
        self.output.clone().unwrap_or_else(|| self.input.default_output(Format::Tgs, target))
    }
}

/// Converts pixel art or video into a `.tgs` sticker. `backend` is only needed
/// for video inputs (see [`needs_ffmpeg`]); pixel art is decoded without
/// ffmpeg.
pub fn convert(
    backend: Option<&Backend>,
    request: &Request,
    cancel: &CancelToken,
    on_event: &mut dyn FnMut(Event),
) -> Result<Outcome> {
    let output = request.output();
    request.input.check_output(&output, request.overwrite)?;
    let (selection, options) = (&request.selection, &request.options);
    let mut settings = options.sticker_options(selection)?;
    settings.threads = request.threads.limit().and_then(|threads| threads.try_into().ok());
    let loaded = load(backend, &request.input, selection, options, request.threads, cancel)?;
    // kept as it is, the part used must already be square
    if options.resize_or_default() == Resize::Contain {
        let animation = &loaded.animation;
        let (width, height) = match settings.normalise.crop {
            Some(crop) => (crop.width, crop.height),
            None => (animation.width(), animation.height()),
        };
        if width != height {
            let reason = format!(
                "contain keeps the {width}×{height} aspect ratio, but TGS stickers must be \
                 square; crop to a square or use pad, crop or stretch"
            );
            return Err(Error::invalid("resize", reason));
        }
    }
    if let Some(part) = &loaded.video {
        // read from the start: as long as the part used
        settings.normalise.start = Duration::ZERO;
        settings.normalise.length = part.length;
    }
    let input = request.input.first().to_path_buf();
    let target = selection.target.unwrap_or_default();
    let sticker = sticker::make(
        loaded.animation,
        &settings,
        &mut |progress| match progress {
            // what reading the art found worth saying, after it
            Progress::Normalised { report } => {
                let found = report.warnings.iter().cloned().map(Warning::Art);
                let warnings: Vec<Warning> = loaded.warnings.iter().cloned().chain(found).collect();
                on_event(Event::Started(Started {
                    input: input.clone(),
                    output: output.clone(),
                    format: Format::Tgs,
                    target,
                    limit: Format::Tgs.max_bytes(target),
                    details: StartDetails::Tgs { report },
                }));
                for message in warnings {
                    on_event(Event::Warning { message });
                }
            }
            Progress::TooLarge { bytes } => on_event(Event::TooLarge { bytes: bytes as u64 }),
            Progress::Reduced { step } => on_event(Event::Reduced { step }),
            Progress::Packing => on_event(Event::Packing),
            Progress::Missed { bytes, aim } => {
                on_event(Event::Missed { bytes: bytes.map(|bytes| bytes as u64), aim: aim as u64 })
            }
            Progress::Advanced { fraction, stage_fraction } => {
                on_event(Event::Advanced { fraction, stage_fraction })
            }
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
        input,
        output,
        format: Format::Tgs,
        target,
        bytes: sticker.bytes as u64,
        limit: Format::Tgs.max_bytes(target),
        changed,
        issues: telegram::check_lottie(sticker.issues, &sticker.stats),
        details: Details::Tgs {
            json_bytes: sticker.json_bytes as u64,
            steps: sticker.steps,
            layers: sticker.layers as u64,
            rectangles: sticker.rectangles as u64,
            drawing: Drawing::new(
                sticker.anim,
                settings.normalise.square == normalise::Square::Stretch,
            ),
        },
    };
    on_event(Event::Finished(outcome.clone()));
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn says_how_far_it_got_before_it_finishes() {
        let dir = tempfile::tempdir().unwrap();
        let files = tgradish_corpus::files();
        let walker = files.iter().find(|file| file.name == "walker.gif").unwrap();
        let input = dir.path().join("walker.gif");
        std::fs::write(&input, &walker.bytes).unwrap();
        let request = Request { overwrite: true, ..Request::new(Input::File(input)) };
        let mut events = Vec::new();
        convert(None, &request, &CancelToken::new(), &mut |event| events.push(event)).unwrap();
        let fractions: Vec<f64> = events
            .iter()
            .filter_map(|event| match event {
                Event::Advanced { fraction, .. } => Some(*fraction),
                _ => None,
            })
            .collect();
        assert!(fractions.len() > 10, "{fractions:?}");
        assert!(fractions.windows(2).all(|pair| pair[0] <= pair[1]), "{fractions:?}");
        // all of it, once started, and before the result is written
        assert!(matches!(events[0], Event::Started(_)));
        assert_eq!(fractions.last(), Some(&1.0));
        let [.., last, finished] = &events[..] else { panic!("{events:?}") };
        assert!(matches!(last, Event::Advanced { .. }) && matches!(finished, Event::Finished(_)));
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
        let padded = make(Resize::Pad).unwrap().preview().unwrap();
        assert!(padded.width < padded.height);
        for resize in [Resize::Crop, Resize::Stretch] {
            let made = make(resize).unwrap();
            assert!(made.issues.is_empty(), "{resize:?}: {:?}", made.issues);
            let preview = made.preview().unwrap();
            assert_eq!(preview.width, preview.height, "{resize:?}");
        }
        // kept as it is only where it is square already: the whole of
        // the walker, but not a crop of it that isn't
        let (width, height) = (
            u32::from(u16::from_le_bytes([walker.bytes[6], walker.bytes[7]])),
            u32::from(u16::from_le_bytes([walker.bytes[8], walker.bytes[9]])),
        );
        assert_eq!(width, height);
        let made = make(Resize::Contain).unwrap().preview().unwrap();
        assert_eq!(made.width, padded.width);
        let crop = crate::options::Crop { x: 0, y: 0, width, height: height - 2 };
        let request = Request {
            output: Some(dir.path().join("contained.tgs")),
            selection: Selection { crop: Some(crop), ..Selection::default() },
            options: Options { resize: Some(Resize::Contain), ..Options::default() },
            overwrite: true,
            ..Request::new(Input::File(input.clone()))
        };
        let refused = convert(None, &request, &CancelToken::new(), &mut |_| {});
        assert!(
            matches!(refused, Err(Error::InvalidOption { option: "resize", .. })),
            "{refused:?}"
        );
    }
}
