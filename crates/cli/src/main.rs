mod args;
mod commands;
mod convert;
mod term;
mod ui;
mod watch;

use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;
use tgradish_core::backend::Backend;
use tgradish_core::config::Config;
use tgradish_core::ffmpeg::CancelToken;

use crate::args::{Cli, Command, Global};

/// Ends the program with an exit code, after the command already reported
/// why.
#[derive(Debug, thiserror::Error)]
#[error("exit with status {0}")]
pub struct Exit(pub u8);

/// Prints an error as a JSON line, see `Event::Error`.
pub fn print_json_error(err: &anyhow::Error, input: Option<&std::path::Path>) {
    let event = tgradish_core::events::Event::Error {
        message: format!("{err:#}"),
        input: input.map(Into::into),
    };
    println!("{}", serde_json::to_string(&event).expect("events serialize"));
}

/// Settings shared by all commands.
pub struct Context {
    pub global: Global,
    pub config: Config,
    pub cancel: CancelToken,
}

impl Context {
    pub fn backend(&self) -> Result<Backend> {
        // an explicit choice on the command line beats a path in the config
        let (choice, path) = match (self.global.ffmpeg_from, &self.global.ffmpeg) {
            (_, Some(path)) => (self.config.ffmpeg.choice, Some(path)),
            (Some(choice), None) => (choice.into(), None),
            (None, None) => (self.config.ffmpeg.choice, self.config.ffmpeg.path.as_ref()),
        };
        Ok(Backend::select(choice, path.map(|p| p.as_path()))?)
    }
}

fn run(cli: Cli) -> Result<()> {
    let config_path = cli.global.config.clone().or_else(Config::default_path);
    let config = match &config_path {
        Some(path) => Config::load(path)?,
        None => Config::default(),
    };

    let cancel = CancelToken::new();
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

    let ctx = Context { global: cli.global, config, cancel };
    match cli.command {
        Command::Convert(args) => convert::run(&ctx, args),
        Command::Watch(args) => watch::run(&ctx, args),
        Command::Spoof(args) => commands::spoof(&ctx, args),
        Command::Inspect(args) => commands::inspect(&ctx, args),
        Command::Describe => commands::describe(&ctx),
        Command::Preset(command) => commands::preset(&ctx, command),
        Command::Config(command) => commands::config(&ctx, command, config_path),
        Command::Ffmpeg(command) => commands::ffmpeg(&ctx, command),
        Command::Licenses => {
            commands::licenses();
            Ok(())
        }
        #[cfg(feature = "gui")]
        Command::Gui => tgradish_gui::run(tgradish_gui::Launch {
            config_path,
            ffmpeg_choice: ctx.global.ffmpeg_from.map(Into::into),
            ffmpeg_path: ctx.global.ffmpeg.clone(),
        })
        .map_err(|err| anyhow::anyhow!("{err}")),
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
    if console::is_our_own() {
        // Windows before 11 24H2 ignores the manifest asking for no
        // console, and the one it made would stay open behind the window
        console::close();
        return true;
    }
    !std::io::stdin().is_terminal() && !std::io::stdout().is_terminal()
}

#[cfg(all(windows, feature = "gui"))]
#[allow(unsafe_code)]
mod console {
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
    #[cfg(feature = "gui")]
    if started_outside_a_terminal() {
        return match tgradish_gui::run(tgradish_gui::Launch::default()) {
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
        term::init(args::ColorChoice::Auto, false);
        ui::introduce();
        return ExitCode::SUCCESS;
    }
    let cli = Cli::parse();
    term::init(cli.global.color, cli.global.json);
    if !matches!(cli.command, Command::Gui) {
        tgradish_core::mark::set_client(tgradish_core::mark::Client::Cli);
    }
    let json = cli.global.json;
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
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
    }
}
