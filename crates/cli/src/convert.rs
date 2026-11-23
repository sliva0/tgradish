use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Result, bail};
use console::style;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use tgradish_core::backend::Backend;
use tgradish_core::convert::{self, needs_backend};
use tgradish_core::events::{Details, StartDetails};
use tgradish_core::input::Input;
use tgradish_core::presets::{Format, FormatOptions, PresetError, Presets};
use tgradish_core::webm::Options;
use tgradish_core::webp;
use tgradish_core::{Event, Outcome, Started};

use crate::Context;
use crate::args::{ConversionArgs, ConvertArgs, FormatArg};
use crate::{term, ui};

/// Why converting one input failed.
#[derive(Debug, thiserror::Error)]
pub enum Failed {
    #[error(transparent)]
    Core(#[from] tgradish_core::Error),
    /// Another input of the same command was converted into `output`.
    #[error(
        "{} was already written from {}; inputs with the same name require different output \
         directories",
        output.display(),
        other.display()
    )]
    SameOutput { output: PathBuf, other: PathBuf },
    /// The folder the output goes in couldn't be made.
    #[error("failed to create {}: {}", dir.display(), tgradish_core::io_reason(error))]
    Folder { dir: PathBuf, error: std::io::Error },
}

/// Whether Telegram would accept a converted output file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Accepted,
    /// Output was created, but Telegram would reject it for the reported
    /// issues; exits with [`crate::REFUSED`].
    Refused,
}

impl Verdict {
    /// Returns the verdict for `issues` according to
    /// [`tgradish_core::telegram::refuses`].
    fn of(issues: &[tgradish_core::telegram::Issue]) -> Verdict {
        if tgradish_core::telegram::refuses(issues) { Verdict::Refused } else { Verdict::Accepted }
    }
}

/// Runs conversions using one command's resolved options and output settings.
pub struct Converter<'a> {
    ctx: &'a Context,
    args: &'a ConversionArgs,
    /// Resolved format options: preset, then `--options-json`, then CLI flags.
    options: FormatOptions,
    /// Cached ffmpeg backend. Resolved up front for WebM and WebP (which always
    /// need it) so a missing ffmpeg fails before converting any input; resolved
    /// lazily for TGS (which only needs it for videos).
    backend: Option<Backend>,
    /// Maps each written output path to its input path so two inputs with the
    /// same stem cannot overwrite each other.
    written: RefCell<HashMap<PathBuf, PathBuf>>,
    /// Tracks whether `--fps` (or `"fps"` in `--options-json`) was supplied so
    /// we warn once when making `.tgs` output from a single-file input with its
    /// own frame timings.
    fps_given: bool,
    fps_unused_said: std::cell::Cell<bool>,
}

/// Deserialises `--options-json` into the option type for `format`.
fn overlay<T: serde::de::DeserializeOwned>(
    json: &serde_json::Map<String, serde_json::Value>,
    format: Format,
) -> Result<T> {
    serde_json::from_value(json.clone().into()).map_err(|err| {
        crate::usage(format!("invalid --options-json for {} output: {err}", format.name()))
    })
}

/// Validates input-independent option values before converting any input.
/// Bad values from a CLI flag or `--options-json` key fail as usage errors
/// (exit 2); bad values from `preset` fail as ordinary errors.
fn check_options(
    options: &impl tgradish_core::options::Checked,
    args: &ConversionArgs,
    json: Option<&serde_json::Map<String, serde_json::Value>>,
    preset: &str,
) -> Result<()> {
    let Err(tgradish_core::Error::InvalidOption { option, reason }) =
        tgradish_core::options::check(options)
    else {
        return Ok(());
    };
    let mut flags = tgradish_core::options::set_keys(&args.options.to_options());
    flags.extend(tgradish_core::options::set_keys(&args.options.to_tgs_options()));
    if flags.iter().any(|key| key == option) {
        return Err(crate::usage(format!("--{option} {reason}")));
    }
    if json.is_some_and(|json| json.contains_key(option)) {
        return Err(crate::usage(format!("{option:?} in --options-json {reason}")));
    }
    bail!("preset {preset:?} sets {option}, which {reason}")
}

/// Resolves the output format from `--format`, then `-o`'s extension, then
/// WebM.
fn format_of(format: Option<FormatArg>, output: Option<&Path>) -> Format {
    match (format, output.and_then(Format::of_path)) {
        (Some(format), _) => format.into(),
        (None, Some(format)) => format,
        (None, None) => Format::default(),
    }
}

