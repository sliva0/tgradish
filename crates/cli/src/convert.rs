use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use console::style;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use tgradish_core::backend::Backend;
use tgradish_core::convert::{Request, convert, default_output};
use tgradish_core::events::Event;
use tgradish_core::options::Options;
use tgradish_core::presets::{Format, FormatOptions, Presets};
use tgradish_core::tgs::{self, TgsEvent, TgsOptions, TgsRequest};

use crate::Context;
use crate::args::{ConversionArgs, ConvertArgs};
use crate::ui;

/// Options of the format a command converts to.
enum Resolved {
    Webm { options: Options, backend: Backend },
    Tgs { options: TgsOptions },
}

/// Runs conversions with the options and output settings of one command.
pub struct Converter<'a> {
    ctx: &'a Context,
    args: &'a ConversionArgs,
    resolved: Resolved,
    /// Outputs written by this command, and their inputs, so two inputs
    /// with the same name never overwrite each other's result.
    written: RefCell<HashMap<PathBuf, PathBuf>>,
}

/// The format a command converts to: `--format`, or else the extension of
/// `output` (the `-o` file), or else the preset's format.
fn format_of(args: &ConversionArgs, output: Option<&Path>, presets: &Presets) -> Result<Format> {
    Ok(match (args.format, output.and_then(Format::of_path), &args.preset) {
        (Some(format), _, _) => format.into(),
        (None, Some(format), _) => format,
        (None, None, Some(preset)) => presets.format(preset)?,
        (None, None, None) => Format::default(),
    })
}

