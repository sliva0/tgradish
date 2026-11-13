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

/// The exit status when whoever read stdout stopped reading: 141 on Unix,
/// as shells show a program SIGPIPE ended, which Rust's programs ignore;
/// 1 elsewhere, where nothing says otherwise.
const STDOUT_CLOSED: u8 = if cfg!(unix) { 128 + 13 } else { 1 };

/// Whether a write to stdout found it closed.
static STDOUT_WAS_CLOSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn stdout_was_closed() -> bool {
    STDOUT_WAS_CLOSED.load(std::sync::atomic::Ordering::Relaxed)
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
    pub config: Config,
    pub cancel: CancelToken,
}

/// The ffmpeg given over the config file's, as the window takes it too: by
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

impl Context {
    /// [`ffmpeg_given`] by this command line.
    fn ffmpeg_given(&self) -> (Option<FfmpegChoice>, Option<PathBuf>) {
        ffmpeg_given(self.global.ffmpeg.as_deref(), self.global.ffmpeg_from)
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
    let config_path = cli.global.config.clone().or_else(Config::default_path);
    let config = match &config_path {
        Some(path) => Config::load(path)?,
        None => Config::default(),
    };

    let cancel = CANCEL.get_or_init(CancelToken::new).clone();
    // the window never reads the token, so Ctrl-C there ends it as usual
    if !cli.command.is_window() {
        ctrlc::set_handler({
            let cancel = cancel.clone();
            move || {
                if cancel.is_cancelled() {
                    // second Ctrl-C: stop waiting for cleanup
                    std::process::exit(130);
                }
                cancel.cancel();
            }
        })?;
    }

    let ctx = Context { global: cli.global, config, cancel };
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
        #[cfg(feature = "gui")]
        Command::Gui => {
            let (choice, path) = ctx.ffmpeg_given();
            let launch = tgradish_gui::Launch::default().config_path(config_path);
            tgradish_gui::run(launch.ffmpeg(choice, path)).map_err(|err| anyhow::anyhow!("{err}"))
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
    #[cfg(feature = "gui")]
    if started_outside_a_terminal() {
        let (choice, path) = ffmpeg_given(None, None);
        return match tgradish_gui::run(tgradish_gui::Launch::default().ffmpeg(choice, path)) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                // there is no terminal to print to
                tgradish_gui::show_error(&format!("The window couldn't open: {err}"));
                ExitCode::FAILURE
            }
        };
    }
    // `tgradish` alone, in a terminal: what it does, and that a window opens
    // too
    if std::env::args_os().len() == 1 {
        term::init(args::ColourChoice::Auto, false);
        ui::introduce();
        return if stdout_was_closed() { ExitCode::from(STDOUT_CLOSED) } else { ExitCode::SUCCESS };
    }
    let cli = Cli::parse();
    if let Some(misused) = cli.misused() {
        use clap::CommandFactory;
        Cli::command().error(clap::error::ErrorKind::ArgumentConflict, misused).exit();
    }
    term::init(cli.global.colour, cli.global.json);
    if !cli.command.is_window() {
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
    if cancelled { ExitCode::from(130) } else { ExitCode::FAILURE }
}

#[cfg(test)]
mod tests {
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
}
