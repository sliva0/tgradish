use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use console::style;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use tgradish_core::backend::Backend;
use tgradish_core::convert::{Outcome, Request, convert, default_output};
use tgradish_core::events::Event;
use tgradish_core::options::Options;
use tgradish_core::presets::{DEFAULT_PRESET, Presets};

use crate::Context;
use crate::args::{ConversionArgs, ConvertArgs};
use crate::ui;

/// Runs conversions with the options and output settings of one command.
pub struct Converter<'a> {
    ctx: &'a Context,
    args: &'a ConversionArgs,
    backend: Backend,
    options: Options,
    /// Outputs written by this command, and their inputs, so two inputs
    /// with the same name never overwrite each other's result.
    written: RefCell<HashMap<PathBuf, PathBuf>>,
}

impl<'a> Converter<'a> {
    pub fn new(ctx: &'a Context, args: &'a ConversionArgs) -> Result<Self> {
        // preset, then --options-json, then flags
        let presets = Presets::load_user()?;
        let name =
            args.preset.as_deref().or(ctx.config.preset.as_deref()).unwrap_or(DEFAULT_PRESET);
        let mut options = presets.resolve(name)?;
        if let Some(json) = &args.options_json {
            let overlay: Options = serde_json::from_str(json).context("invalid --options-json")?;
            options = options.merged(&overlay);
        }
        let options = options.merged(&args.options.to_options());
        if let Some(dir) = &args.output_dir {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("could not create {}", dir.display()))?;
        }
        if ctx.global.json {
            listen_for_cancel(ctx);
        }
        Ok(Self { ctx, args, backend: ctx.backend()?, options, written: Default::default() })
    }

    /// Where the result for `input` goes, unless `-o` says otherwise.
    pub fn output_for(&self, input: &Path) -> PathBuf {
        self.output_under(input, None)
    }

    /// Like [`Converter::output_for`], but keeps the directories between
    /// `base` and the input when writing to `--output-dir`.
    pub fn output_under(&self, input: &Path, base: Option<&Path>) -> PathBuf {
        let output = default_output(input, self.options.target.unwrap_or_default());
        let (Some(dir), Some(name)) = (&self.args.output_dir, output.file_name()) else {
            return output;
        };
        let relative =
            base.and_then(|base| input.parent()?.strip_prefix(base).ok()).unwrap_or(Path::new(""));
        dir.join(relative).join(name)
    }

    pub fn overwrite(&self) -> bool {
        self.args.overwrite
    }

    /// Converts one input, printing events as text or JSON.
    pub fn run(
        &self,
        input: &Path,
        output: Option<PathBuf>,
        overwrite: bool,
    ) -> tgradish_core::Result<Outcome> {
        let output = output.unwrap_or_else(|| self.output_for(input));
        let key = std::path::absolute(&output).unwrap_or_else(|_| output.clone());
        let input_key = std::path::absolute(input).unwrap_or_else(|_| input.to_path_buf());
        if let Some(other) = self.written.borrow().get(&key)
            && *other != input_key
        {
            return Err(tgradish_core::Error::InvalidOptions(format!(
                "{} was already written from {}; inputs with the same name need \
                 different output directories",
                output.display(),
                other.display()
            )));
        }
        if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let request = Request {
            input: input.to_path_buf(),
            output: Some(output),
            options: self.options.clone(),
            overwrite,
            keep_temp: self.args.keep_temp,
        };
        let cancel = &self.ctx.cancel;
        let result = if self.ctx.global.json {
            convert(&self.backend, &request, cancel, &mut |event| {
                println!("{}", serde_json::to_string(&event).expect("events serialize"));
            })
        } else {
            let mut printer = Printer::new(self.ctx);
            let result = convert(&self.backend, &request, cancel, &mut |e| printer.event(e));
            printer.finish();
            result
        };
        if let Ok(outcome) = &result {
            self.written.borrow_mut().insert(key, input_key);
            if let Some(dir) = &outcome.temp_dir
                && !self.ctx.global.json
            {
                eprintln!("intermediate files kept in {}", dir.display());
            }
        }
        result
    }

    /// Reports a failed conversion of one of several inputs.
    pub fn report_failure(&self, input: &Path, err: tgradish_core::Error) {
        let err = explain(err);
        if self.ctx.global.json {
            crate::print_json_error(&err, Some(input));
        } else {
            eprintln!("{} {}: {err:#}", ui::error_label(), input.display());
        }
    }
}

/// Adds hints to errors where the CLI knows what to do about them.
fn explain(err: tgradish_core::Error) -> anyhow::Error {
    match err {
        tgradish_core::Error::OutputExists(path) => {
            anyhow::anyhow!("{} already exists, use --overwrite to replace it", path.display())
        }
        err => err.into(),
    }
}

pub fn run(ctx: &Context, args: ConvertArgs) -> Result<()> {
    let converter = Converter::new(ctx, &args.conversion)?;
    // keeps a pasted image on disk until it is converted
    let mut _pasted_image = None;
    let inputs = if args.clipboard {
        let (inputs, image) = crate::clipboard::inputs()?;
        _pasted_image = image;
        inputs
    } else {
        args.inputs.clone()
    };
    if args.output.is_some() && inputs.len() > 1 {
        bail!("--output only works with a single input");
    }

    let mut failed = 0;
    for input in &inputs {
        // a pasted image lives in a temporary directory; its result goes to
        // the current one
        let output = match (&args.output, &_pasted_image) {
            (Some(output), _) => Some(output.clone()),
            (None, Some(_)) if args.conversion.output_dir.is_none() => {
                let name = converter.output_for(input);
                Some(std::env::current_dir()?.join(name.file_name().unwrap_or_default()))
            }
            (None, _) => None,
        };
        match converter.run(input, output, converter.overwrite()) {
            Ok(_) => {}
            Err(tgradish_core::Error::Cancelled) => {
                return Err(tgradish_core::Error::Cancelled.into());
            }
            Err(err) if inputs.len() > 1 => {
                failed += 1;
                converter.report_failure(input, err);
            }
            Err(err) => return Err(explain(err)),
        }
    }
    if failed > 0 {
        if ctx.global.json {
            // each failure was already reported with its input
            return Err(crate::Exit(1).into());
        }
        bail!("{failed} of {} conversions failed", inputs.len());
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
