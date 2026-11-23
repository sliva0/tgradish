/// Like `println!`, but stops the command quietly if stdout is closed (for
/// example `tgradish licenses | head -2`) instead of panicking. See
/// [`write_out`].
macro_rules! outln {
    () => {
        $crate::write_out(format_args!(""), true)
    };
    ($($arg:tt)*) => {
        $crate::write_out(format_args!($($arg)*), true)
    };
}

/// Like [`outln!`], without a trailing newline (equivalent to `print!`).
macro_rules! out {
    ($($arg:tt)*) => {
        $crate::write_out(format_args!($($arg)*), false)
    };
}

mod args;
mod commands;
mod convert;
#[cfg(unix)]
mod signals;
mod term;
mod ui;
mod watch;

use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use tgradish_core::CancelToken;
use tgradish_core::backend::Backend;
use tgradish_core::config::Config;
use tgradish_core::ffmpeg::FfmpegChoice;

use crate::args::{Cli, Command, Global};

/// Replaces musl's allocator with mimalloc (including ffmpeg's C allocations
/// via the `override` feature). musl's default malloc makes `.tgs` encoding up
/// to 1.6× slower and video encoding 5–7% slower; mimalloc matches glibc speed.
#[cfg(target_env = "musl")]
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// Lowers mimalloc's purge delay from 1000 ms to 10 ms unless
/// `MIMALLOC_PURGE_DELAY` is set. Holding freed pages for a full second raises
/// peak memory during video conversion by up to 25% over 10 ms; 10 ms keeps
/// peak memory close to glibc with no speed loss.
#[cfg(target_env = "musl")]
#[allow(unsafe_code)]
fn return_freed_memory_sooner() {
    use libmimalloc_sys::{mi_option_get, mi_option_set, mi_option_t};
    // `mi_option_purge_delay` in mimalloc.h (not named in the Rust bindings).
    const PURGE_DELAY: mi_option_t = 15;
    // Expected default, used to verify option 15 still maps to purge delay.
    const DEFAULT: std::ffi::c_long = 1000;
    if std::env::var_os("MIMALLOC_PURGE_DELAY").is_some() {
        return;
    }
    // SAFETY: mimalloc options are unsynchronised, so this runs at the start of
    // main before spawning any threads, and only after verifying the default.
    unsafe {
        if mi_option_get(PURGE_DELAY) == DEFAULT {
            mi_option_set(PURGE_DELAY, 10);
        }
    }
}

/// Exit status when conversion or inspection succeeds, but Telegram would
/// reject the file.
pub const REFUSED: u8 = 3;

/// Sentinel error that exits with the given status code after the command has
/// already printed its diagnostics.
#[derive(Debug, thiserror::Error)]
#[error("exit with status {0}")]
pub struct Exit(pub u8);

/// Exit status for invalid CLI usage (matching clap).
pub const USAGE: u8 = 2;

/// CLI usage error discovered after clap parsing (for example an invalid output
/// path). Printed like other errors and exits with [`USAGE`].
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct Usage(pub String);

/// Wraps `message` in a [`Usage`] error.
pub fn usage(message: impl Into<String>) -> anyhow::Error {
    Usage(message.into()).into()
}

/// Exit status when stdout closes early: 141 (`128 + SIGPIPE`) on Unix,
/// matching how shells report a process killed by SIGPIPE (which Rust programs
/// ignore), or 1 elsewhere.
const STDOUT_CLOSED: u8 = if cfg!(unix) { 128 + 13 } else { 1 };

/// Set to true once a write to stdout fails with `BrokenPipe`.
static STDOUT_WAS_CLOSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn stdout_was_closed() -> bool {
    STDOUT_WAS_CLOSED.load(std::sync::atomic::Ordering::Relaxed)
}

/// Window after settling on Ctrl-C during which a second Ctrl-C cancels instead
/// of settling the next file.
const SETTLE_AGAIN: std::time::Duration = std::time::Duration::from_secs(2);

/// Tracks Ctrl-C state: `convert` settles on the best WebM encode so far on the
/// first press; other commands cancel immediately.
#[derive(Debug)]
struct CtrlC {
    /// Whether Ctrl-C may settle instead of cancelling (true for `convert`).
    settles: bool,
    /// Timestamp of the last settle action.
    settled_at: Option<std::time::Instant>,
}

