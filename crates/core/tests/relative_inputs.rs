//! Tests relative input filenames that ffmpeg would otherwise misinterpret as
//! flags or URL protocols. Kept in its own test binary because it changes the
//! process-wide working directory.

mod common;

use std::path::{Path, PathBuf};

use tgradish_core::options::{Effort, Fit};

use tgradish_core::input::Input;
use tgradish_core::webm::{Options, Request, convert};
use tgradish_core::{CancelToken, Error, webp};

#[test]
fn reads_names_that_look_like_urls_and_options() {
    let Some(backends) = common::backends() else { return };
    let dir = tempfile::tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    let files = tgradish_corpus::files();
    let walker = files.iter().find(|file| file.name == "walker_2x.gif").unwrap();
    // Leading dashes look like CLI flags, and colons before a path separator
    // look like URL schemes to ffmpeg (tested on Unix since Windows forbids
    // colons in filenames).
    let mut names = vec!["-dash.gif"];
    if cfg!(unix) {
        names.push("2026-11-12T10:30:00.gif");
    }
    let cancel = CancelToken::new();
    for name in names {
        std::fs::write(name, &walker.bytes).unwrap();
        let input = PathBuf::from(name);
        for backend in &backends {
            let label = format!("{} {name}", common::name(backend));
            let probe =
                backend.probe(&input, &cancel).unwrap_or_else(|err| panic!("{label}: {err}"));
            let starts = backend.frame_starts(&input, &probe, &cancel);
            let starts = starts.unwrap_or_else(|err| panic!("{label}: {err}"));
            assert!(starts.len() > 1, "{label}: {starts:?}");
            // Convert each to a WebM sticker beside it, and its frame at a
            // time to a static WebP.
            let options =
                Options { fit: Some(Fit::Off), effort: Some(Effort::Fast), ..Options::default() };
            let request =
                Request { options, overwrite: true, ..Request::new(Input::File(input.clone())) };
            let made = convert(backend, &request, &cancel, &mut |_| {});
            let made = made.unwrap_or_else(|err| panic!("{label}: {err}"));
            assert_eq!(made.output, input.with_extension("sticker.webm"), "{label}");
            assert!(made.output.exists(), "{label}");
            let options = Options { start: Some(0.1), ..Options::default() };
            let request = webp::Request {
                options,
                overwrite: true,
                ..webp::Request::new(Input::File(input.clone()))
            };
            let made = webp::convert(backend, &request, &cancel, &mut |_| {});
            assert!(made.unwrap_or_else(|err| panic!("{label}: {err}")).output.exists(), "{label}");
        }
    }
    // Errors name the path as given, not the absolute one passed to ffmpeg.
    let missing = Path::new("-missing.gif");
    for backend in &backends {
        let err = backend.probe(missing, &cancel).unwrap_err();
        assert!(matches!(&err, Error::Unreadable { path, .. } if path == missing), "{err}");
    }
    // Leave the temporary directory before it is deleted on drop.
    std::env::set_current_dir(env!("CARGO_MANIFEST_DIR")).unwrap();
}
