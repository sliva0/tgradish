/// `println!` for what goes to stdout, which stops the program quietly
/// when stdout was closed, as by `tgradish licenses | head -2`, rather
/// than panicking as `println!` does: see [`write_out`].
macro_rules! outln {
    () => {
        $crate::write_out(format_args!(""), true)
    };
    ($($arg:tt)*) => {
        $crate::write_out(format_args!($($arg)*), true)
    };
}

/// [`outln`] without the line's end, as `print!`.
macro_rules! out {
    ($($arg:tt)*) => {
        $crate::write_out(format_args!($($arg)*), false)
    };
}

mod args;
mod commands;
mod convert;
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

/// musl's own malloc is slow at what tgradish does: `.tgs` stickers take up
/// to 1.6 times as long, videos 5–7% longer. mimalloc stands in for it in
/// the whole program, ffmpeg's C code too (its `override` feature), which
/// brings musl builds level with glibc's.
#[cfg(target_env = "musl")]
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// mimalloc keeps freed memory for a second before handing it back, which
/// raises the peak of a video's conversion by up to a quarter over 10 ms; at
/// 10 ms it is near glibc's, as fast. Unless MIMALLOC_PURGE_DELAY says.
#[cfg(target_env = "musl")]
#[allow(unsafe_code)]
fn return_freed_memory_sooner() {
    use libmimalloc_sys::{mi_option_get, mi_option_set, mi_option_t};
    // mi_option_purge_delay in mimalloc.h, which the bindings don't name
    const PURGE_DELAY: mi_option_t = 15;
    // its default, which also shows the number still names the delay
    const DEFAULT: std::ffi::c_long = 1000;
    if std::env::var_os("MIMALLOC_PURGE_DELAY").is_some() {
        return;
    }
    // SAFETY: mimalloc reads and writes options without synchronising, so
    // this runs first in main, before any other thread; and only once the
    // default above shows the bundled mimalloc numbers the delay as 15
    unsafe {
        if mi_option_get(PURGE_DELAY) == DEFAULT {
            mi_option_set(PURGE_DELAY, 10);
        }
    }
}

/// The exit status when everything was made or read, but Telegram would
/// refuse a result, which was said with it.
pub const REFUSED: u8 = 3;

/// Ends the program with an exit code, after the command already reported
/// why.
#[derive(Debug, thiserror::Error)]
#[error("exit with status {0}")]
pub struct Exit(pub u8);

/// The exit status of a command line that was wrong, as clap exits with.
pub const USAGE: u8 = 2;

/// A mistake in the command line found once clap had parsed it, as an
/// output the format can't be written to: said as any error is, and the
/// command exits with [`USAGE`].
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct Usage(pub String);

/// A [`Usage`] error saying `message`.
pub fn usage(message: impl Into<String>) -> anyhow::Error {
    Usage(message.into()).into()
}

/// The exit status when whoever read stdout stopped reading: 141 on Unix,
/// as shells show a program SIGPIPE ended, which Rust's programs ignore;
/// 1 elsewhere, where nothing says otherwise.
const STDOUT_CLOSED: u8 = if cfg!(unix) { 128 + 13 } else { 1 };

/// Whether a write to stdout found it closed.
static STDOUT_WAS_CLOSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn stdout_was_closed() -> bool {
    STDOUT_WAS_CLOSED.load(std::sync::atomic::Ordering::Relaxed)
}

/// How soon after Ctrl-C kept the best WebM encode so far another one
/// cancels, rather than keeping the best of the next input's.
const SETTLE_AGAIN: std::time::Duration = std::time::Duration::from_secs(2);

/// What Ctrl-C means to a command: keeping the best WebM encode so far,
/// in `convert`, or else cancelling.
#[derive(Debug)]
struct CtrlC {
    /// Whether it may keep the best so far: in `convert`.
    settles: bool,
    /// When it last did.
    settled_at: Option<std::time::Instant>,
}

