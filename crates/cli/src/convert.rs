use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Result, bail};
use console::style;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use tgradish_core::backend::Backend;
use tgradish_core::config::Config;
use tgradish_core::convert::{self, needs_backend};
use tgradish_core::events::{Details, StartDetails};
use tgradish_core::input::Input;
use tgradish_core::presets::{Format, FormatOptions, Presets};
use tgradish_core::resolve::{Origin, ResolveError, SetAside, resolve};
use tgradish_core::webp;
use tgradish_core::{Event, Outcome, Started};

use crate::args::{ConversionArgs, ConvertArgs, FormatArg};
use crate::{ConfigFile, Context, term, ui};

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
    /// Warnings of the values set aside, given with each input as it starts.
    set_aside: Vec<String>,
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

/// `--options-json`'s options, as `format` takes them.
fn json_options(
    json: &serde_json::Map<String, serde_json::Value>,
    format: Format,
) -> Result<FormatOptions> {
    let json = serde_json::Value::Object(json.clone());
    let read = match format {
        Format::Webm => serde_json::from_value(json).map(FormatOptions::Webm),
        Format::Webp => serde_json::from_value(json).map(FormatOptions::Webp),
        Format::Tgs => serde_json::from_value(json).map(FormatOptions::Tgs),
    };
    read.map_err(|err| {
        crate::usage(format!("invalid --options-json for {} output: {err}", format.name()))
    })
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

/// What the options for `format` come to: the preset's (named by
/// `--preset`, or else by the config), then `--options-json`'s, then the
/// flags', each over the last, checked before any input is read. Also
/// returns the warnings each input gets of the values set aside.
fn layer_options(
    presets: &Presets,
    config: &Config,
    config_file: Option<&ConfigFile>,
    args: &ConversionArgs,
    json: Option<&serde_json::Map<String, serde_json::Value>>,
    format: Format,
) -> Result<(FormatOptions, Vec<String>)> {
    let mut layers = Vec::new();
    if let Some(json) = json {
        layers.push(json_options(json, format)?);
    }
    let flags = layers.len();
    let flagged = match format {
        Format::Webm => FormatOptions::Webm(args.options.to_options()),
        Format::Webp => FormatOptions::Webp(webp::Options::from(&args.options.to_options())),
        Format::Tgs => FormatOptions::Tgs(args.options.to_tgs_options()),
    };
    layers.push(args.options.to_selection().for_format(format).into_options(flagged));
    let name = args.preset.as_deref().unwrap_or_else(|| config.preset_for(format));
    let refused = |err| match err {
        ResolveError::UnknownPreset { known, named, .. } => {
            let known = known.join(", ");
            if named {
                return crate::usage(format!("unknown --preset {name:?}; available: {known}"));
            }
            // the only other place a preset is named
            let key = if format == Format::Tgs { "tgs-preset" } else { "preset" };
            let file = config_file.map(|file| file.path.display());
            let place = file.map_or(String::new(), |file| format!(" ({file}: {key})"));
            anyhow::anyhow!("unknown {key} {name:?} in config{place}; available: {known}")
        }
        ResolveError::Preset(err) => tgradish_core::Error::Preset(err).into(),
        // clap checks most flags' values as it reads them, so mostly JSON's
        ResolveError::Invalid { option, reason, set_by: Origin::Layer(layer), .. } => {
            if layer == flags {
                crate::usage(format!("--{option} {reason}"))
            } else {
                crate::usage(format!("{option:?} in --options-json {reason}"))
            }
        }
        ResolveError::Requires { option, requires, set_by: Origin::Layer(layer) } => {
            if layer == flags {
                crate::usage(format!("--{option} requires --{requires}"))
            } else {
                crate::usage(format!("{option:?} in --options-json requires {requires:?}"))
            }
        }
        // the preset's, said naming it
        err => err.into(),
    };
    let resolved =
        resolve(presets, config, format, args.preset.as_deref(), &layers).map_err(refused)?;
    let mut warnings = Vec::new();
    for SetAside { option, why, set_by } in resolved.set_aside {
        // lossless asked for where fitting has no use for it is refused, as
        // what was asked can't be made; a preset's is only warned of
        match (option, set_by) {
            ("lossless", Origin::Layer(layer)) => {
                let named =
                    if layer == flags { "--lossless" } else { "\"lossless\" in --options-json" };
                let said = format!("{named} only works with fit fps, length or off");
                return Err(crate::usage(said));
            }
            _ => warnings.push(format!("{option} is ignored because {why}")),
        }
    }
    Ok((resolved.options, warnings))
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
        let config_file = ctx.config_file.as_ref();
        let (options, set_aside) =
            layer_options(&presets, &ctx.config, config_file, args, json.as_ref(), format)?;
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
            set_aside,
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
            let started = matches!(event, Event::Started(_));
            let mut say = |event: Event| match json {
                true => crate::json_line(&event),
                false => printer.event(event),
            };
            say(event);
            // of each input, where planning it would say them
            if started {
                for message in &self.set_aside {
                    say(Event::Warning { message: message.clone() });
                }
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

    // What the command line layers, scenario by scenario. Each test has a
    // twin of the same name among the GUI's tests (in item.rs, the last in
    // preset_dialogs.rs), which pins what the GUI makes of the same. Where
    // the two differ, both name the divergence by its number in the plan to
    // unify how options are layered, so that unifying them changes these
    // on purpose.

    use clap::Parser;
    use tgradish_core::options::{Effort, Fit, IntegerScale, Resize, Scaling, Spoof};
    use tgradish_core::tgs;
    use tgradish_core::webm::Options;

    /// The conversion arguments clap reads from `flags`.
    fn args(flags: &[&str]) -> ConversionArgs {
        let mut all = vec!["tgradish", "convert", "a.png"];
        all.extend(flags);
        let crate::args::Command::Convert(convert) = crate::args::Cli::parse_from(all).command
        else {
            panic!("not a conversion")
        };
        convert.conversion
    }

    /// The built-in presets and `files`, each `(name, text)`.
    fn presets(files: &[(&str, &str)]) -> Presets {
        let dir = tempfile::tempdir().unwrap();
        for (name, text) in files {
            std::fs::write(dir.path().join(format!("{name}.toml")), text).unwrap();
        }
        Presets::load(Some(dir.path())).unwrap()
    }

    /// What the command line makes of `flags` and `json` for `format`, and
    /// the warnings each input gets.
    fn layered_warned(
        presets: &Presets,
        config: &Config,
        format: Format,
        flags: &[&str],
        json: Option<serde_json::Value>,
    ) -> Result<(FormatOptions, Vec<String>)> {
        let json = json.map(|json| json.as_object().expect("an object").clone());
        layer_options(presets, config, None, &args(flags), json.as_ref(), format)
    }

    /// What the command line makes of `flags` and `json` for `format`.
    fn layered(
        presets: &Presets,
        config: &Config,
        format: Format,
        flags: &[&str],
        json: Option<serde_json::Value>,
    ) -> Result<FormatOptions> {
        layered_warned(presets, config, format, flags, json).map(|(options, _)| options)
    }

    /// Why layering failed, and whether it is the command line's fault
    /// (exit 2) or else the preset's or config's (exit 1).
    fn refused<T: std::fmt::Debug>(layered: Result<T>) -> (bool, String) {
        let err = layered.expect_err("refused");
        (err.downcast_ref::<crate::Usage>().is_some(), err.to_string())
    }

    fn webm(options: Options) -> FormatOptions {
        FormatOptions::Webm(options)
    }

    fn webp(options: webp::Options) -> FormatOptions {
        FormatOptions::Webp(options)
    }

    #[test]
    fn layers_an_inherited_preset() {
        let child = "extends = 'fast'\n[webm]\nfit = 'crf'\ncrf = 20\n[tgs]\nexact = false\n";
        let presets = presets(&[("child", child)]);
        let config = Config::default();
        let layered = |format| layered(&presets, &config, format, &["-p", "child"], None).unwrap();
        // the crf fitting has no say over is set aside, as the GUI sets it
        // aside, and warned of with each input, which the GUI doesn't do
        // (divergence 7); the target is left to its default
        let warned = layered_warned(&presets, &config, Format::Webm, &["-p", "child"], None);
        let fitted = Options { fit: Some(Fit::Crf), effort: Some(Effort::Fast), ..default() };
        assert_eq!(
            warned.unwrap(),
            (webm(fitted), vec!["crf is ignored because fit is crf".into()])
        );
        let tgs = tgs::Options { effort: Some(Effort::Fast), exact: Some(false), ..default() };
        assert_eq!(layered(Format::Tgs), FormatOptions::Tgs(tgs));
        // a picture's only, and only those checked
        assert_eq!(layered(Format::Webp), webp(default()));
    }

    #[test]
    fn keeps_an_explicit_false_over_a_presets_true() {
        let on = "[webm]\nfit = 'off'\nlossless = true\nwatermark = true\n\
                  [tgs]\nkeep-canvas = true\nwatermark = true\n";
        let presets = presets(&[("on", on)]);
        let config = Config::default();
        let off = ["-p", "on", "--no-watermark", "--no-lossless", "--no-keep-canvas"];
        let layered = |format| layered(&presets, &config, format, &off, None).unwrap();
        let webm_off = Options {
            fit: Some(Fit::Off),
            lossless: Some(false),
            watermark: Some(false),
            ..default()
        };
        assert_eq!(layered(Format::Webm), webm(webm_off));
        let picture_off = webp::Options { watermark: Some(false), ..default() };
        assert_eq!(layered(Format::Webp), webp(picture_off));
        let tgs = tgs::Options { keep_canvas: Some(false), watermark: Some(false), ..default() };
        assert_eq!(layered(Format::Tgs), FormatOptions::Tgs(tgs));
    }

    #[test]
    fn crf_under_a_presets_bitrate_fit() {
        let presets = presets(&[("by bitrate", "[webm]\nfit = 'bitrate'\ncrf = 20\n")]);
        let layered =
            layered_warned(&presets, &Config::default(), Format::Webm, &["-p", "by bitrate"], None);
        // set aside, as the GUI does, and warned of with each input, which
        // the GUI doesn't do (divergence 7)
        let fitted = Options { fit: Some(Fit::Bitrate), ..default() };
        let warned = vec!["crf is ignored because fit is bitrate".to_owned()];
        assert_eq!(layered.unwrap(), (webm(fitted), warned));
    }

    #[test]
    fn lossless_under_fit_auto() {
        let presets = presets(&[("lossless", "[webm]\nlossless = true\n")]);
        let config = Config::default();
        let flags = ["-p", "lossless", "--fit", "auto"];
        let presets_lossless = layered_warned(&presets, &config, Format::Webm, &flags, None);
        // a preset's set aside, as the GUI does, and warned of with each
        // input
        let fitted = Options { fit: Some(Fit::Auto), ..default() };
        let warned = vec!["lossless is ignored because fit is auto".to_owned()];
        assert_eq!(presets_lossless.unwrap(), (webm(fitted.clone()), warned));
        // the command line's refused before any input is read, as what it
        // asks for can't be made
        let flags = ["--lossless", "--fit", "auto"];
        let said = "--lossless only works with fit fps, length or off";
        assert_eq!(
            refused(layered_warned(&presets, &config, Format::Webm, &flags, None)),
            (true, said.into())
        );
        let json = Some(serde_json::json!({ "lossless": true, "fit": "bitrate" }));
        let said = "\"lossless\" in --options-json only works with fit fps, length or off";
        assert_eq!(
            refused(layered(&presets, &config, Format::Webm, &[], json)),
            (true, said.into())
        );
        // and off where it has no say, which changes nothing, is said by
        // neither
        let flags = ["-p", "lossless", "--no-lossless", "--fit", "auto"];
        let off = layered_warned(&presets, &config, Format::Webm, &flags, None);
        assert_eq!(off.unwrap(), (webm(fitted), vec![]));
    }

    #[test]
    fn drops_a_fit_range_of_another_unit() {
        let quality = "[webm]\nfit = 'crf'\nfit-range = { min = 20, max = 40 }\n";
        let presets = presets(&[("quality", quality)]);
        let config = Config::default();
        let by_bitrate = Options { fit: Some(Fit::Bitrate), ..default() };
        let flags = ["-p", "quality", "--fit", "bitrate"];
        let layered_flags = layered(&presets, &config, Format::Webm, &flags, None);
        assert_eq!(layered_flags.unwrap(), webm(by_bitrate.clone()));
        let json = serde_json::json!({ "fit": "bitrate" });
        let layered_json = layered(&presets, &config, Format::Webm, &["-p", "quality"], Some(json));
        assert_eq!(layered_json.unwrap(), webm(by_bitrate));
    }

    #[test]
    fn fake_duration_under_spoof_never() {
        let presets = presets(&[
            ("never", "[webm]\nspoof = 'never'\nfake-duration = 1.0\n"),
            ("too long", "[webm]\nspoof = 'never'\nfake-duration = 5.0\n"),
        ]);
        let config = Config::default();
        // set aside, as the GUI does, and warned of with each input, which
        // the GUI doesn't do (divergence 7)
        let never = layered_warned(&presets, &config, Format::Webm, &["-p", "never"], None);
        let never_spoofed = Options { spoof: Some(Spoof::Never), ..default() };
        let warned = vec!["fake-duration is ignored because spoof is never".to_owned()];
        assert_eq!(never.unwrap(), (webm(never_spoofed), warned));
        // and checked first, though it has no say (divergence 4)
        let too_long = layered(&presets, &config, Format::Webm, &["-p", "too long"], None);
        assert_eq!(
            refused(too_long),
            (
                false,
                "preset \"too long\" sets fake-duration, which must be greater than 0 and at most \
                 3 seconds"
                    .into()
            )
        );
    }

    #[test]
    fn integer_scale_without_integer_scaling() {
        let presets = presets(&[
            ("scaled", "[webm]\ninteger-scale = '2'\n"),
            ("cropped", "[webm]\nscaling = 'integer'\ninteger-scale = '2'\nresize = 'crop'\n"),
        ]);
        let config = Config::default();
        let two = "2".parse::<IntegerScale>().ok();
        // kept, as core scales by it whatever the scaling (divergence 2)
        let scaled = |format| layered(&presets, &config, format, &["-p", "scaled"], None).unwrap();
        assert_eq!(scaled(Format::Webm), webm(Options { integer_scale: two, ..default() }));
        assert_eq!(scaled(Format::Webp), webp(webp::Options { integer_scale: two, ..default() }));
        // and the crop it can't go with, for planning to refuse
        // (divergence 3)
        let cropped = layered(&presets, &config, Format::Webm, &["-p", "cropped"], None);
        let kept = Options {
            resize: Some(Resize::Crop),
            scaling: Some(Scaling::Integer),
            integer_scale: two,
            ..default()
        };
        assert_eq!(cropped.unwrap(), webm(kept));
    }

    #[test]
    fn refuses_a_preset_setting_one_files_options() {
        let presets = presets(&[
            ("crop", "[webm]\ncrop = { x = 0, y = 0, width = 8, height = 8 }\n"),
            ("webm sheet", "[webm]\nsheet = '4x1'\n"),
            ("tgs sheet", "[tgs]\nsheet = '4x1'\n"),
        ]);
        let config = Config::default();
        for (name, key) in
            [("crop", "webm.crop"), ("webm sheet", "webm.sheet"), ("tgs sheet", "tgs.sheet")]
        {
            // the whole preset, for every format
            for format in [Format::Webm, Format::Webp, Format::Tgs] {
                let (usage, said) =
                    refused(layered(&presets, &config, format, &["-p", name], None));
                assert!(!usage, "{name} {format:?}");
                assert!(said.starts_with("invalid preset "), "{said}");
                let per_file =
                    format!(": {key} is a per-file option and cannot be set in a preset");
                assert!(said.ends_with(&per_file), "{said}");
            }
        }
    }

    #[test]
    fn reads_options_by_their_old_names() {
        let renamed = "[webm]\nspeed = 'best'\nscaling = 'pixel-perfect'\nexact-scale = '3'\n\
                       [tgs]\nspeed = 'fast'\n";
        let presets = presets(&[("renamed", renamed)]);
        let config = Config::default();
        let scaled = |effort, scale: &str| Options {
            scaling: Some(Scaling::Integer),
            integer_scale: scale.parse().ok(),
            effort: Some(effort),
            ..default()
        };
        let preset = ["-p", "renamed"];
        let layered =
            |format, flags: &[&str], json| layered(&presets, &config, format, flags, json);
        assert_eq!(layered(Format::Webm, &preset, None).unwrap(), webm(scaled(Effort::Best, "3")));
        let tgs = tgs::Options { effort: Some(Effort::Fast), ..default() };
        assert_eq!(layered(Format::Tgs, &preset, None).unwrap(), FormatOptions::Tgs(tgs));
        let json = serde_json::json!({
            "speed": "fast",
            "scaling": "pixel-perfect",
            "exact-scale": "2",
        });
        let from_json = layered(Format::Webm, &preset, Some(json));
        assert_eq!(from_json.unwrap(), webm(scaled(Effort::Fast, "2")));
        let flags =
            ["-p", "renamed", "--speed=balanced", "--scaling=pixel-perfect", "--exact-scale=1/2"];
        let from_flags = layered(Format::Webm, &flags, None);
        assert_eq!(from_flags.unwrap(), webm(scaled(Effort::Balanced, "1/2")));
        // a picture's is read by its old name too
        let json = serde_json::json!({ "exact-scale": "2" });
        let picture = layered(Format::Webp, &preset, Some(json));
        let scaled = webp::Options {
            scaling: Some(Scaling::Integer),
            integer_scale: "2".parse().ok(),
            ..default()
        };
        assert_eq!(picture.unwrap(), webp(scaled));
    }

    #[test]
    fn makes_a_picture_of_a_presets_webm_options() {
        let presets = presets(&[
            ("picture", "[webm]\nfit = 'off'\ncrf = 20\nscaling = 'sharp'\n"),
            ("fast tempo", "[webm]\ntempo = 50.0\n"),
        ]);
        let config = Config::default();
        // a picture's only
        let flags = ["-p", "picture", "--start", "1.5"];
        let picture = layered(&presets, &config, Format::Webp, &flags, None);
        let sharp = webp::Options { scaling: Some(Scaling::Sharp), start: Some(1.5), ..default() };
        assert_eq!(picture.unwrap(), webp(sharp));
        // and only those checked, so a value a picture doesn't use doesn't
        // stop one being made, as in the GUI
        let tempo = layered(&presets, &config, Format::Webp, &["-p", "fast tempo"], None);
        assert_eq!(tempo.unwrap(), webp(default()));
    }

    #[test]
    fn bad_values_and_where_they_came_from() {
        let presets = presets(&[("slow", "[webm]\ntempo = 50.0\n[tgs]\ntempo = 50.0\n")]);
        let config = Config::default();
        let layered =
            |format, flags: &[&str], json| layered(&presets, &config, format, flags, json);
        let preset = "preset \"slow\" sets tempo, which must be from 0.1 to 10";
        for format in [Format::Webm, Format::Tgs] {
            let from_preset = layered(format, &["-p", "slow"], None);
            assert_eq!(refused(from_preset), (false, preset.into()), "{format:?}");
            let json = Some(serde_json::json!({ "tempo": 50 }));
            let from_json = layered(format, &[], json);
            let said = "\"tempo\" in --options-json must be from 0.1 to 10";
            assert_eq!(refused(from_json), (true, said.into()), "{format:?}");
            // what is set over the preset's is what is checked
            let json = Some(serde_json::json!({ "tempo": 2 }));
            assert!(layered(format, &["-p", "slow"], json).is_ok(), "{format:?}");
        }
        // a flag's value clap checks as it reads it, so layering never
        // finds one wrong
        let flag = crate::args::Cli::try_parse_from(["tgradish", "convert", "a", "--tempo", "50"]);
        let err = flag.expect_err("refused");
        assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation);
        assert!(err.to_string().contains("must be from 0.1 to 10"), "{err}");
    }

    #[test]
    fn sheet_frames_without_a_sheet() {
        let presets = Presets::builtin();
        let config = Config::default();
        let layered = |flags: &[&str], json| layered(&presets, &config, Format::Tgs, flags, json);
        let flag = layered(&["--sheet-frames", "3"], None);
        assert_eq!(refused(flag), (true, "--sheet-frames requires --sheet".into()));
        let json = Some(serde_json::json!({ "sheet-frames": 3 }));
        let said = "\"sheet-frames\" in --options-json requires \"sheet\"";
        assert_eq!(refused(layered(&[], json)), (true, said.into()));
        // with one, from either
        let json = Some(serde_json::json!({ "sheet-frames": 3 }));
        let Ok(FormatOptions::Tgs(sheet)) = layered(&["--sheet", "4x1"], json) else { panic!() };
        assert_eq!((sheet.sheet, sheet.sheet_frames), ("4x1".parse().ok(), Some(3)));
    }

    #[test]
    fn picks_the_configs_preset_for_each_format() {
        let presets = Presets::builtin();
        let config = Config {
            preset: Some("fast".into()),
            tgs_preset: Some("balanced".into()),
            ..Config::default()
        };
        let layered = |format, flags: &[&str]| layered(&presets, &config, format, flags, None);
        let fast = Options { fit: Some(Fit::Bitrate), effort: Some(Effort::Fast), ..default() };
        assert_eq!(layered(Format::Webm, &[]).unwrap(), webm(fast));
        // of which a picture takes none
        assert_eq!(layered(Format::Webp, &[]).unwrap(), webp(default()));
        let balanced = tgs::Options { effort: Some(Effort::Balanced), ..default() };
        assert_eq!(layered(Format::Tgs, &[]).unwrap(), FormatOptions::Tgs(balanced));
        // unless one is named
        let best = tgs::Options { effort: Some(Effort::Best), ..default() };
        assert_eq!(layered(Format::Tgs, &["-p", "best"]).unwrap(), FormatOptions::Tgs(best));
        // and one there isn't, said as where it was named
        let none = Config { preset: Some("nope".into()), ..Config::default() };
        let available = "available: balanced, best, fast";
        let said = format!("unknown preset \"nope\" in config; {available}");
        let from_config = layer_options(&presets, &none, None, &args(&[]), None, Format::Webm);
        assert_eq!(refused(from_config), (false, said));
        let file = ConfigFile { path: "config.toml".into(), from: crate::ConfigFrom::Flag };
        let named = layer_options(&presets, &none, Some(&file), &args(&[]), None, Format::Webp);
        let said = format!("unknown preset \"nope\" in config (config.toml: preset); {available}");
        assert_eq!(refused(named), (false, said));
        // which leaves .tgs to its own
        assert!(layer_options(&presets, &none, None, &args(&[]), None, Format::Tgs).is_ok());
        let flag = layered(Format::Webm, &["-p", "nope"]);
        assert_eq!(refused(flag), (true, format!("unknown --preset \"nope\"; {available}")));
    }

    #[test]
    fn integer_scale_of_a_preset_the_gui_saved() {
        // as the GUI saves one where the item chose smooth scaling over a
        // preset's integer scale: without the scale, which the GUI set
        // aside
        let saved = "[tgs]\n\n[webm]\nscaling = \"smooth\"\n";
        let presets = presets(&[("mine", saved)]);
        let layered = layered(&presets, &Config::default(), Format::Webm, &["-p", "mine"], None);
        // so made as the GUI made it
        let smooth = Options { scaling: Some(Scaling::Smooth), ..default() };
        assert_eq!(layered.unwrap(), webm(smooth));
    }

    /// `--options-json` takes every key its format uses, by its old name
    /// too, and the options it makes are written back as it was given,
    /// in the current names.
    #[test]
    fn options_json_takes_every_key_its_format_uses() {
        use serde_json::json;
        // a preset of nothing, so only the JSON's are written back
        let presets = presets(&[("none", "")]);
        let config = Config::default();
        let picture = json!({
            "target": "emoji",
            "tag": "idle",
            "show-layers": ["Hat"],
            "hide-layers": ["Sketch", "Guides"],
            "crop": { "x": 1, "y": 2, "width": 30, "height": 40 },
            "resize": "pad",
            "scaling": "integer",
            "integer-scale": "1/2",
            "start": 1.25,
            "title": "pig",
            "watermark": false,
        });
        let mut webm = picture.clone();
        webm.as_object_mut().unwrap().extend(
            json!({
                "fit": "crf",
                "attempts": 5,
                "fit-range": { "min": 4.0, "max": 40.5 },
                "length": 2.0,
                "tempo": 1.5,
                "fps": 24.0,
                "bitrate": 600.0,
                "crf": 30,
                "effort": "best",
                "lossless": false,
                "spoof": "never",
                "fake-duration": 0.5,
                "encoder-options": { "tune-content": "screen" },
                "ffmpeg-args": ["-x", "a b"],
            })
            .as_object()
            .unwrap()
            .clone(),
        );
        let tgs = json!({
            "target": "emoji",
            "start": 0.5,
            "length": 1.5,
            "tempo": 0.5,
            "effort": "fast",
            "exact": false,
            "reductions": ["merge-colours", "drop-frames"],
            "compromise": "detail",
            "crop": { "x": 0, "y": 0, "width": 8, "height": 8 },
            "resize": "stretch",
            "keep-canvas": true,
            "pixel-scale": 2,
            "tag": "run",
            "show-layers": ["Outline"],
            "hide-layers": ["Sketch"],
            "sheet": "4x2",
            "sheet-frames": 7,
            "fps": 12.5,
            "title": "pig",
            "watermark": true,
        });
        let old_webm = json!({ "speed": "best", "scaling": "pixel-perfect", "exact-scale": "2" });
        let new_webm = json!({ "scaling": "integer", "integer-scale": "2", "effort": "best" });
        let old_picture = json!({ "scaling": "pixel-perfect", "exact-scale": "2" });
        let new_picture = json!({ "scaling": "integer", "integer-scale": "2" });
        for (format, given, written) in [
            (Format::Webm, &webm, &webm),
            (Format::Webp, &picture, &picture),
            (Format::Tgs, &tgs, &tgs),
            (Format::Webm, &old_webm, &new_webm),
            (Format::Webp, &old_picture, &new_picture),
            (Format::Tgs, &json!({ "speed": "fast" }), &json!({ "effort": "fast" })),
        ] {
            let args = args(&["-p", "none"]);
            assert_eq!(args.options.unused(format, given.as_object()), [""; 0], "{format:?}");
            let layered = layered(&presets, &config, format, &["-p", "none"], Some(given.clone()));
            layered.unwrap_or_else(|err| panic!("{format:?}: {err}"));
            // as read, before what has no say is set aside
            let read = json_options(given.as_object().unwrap(), format).unwrap();
            assert_eq!(&serde_json::to_value(&read).unwrap(), written, "{format:?}");
        }
    }

    fn default<T: Default>() -> T {
        T::default()
    }
}
