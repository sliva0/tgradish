//! Converting into any of the three formats through one call: a
//! [`Request`] with the options of the format to make, which [`convert`]
//! hands to that format's own `convert`, reporting through the same
//! [`Event`]s and returning the same [`Outcome`] as each does.
//!
//! ```no_run
//! # use tgradish_core::{backend::Backend, convert, input::Input, presets::FormatOptions, CancelToken};
//! let request = convert::Request::new(
//!     Input::File("walk.gif".into()),
//!     FormatOptions::Tgs(Default::default()),
//! );
//! // pixel art is read without ffmpeg
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

/// A conversion into any format, which its options say: built as each
/// format's own request is, from [`Request::new`].
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    /// What to convert.
    pub input: Input,
    /// Where the result goes: by default [`Input::default_output`], as
    /// [`Request::output`] gives it.
    pub output: Option<PathBuf>,
    /// What to make, and how.
    pub options: FormatOptions,
    /// Replace the output if it exists.
    pub overwrite: bool,
    /// For WebM: keep intermediate files, as
    /// [`webm::Request::keep_temp`] does; the other formats leave none.
    pub keep_temp: bool,
    /// How many threads it uses.
    pub threads: Threads,
}

impl Request {
    /// A conversion of `input` with `options`, beside it, not replacing
    /// one there, intermediate files deleted.
    pub fn new(input: Input, options: FormatOptions) -> Self {
        let (overwrite, keep_temp, threads) = (false, false, Threads::Auto);
        Self { input, output: None, options, overwrite, keep_temp, threads }
    }

    /// Where the result goes, as asked or by default.
    pub fn output(&self) -> PathBuf {
        let (format, target) = (self.options.format(), self.options.target());
        self.output.clone().unwrap_or_else(|| self.input.default_output(format, target))
    }
}

/// Whether converting `request` needs ffmpeg: WebM and WebP always do, and
/// a `.tgs` only of a video, as [`tgs::needs_ffmpeg`] says.
pub fn needs_backend(request: &Request) -> bool {
    match &request.options {
        FormatOptions::Webm(_) | FormatOptions::Webp(_) => true,
        FormatOptions::Tgs(_) => tgs::needs_ffmpeg(&request.input),
    }
}

/// Converts `request` into the format its options are for, through that
/// format's own `convert`. `backend` is ffmpeg, which only what
/// [`needs_backend`] says needs it: asked to make WebM or WebP without
/// one, this fails as [`Error::FfmpegNotFound`].
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