/// Action chosen for a Ctrl-C press.
#[derive(Debug, PartialEq, Eq)]
enum Meaning {
    Settle,
    Cancel,
}

impl CtrlC {
    /// Decides whether a Ctrl-C at `now` should settle or cancel.
    ///
    /// Calls `settle` only when settling is allowed and at least
    /// [`SETTLE_AGAIN`] has elapsed since the previous settle. `settle` checks
    /// availability and triggers settling atomically.
    fn press(&mut self, now: std::time::Instant, settle: impl FnOnce() -> bool) -> Meaning {
        let again =
            self.settled_at.is_some_and(|at| now.saturating_duration_since(at) < SETTLE_AGAIN);
        if self.settles && !again && settle() {
            self.settled_at = Some(now);
            return Meaning::Settle;
        }
        Meaning::Cancel
    }
}

/// Process-wide cancellation token, also triggered when stdout closes.
static CANCEL: std::sync::OnceLock<CancelToken> = std::sync::OnceLock::new();

/// Writes formatted text to stdout for [`outln!`] and [`out!`].
///
/// If stdout returns `BrokenPipe`, marks stdout closed so nothing more is
/// written (quietly, since no reader remains) and cancels the active operation
/// so ffmpeg stops and temporary files are cleaned up before `main` exits with
/// [`STDOUT_CLOSED`].
pub fn write_out(text: std::fmt::Arguments<'_>, newline: bool) {
    use std::io::Write;
    use std::sync::atomic::Ordering;
    if STDOUT_WAS_CLOSED.load(Ordering::Relaxed) {
        return;
    }
    let mut stdout = std::io::stdout().lock();
    let mut written = stdout.write_fmt(text);
    if newline {
        // Line-buffered stdout flushes on newline, where BrokenPipe surfaces.
        written = written.and_then(|()| stdout.write_all(b"\n"));
    }
    match written {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::BrokenPipe => {
            STDOUT_WAS_CLOSED.store(true, Ordering::Relaxed);
            if let Some(cancel) = CANCEL.get() {
                cancel.cancel();
            }
        }
        Err(err) => panic!("failed printing to stdout: {err}"),
    }
}

/// Prints `value` as a single JSON line. If serialisation fails (for example
/// on a non-Unicode path), prints a JSON error event instead.
pub fn json_line(value: &impl serde::Serialize) {
    match serde_json::to_string(value) {
        Ok(line) => outln!("{line}"),
        Err(err) => {
            let failure = tgradish_core::protocol::Failure {
                message: format!("failed to serialise as JSON: {err}"),
                input: None,
            };
            json_line(&tgradish_core::Event::Error(failure));
        }
    }
}

/// Prints an error as a JSON `Event::Error` line.
pub fn print_json_error(err: &anyhow::Error, input: Option<&std::path::Path>) {
    json_line(&tgradish_core::Event::Error(tgradish_core::protocol::Failure {
        message: format!("{err:#}"),
        input: input.map(Into::into),
    }));
}

/// Adds CLI-specific hints (such as flag names) to a core error.
pub fn explain(err: tgradish_core::Error) -> anyhow::Error {
    use anyhow::anyhow;
    use tgradish_core::{Error, FfmpegMissing};
    match err {
        Error::OutputExists(path) => {
            anyhow!("{} already exists; pass --overwrite to replace it", path.display())
        }
        Error::IsFolder(_) => anyhow!("{err}: pass --sequence"),
        Error::FfmpegNotFound(FfmpegMissing::NotOnPath) => {
            anyhow!("{err}; install them or pass --ffmpeg PATH")
        }
        Error::FfmpegNotFound(FfmpegMissing::NotBuiltIn) => {
            anyhow!("{err}; pass --ffmpeg-from system or --ffmpeg PATH")
        }
        Error::FfmpegArgsNeedProgram => anyhow!(
            "--ffmpeg-args requires an external ffmpeg executable (pass --ffmpeg-from system or \
             --ffmpeg PATH), or use --encoder-options for encoder settings, which work with either"
        ),
        Error::NothingFits { smallest, limit } => anyhow!(
            "could not fit within {limit} bytes (smallest attempt was {smallest} bytes); try a \
             shorter --length, lower --fps or wider --fit-range"
        ),
        err => err.into(),
    }
}