/// Rejects an `-o` path before any work if it is a directory or has an
/// extension incompatible with `format` (which would produce a file claiming to
/// be another format).
fn check_output(output: &Path, format: Format) -> Result<()> {
    if output.is_dir() {
        return Err(crate::usage(format!(
            "{} is a directory; use -o for a single output file or -O/--output-dir for a directory",
            output.display()
        )));
    }
    let wrong = |message: String| Err(crate::usage(message));
    match Format::of_path(output) {
        Some(asked) if asked == format => Ok(()),
        Some(asked) => wrong(format!(
            "{} has a .{} extension, which does not match {} output; use .{} or remove --format",
            output.display(),
            asked.extension(),
            format.name(),
            format.extension()
        )),
        None => wrong(format!(
            "{} must end in .{} for {} output (use .tgs or .webp for those formats)",
            output.display(),
            format.extension(),
            format.name()
        )),
    }
}

impl<'a> Converter<'a> {
    /// Creates a converter for `format` (or the format inferred from `output`).
    pub fn new(
        ctx: &'a Context,
        args: &'a ConversionArgs,
        format: Option<FormatArg>,
        output: Option<&Path>,
    ) -> Result<Self> {
        let format = format_of(format, output);
        if let Some(output) = output {
            check_output(output, format)?;
        }
        let json = match &args.options_json {
            Some(json) => Some(
                serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(json).map_err(
                    |err| crate::usage(format!("--options-json must be a JSON object: {err}")),
                )?,
            ),
            None => None,
        };
        let unused = args.options.unused(format, json.as_ref());
        if !unused.is_empty() {
            let unused = unused.join(", ");
            return Err(crate::usage(format!(
                "{unused} cannot be used with {} output",
                format.name()
            )));
        }
        if args.keep_temp && format != Format::Webm {
            return Err(crate::usage(
                "--keep-temp only applies to WebM output, which alone leaves intermediate files",
            ));
        }
        let presets = Presets::load_user()?;
        // preset, then --options-json, then flags
        let name = args.preset.as_deref().unwrap_or_else(|| ctx.config.preset_for(format));
        let webm = |mut options: Options| -> Result<Options> {
            if let Some(json) = &json {
                options = options.merged(&overlay(json, format)?);
            }
            let options = options.merged(&args.options.to_options());
            check_options(&options, args, json.as_ref(), name)?;
            Ok(options)
        };
        let resolved = presets.resolve(name, format).map_err(|err| match err {
            tgradish_core::Error::Preset(PresetError::Unknown { known, .. }) => {
                let known = known.join(", ");
                if args.preset.is_some() {
                    return crate::usage(format!("unknown --preset {name:?}; available: {known}"));
                }
                // the only other place a preset is named
                let key = if format == Format::Tgs { "tgs-preset" } else { "preset" };
                let file = ctx.config_file.as_ref().map(|file| file.path.display());
                let place = file.map_or(String::new(), |file| format!(" ({file}: {key})"));
                anyhow::anyhow!("unknown {key} {name:?} in config{place}; available: {known}")
            }
            err => err.into(),
        })?;
        let options = match resolved {
            FormatOptions::Webm(options) => FormatOptions::Webm(webm(options)?),
            FormatOptions::Webp(options) => FormatOptions::Webp(webm(options)?),
            FormatOptions::Tgs(mut options) => {
                if let Some(json) = &json {
                    options = options.merged(&overlay(json, format)?);
                }
                let options = options.merged(&args.options.to_tgs_options());
                check_options(&options, args, json.as_ref(), name)?;
                // how a sheet is read, of a sheet that isn't one
                if options.sheet_frames.is_some() && options.sheet.is_none() {
                    return Err(crate::usage(match args.options.sheet_frames {
                        Some(_) => "--sheet-frames requires --sheet",
                        None => "\"sheet-frames\" in --options-json requires \"sheet\"",
                    }));
                }
                FormatOptions::Tgs(options)
            }
        };
        let backend = match format {
            Format::Webm | Format::Webp => Some(ctx.backend().map_err(|err| ctx.explain(err))?),
            Format::Tgs => None,
        };
        if let Some(dir) = &args.output_dir {
            std::fs::create_dir_all(dir).map_err(|err| {
                anyhow::anyhow!(
                    "failed to create {}: {}",
                    dir.display(),
                    tgradish_core::io_reason(&err)
                )
            })?;
        }
        if args.cancel_on_stdin {
            listen_for_cancel(ctx);
        }
        let fps_given = args.options.fps.is_some()
            || json.as_ref().is_some_and(|json| json.contains_key("fps"));
        Ok(Self {
            ctx,
            args,
            options,
            backend,
            written: Default::default(),
            fps_given,
            fps_unused_said: Default::default(),
        })
    }