impl<'a> Converter<'a> {
    /// `output` is the `-o` file, which can choose the format.
    pub fn new(ctx: &'a Context, args: &'a ConversionArgs, output: Option<&Path>) -> Result<Self> {
        let presets = Presets::load_user()?;
        let format = format_of(args, output, &presets)?;
        if let Some(asked) = output.and_then(Format::of_path).filter(|&f| f != format) {
            bail!(
                "the output is a .{} file, but this converts to {}",
                asked.extension(),
                format.extension()
            );
        }
        let foreign = args.options.foreign(format);
        if !foreign.is_empty() {
            bail!("{} can't be used for {} output", foreign.join(", "), format.extension());
        }
        // preset, then --options-json, then flags
        let name = args.preset.as_deref().unwrap_or_else(|| ctx.config.preset_for(format));
        let invalid_json = || format!("invalid --options-json for {} output", format.extension());
        let resolved = match presets.resolve(name)? {
            FormatOptions::Webm(mut options) if format == Format::Webm => {
                if let Some(json) = &args.options_json {
                    let overlay: Options = serde_json::from_str(json).with_context(invalid_json)?;
                    options = options.merged(&overlay);
                }
                let options = options.merged(&args.options.to_options());
                Resolved::Webm { options, backend: ctx.backend()? }
            }
            FormatOptions::Tgs(mut options) if format == Format::Tgs => {
                if let Some(json) = &args.options_json {
                    let overlay: TgsOptions =
                        serde_json::from_str(json).with_context(invalid_json)?;
                    options = options.merged(&overlay);
                }
                Resolved::Tgs { options: options.merged(&args.options.to_tgs_options()) }
            }
            other => bail!(
                "preset {name:?} is for {}, not {}",
                other.format().extension(),
                format.extension()
            ),
        };
        if let Some(dir) = &args.output_dir {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("could not create {}", dir.display()))?;
        }
        if ctx.global.json {
            listen_for_cancel(ctx);
        }
        Ok(Self { ctx, args, resolved, written: Default::default() })
    }

    pub fn format(&self) -> Format {
        match self.resolved {
            Resolved::Webm { .. } => Format::Webm,
            Resolved::Tgs { .. } => Format::Tgs,
        }
    }

    /// Where the result for `input` goes, unless `-o` says otherwise.
    pub fn output_for(&self, input: &Path) -> PathBuf {
        self.output_under(input, None)
    }

    /// Like [`Converter::output_for`], but keeps the directories between
    /// `base` and the input when writing to `--output-dir`.
    pub fn output_under(&self, input: &Path, base: Option<&Path>) -> PathBuf {
        let output = match &self.resolved {
            Resolved::Webm { options, .. } => {
                default_output(input, options.target.unwrap_or_default())
            }
            Resolved::Tgs { options } => {
                tgs::default_output(input, options.target.unwrap_or_default())
            }
        };
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
    ) -> tgradish_core::Result<()> {
        self.run_inputs(&[input.to_path_buf()], false, output, overwrite)
    }

    /// Converts `inputs`: one file, or with `sequence` the frames of one
    /// animated sticker.
    pub fn run_inputs(
        &self,
        inputs: &[PathBuf],
        sequence: bool,
        output: Option<PathBuf>,
        overwrite: bool,
    ) -> tgradish_core::Result<()> {
        let input = &inputs[0];
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
        let cancel = &self.ctx.cancel;
        let json = self.ctx.global.json;
        match &self.resolved {
            Resolved::Webm { options, backend } => {
                let request = Request {
                    input: input.to_path_buf(),
                    output: Some(output),
                    options: options.clone(),
                    overwrite,
                    keep_temp: self.args.keep_temp,
                };
                let outcome = if json {
                    convert(backend, &request, cancel, &mut |event| {
                        println!("{}", serde_json::to_string(&event).expect("events serialize"));
                    })
                } else {
                    let mut printer = Printer::new(self.ctx);
                    let result = convert(backend, &request, cancel, &mut |e| printer.event(e));
                    printer.finish();
                    result
                }?;
                if let Some(dir) = &outcome.temp_dir
                    && !json
                {
                    eprintln!("intermediate files kept in {}", dir.display());
                }
            }
            Resolved::Tgs { options } => {
                let request = TgsRequest {
                    inputs: inputs.to_vec(),
                    sequence,
                    output,
                    options: options.clone(),
                    overwrite,
                };
                if json {
                    tgs::convert(&request, cancel, &mut |event| {
                        println!("{}", serde_json::to_string(&event).expect("events serialize"));
                    })?;
                } else {
                    let mut printer = TgsPrinter::new(self.ctx);
                    let result = tgs::convert(&request, cancel, &mut |e| printer.event(e));
                    printer.finish();
                    result?;
                }
            }
        }
        self.written.borrow_mut().insert(key, input_key);
        Ok(())
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
    // before Converter::new, which looks for ffmpeg when making WebM
    if args.sequence
        && format_of(&args.conversion, args.output.as_deref(), &Presets::load_user()?)?
            != Format::Tgs
    {
        bail!("--sequence makes animated stickers; add --format tgs or an output ending in .tgs");
    }
    let converter = Converter::new(ctx, &args.conversion, args.output.as_deref())?;
    if args.sequence {
        return converter
            .run_inputs(&args.inputs, true, args.output.clone(), converter.overwrite())
            .map_err(explain);
    }
    // keeps a pasted image on disk until it is converted
    let mut _pasted_image = None;
    let inputs = if args.clipboard {
        let pasted = tgradish_core::clipboard::paste()?;
        _pasted_image = pasted.image_dir;
        pasted.files
    } else {
        args.inputs.clone()
    };
    if args.output.is_some() && inputs.len() > 1 {
        bail!("--output only works with a single input, or with --sequence");
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

/// Telegram's limit for `.tgs` files.
const TGS_LIMIT: u64 = 64 * 1024;

/// Shows `.tgs` conversion events as text.
struct TgsPrinter {
    quiet: bool,
    bar: Option<ProgressBar>,
}

impl TgsPrinter {
    fn new(ctx: &Context) -> Self {
        Self { quiet: ctx.global.quiet, bar: None }
    }

    fn spinner(&mut self, message: String) {
        if self.quiet {
            return;
        }
        let bar = self.bar.get_or_insert_with(|| {
            let bar = ProgressBar::with_draw_target(None, ProgressDrawTarget::stderr());
            bar.set_style(
                ProgressStyle::with_template("  {spinner} {msg}").expect("valid template"),
            );
            bar.enable_steady_tick(Duration::from_millis(120));
            bar
        });
        bar.set_message(message);
    }

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

    fn event(&mut self, event: TgsEvent) {
        match event {
            TgsEvent::Started { input, report } if !self.quiet => {
                let scale = match report.scale {
                    1 => String::new(),
                    scale => format!(" at {scale}x"),
                };
                let mut line = format!(
                    "{}\n  {}x{} cells{scale}, {} colours, {} frames, {}",
                    input.display(),
                    report.width,
                    report.height,
                    report.colours,
                    report.frames,
                    ui::seconds(f64::from(report.ticks) / 60.0),
                );
                if report.speed > 1.0 {
                    line.push_str(&format!(", sped up {:.2}x", report.speed));
                }
                if report.trimmed {
                    line.push_str(", trimmed to 3 s");
                }
                self.line(line);
                if let Some(likely) = report.likely_scale {
                    self.line(format!(
                        "  {} the art looks like {}x with {:.1}% of its edges off that grid; \
                         --pixel-scale {} snaps them",
                        ui::warning_label(),
                        likely.scale,
                        (1.0 - likely.fit) * 100.0,
                        likely.scale
                    ));
                }
                self.spinner("encoding".into());
            }
            TgsEvent::TooLarge { bytes } if !self.quiet => {
                self.line(format!(
                    "  about {} losslessly, too large; fitting",
                    ui::size(bytes as u64)
                ));
            }
            TgsEvent::Reduced { step } if !self.quiet => {
                self.line(format!(
                    "  {} → about {}",
                    reduction_text(&step.reduction),
                    ui::kib(step.bytes as u64)
                ));
            }
            TgsEvent::Packing => self.spinner("compressing".into()),
            TgsEvent::Warning { message } => {
                self.line(format!("{} {message}", ui::warning_label()));
            }
            TgsEvent::Finished { output, bytes, lossy, steps, issues, .. } => {
                self.finish();
                let lossy = if lossy && !steps.is_empty() {
                    format!(", {} lossy", style("changed to fit:").yellow())
                } else if lossy {
                    ", lossy".to_string()
                } else {
                    ", lossless".to_string()
                };
                println!(
                    "{} {}: {}{lossy}",
                    style("done").green().bold(),
                    output.display(),
                    ui::size_within(bytes, TGS_LIMIT),
                );
                for issue in issues {
                    let label = match issue.severity {
                        tgradish_core::tgs::Severity::Error => ui::error_label(),
                        tgradish_core::tgs::Severity::Warning => ui::warning_label(),
                    };
                    eprintln!("{label} {}", issue.message);
                }
            }
            _ => {}
        }
    }
}

fn reduction_text(reduction: &tgradish_core::tgs::Reduction) -> String {
    use tgradish_core::tgs::Reduction;
    match *reduction {
        Reduction::SnapToGrid { scale } => format!("snapped to the {scale}x pixel grid"),
        Reduction::MergeColours { distance } => format!("merged colours closer than {distance:.3}"),
        Reduction::MergeFrames { changed } => {
            format!("merged frames differing in under {:.1}% of the area", changed * 100.0)
        }
        Reduction::DropFrames { share } => format!("dropped {:.0}% of frames", share * 100.0),
        Reduction::Despeckle => "removed lone pixels".to_string(),
        Reduction::Downscale { factor } => format!("scaled the art to {:.0}%", factor * 100.0),
    }
}