/// Settings shared by all commands.
pub struct Context {
    pub global: Global,
    /// Loaded config settings for commands where [`Command::reads_config`] is
    /// true, or defaults for other commands.
    pub config: Config,
    /// Resolved config file path and source, if any.
    pub config_file: Option<ConfigFile>,
    pub cancel: CancelToken,
}

/// Resolved config file path and where it was specified.
#[derive(Debug, Clone)]
pub struct ConfigFile {
    pub path: PathBuf,
    pub from: ConfigFrom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigFrom {
    /// `--config`.
    Flag,
    /// `TGRADISH_CONFIG`.
    Env,
    /// Default user config directory path.
    Default,
}

impl ConfigFile {
    /// Resolves the config file from `--config`, then `TGRADISH_CONFIG`, then
    /// the default config directory.
    fn find(flag: Option<&std::path::Path>) -> Option<ConfigFile> {
        let env = std::env::var_os("TGRADISH_CONFIG").filter(|path| !path.is_empty());
        match (flag, env) {
            (Some(path), _) => Some(ConfigFile { path: path.into(), from: ConfigFrom::Flag }),
            (None, Some(path)) => Some(ConfigFile { path: path.into(), from: ConfigFrom::Env }),
            (None, None) => {
                let path = Config::default_path()?;
                Some(ConfigFile { path, from: ConfigFrom::Default })
            }
        }
    }

    /// Loads the config file. A missing default config returns defaults,
    /// whereas an explicitly named file (`--config` or `TGRADISH_CONFIG`) that
    /// does not exist returns an error.
    fn load(&self) -> Result<Config> {
        let path = self.path.display();
        let named = match self.from {
            ConfigFrom::Flag => Some(format!("config file from --config does not exist: {path}")),
            ConfigFrom::Env => {
                Some(format!("config file from TGRADISH_CONFIG does not exist: {path}"))
            }
            ConfigFrom::Default => None,
        };
        if let Some(missing) = named
            && !self.path.exists()
        {
            anyhow::bail!("{missing}");
        }
        Ok(Config::load(&self.path)?)
    }
}

/// Resolves the ffmpeg override above the config file (shared by the CLI and
/// GUI): `--ffmpeg` or `--ffmpeg-from` (mutually exclusive), then
/// `TGRADISH_FFMPEG`.
fn ffmpeg_given(
    path: Option<&std::path::Path>,
    from: Option<args::FfmpegFrom>,
) -> (Option<FfmpegChoice>, Option<PathBuf>) {
    match (path, from) {
        (Some(path), _) => (None, Some(path.to_path_buf())),
        (None, Some(from)) => (Some(from.into()), None),
        (None, None) => {
            let env = std::env::var_os("TGRADISH_FFMPEG").filter(|path| !path.is_empty());
            (None, env.map(PathBuf::from))
        }
    }
}

/// Source of an explicit ffmpeg path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FfmpegNamed {
    /// `--ffmpeg`.
    Flag,
    /// `TGRADISH_FFMPEG`.
    Env,
    /// The config file's `[ffmpeg] path`.
    Config,
}

impl Context {
    /// Returns [`ffmpeg_given`] for this invocation's global flags.
    fn ffmpeg_given(&self) -> (Option<FfmpegChoice>, Option<PathBuf>) {
        ffmpeg_given(self.global.ffmpeg.as_deref(), self.global.ffmpeg_from)
    }

    /// Returns the explicit ffmpeg path (or directory) used by
    /// [`Context::backend`] and its source, or `None` when searching `PATH` or
    /// using the built-in backend.
    pub fn ffmpeg_path(&self) -> Option<(PathBuf, FfmpegNamed)> {
        match self.ffmpeg_given() {
            (_, Some(path)) if self.global.ffmpeg.is_some() => Some((path, FfmpegNamed::Flag)),
            (_, Some(path)) => Some((path, FfmpegNamed::Env)),
            (Some(_), None) => None,
            (None, None) => Some((self.config.ffmpeg.path.clone()?, FfmpegNamed::Config)),
        }
    }