    /// Computes the default output path for `input`, placing it under
    /// `--output-dir` when set (preserving any relative subdirectories under
    /// `base`).
    pub fn output_for(&self, input: &Input, base: Option<&Path>) -> PathBuf {
        let output = input.default_output(self.options.format(), self.options.target());
        let (Some(dir), Some(name)) = (&self.args.output_dir, output.file_name()) else {
            return output;
        };
        let parent = input.first().parent();
        let relative =
            base.and_then(|base| parent?.strip_prefix(base).ok()).unwrap_or(Path::new(""));
        dir.join(relative).join(name)
    }

    pub fn overwrite(&self) -> bool {
        self.args.overwrite
    }

    /// Returns the cached ffmpeg backend, or resolves one on demand.
    fn backend(&self) -> tgradish_core::Result<Backend> {
        match &self.backend {
            Some(backend) => Ok(backend.clone()),
            None => self.ctx.backend(),
        }
    }

    /// Converts `input`, printing events as text or JSON.
    pub fn run(
        &self,
        input: &Input,
        output: Option<PathBuf>,
        overwrite: bool,
    ) -> Result<Verdict, Failed> {
        let output = output.unwrap_or_else(|| self.output_for(input, None));
        let key = std::path::absolute(&output).unwrap_or_else(|_| output.clone());
        let first = input.first();
        let input_key = std::path::absolute(first).unwrap_or_else(|_| first.to_path_buf());
        if let Some(other) = self.written.borrow().get(&key)
            && *other != input_key
        {
            return Err(Failed::SameOutput { output, other: other.clone() });
        }
        if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)
                .map_err(|error| Failed::Folder { dir: parent.to_path_buf(), error })?;
        }
        let cancel = &self.ctx.cancel;
        let json = self.ctx.global.json;
        let (input, output) = (input.clone(), Some(output));
        let exact =
            matches!(&self.options, FormatOptions::Tgs(options) if options.exact_or_default());
        let mut printer = Printer::new(self.ctx, exact);
        let mut on_event = |event: Event| {
            if json {
                crate::json_line(&event);
            } else {
                printer.event(event);
            }
        };
        let request = convert::Request {
            input,
            output,
            options: self.options.clone(),
            overwrite,
            keep_temp: self.args.keep_temp,
            threads: self.args.threads.unwrap_or(self.ctx.config.threads),
        };
        // a frame rate is how fast pictures that have none play
        if let FormatOptions::Tgs(options) = &self.options
            && matches!(request.input, Input::File(_))
            && options.sheet.is_none()
            && self.fps_given
            && !self.fps_unused_said.replace(true)
        {
            let message =
                "--fps only sets frame rate for image sequences and sprite sheets in .tgs output";
            if json {
                crate::json_line(&Event::Warning { message: message.into() });
            } else {
                eprintln!("{} {message}", ui::warning_label());
            }
        }
        // a .tgs needs ffmpeg only for videos
        let backend = needs_backend(&request).then(|| self.backend()).transpose()?;
        let made = convert::convert(backend.as_ref(), &request, cancel, &mut on_event);
        if !json {
            printer.bar.finish();
        }
        let outcome = made?;
        if !json {
            self.after(&outcome, backend.as_ref());
        }
        self.written.borrow_mut().insert(key, input_key);
        Ok(Verdict::of(&outcome.issues))
    }

    /// Prints post-conversion details in text mode: kept WebM temp directory
    /// and an inline terminal preview if supported.
    fn after(&self, outcome: &Outcome, backend: Option<&Backend>) {
        if let Details::Webm { temp_dir: Some(dir), .. } = &outcome.details {
            eprintln!("{}", ui::status("Kept", format!("intermediate files in {}", dir.display())));
        }
        if self.ctx.global.quiet || !term::shows_pictures() {
            return;
        }
        let preview = match outcome.format {
            Format::Webm => backend
                .and_then(|backend| backend.preview(&outcome.output, 256, &self.ctx.cancel).ok()),
            Format::Tgs => outcome.preview(),
            Format::Webp => webp::preview(&outcome.output).ok(),
        };
        if let Some(preview) = preview {
            show_result(&preview);
        }
    }

    /// Removes `input` from the output collision map (used by `watch` when a
    /// source file disappears) and returns the freed absolute output paths.
    pub fn forget(&self, input: &Path) -> Vec<PathBuf> {
        let input = std::path::absolute(input).unwrap_or_else(|_| input.to_path_buf());
        let mut freed = Vec::new();
        self.written.borrow_mut().retain(|output, from| {
            let held = *from == input;
            if held {
                freed.push(output.clone());
            }
            !held
        });
        freed
    }

    /// Reports a failed conversion of one of several inputs.
    pub fn report_failure(&self, input: &Path, err: Failed) {
        let err = match err {
            Failed::Core(err) => self.ctx.explain(err),
            err => err.into(),
        };
        if self.ctx.global.json {
            crate::print_json_error(&err, Some(input));
        } else {
            eprintln!("{}", ui::status_bad("Failed", ui::about(input, &format!("{err:#}"))));
        }
    }
}

