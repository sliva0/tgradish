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
        "{} was already written from {}; inputs with the same name need different output \
         directories",
        output.display(),
        other.display()
    )]
    SameOutput { output: PathBuf, other: PathBuf },
    /// The folder the output goes in couldn't be made.
    #[error("couldn't create {}: {}", dir.display(), tgradish_core::io_reason(error))]
    Folder { dir: PathBuf, error: std::io::Error },
}

/// What Telegram would say of a result that was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Accepted,
    /// It was made, but Telegram would refuse it, as the issues printed
    /// with it say: the command exits with [`crate::REFUSED`].
    Refused,
}

impl Verdict {
    /// What Telegram would say of a result with `issues`, as
    /// [`tgradish_core::telegram::refuses`] them.
    fn of(issues: &[tgradish_core::telegram::Issue]) -> Verdict {
        if tgradish_core::telegram::refuses(issues) { Verdict::Refused } else { Verdict::Accepted }
    }
}

/// Runs conversions with the options and output settings of one command.
pub struct Converter<'a> {
    ctx: &'a Context,
    args: &'a ConversionArgs,
    /// The format's options: the preset's, then `--options-json`'s, then
    /// the flags'.
    options: FormatOptions,
    /// ffmpeg, found once for WebM and WebP, which need it for every
    /// input, so that a missing one fails the command before any; `.tgs`
    /// asks for it only for videos.
    backend: Option<Backend>,
    /// Outputs written by this command, and their inputs, so two inputs
    /// with the same name never overwrite each other's result.
    written: RefCell<HashMap<PathBuf, PathBuf>>,
    /// Whether `--fps`, or `--options-json`, set a frame rate: one that
    /// `.tgs` can't use for an input with times of its own is said to be
    /// unused, once.
    fps_given: bool,
    fps_unused_said: std::cell::Cell<bool>,
}

/// The options `--options-json` gave, as `format` takes them.
fn overlay<T: serde::de::DeserializeOwned>(
    json: &serde_json::Map<String, serde_json::Value>,
    format: Format,
) -> Result<T> {
    serde_json::from_value(json.clone().into()).map_err(|err| {
        crate::usage(format!("invalid --options-json for {} output: {err}", format.name()))
    })
}

/// Checks the options a command converts with, `options`, for values
/// wrong whatever the input, before any is converted: named by the flag
/// that gave one, or its key in `--options-json`, both mistakes in the
/// command line; or else the preset `preset`, which set it.
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
    bail!("the preset {preset:?} sets {option}, which {reason}")
}

/// The format a command converts to: `--format`, or else the extension of
/// `output` (the `-o` file), or else WebM.
fn format_of(format: Option<FormatArg>, output: Option<&Path>) -> Format {
    match (format, output.and_then(Format::of_path)) {
        (Some(format), _) => format.into(),
        (None, Some(format)) => format,
        (None, None) => Format::default(),
    }
}

/// Refuses an `-o` that can't hold the result, before any work: a
/// directory, or a name whose extension isn't the format's, which would
/// make a file that says it is something else.
fn check_output(output: &Path, format: Format) -> Result<()> {
    if output.is_dir() {
        return Err(crate::usage(format!(
            "{} is a directory: -o names the result's file, -O/--output-dir a directory for \
             results",
            output.display()
        )));
    }
    let wrong = |message: String| Err(crate::usage(message));
    match Format::of_path(output) {
        Some(asked) if asked == format => Ok(()),
        Some(asked) => wrong(format!(
            "{} is a .{} file, but this converts to {}: end it .{} or drop --format",
            output.display(),
            asked.extension(),
            format.name(),
            format.extension()
        )),
        None => wrong(format!(
            "{} doesn't end in .{}, as {} output does; .tgs and .webp ask for those formats",
            output.display(),
            format.extension(),
            format.name()
        )),
    }
}