    /// Formats `named` for display (`--ffmpeg`, `TGRADISH_FFMPEG`, or the
    /// config file's `[ffmpeg] path`).
    pub fn ffmpeg_named_by(&self, named: FfmpegNamed) -> String {
        match named {
            FfmpegNamed::Flag => "--ffmpeg".into(),
            FfmpegNamed::Env => "TGRADISH_FFMPEG".into(),
            FfmpegNamed::Config => match &self.config_file {
                Some(file) => format!("{}'s [ffmpeg] path", file.path.display()),
                None => "config [ffmpeg] path".into(),
            },
        }
    }

    /// Like [`explain`], also attributing missing explicit ffmpeg paths to the
    /// flag, environment variable, or config setting that supplied them.
    pub fn explain(&self, err: tgradish_core::Error) -> anyhow::Error {
        use tgradish_core::{Error, FfmpegMissing};
        let Some((given, named)) = self.ffmpeg_path() else { return explain(err) };
        let fix = match named {
            FfmpegNamed::Env => "unset it or point it to ffmpeg or its directory",
            FfmpegNamed::Flag | FfmpegNamed::Config => "point it to ffmpeg or its directory",
        };
        let from = self.ffmpeg_named_by(named);
        let given_text = given.display();
        match err {
            Error::FfmpegNotFound(FfmpegMissing::NoProgram(_)) if given.is_dir() => {
                anyhow::anyhow!(
                    "ffmpeg not found: {given_text} (from {from}) contains no ffmpeg executable; \
                     {fix}"
                )
            }
            Error::FfmpegNotFound(FfmpegMissing::NoProgram(_)) => {
                anyhow::anyhow!(
                    "ffmpeg not found: {given_text} (from {from}) does not exist; {fix}"
                )
            }
            Error::FfmpegNotFound(FfmpegMissing::NoFfprobe(ffprobe)) => anyhow::anyhow!(
                "ffmpeg not found: no ffprobe beside {given_text} (from {from}, expected {}); \
                 {fix}",
                ffprobe.display()
            ),
            err => explain(err),
        }
    }