pub fn run(ctx: &Context, args: ConvertArgs) -> Result<()> {
    if args.output.is_some() && !args.sequence && args.inputs.len() > 1 {
        return Err(crate::usage("-o/--output only accepts a single input or --sequence"));
    }
    let converter = Converter::new(ctx, &args.conversion, args.format, args.output.as_deref())?;
    if args.sequence {
        let input = Input::Sequence(args.inputs.clone());
        return match converter.run(&input, args.output.clone(), converter.overwrite()) {
            Ok(Verdict::Accepted) => Ok(()),
            Ok(Verdict::Refused) => Err(crate::Exit(crate::REFUSED).into()),
            Err(Failed::Core(tgradish_core::Error::Cancelled)) => {
                Err(tgradish_core::Error::Cancelled.into())
            }
            // named by its first picture, as its result is
            Err(err) => {
                converter.report_failure(input.first(), err);
                Err(crate::Exit(1).into())
            }
        };
    }
    // keeps a pasted image on disk until it is converted
    let mut pasted_image = None;
    let inputs = if args.clipboard {
        let pasted = tgradish_core::clipboard::paste()?;
        pasted_image = pasted.image_dir;
        pasted.files
    } else {
        args.inputs.clone()
    };
    if args.output.is_some() && inputs.len() > 1 {
        return Err(crate::usage("-o/--output only accepts a single input or --sequence"));
    }

    let (mut failed, mut refused) = (0, false);
    for path in &inputs {
        let input = Input::File(path.clone());
        // a pasted image lives in a temporary directory; its result goes to
        // the current one
        let output = match (&args.output, &pasted_image) {
            (Some(output), _) => Some(output.clone()),
            (None, Some(_)) if args.conversion.output_dir.is_none() => {
                let name = converter.output_for(&input, None);
                Some(std::env::current_dir()?.join(name.file_name().unwrap_or_default()))
            }
            (None, _) => None,
        };
        match converter.run(&input, output, converter.overwrite()) {
            Ok(Verdict::Accepted) => {}
            Ok(Verdict::Refused) => refused = true,
            Err(Failed::Core(tgradish_core::Error::Cancelled)) => {
                return Err(tgradish_core::Error::Cancelled.into());
            }
            Err(err) => {
                failed += 1;
                converter.report_failure(path, err);
            }
        }
    }
    if failed > 0 {
        // each failure was already reported with its input
        if ctx.global.json || inputs.len() == 1 {
            return Err(crate::Exit(1).into());
        }
        bail!("{failed} of {} conversions failed", inputs.len());
    }
    if refused {
        // each says why with its result
        return Err(crate::Exit(crate::REFUSED).into());
    }
    Ok(())
}

/// Listens on stdin for `cancel` or `settle` lines when `--cancel-on-stdin` is
/// set (for front-ends, since sending Ctrl-C to a child process is awkward on
/// Windows). Ignored unless requested so scripts piping stdin are unaffected.
/// EOF is ignored.
fn listen_for_cancel(ctx: &Context) {
    let cancel = ctx.cancel.clone();
    std::thread::spawn(move || {
        for line in std::io::stdin().lines().map_while(Result::ok) {
            match line.trim() {
                "cancel" => cancel.cancel(),
                // No-op if no fitting encode exists yet.
                "settle" => _ = cancel.settle(),
                _ => {}
            }
        }
    });
}

