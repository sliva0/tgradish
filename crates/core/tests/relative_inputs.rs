//! Inputs named from the working directory, by names ffmpeg would take
//! for something else. A test binary of its own, with one test: it changes
//! the working directory, which all of a process's threads share.

mod common;

use std::path::{Path, PathBuf};

use tgradish_core::options::{Fit, Options, Speed};
use tgradish_core::webm::{Request, convert};
use tgradish_core::{CancelToken, Error, still};

#[test]
fn reads_names_that_look_like_urls_and_options() {
    let Some(backends) = common::backends() else { return };
    let dir = tempfile::tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    let files = tgradish_corpus::files();
    let walker = files.iter().find(|file| file.name == "walker_2x.gif").unwrap();
    // a dash starts an option; and before a colon ffmpeg expects a
    // protocol, as in a time, which Windows keeps out of names
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
            // a video sticker, beside it, and the frame at a time as a picture
            let options =
                Options { fit: Some(Fit::Off), speed: Some(Speed::Fast), ..Options::default() };
            let request = Request { options, overwrite: true, ..Request::new(input.clone()) };
            let made = convert(backend, &request, &cancel, &mut |_| {});
            let made = made.unwrap_or_else(|err| panic!("{label}: {err}"));
            assert_eq!(made.output, input.with_extension("sticker.webm"), "{label}");
            assert!(made.output.exists(), "{label}");
            let options = Options { start: Some(0.1), ..Options::default() };
            let request = Request { options, overwrite: true, ..Request::new(input.clone()) };
            let made = still::convert(backend, &request, &cancel);
            assert!(made.unwrap_or_else(|err| panic!("{label}: {err}")).output.exists(), "{label}");
        }
    }
    // what can't be read is named as it was given
    let missing = Path::new("-missing.gif");
    for backend in &backends {
        let err = backend.probe(missing, &cancel).unwrap_err();
        assert!(matches!(&err, Error::Unreadable { path, .. } if path == missing), "{err}");
    }
    // out of the directory before it is removed
    std::env::set_current_dir(env!("CARGO_MANIFEST_DIR")).unwrap();
}