    pub fn backend(&self) -> tgradish_core::Result<Backend> {
        let config = &self.config.ffmpeg;
        match self.ffmpeg_given() {
            (_, Some(path)) => Backend::select(config.choice, Some(&path)),
            // Explicit --ffmpeg-from overrides [ffmpeg] path in the config.
            (Some(choice), None) => Backend::select(choice, None),
            (None, None) => Backend::select(config.choice, config.path.as_deref()),
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let config_file = ConfigFile::find(cli.global.config.as_deref());
    // Load the config file only for commands that use it, so a broken config
    // never blocks commands like `config path` or `completions`.
    let config = match &config_file {
        Some(file) if cli.command.reads_config() => file.load()?,
        _ => Config::default(),
    };
    let config_path = config_file.as_ref().map(|file| file.path.clone());

    let cancel = CANCEL.get_or_init(CancelToken::new).clone();
    // The GUI does not use CancelToken; it handles SIGINT/SIGTERM via
    // stop_on_termination.
    if !cli.command.is_gui() {
        // On convert's first Ctrl-C, settle on the best fitting WebM encode so
        // far if one exists. Because ffmpeg shares the terminal's process
        // group, it receives SIGINT immediately; core waits briefly for the
        // signal handler thread to record whether Ctrl-C meant settle or
        // cancel.
        let settles = matches!(cli.command, Command::Convert(_));
        let presses = std::sync::Mutex::new(CtrlC { settles, settled_at: None });
        ctrlc::set_handler({
            let cancel = cancel.clone();
            move || {
                if cancel.is_cancelled() {
                    // Ctrl-C after cancelling: exit now, without waiting for
                    // cleanup or for ffmpeg to stop.
                    cancel.cancel();
                    std::process::exit(130);
                }
                let mut presses = presses.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                let now = std::time::Instant::now();
                if presses.press(now, || cancel.settle()) == Meaning::Cancel {
                    cancel.cancel();
                }
            }
        })?;
    }
    #[cfg(unix)]
    stop_on_termination((!cli.command.is_gui()).then(|| cancel.clone()))?;

    let ctx = Context { global: cli.global, config, config_file, cancel };
    match cli.command {
        Command::Convert(args) => convert::run(&ctx, args),
        Command::Watch(args) => watch::run(&ctx, args),
        Command::Spoof(args) => commands::spoof(&ctx, args),
        Command::Inspect(args) => commands::inspect(&ctx, args),
        Command::Describe => commands::describe(&ctx),
        Command::Preset(command) => commands::preset(&ctx, command),
        Command::Config(command) => commands::config(&ctx, command, config_path),
        Command::Ffmpeg { command } => {
            commands::ffmpeg(&ctx, command.unwrap_or(args::FfmpegCommand::Status))
        }
        Command::Licenses => {
            commands::licenses(&ctx);
            Ok(())
        }
        Command::Completions { shell } => {
            commands::completions(shell);
            Ok(())
        }

        #[cfg(feature = "gui")]
        Command::Gui { files } => {
            if let Some(why) = no_display() {
                anyhow::bail!("could not start the GUI: {why}");
            }
            let (choice, path) = ctx.ffmpeg_given();
            let first = files.first().cloned();
            let launch = tgradish_gui::Launch::default().config_path(config_path).files(files);
            let started = tgradish_gui::run(launch.ffmpeg(choice, path))
                .map_err(|err| anyhow::anyhow!("could not start the GUI: {}", gui_error(&err)))?;
            if let tgradish_gui::Started::HandedOver { files } = started
                && !ctx.global.quiet
            {
                // Best-effort write: on Windows without an attached console
                // (e.g. launched from a shortcut), stderr may be unavailable.
                use std::io::Write;
                let _ = writeln!(std::io::stderr(), "{}", ui::handed_over(files, first.as_deref()));
            }
            Ok(())
        }
    }
}

/// Installs Unix termination signal handlers (see [`signals`]).
///
/// Cancels `cancel` for CLI commands, or closes the GUI window (also handling
/// SIGINT) when `cancel` is `None`. `main` re-raises the signal once shutdown
/// finishes.
#[cfg(unix)]
fn stop_on_termination(cancel: Option<CancelToken>) -> std::io::Result<()> {
    signals::on_termination(cancel.is_none(), move || match cancel {
        Some(cancel) => cancel.cancel(),
        #[cfg(feature = "gui")]
        None => tgradish_gui::close(),
        #[cfg(not(feature = "gui"))]
        None => {}
    })
}

/// Returns true when launched with no arguments outside a terminal (for example
/// from a file manager, desktop entry, or Start menu).
#[cfg(feature = "gui")]
fn started_outside_a_terminal() -> bool {
    use std::io::IsTerminal;
    if std::env::args_os().len() != 1 {
        return false;
    }
    #[cfg(windows)]
    if windows_console::is_our_own() {
        // Windows before 11 24H2 ignores the manifest and allocates a console;
        // free it so an empty console window does not stay open behind the GUI.
        windows_console::close();
        return true;
    }
    !std::io::stdin().is_terminal() && !std::io::stdout().is_terminal()
}

/// Returns an explanation when no display server is available on Linux/BSD.
#[cfg(feature = "gui")]
fn no_display() -> Option<&'static str> {
    if !cfg!(all(unix, not(target_os = "macos"))) {
        return None;
    }
    let set = |name: &str| std::env::var_os(name).is_some_and(|value| !value.is_empty());
    let display = ["WAYLAND_DISPLAY", "WAYLAND_SOCKET", "DISPLAY"].into_iter().any(set);
    (!display).then_some("no display found (neither WAYLAND_DISPLAY nor DISPLAY is set)")
}

/// Formats a GUI startup error without winit's internal source file path.
#[cfg(feature = "gui")]
fn gui_error(err: &tgradish_gui::Error) -> String {
    without_source_place(err.to_string())
}

/// Strips winit's `"os error at FILE:LINE: "` prefix from `message`.
#[cfg_attr(not(feature = "gui"), allow(dead_code))]
fn without_source_place(message: String) -> String {
    const AT: &str = "os error at ";
    let Some(start) = message.find(AT) else { return message };
    // FILE may contain colons (for example Windows drive letters).
    let rest = &message[start + AT.len()..];
    let why = rest.char_indices().find_map(|(index, _)| {
        let (place, why) = rest[index..].split_once(": ")?;
        place.parse::<u32>().ok().map(|_| why)
    });
    match why {
        Some(why) => format!("{}{why}", &message[..start]),
        None => message,
    }
}

#[cfg(all(windows, feature = "gui"))]
#[allow(unsafe_code)]
mod windows_console {
    use windows_sys::Win32::System::Console::{FreeConsole, GetConsoleProcessList};