/// Builds a hidden cargo-style progress bar. Kept hidden until [`Bar::show`]
/// clears any previous bar so lines do not overwrite each other.
fn progress_bar(verb: &str, length: Option<u64>) -> ProgressBar {
    let bar = ProgressBar::with_draw_target(length, ProgressDrawTarget::hidden());
    // Put `{wide_msg}` last so indicatif truncates to terminal width: a line
    // that wraps would leave its first part behind on every redraw.
    let template = match length {
        Some(_) => "{prefix:>12.green.bold} {bar:20.green/dim} {percent:>3}% {wide_msg}",
        None => "{prefix:>12.green.bold} {spinner:.green} {wide_msg}",
    };
    bar.set_style(
        ProgressStyle::with_template(template)
            .expect("valid template")
            .progress_chars("━╸─")
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ "),
    );
    bar.set_prefix(verb.to_owned());
    bar
}

/// Active progress bar, plus helper for printing status lines above it.
#[derive(Default)]
struct Bar(Option<ProgressBar>);

impl Bar {
    /// Clears any previous progress bar and shows `bar` on stderr (drawing
    /// before clearing the old bar would leave its line behind).
    fn show(&mut self, bar: ProgressBar) {
        self.clear();
        bar.set_draw_target(ProgressDrawTarget::stderr());
        bar.enable_steady_tick(Duration::from_millis(100));
        self.0 = Some(bar);
    }

    /// Prints a line above the progress bar (or directly to stderr if hidden).
    fn line(&self, text: impl AsRef<str>) {
        match &self.0 {
            // A hidden bar (when stderr is not a terminal) prints nothing on
            // println.
            Some(bar) if !bar.is_hidden() => bar.println(text),
            _ => eprintln!("{}", text.as_ref()),
        }
    }

    fn clear(&mut self) {
        if let Some(bar) = self.0.take() {
            bar.finish_and_clear();
        }
    }

    /// Clears the progress bar and resets terminal taskbar progress.
    fn finish(&mut self) {
        self.clear();
        term::progress(None);
    }
}

/// Renders conversion events for any format as human-readable text.
struct Printer {
    verbose: u8,
    quiet: bool,
    bar: Bar,
    /// Telegram size limit in bytes for the active target/format.
    limit: u64,
    /// For `.tgs`: whether lossy fitting is disabled (`--exact`).
    exact: bool,
    /// For `.tgs`: overall completion fraction (`0.0..=1.0`).
    done: f64,
    /// For `.tgs`: count of oversize compression retries so far.
    retries: u32,
}

impl Printer {
    fn new(ctx: &Context, exact: bool) -> Self {
        Self {
            verbose: ctx.global.verbose,
            quiet: ctx.global.quiet,
            bar: Bar::default(),
            limit: tgradish_core::telegram::MAX_STICKER_BYTES,
            exact,
            done: 0.0,
            retries: 0,
        }
    }

    /// A bar of how much of making a `.tgs` is done, saying the stage it
    /// is at.
    fn tgs_bar(&mut self, verb: &str, message: &str) {
        if self.quiet {
            return;
        }
        let bar = progress_bar(verb, Some(1000));
        bar.set_message(message.to_owned());
        bar.set_position((self.done * 1000.0) as u64);
        self.bar.show(bar);
    }