impl<'a> Converter<'a> {
    /// `format` is `--format`, and `output` the `-o` file, which can
    /// choose the format instead.
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
                    |err| crate::usage(format!("--options-json isn't a JSON object: {err}")),
                )?,
            ),
            None => None,
        };
        let unused = args.options.unused(format, json.as_ref());
        if !unused.is_empty() {
            let unused = unused.join(", ");
            return Err(crate::usage(format!(
                "{unused} can't be used for {} output",
                format.name()
            )));
        }
        if args.keep_temp && format != Format::Webm {
            return Err(crate::usage(
                "--keep-temp is for WebM output, which alone leaves intermediate files",
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
                    return crate::usage(format!(
                        "--preset {name:?} doesn't exist; available: {known}"
                    ));
                }
                // the only other place a preset is named
                let key = if format == Format::Tgs { "tgs-preset" } else { "preset" };
                let file = ctx.config_file.as_ref().map(|file| file.path.display());
                let place = file.map_or(String::new(), |file| format!(" ({file}: {key})"));
                anyhow::anyhow!(
                    "the config's {key} {name:?} doesn't exist{place}; available: {known}"
                )
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
                        Some(_) => "--sheet-frames needs --sheet",
                        None => "\"sheet-frames\" in --options-json needs \"sheet\"",
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
                    "couldn't create {}: {}",
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

    /// Where the result for `input` goes, unless `-o` says otherwise: as
    /// core names it, in `--output-dir` when given, keeping the folders
    /// between `base` and the input there.
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

    /// The ffmpeg found for the command, or else the one it would use.
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
                "--fps only sets how fast image sequences and sprite sheets play for .tgs";
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

    /// What is said of a result once it is made, as text: where a WebM's
    /// intermediate files are kept, and, in terminals that show pictures,
    /// a frame of it.
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
        return Err(crate::usage("-o/--output names one result: give one input, or --sequence"));
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
        return Err(crate::usage("-o/--output names one result: give one input, or --sequence"));
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

/// Cancels when a front-end writes `cancel` to stdin, and keeps the best
/// WebM encode so far on `settle`, with `--cancel-on-stdin`: sending
/// Ctrl-C to a child process is awkward on
/// Windows. Only when asked, as a script reading its own stdin in a loop
/// would lose what this read. End of input is ignored, so running without
/// stdin is fine.
fn listen_for_cancel(ctx: &Context) {
    let cancel = ctx.cancel.clone();
    std::thread::spawn(move || {
        for line in std::io::stdin().lines().map_while(Result::ok) {
            match line.trim() {
                "cancel" => cancel.cancel(),
                // where nothing fits yet, there is nothing to keep
                "settle" => _ = cancel.settle(),
                _ => {}
            }
        }
    });
}

/// A progress bar, cargo-like: `verb` coloured on the left. It draws
/// nothing until [`Bar::show`] puts it on stderr in place of the one
/// before, as setting its message would draw it over that one.
fn progress_bar(verb: &str, length: Option<u64>) -> ProgressBar {
    let bar = ProgressBar::with_draw_target(length, ProgressDrawTarget::hidden());
    // the message last and wide, so that indicatif cuts it to the
    // terminal's width: a line longer than that wraps, and each redraw
    // would leave its first part behind
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

/// The progress bar a printer shows, if any, with lines printed above it.
#[derive(Default)]
struct Bar(Option<ProgressBar>);

impl Bar {
    /// Shows `bar` in place of the one before, which is cleared first: a
    /// bar drawn before then would leave the old one's line behind.
    fn show(&mut self, bar: ProgressBar) {
        self.clear();
        bar.set_draw_target(ProgressDrawTarget::stderr());
        bar.enable_steady_tick(Duration::from_millis(100));
        self.0 = Some(bar);
    }

    /// Prints a line above the progress bar.
    fn line(&self, text: impl AsRef<str>) {
        match &self.0 {
            // a hidden bar, when stderr is not a terminal, prints nothing
            Some(bar) if !bar.is_hidden() => bar.println(text),
            _ => eprintln!("{}", text.as_ref()),
        }
    }

    fn clear(&mut self) {
        if let Some(bar) = self.0.take() {
            bar.finish_and_clear();
        }
    }

    /// Clears the bar, and the progress the terminal shows.
    fn finish(&mut self) {
        self.clear();
        term::progress(None);
    }
}

/// Shows conversion events of any format as text, with a progress bar.
struct Printer {
    verbose: u8,
    quiet: bool,
    bar: Bar,
    /// Telegram's size limit for what is being made.
    limit: u64,
    /// For `.tgs`: the art is kept exactly as it is, so a sticker too
    /// large isn't fitted.
    exact: bool,
    /// For `.tgs`: the size unchanged, to show how much fitting has taken
    /// off.
    unchanged: Option<u64>,
}

impl Printer {
    fn new(ctx: &Context, exact: bool) -> Self {
        Self {
            verbose: ctx.global.verbose,
            quiet: ctx.global.quiet,
            bar: Bar::default(),
            limit: tgradish_core::telegram::MAX_STICKER_BYTES,
            exact,
            unchanged: None,
        }
    }

    fn spinner(&mut self, verb: &str, message: String) {
        if self.quiet {
            return;
        }
        let bar = progress_bar(verb, None);
        bar.set_message(message);
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
                    .line(ui::status("Compared", ui::dim(format!("likeness (SSIM) {ssim:.4}"))));
            }
            // why fitting stops, or tries a lower frame rate or not
            Event::Decided { message, .. } if !self.quiet => {
                self.bar.line(ui::status("Decided", ui::dim(message)));
            }
            Event::Log { line } if self.verbose > 1 => {
                self.bar.line(ui::dim(format!("    {line}")))
            }
            Event::TooLarge { bytes } if !self.quiet && self.exact => {
                self.bar.line(format!(
                    "{} about {} as it is, too large; --no-exact lets fitting change the art",
                    ui::warning_label(),
                    ui::size_within(bytes, self.limit)
                ));
            }
            Event::TooLarge { bytes } if !self.quiet => {
                self.unchanged = Some(bytes);
                self.bar.line(ui::status(
                    "Fitting",
                    format!("about {} as it is, too large", ui::size_within(bytes, self.limit)),
                ));
                term::progress(Some(0.3));
                self.spinner("Fitting", "into 64 KiB".into());
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
                // how much of the excess is gone, on a log scale
                if let Some(unchanged) = self.unchanged {
                    let (unchanged, now) = (unchanged as f64, step.bytes as f64);
                    let target = self.limit as f64;
                    let gone = ((unchanged / now).ln() / (unchanged / target).ln()).clamp(0.0, 1.0);
                    term::progress(Some(0.3 + 0.5 * gone));
                }
            }
            Event::Packing => {
                term::progress(Some(0.85));
                self.spinner("Compressing", "with zopfli".into());
            }
            Event::Warning { message } => {
                self.bar.line(format!("{} {message}", ui::warning_label()));
            }
            Event::Finished(outcome) => self.finished(&outcome),
            _ => {}
        }
    }

    /// The lines that start a conversion: what goes where, and what a
    /// WebM's plan or a `.tgs`'s art comes to.
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
                    line.push_str(&format!(", played {:.2}x as fast", report.tempo));
                }
                if report.trimmed {
                    line.push_str(", cut at 3 s");
                }
                self.bar.line(ui::status("Read", ui::dim(line)));
                if let Some(likely) = report.likely_scale {
                    self.bar.line(format!(
                        "{} the pixel art looks like {}x with {:.1}% of its edges off that grid; \
                         --pixel-scale {} snaps them",
                        ui::warning_label(),
                        likely.scale,
                        (1.0 - likely.fit) * 100.0,
                        likely.scale
                    ));
                }
                term::progress(Some(0.1));
                self.spinner("Converting", "to vector".into());
            }
            _ => {}
        }
    }

    /// The result's line on stdout, even when quiet, as every format's is,
    /// and what Telegram would have a problem with in it.
    fn finished(&mut self, outcome: &Outcome) {
        self.bar.finish();
        let size = ui::size_within(outcome.bytes, outcome.limit);
        let said = match &outcome.details {
            Details::Webm { params, spoofed, settled, .. } => {
                let spoofed = if *spoofed { ", duration spoofed" } else { "" };
                // the result line says it, where it says more than its path
                let settled = if *settled { ", the best so far as asked" } else { "" };
                format!(", {}{spoofed}{settled}", ui::params(params))
            }
            Details::Tgs { steps, .. } if outcome.changed && !steps.is_empty() => {
                format!(", {}", style("changed to fit").yellow())
            }
            Details::Tgs { .. } if outcome.changed => ", changed".to_string(),
            Details::Tgs { .. } => ", exact".to_string(),
            Details::Webp { width, height, encoding } => match encoding {
                webp::Encoding::Lossy { quality } => {
                    format!(", {width}×{height}, lossy at quality {quality} to fit")
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

/// The line on stdout of a file made, `output`: "Finished" and `line`,
/// which says what it is, or `quiet`, its path alone, for scripts.
fn finished(quiet: bool, output: &Path, line: String) {
    if quiet {
        outln!("{}", output.display());
    } else {
        outln!("{}", ui::status("Finished", line));
    }
}

/// What Telegram would have a problem with in a result, under its line,
/// on stderr.
fn print_issues(issues: &[tgradish_core::telegram::Issue]) {
    for issue in issues {
        eprintln!("{}", ui::issue(issue));
    }
}

/// The line that starts every conversion, whatever it makes: `input` →
/// `output`.
fn converting(input: &Path, output: &Path) -> String {
    let input = term::link(input.display(), input);
    let output = term::link(style(output.display()).bold(), output);
    ui::status("Converting", format!("{input} → {output}"))
}

/// A frame of a finished sticker, shown in terminals that show pictures.
fn show_result(preview: &tgradish_core::preview::Preview) {
    let Some((rgba, _)) = preview.frames.get(preview.frames.len() / 3) else { return };
    term::picture(rgba, preview.width, preview.height, 8);
}

fn reduction_text(reduction: &tgradish_core::tgradish_tgs::reduce::Reduction) -> String {
    use tgradish_core::tgradish_tgs::reduce::Reduction;
    match *reduction {
        Reduction::SnapToGrid { scale } => format!("snapped to the {scale}x pixel grid"),
        Reduction::MergeColours { distance } => format!("merged colours closer than {distance:.3}"),
        Reduction::MergeFrames { changed } => {
            format!("merged frames differing in under {:.1}% of the area", changed * 100.0)
        }
        Reduction::DropFrames { share } => format!("dropped {:.0}% of frames", share * 100.0),
        Reduction::Despeckle => "removed single stray pixels".to_string(),
        Reduction::Downscale { factor } => {
            format!("lowered the resolution to {:.0}%", factor * 100.0)
        }
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