    /// Returns true if this process owns the console alone (for example when
    /// launched from Explorer rather than a terminal).
    pub fn is_our_own() -> bool {
        let mut processes = [0u32; 2];
        // SAFETY: buffer length matches the count passed to
        // GetConsoleProcessList.
        let count = unsafe { GetConsoleProcessList(processes.as_mut_ptr(), 2) };
        count == 1
    }

    pub fn close() {
        // SAFETY: no console handles have been used yet.
        unsafe { FreeConsole() };
    }
}

fn main() -> ExitCode {
    #[cfg(target_env = "musl")]
    return_freed_memory_sooner();
    // Internal entry point for package builds (`tgradish __man DIR`), handled
    // before clap so it stays omitted from `--help` and shell completions.
    let given: Vec<std::ffi::OsString> = std::env::args_os().collect();
    if let [_, man, dir] = given.as_slice()
        && man == "__man"
    {
        return match commands::man(std::path::Path::new(dir)) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("error: {err:#}");
                ExitCode::FAILURE
            }
        };
    }
    #[cfg(feature = "gui")]
    if started_outside_a_terminal() {
        // When invoked with no arguments and no display (e.g. from a script or
        // over SSH), print the CLI introduction to stderr and exit with USAGE.
        if no_display().is_some() {
            term::init(args::ColourChoice::Auto, false);
            ui::introduce(true);
            return ExitCode::from(USAGE);
        }
        let (choice, path) = ffmpeg_given(None, None);
        // Ignore registration errors; signals will still terminate the process.
        #[cfg(unix)]
        let _ = stop_on_termination(None);
        let opened = tgradish_gui::run(tgradish_gui::Launch::default().ffmpeg(choice, path));
        #[cfg(unix)]
        signals::end_by_received();
        return match opened {
            // No terminal is attached to report handover status.
            Ok(_) => ExitCode::SUCCESS,
            Err(err) => {
                // Show a native dialog because no terminal is attached.
                let err = gui_error(&err);
                tgradish_gui::show_error(&format!("could not start tgradish: {err}"));
                ExitCode::FAILURE
            }
        };
    }
    // `tgradish` with no arguments in a terminal prints a short overview and
    // mentions `tgradish gui`.
    if std::env::args_os().len() == 1 {
        term::init(args::ColourChoice::Auto, false);
        ui::introduce(false);
        return if stdout_was_closed() { ExitCode::from(STDOUT_CLOSED) } else { ExitCode::SUCCESS };
    }
    let cli = Cli::parse();
    if let Some(misused) = cli.misused() {
        use clap::CommandFactory;
        Cli::command().error(clap::error::ErrorKind::ArgumentConflict, misused).exit();
    }
    term::init(cli.global.colour, cli.global.json);
    if !cli.command.is_gui() {
        tgradish_core::mark::set_client(tgradish_core::mark::Client::Cli);
    }
    let json = cli.global.json;
    let ran = run(cli);
    // If terminated by a signal, re-raise it immediately without printing an
    // error.
    #[cfg(unix)]
    signals::end_by_received();
    // Skip error output if stdout already closed (e.g. piped into `head`).
    let status = (!stdout_was_closed()).then(|| report(ran, json));
    // Re-check `stdout_was_closed()` in case `report` itself hit BrokenPipe.
    match status {
        Some(status) if !stdout_was_closed() => status,
        _ => ExitCode::from(STDOUT_CLOSED),
    }
}