    fn event(&mut self, event: Event) {
        match event {
            Event::Started(started) => {
                self.limit = started.limit;
                if !self.quiet {
                    self.started(started);
                }
            }
            Event::AttemptStarted { attempt, params } if !self.quiet => {
                let bar = progress_bar("Encoding", Some(1000));
                bar.set_message(format!("{attempt}: {}", ui::params(&params)));
                self.bar.show(bar);
            }
            Event::Progress { pass, passes, fraction, .. } => {
                let done = (f64::from(pass - 1) + fraction) / f64::from(passes);
                if let Some(bar) = &self.bar.0 {
                    bar.set_position((done * 1000.0) as u64);
                }
                term::progress(Some(done));
            }
            Event::AttemptFinished { attempt, params, bytes, fits, .. } if !self.quiet => {
                self.bar.clear();
                let mark = if fits { style("fits").green() } else { style("too large").red() };
                self.bar.line(ui::status(
                    "Encoded",
                    format!(
                        "{attempt}: {} → {}, {mark}",
                        ui::params(&params),
                        ui::size_within(bytes, self.limit)
                    ),
                ));
            }
            Event::Scored { ssim, .. } if self.verbose > 0 => {
                self.bar
                    .line(ui::status("Compared", ui::dim(format!("similarity (SSIM) {ssim:.4}"))));
            }
            // why fitting stops, or tries a lower frame rate
            Event::Decided { message, .. } if !self.quiet => {
                self.bar.line(ui::status("Decided", ui::dim(message)));
            }
            Event::Log { line } if self.verbose > 1 => {
                self.bar.line(ui::dim(format!("    {line}")))
            }
            Event::TooLarge { bytes } if !self.quiet && self.exact => {
                self.bar.line(format!(
                    "{} about {} as-is (too large); pass --no-exact to allow lossy fitting",
                    ui::warning_label(),
                    ui::size_within(bytes, self.limit)
                ));
            }
            Event::TooLarge { bytes } if !self.quiet => {
                self.bar.line(ui::status(
                    "Fitting",
                    format!("about {} as-is (too large)", ui::size_within(bytes, self.limit)),
                ));
                self.tgs_bar("Fitting", "into 64 KiB");
            }
            Event::Reduced { step } if !self.quiet => {
                self.bar.line(ui::status(
                    "Reduced",
                    format!(
                        "{} → about {}",
                        reduction_text(&step.reduction),
                        ui::kib(step.bytes as u64)
                    ),
                ));
            }
            // a try again's bar is all of it, compressing too
            Event::Packing if self.retries == 0 => self.tgs_bar("Compressing", "with zopfli"),
            Event::Missed { bytes, aim } => {
                self.retries += 1;
                if !self.quiet {
                    self.bar.clear();
                    // the try that missed, as a WebM's encode says it
                    let tried = self.retries;
                    self.bar.line(match bytes {
                        Some(bytes) => ui::status(
                            "Compressed",
                            format!(
                                "attempt {tried} → {}, {}",
                                ui::size_within(bytes, self.limit),
                                style("too large").red()
                            ),
                        ),
                        None => ui::status(
                            "Missed",
                            format!(
                                "attempt {tried}: {}",
                                style("far too large to compress").red()
                            ),
                        ),
                    });
                    let bar = progress_bar("Retrying", Some(1000));
                    bar.set_message(format!("{}: targeting {}", self.retries + 1, ui::kib(aim)));
                    self.bar.show(bar);
                }
            }
            Event::Advanced { fraction, stage_fraction } => {
                // a try again's bar is of that try, as an encode's is
                let shown = if self.retries > 0 { stage_fraction } else { fraction };
                if let Some(bar) = &self.bar.0 {
                    bar.set_position((shown * 1000.0) as u64);
                }
                // the terminal's progress is in whole percent
                let percent = |fraction: f64| (fraction * 100.0).round();
                if percent(fraction) != percent(self.done) {
                    term::progress(Some(fraction));
                }
                self.done = fraction;
            }
            Event::Warning { message } => {
                self.bar.line(format!("{} {message}", ui::warning_label()));
            }
            Event::Finished(outcome) => self.finished(&outcome),
            _ => {}
        }
    }

    /// Prints the initial conversion status and plan/input summary.
    fn started(&mut self, started: Started) {
        self.bar.line(converting(&started.input, &started.output));
        match started.details {
            StartDetails::Webm { plan } => {
                let spoof = if plan.spoofs(plan.length) { ", spoofed" } else { "" };
                self.bar.line(ui::status(
                    "Planned",
                    ui::dim(format!(
                        "{} {}×{}, {}, {}, fit {}{spoof}",
                        plan.target.name(),
                        plan.width,
                        plan.height,
                        ui::fps(plan.fps),
                        ui::seconds(plan.length),
                        ui::name(&plan.fit),
                    )),
                ));
            }
            StartDetails::Tgs { report } => {
                let scale = match report.scale {
                    1 => String::new(),
                    scale => format!(" at {scale}x"),
                };
                let mut line = format!(
                    "{}×{} cells{scale}, {}, {}, {}",
                    report.width,
                    report.height,
                    ui::count(report.colours, "colour"),
                    ui::count(report.frames, "frame"),
                    ui::seconds(f64::from(report.ticks) / 60.0),
                );
                if report.tempo != 1.0 {
                    line.push_str(&format!(", {:.2}x speed", report.tempo));
                }
                if report.trimmed {
                    line.push_str(", trimmed to 3 s");
                }
                self.bar.line(ui::status("Read", ui::dim(line)));
                if let Some(likely) = report.likely_scale {
                    self.bar.line(format!(
                        "{} pixel art looks like {}x with {:.1}% misaligned edges; pass \
                         --pixel-scale {} to snap them",
                        ui::warning_label(),
                        likely.scale,
                        (1.0 - likely.fit) * 100.0,
                        likely.scale
                    ));
                }
                self.done = 0.0;
                self.tgs_bar("Converting", "to vector");
            }
            _ => {}
        }
    }