/// What a Ctrl-C was taken as.
#[derive(Debug, PartialEq, Eq)]
enum Meaning {
    Settle,
    Cancel,
}

impl CtrlC {
    /// What Ctrl-C pressed at `now` means: keeping the best so far where
    /// `settle` does so, unless it did within [`SETTLE_AGAIN`] before, as
    /// then it is asked twice; cancelling otherwise, for whoever pressed
    /// it to do. `settle` is called only where it would settle, and both
    /// says whether it can and does it, so that the two are of the same
    /// moment.
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

/// The command's token, which a closed stdout cancels.
static CANCEL: std::sync::OnceLock<CancelToken> = std::sync::OnceLock::new();

/// Writes to stdout for [`outln`] and [`out`]. A closed pipe stops the
/// command, quietly: there is no one to tell, and nothing after it would
/// be read. It is cancelled rather than ended then and there, which may
/// be in the middle of a conversion's events, so that its temporary files
/// go and ffmpeg is stopped; nothing more is written, and `main` exits
/// with [`STDOUT_CLOSED`] once it is over.
pub fn write_out(text: std::fmt::Arguments<'_>, newline: bool) {
    use std::io::Write;
    use std::sync::atomic::Ordering;
    if STDOUT_WAS_CLOSED.load(Ordering::Relaxed) {
        return;
    }
    let mut stdout = std::io::stdout().lock();
    let mut written = stdout.write_fmt(text);
    if newline {
        // stdout keeps lines until their end, so this is where a closed
        // pipe shows
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

/// Prints `value` as one line of JSON, as `--json` prints events and
/// each file's findings; or if it can't be, as a config holding a path
/// that isn't Unicode can't, an error event saying why.
pub fn json_line(value: &impl serde::Serialize) {
    match serde_json::to_string(value) {
        Ok(line) => outln!("{line}"),
        Err(err) => {
            let failure = tgradish_core::protocol::Failure {
                message: format!("couldn't write this as JSON: {err}"),
                input: None,
            };
            json_line(&tgradish_core::webm::Event::Error(failure));
        }
    }
}

/// Prints an error as a JSON line, see `Event::Error`: the same in every
/// stream.
pub fn print_json_error(err: &anyhow::Error, input: Option<&std::path::Path>) {
    json_line(&tgradish_core::webm::Event::Error(tgradish_core::protocol::Failure {
        message: format!("{err:#}"),
        input: input.map(Into::into),
    }));
}

/// `err` with what to do about it on the command line, where that is
/// known: core says what is wrong, and the flags are the command line's.
pub fn explain(err: tgradish_core::Error) -> anyhow::Error {
    use anyhow::anyhow;
    use tgradish_core::{Error, FfmpegMissing};
    match err {
        Error::OutputExists(path) => {
            anyhow!("{} already exists, use --overwrite to replace it", path.display())
        }
        Error::IsFolder(_) => anyhow!("{err}: use --sequence"),
        Error::FfmpegNotFound(FfmpegMissing::NotOnPath) => {
            anyhow!("{err}: install them or pass --ffmpeg PATH")
        }
        Error::FfmpegNotFound(FfmpegMissing::NotBuiltIn) => {
            anyhow!("{err}: use --ffmpeg-from system or --ffmpeg PATH")
        }
        Error::FfmpegArgsNeedProgram => anyhow!(
            "--ffmpeg-args are arguments for ffmpeg's command line, which the built-in ffmpeg \
             doesn't have: use --ffmpeg-from system or --ffmpeg PATH, or --encoder-options for \
             encoder settings, which work with either"
        ),
        Error::NothingFits { smallest, limit } => anyhow!(
            "nothing fits the {limit} byte limit, the smallest attempt was {smallest} bytes; try \
             a shorter --length, a lower --fps or a wider --fit-range"
        ),
        err => err.into(),
    }
}

/// Settings shared by all commands.
pub struct Context {
    pub global: Global,
    /// The config file's settings, for the commands that read them, which
    /// [`Command::reads_config`] names; the defaults for the rest.
    pub config: Config,
    /// Where the config file is, or would be.
    pub config_file: Option<ConfigFile>,
    pub cancel: CancelToken,
}

/// Where the config file is, and who said so.
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
    /// Where it is unless something says otherwise.
    Default,
}

impl ConfigFile {
    /// The config file `--config` names, or else `TGRADISH_CONFIG`, or
    /// else the one in the config dir, if there is a config dir.
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

    /// Reads the file. One that isn't there gives the defaults, unless it
    /// was named, which would be a mistake: `--config` or
    /// `TGRADISH_CONFIG` naming nothing are said to.
    fn load(&self) -> Result<Config> {
        let path = self.path.display();
        let named = match self.from {
            ConfigFrom::Flag => Some(format!("--config {path} doesn't exist")),
            ConfigFrom::Env => Some(format!("{path}, from TGRADISH_CONFIG, doesn't exist")),
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

/// The ffmpeg given over the config file's, as the GUI takes it too: by
/// `--ffmpeg` or `--ffmpeg-from`, which can't be given together, or else
/// a path in `TGRADISH_FFMPEG`, which either flag beats.
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

/// Who named the ffmpeg in use, by a path to it.
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
    /// [`ffmpeg_given`] by this command line.
    fn ffmpeg_given(&self) -> (Option<FfmpegChoice>, Option<PathBuf>) {
        ffmpeg_given(self.global.ffmpeg.as_deref(), self.global.ffmpeg_from)
    }

    /// The path to ffmpeg, or its folder, that [`Context::backend`] uses,
    /// and who gave it; none where it looks on PATH or uses its own.
    pub fn ffmpeg_path(&self) -> Option<(PathBuf, FfmpegNamed)> {
        match self.ffmpeg_given() {
            (_, Some(path)) if self.global.ffmpeg.is_some() => Some((path, FfmpegNamed::Flag)),
            (_, Some(path)) => Some((path, FfmpegNamed::Env)),
            (Some(_), None) => None,
            (None, None) => Some((self.config.ffmpeg.path.clone()?, FfmpegNamed::Config)),
        }
    }

    /// Where `named` is, in words: `--ffmpeg`, `TGRADISH_FFMPEG`, or the
    /// config file's `[ffmpeg] path`.
    pub fn ffmpeg_named_by(&self, named: FfmpegNamed) -> String {
        match named {
            FfmpegNamed::Flag => "--ffmpeg".into(),
            FfmpegNamed::Env => "TGRADISH_FFMPEG".into(),
            FfmpegNamed::Config => match &self.config_file {
                Some(file) => format!("{} [ffmpeg] path", file.path.display()),
                None => "the config's [ffmpeg] path".into(),
            },
        }
    }

    /// [`explain`], and for an ffmpeg a path gave that isn't there, which
    /// path it was, and who gave it.
    pub fn explain(&self, err: tgradish_core::Error) -> anyhow::Error {
        use tgradish_core::{Error, FfmpegMissing};
        let Some((given, named)) = self.ffmpeg_path() else { return explain(err) };
        let fix = match named {
            FfmpegNamed::Env => "unset it or point it at ffmpeg or its folder",
            FfmpegNamed::Flag | FfmpegNamed::Config => "point it at ffmpeg or its folder",
        };
        let from = self.ffmpeg_named_by(named);
        let given_text = given.display();
        match err {
            Error::FfmpegNotFound(FfmpegMissing::NoProgram(_)) if given.is_dir() => {
                anyhow::anyhow!(
                    "ffmpeg not found: {given_text}, from {from}, has no ffmpeg in it: {fix}"
                )
            }
            Error::FfmpegNotFound(FfmpegMissing::NoProgram(_)) => {
                anyhow::anyhow!("ffmpeg not found: {given_text}, from {from}, doesn't exist: {fix}")
            }
            Error::FfmpegNotFound(FfmpegMissing::NoFfprobe(ffprobe)) => anyhow::anyhow!(
                "ffmpeg not found: {given_text}, from {from}, has no ffprobe beside it, as {}: \
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
            // a choice given beats a path in the config
            (Some(choice), None) => Backend::select(choice, None),
            (None, None) => Backend::select(config.choice, config.path.as_deref()),
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let config_file = ConfigFile::find(cli.global.config.as_deref());
    // read only where it is used, so a mistake in it never stops what
    // could say where it is, as `config path`, or what has nothing to do
    // with it
    let config = match &config_file {
        Some(file) if cli.command.reads_config() => file.load()?,
        _ => Config::default(),
    };
    let config_path = config_file.as_ref().map(|file| file.path.clone());

    let cancel = CANCEL.get_or_init(CancelToken::new).clone();
    // the GUI never reads the token, so Ctrl-C there ends it as usual
    if !cli.command.is_gui() {
        // convert's first Ctrl-C keeps the best WebM encode so far, where
        // one fits. ffmpeg, in the terminal's process group, gets Ctrl-C
        // too, and stops at once; core waits a moment for what it meant,
        // decided here on the signal's thread, so promptly
        let settles = matches!(cli.command, Command::Convert(_));
        let presses = std::sync::Mutex::new(CtrlC { settles, settled_at: None });
        ctrlc::set_handler({
            let cancel = cancel.clone();
            move || {
                if cancel.is_cancelled() {
                    // second Ctrl-C: stop waiting for cleanup, and ffmpeg
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
                anyhow::bail!("the GUI can't start: {why}");
            }
            let (choice, path) = ctx.ffmpeg_given();
            let first = files.first().cloned();
            let launch = tgradish_gui::Launch::default().config_path(config_path).files(files);
            let started = tgradish_gui::run(launch.ffmpeg(choice, path))
                .map_err(|err| anyhow::anyhow!("the GUI can't start: {}", gui_error(&err)))?;
            if let tgradish_gui::Started::HandedOver { files } = started
                && !ctx.global.quiet
            {
                // written as it can be: without a console, as on Windows
                // started from a shortcut, there is no one to tell
                use std::io::Write;
                let _ = writeln!(std::io::stderr(), "{}", ui::handed_over(files, first.as_deref()));
            }
            Ok(())
        }
    }
}

/// Started without arguments from a file manager, desktop entry or the
/// Start menu, rather than a terminal.
#[cfg(feature = "gui")]
fn started_outside_a_terminal() -> bool {
    use std::io::IsTerminal;
    if std::env::args_os().len() != 1 {
        return false;
    }
    #[cfg(windows)]
    if windows_console::is_our_own() {
        // Windows before 11 24H2 ignores the manifest asking for no
        // console, and the one it made would stay open behind the window
        windows_console::close();
        return true;
    }
    !std::io::stdin().is_terminal() && !std::io::stdout().is_terminal()
}

/// Why no window can open here, where that can be told without trying:
/// on Linux and the BSDs, with neither Wayland nor X to open it on.
#[cfg(feature = "gui")]
fn no_display() -> Option<&'static str> {
    if !cfg!(all(unix, not(target_os = "macos"))) {
        return None;
    }
    let set = |name: &str| std::env::var_os(name).is_some_and(|value| !value.is_empty());
    let display = ["WAYLAND_DISPLAY", "WAYLAND_SOCKET", "DISPLAY"].into_iter().any(set);
    (!display).then_some("no display (neither WAYLAND_DISPLAY nor DISPLAY is set)")
}

/// What stopped the GUI, without what winit adds of where in its
/// source it was found, a path on the machine that built tgradish.
#[cfg(feature = "gui")]
fn gui_error(err: &tgradish_gui::Error) -> String {
    without_source_place(err.to_string())
}

/// `message` without winit's "os error at FILE:LINE: ".
#[cfg_attr(not(feature = "gui"), allow(dead_code))]
fn without_source_place(message: String) -> String {
    const AT: &str = "os error at ";
    let Some(start) = message.find(AT) else { return message };
    // "os error at FILE:LINE: why", where FILE may hold colons too
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

    /// Whether the console was made for this process alone, as when it was
    /// started from Explorer rather than a terminal.
    pub fn is_our_own() -> bool {
        let mut processes = [0u32; 2];
        // SAFETY: the buffer holds as many ids as it is said to
        let count = unsafe { GetConsoleProcessList(processes.as_mut_ptr(), 2) };
        count == 1
    }

    pub fn close() {
        // SAFETY: nothing has used the console's handles yet
        unsafe { FreeConsole() };
    }
}

fn main() -> ExitCode {
    #[cfg(target_env = "musl")]
    return_freed_memory_sooner();
    // for packages: the man pages, written by `tgradish __man DIR`, which
    // isn't a command clap knows, so neither help nor completions name it
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
        // started from a script, or over ssh: there is no window to open,
        // but maybe someone to read why
        if no_display().is_some() {
            term::init(args::ColourChoice::Auto, false);
            ui::introduce(true);
            return ExitCode::from(USAGE);
        }
        let (choice, path) = ffmpeg_given(None, None);
        return match tgradish_gui::run(tgradish_gui::Launch::default().ffmpeg(choice, path)) {
            // handed over or not, there is no terminal to say so in
            Ok(_) => ExitCode::SUCCESS,
            Err(err) => {
                // there is no terminal to print to
                let err = gui_error(&err);
                tgradish_gui::show_error(&format!("tgradish couldn't start: {err}"));
                ExitCode::FAILURE
            }
        };
    }
    // `tgradish` alone, in a terminal: what it does, and that it has a GUI
    // too
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
    // whatever came of it, no one read it, so it goes unsaid
    let status = (!stdout_was_closed()).then(|| report(ran, json));
    // told only now, as saying what came of it may be what finds stdout
    // closed
    match status {
        Some(status) if !stdout_was_closed() => status,
        _ => ExitCode::from(STDOUT_CLOSED),
    }
}

/// Says what came of the command, where it failed and hasn't said so
/// itself, and gives its exit status.
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
        let never = || -> bool { panic!("asked to settle as it shouldn't") };
        // the first, where an encode fits, keeps it
        let mut convert = CtrlC { settles: true, settled_at: None };
        assert_eq!(convert.press(at(0.0), fits), Meaning::Settle);
        // again within 2 seconds cancels, though it could settle again,
        // as the next input's encode fits by then
        assert_eq!(convert.press(at(1.9), never), Meaning::Cancel);
        // after them, it keeps the best so far again
        let mut convert = CtrlC { settles: true, settled_at: None };
        assert_eq!(convert.press(at(0.0), fits), Meaning::Settle);
        assert_eq!(convert.press(at(2.1), fits), Meaning::Settle);
        // nothing fitting yet, it cancels
        let mut convert = CtrlC { settles: true, settled_at: None };
        assert_eq!(convert.press(at(0.0), || false), Meaning::Cancel);
        // as it does in other commands, whatever fits
        let mut watch = CtrlC { settles: false, settled_at: None };
        assert_eq!(watch.press(at(0.0), never), Meaning::Cancel);
    }

    /// build.rs writes Cargo.toml's version into the manifest built into
    /// tgradish.exe, with the fourth number Windows wants.
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

    /// What `tgradish gui` says where a GUI open already took its files.
    #[test]
    fn says_what_the_gui_open_already_took() {
        use std::path::Path;
        let said = |files, first: Option<&str>| super::ui::handed_over(files, first.map(Path::new));
        assert_eq!(said(0, None), "tgradish is already open: brought it to the front");
        assert_eq!(said(1, Some("frog.gif")), "tgradish is already open: added frog.gif to it");
        assert_eq!(said(3, Some("a.png")), "tgradish is already open: added 3 files to it");
    }

    /// Where in winit an error was found is no one's business but its.
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
