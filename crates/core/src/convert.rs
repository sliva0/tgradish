//! Unified conversion entry point across all three output formats.
//!
//! Dispatches a [`Request`] to the format module selected by its options,
//! reporting progress through [`Event`] and returning [`Outcome`].
//!
//! ```no_run
//! # use tgradish_core::{backend::Backend, convert, input::Input, presets::FormatOptions, CancelToken};
//! let request = convert::Request::new(
//!     Input::File("walk.gif".into()),
//!     FormatOptions::Tgs(Default::default()),
//! );
//! // Pixel art does not need ffmpeg.
//! let backend = match convert::needs_backend(&request) {
//!     true => Some(Backend::select(Default::default(), None)?),
//!     false => None,
//! };
//! let outcome = convert::convert(backend.as_ref(), &request, &CancelToken::new(), &mut |event| {
//!     println!("{event:?}")
//! })?;
//! # Ok::<(), tgradish_core::Error>(())
//! ```

use std::path::PathBuf;

use crate::backend::Backend;
use crate::config::Threads;
use crate::error::{Error, FfmpegMissing, Result};
use crate::input::Input;
use crate::presets::FormatOptions;
use crate::{CancelToken, Event, Outcome};
use crate::{tgs, webm, webp};

/// Conversion request for any output format (determined by `options`).
/// Construct with [`Request::new`].
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    /// Input file or image sequence.
    pub input: Input,
    /// Destination path; defaults to [`Input::default_output`] (see
    /// [`Request::output`]).
    pub output: Option<PathBuf>,
    /// Target format and format-specific options.
    pub options: FormatOptions,
    /// Replace an existing output file if present.
    pub overwrite: bool,
    /// Keep intermediate WebM files (see [`webm::Request::keep_temp`]).
    /// Ignored for other formats.
    pub keep_temp: bool,
    /// Maximum threads used for this conversion.
    pub threads: Threads,
}

impl Request {
    /// Creates a request writing beside `input`, without overwriting or
    /// keeping temporary files, using automatic thread count.
    pub fn new(input: Input, options: FormatOptions) -> Self {
        let (overwrite, keep_temp, threads) = (false, false, Threads::Auto);
        Self { input, output: None, options, overwrite, keep_temp, threads }
    }

    /// Returns the explicit output path, or the default path beside the input.
    pub fn output(&self) -> PathBuf {
        let (format, target) = (self.options.format(), self.options.target());
        self.output.clone().unwrap_or_else(|| self.input.default_output(format, target))
    }
}

/// Returns true if converting `request` requires ffmpeg. WebM and WebP always
/// need ffmpeg; `.tgs` needs it only for video inputs (see
/// [`tgs::needs_ffmpeg`]).
pub fn needs_backend(request: &Request) -> bool {
    match &request.options {
        FormatOptions::Webm(_) | FormatOptions::Webp(_) => true,
        FormatOptions::Tgs(_) => tgs::needs_ffmpeg(&request.input),
    }
}

/// Converts `request` by delegating to the format module selected by
/// `request.options`. Pass `backend` whenever [`needs_backend`] returns true;
/// omitting it when required returns [`Error::FfmpegNotFound`].
pub fn convert(
    backend: Option<&Backend>,
    request: &Request,
    cancel: &CancelToken,
    on_event: &mut dyn FnMut(Event),
) -> Result<Outcome> {
    let format = request.options.format();
    let backend_for = || backend.ok_or(Error::FfmpegNotFound(FfmpegMissing::ForFormat(format)));
    let (input, output, overwrite, threads) =
        (request.input.clone(), request.output.clone(), request.overwrite, request.threads);
    match &request.options {
        FormatOptions::Webm(options) => {
            let keep_temp = request.keep_temp;
            let options = options.clone();
            let request = webm::Request { input, output, options, overwrite, keep_temp, threads };
            webm::convert(backend_for()?, &request, cancel, on_event)
        }
        FormatOptions::Webp(options) => {
            let options = options.clone();
            let request = webp::Request { input, output, options, overwrite, threads };
            webp::convert(backend_for()?, &request, cancel, on_event)
        }
        FormatOptions::Tgs(options) => {
            let options = options.clone();
            let request = tgs::Request { input, output, options, overwrite, threads };
            tgs::convert(backend, &request, cancel, on_event)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::Format;

    #[test]
    fn converts_into_the_format_asked_for() {
        let dir = tempfile::tempdir().unwrap();
        let files = tgradish_corpus::files();
        let walker = files.iter().find(|file| file.name == "walker_2x.gif").unwrap();
        let input = dir.path().join("walker.gif");
        std::fs::write(&input, &walker.bytes).unwrap();
        let request =
            Request::new(Input::File(input.clone()), FormatOptions::Tgs(Default::default()));
        assert!(!needs_backend(&request));
        assert_eq!(request.output(), dir.path().join("walker.sticker.tgs"));
        let mut events = Vec::new();
        let made = convert(None, &request, &CancelToken::new(), &mut |event| events.push(event));
        let made = made.unwrap();
        assert_eq!(
            (made.format, &made.output, &made.input),
            (Format::Tgs, &request.output(), &input)
        );
        assert!(matches!(events.first(), Some(Event::Started(_))));
        assert!(matches!(events.last(), Some(Event::Finished(last)) if *last == made));
        // what ffmpeg makes isn't made without it
        let webm = Request { options: FormatOptions::Webm(Default::default()), ..request };
        assert!(needs_backend(&webm));
        let refused = convert(None, &webm, &CancelToken::new(), &mut |_| {});
        assert!(matches!(
            refused,
            Err(Error::FfmpegNotFound(FfmpegMissing::ForFormat(Format::Webm)))
        ));
    }
}