    /// Prints the finished output line to stdout (even in quiet mode) and any
    /// Telegram issues to stderr.
    fn finished(&mut self, outcome: &Outcome) {
        self.bar.finish();
        let size = ui::size_within(outcome.bytes, outcome.limit);
        let said = match &outcome.details {
            Details::Webm { params, spoofed, settled, .. } => {
                let spoofed = if *spoofed { ", duration spoofed" } else { "" };
                // the result line says it, where it says more than its path
                let settled = if *settled { ", kept best so far" } else { "" };
                format!(", {}{spoofed}{settled}", ui::params(params))
            }
            Details::Tgs { steps, .. } if outcome.changed && !steps.is_empty() => {
                format!(", {}", style("changed to fit").yellow())
            }
            Details::Tgs { .. } if outcome.changed => ", changed".to_string(),
            Details::Tgs { .. } => ", exact".to_string(),
            Details::Webp { width, height, encoding } => match encoding {
                webp::Encoding::Lossy { quality } => {
                    format!(", {width}×{height}, lossy (quality {quality}) to fit")
                }
                _ => format!(", {width}×{height}"),
            },
            _ => String::new(),
        };
        let output = term::link_out(style(outcome.output.display()).bold(), &outcome.output);
        finished(self.quiet, &outcome.output, format!("{output} {size}{said}"));
        print_issues(&outcome.issues);
    }
}

/// Prints the completed file to stdout: `Finished <line>`, or just the path
/// when `quiet` is true.
fn finished(quiet: bool, output: &Path, line: String) {
    if quiet {
        outln!("{}", output.display());
    } else {
        outln!("{}", ui::status("Finished", line));
    }
}

/// Prints any Telegram validation issues for a result to stderr.
fn print_issues(issues: &[tgradish_core::telegram::Issue]) {
    for issue in issues {
        eprintln!("{}", ui::issue(issue));
    }
}

/// Formats the `Converting <input> → <output>` status line.
fn converting(input: &Path, output: &Path) -> String {
    let input = term::link(input.display(), input);
    let output = term::link(style(output.display()).bold(), output);
    ui::status("Converting", format!("{input} → {output}"))
}

/// Displays a representative frame of a finished sticker in terminals that
/// support inline images.
fn show_result(preview: &tgradish_core::preview::Preview) {
    let Some((rgba, _)) = preview.frames.get(preview.frames.len() / 3) else { return };
    term::picture(rgba, preview.width, preview.height, 8);
}

fn reduction_text(reduction: &tgradish_core::tgradish_tgs::reduce::Reduction) -> String {
    use tgradish_core::tgradish_tgs::reduce::Reduction;
    match *reduction {
        Reduction::SnapToGrid { scale } => format!("snapped to {scale}x pixel grid"),
        Reduction::MergeColours { distance } => format!("merged colours closer than {distance:.3}"),
        Reduction::MergeFrames { changed } => {
            format!("merged frames differing by under {:.1}% of pixels", changed * 100.0)
        }
        Reduction::DropFrames { share } => format!("dropped {:.0}% of frames", share * 100.0),
        Reduction::Despeckle => "removed isolated stray pixels".to_string(),
        Reduction::Downscale { factor } => format!("downscaled to {:.0}%", factor * 100.0),
        other => format!("{other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bar draws only once shown, so the one it replaces is cleared
    /// first rather than left on screen above it.
    #[test]
    fn draws_a_bar_only_once_shown() {
        let bar = progress_bar("Converting", None);
        bar.set_message("to vector");
        assert!(bar.is_hidden());
    }
}
