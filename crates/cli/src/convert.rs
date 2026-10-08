use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use console::style;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use tgradish_core::convert::{Request, convert};
use tgradish_core::events::Event;
use tgradish_core::options::Options;
use tgradish_core::presets::{DEFAULT_PRESET, Presets};

use crate::Context;
use crate::args::ConvertArgs;
use crate::ui;

/// Preset, then `--options-json`, then flags.
fn options(ctx: &Context, args: &ConvertArgs) -> Result<Options> {
    let presets = Presets::load_user()?;
    let name = args.preset.as_deref().or(ctx.config.preset.as_deref()).unwrap_or(DEFAULT_PRESET);
    let mut options = presets.resolve(name)?;
    if let Some(json) = &args.options_json {
        let overlay: Options = serde_json::from_str(json).context("invalid --options-json")?;
        options = options.merged(&overlay);
    }
    Ok(options.merged(&args.options.to_options()))
}

pub fn run(ctx: &Context, args: ConvertArgs) -> Result<()> {
    if args.output.is_some() && args.inputs.len() > 1 {
        bail!("--output only works with a single input");
    }
    let options = options(ctx, &args)?;
    let backend = ctx.backend()?;
    if ctx.global.json {
        listen_for_cancel(ctx);
    }

    let mut failed = 0;
    for input in &args.inputs {
        let request = Request {
            input: input.clone(),
            output: args.output.clone(),
            options: options.clone(),
            overwrite: args.overwrite,
            keep_temp: args.keep_temp,
        };
        let result = if ctx.global.json {
            convert(&backend, &request, &ctx.cancel, &mut |event| {
                println!("{}", serde_json::to_string(&event).expect("events serialize"));
            })
        } else {
            let mut printer = Printer::new(ctx);
            let result = convert(&backend, &request, &ctx.cancel, &mut |e| printer.event(e));
            printer.finish();
            result
        };

        match result {
            Ok(outcome) => {
                if let Some(dir) = outcome.temp_dir
                    && !ctx.global.json
                {
                    eprintln!("intermediate files kept in {}", dir.display());
                }
            }
            Err(tgradish_core::Error::Cancelled) => {
                return Err(tgradish_core::Error::Cancelled.into());
            }
            Err(err) if args.inputs.len() > 1 => {
                failed += 1;
                if ctx.global.json {
                    crate::print_json_error(&err.into(), Some(input));
                } else {
                    eprintln!("{} {}: {err}", ui::error_label(), input.display());
                }
            }
            Err(tgradish_core::Error::OutputExists(path)) => {
                bail!("{} already exists, use --overwrite to replace it", path.display())
            }
            Err(err) => return Err(err.into()),
        }
    }
    if failed > 0 {
        if ctx.global.json {
            // each failure was already reported with its input
            return Err(crate::Exit(1).into());
        }
        bail!("{failed} of {} conversions failed", args.inputs.len());
    }
    Ok(())
}

/// Cancels when a front-end writes `cancel` to stdin. Sending Ctrl-C to a
/// child process is awkward on Windows. End of input is ignored, so running
/// without stdin is fine.
fn listen_for_cancel(ctx: &Context) {
    let cancel = ctx.cancel.clone();
    std::thread::spawn(move || {
        for line in std::io::stdin().lines().map_while(Result::ok) {
            if line.trim() == "cancel" {
                cancel.cancel();
            }
        }
    });
}

/// Shows conversion events as text with a progress bar.
struct Printer {
    verbose: u8,
    quiet: bool,
    bar: Option<ProgressBar>,
}

impl Printer {
    fn new(ctx: &Context) -> Self {
        Self { verbose: ctx.global.verbose, quiet: ctx.global.quiet, bar: None }
    }

    /// Prints a line above the progress bar.
    fn line(&self, text: impl AsRef<str>) {
        match &self.bar {
            Some(bar) => bar.println(text),
            None => eprintln!("{}", text.as_ref()),
        }
    }

    fn finish(&mut self) {
        if let Some(bar) = self.bar.take() {
            bar.finish_and_clear();
        }
    }

    fn event(&mut self, event: Event) {
        match event {
            Event::Started { plan } if !self.quiet => {
                let spoof = if plan.spoofs(plan.length) { ", spoofed" } else { "" };
                self.line(format!(
                    "{} → {}\n  {} {}x{}, {}, {}, fit {}{spoof}",
                    plan.input.display(),
                    style(plan.output.display()).bold(),
                    plan.target.name(),
                    plan.width,
                    plan.height,
                    ui::fps(plan.fps),
                    ui::seconds(plan.length),
                    ui::name(&plan.fit),
                ));
            }
            Event::AttemptStarted { attempt, params } if !self.quiet => {
                self.finish();
                let bar = ProgressBar::with_draw_target(Some(1000), ProgressDrawTarget::stderr());
                bar.set_style(
                    ProgressStyle::with_template("  {msg} [{bar:30}] {percent:>3}%")
                        .expect("valid template")
                        .progress_chars("=> "),
                );
                bar.set_message(format!("attempt {attempt}: {}", ui::params(&params)));
                bar.enable_steady_tick(Duration::from_millis(200));
                self.bar = Some(bar);
            }
            Event::Progress { pass, passes, fraction, .. } => {
                if let Some(bar) = &self.bar {
                    let done = (f64::from(pass - 1) + fraction) / f64::from(passes);
                    bar.set_position((done * 1000.0) as u64);
                }
            }
            Event::AttemptFinished { attempt, params, bytes, fits } if !self.quiet => {
                self.finish();
                let mark = if fits { style("fits").green() } else { style("too big").red() };
                eprintln!(
                    "  attempt {attempt}: {} → {}, {mark}",
                    ui::params(&params),
                    ui::size(bytes)
                );
            }
            Event::Scored { ssim, .. } if self.verbose > 0 => {
                self.line(format!("    similarity to source (SSIM): {ssim:.4}"));
            }
            Event::Warning { message } => {
                self.line(format!("{} {message}", ui::warning_label()));
            }
            Event::Log { line } if self.verbose > 1 => self.line(format!("    {line}")),
            Event::Finished { output, bytes, params, spoofed, issues, .. } => {
                self.finish();
                let spoofed = if spoofed { ", duration spoofed" } else { "" };
                println!(
                    "{} {}: {}, {}{spoofed}",
                    style("done").green().bold(),
                    output.display(),
                    ui::size(bytes),
                    ui::params(&params),
                );
                for issue in issues {
                    eprintln!("{} Telegram will reject it: {issue}", ui::warning_label());
                }
            }
            _ => {}
        }
    }
}
