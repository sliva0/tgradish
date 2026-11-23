//! The ffmpeg the GUI converts and reads with: the one the settings or the
//! command line choose, and the one found for that choice.

use std::cell::RefCell;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use tgradish_core::backend::Backend;
use tgradish_core::config::FfmpegConfig;
use tgradish_core::ffmpeg::FfmpegChoice;

/// How long an ffmpeg found, or not found, is taken as it is.
const LOOK_AGAIN: Duration = Duration::from_secs(3);

/// Which ffmpeg the command line gave, and the one found last: the
/// settings it is chosen with are the config's, which `App` holds.
#[derive(Default)]
pub(crate) struct Ffmpeg {
    /// The ffmpeg given as the GUI was started, by the command line or
    /// `TGRADISH_FFMPEG`, over the config's for this run alone, so
    /// Settings doesn't save it.
    pub(crate) given: (Option<FfmpegChoice>, Option<PathBuf>),
    /// The ffmpeg found for the settings, and when: looking for it
    /// searches the PATH, so it is looked for again only every few
    /// seconds, to notice one installed meanwhile, or when the settings
    /// change. Asked for where `App` is only borrowed, hence the cell.
    found: RefCell<Option<FoundBackend>>,
}

impl Ffmpeg {
    /// Which ffmpeg to use and where, as the settings `config` and the
    /// command line say: as on the command line, a path beats a choice.
    pub(crate) fn settings(&self, config: &FfmpegConfig) -> (FfmpegChoice, Option<PathBuf>) {
        match &self.given {
            (choice, Some(path)) => (choice.unwrap_or(config.choice), Some(path.clone())),
            (Some(choice), None) => (*choice, None),
            (None, None) => (config.choice, config.path.clone()),
        }
    }

    /// The ffmpeg found for [`Ffmpeg::settings`], or why there is none.
    pub(crate) fn backend(&self, config: &FfmpegConfig) -> Result<Backend, String> {
        let settings = self.settings(config);
        let mut found = self.found.borrow_mut();
        match &*found {
            Some(seen) if seen.settings == settings && seen.when.elapsed() < LOOK_AGAIN => {
                seen.backend.clone()
            }
            _ => {
                let backend = Backend::select(settings.0, settings.1.as_deref())
                    .map_err(|err| crate::jobs::failure(&err));
                let when = Instant::now();
                *found = Some(FoundBackend { settings, when, backend: backend.clone() });
                backend
            }
        }
    }
}

/// The ffmpeg found for the settings `settings`, at `when`.
struct FoundBackend {
    settings: (FfmpegChoice, Option<PathBuf>),
    when: Instant,
    backend: Result<Backend, String>,
}
