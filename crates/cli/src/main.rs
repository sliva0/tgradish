mod args;
mod commands;
mod convert;
mod ui;

use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;
use tgradish_core::config::Config;
use tgradish_core::ffmpeg::{self, CancelToken, Ffmpeg};

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
    pub fn ffmpeg(&self) -> Result<Ffmpeg> {
        let path = self.global.ffmpeg.as_ref().or(self.config.ffmpeg.path.as_ref());
        let choice = self.global.ffmpeg_from.map_or(self.config.ffmpeg.choice, Into::into);
        Ok(ffmpeg::locate(choice, path.map(|p| p.as_path()))?)
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
        Command::Spoof(args) => commands::spoof(&ctx, args),
        Command::Inspect(args) => commands::inspect(&ctx, args),
        Command::Describe => commands::describe(&ctx),
        Command::Preset(command) => commands::preset(&ctx, command),
        Command::Config(command) => commands::config(&ctx, command, config_path),
        Command::Ffmpeg(command) => commands::ffmpeg(&ctx, command),
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
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