/// Prints any unreported command error and returns the corresponding exit code.
fn report(ran: Result<()>, json: bool) -> ExitCode {
    let Err(err) = ran else { return ExitCode::SUCCESS };
    if let Some(Exit(code)) = err.downcast_ref() {
        return ExitCode::from(*code);
    }
    let cancelled = matches!(err.downcast_ref(), Some(tgradish_core::Error::Cancelled));
    if json {
        print_json_error(&err, None);
    } else {
        eprintln!("{} {err:#}", ui::error_label());
    }
    if cancelled {
        ExitCode::from(130)
    } else if err.downcast_ref::<Usage>().is_some() {
        ExitCode::from(USAGE)
    } else {
        ExitCode::FAILURE
    }
}

#[cfg(test)]
mod tests {
    use super::{CtrlC, Meaning};
    use std::time::{Duration, Instant};

    #[test]
    fn ctrl_c_keeps_the_best_so_far_unless_pressed_again_soon() {
        let start = Instant::now();
        let at = |secs: f64| start + Duration::from_secs_f64(secs);
        let fits = || true;
        let never = || -> bool { panic!("asked to settle when it should not") };
        // First press when an encode fits settles on the best so far.
        let mut convert = CtrlC { settles: true, settled_at: None };
        assert_eq!(convert.press(at(0.0), fits), Meaning::Settle);
        // Second press within 2 s cancels even if the next input already fits.
        assert_eq!(convert.press(at(1.9), never), Meaning::Cancel);
        // After 2 s, Ctrl-C can settle again on a subsequent file.
        let mut convert = CtrlC { settles: true, settled_at: None };
        assert_eq!(convert.press(at(0.0), fits), Meaning::Settle);
        assert_eq!(convert.press(at(2.1), fits), Meaning::Settle);
        // If no attempt fits yet, Ctrl-C cancels.
        let mut convert = CtrlC { settles: true, settled_at: None };
        assert_eq!(convert.press(at(0.0), || false), Meaning::Cancel);
        // Other commands always cancel on the first press.
        let mut watch = CtrlC { settles: false, settled_at: None };
        assert_eq!(watch.press(at(0.0), never), Meaning::Cancel);
    }

    /// `build.rs` embeds `Cargo.toml`'s version into `tgradish.exe.manifest` in
    /// Windows four-part `X.Y.Z.0` format.
    #[test]
    fn windows_manifest_has_the_version() {
        let manifest = include_str!(concat!(env!("OUT_DIR"), "/tgradish.versioned.manifest"));
        let version = format!(
            "name=\"tgradish\" version=\"{}.{}.{}.0\"",
            env!("CARGO_PKG_VERSION_MAJOR"),
            env!("CARGO_PKG_VERSION_MINOR"),
            env!("CARGO_PKG_VERSION_PATCH")
        );
        assert!(manifest.contains(&version), "tgradish.exe.manifest should have {version}");
    }

    /// Tests the stderr message printed when `tgradish gui` hands files to an
    /// already running GUI instance.
    #[test]
    fn says_what_the_gui_open_already_took() {
        use std::path::Path;
        let said = |files, first: Option<&str>| super::ui::handed_over(files, first.map(Path::new));
        assert_eq!(said(0, None), "tgradish is already running: brought window to the front");
        assert_eq!(said(1, Some("frog.gif")), "tgradish is already running: added frog.gif to it");
        assert_eq!(said(3, Some("a.png")), "tgradish is already running: added 3 files to it");
    }

    /// Strips winit's internal `os error at FILE:LINE: ` prefix from GUI
    /// startup errors.
    #[test]
    fn says_why_the_gui_failed_without_winits_source() {
        let raw = "winit EventLoopError: os error at /home/me/.cargo/registry/src/c:d/winit-0.30.13/\
                   src/platform_impl/linux/mod.rs:765: neither WAYLAND_DISPLAY nor DISPLAY: set";
        assert_eq!(
            super::without_source_place(raw.into()),
            "winit EventLoopError: neither WAYLAND_DISPLAY nor DISPLAY: set"
        );
        assert_eq!(super::without_source_place("no GPU".into()), "no GPU");
    }
}
