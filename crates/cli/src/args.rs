use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use tgradish_core::config::Threads;
use tgradish_core::ffmpeg::FfmpegChoice;
use tgradish_core::options::{self, Crop, IntegerScale, Range, Selection};
use tgradish_core::presets::Format;
use tgradish_core::telegram::Target;
use tgradish_core::tgradish_tgs::reduce;
use tgradish_core::tgs;
use tgradish_core::webm::{self, Options};

pub use crate::term::ColourChoice;

/// The help pages' sections after the options, each under its heading,
/// which [`sections`] adds; each line indented as clap indents options.
const EXAMPLES: &str =
    "  tgradish convert clip.mp4                 video sticker (clip.sticker.webm)
  tgradish convert clip.mp4 --target emoji  video emoji (clip.emoji.webm)
  tgradish convert walk.ase -f tgs          animated sticker (walk.sticker.tgs)
  tgradish convert photo.jpg -f webp        static sticker (photo.sticker.webp)
  tgradish watch ~/Videos -O ~/Stickers     convert new media as it appears
  tgradish inspect clip.sticker.webm        check against Telegram's limits
  tgradish inspect clip.mp4                 show size, frames, length and fps";

const CONVERT_EXAMPLES: &str =
    "  tgradish convert clip.mp4 -s 2 -t 5         5 s from 0:02, spoofed to pass
  tgradish convert frames/ --sequence -f tgs  join frames/ as one TGS sticker
  tgradish convert *.gif -O stickers -y       to stickers/, overwriting files
  tgradish convert -q *.png | xargs ls -l     output paths only, one per line";

const ENVIRONMENT: &str = "  TGRADISH_CONFIG  Config file path (overridden by --config).
  TGRADISH_FFMPEG  Path to ffmpeg or its directory (overrides the config;
                   overridden by --ffmpeg or --ffmpeg-from).
  TGRADISH_LANG    GUI language, such as de (overrides the config and the
                   system's language).
  NO_COLOR         Disable colours when --colour is auto.
  CLICOLOR_FORCE   Force colours when --colour is auto, even off a terminal.
  FORCE_HYPERLINK  Set to 1 to force file hyperlinks, or 0 to disable them.";

const EXIT_STATUS: &str = "  0      Success. watch also exits with 0 when cancelled.
  1      Something failed. With several inputs, this wins over 3.
  2      Invalid command-line arguments.
  3      Everything was converted or read, but Telegram would reject a
         result or an inspected file: for example a .tgs that --exact keeps
         too large, a WebM over the limit with --fit off, or an image of
         the wrong size.
  128+N  Stopped by signal N (SIGTERM or SIGHUP) on Unix.
  130    convert was cancelled by Ctrl-C or --cancel-on-stdin.
  141    stdout was closed early on Unix (1 on other platforms).";

/// `sections` under their headings, styled as clap styles its own.
fn sections(sections: &[(&str, &str)]) -> String {
    let style = *crate::ui::help_styles().get_header();
    let texts: Vec<String> = sections
        .iter()
        .map(|(heading, text)| format!("{style}{heading}:{style:#}\n{text}"))
        .collect();
    texts.join("\n\n")
}

fn top_help() -> String {
    sections(&[("Examples", EXAMPLES), ("Environment", ENVIRONMENT), ("Exit status", EXIT_STATUS)])
}

const CONVERT_STOPPING: &str =
    "  Ctrl-C while a WebM is being fitted stops fitting if an encode already
  fits: tgradish keeps the best encode so far and moves on to the next
  input. Otherwise Ctrl-C cancels: before anything fits, when pressed again
  within 2 seconds, and for every other format.";

fn convert_help() -> String {
    sections(&[
        ("Examples", CONVERT_EXAMPLES),
        ("Stopping", CONVERT_STOPPING),
        ("Exit status", EXIT_STATUS),
    ])
}

fn config_help() -> String {
    sections(&[(
        "Examples",
        "  tgradish config init   start a config file where tgradish reads it",
    )])
}

/// The exit statuses alone, for the commands with no examples of their
/// own: inspect, watch and spoof.
fn exit_status_help() -> String {
    sections(&[("Exit status", EXIT_STATUS)])
}

/// What `--version` says, `-V` saying the version alone: how this build
/// was made, with which ffmpeg and whether it has the GUI.
fn long_version() -> &'static str {
    static TEXT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    TEXT.get_or_init(|| {
        let ffmpeg = if cfg!(feature = "linked") { "ffmpeg built in" } else { "system ffmpeg" };
        let gui = if cfg!(feature = "gui") { "with the GUI" } else { "no GUI" };
        format!("{} ({ffmpeg}; {gui})", env!("CARGO_PKG_VERSION"))
    })
}

const TGS: &str = "Animated stickers (TGS)";
const ASEPRITE: &str = "Aseprite files";

/// Telegram video, static and animated stickers and emoji from videos,
/// images or pixel art.
///
/// Video stickers longer than Telegram's 3-second limit can have their
/// container duration spoofed so Telegram accepts them.
#[derive(Debug, Parser)]
#[command(version, long_version = long_version(), max_term_width = 100,
          styles = crate::ui::help_styles(), after_help = top_help())]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
    #[command(flatten, next_help_heading = "Global options")]
    pub global: Global,
}

#[derive(Debug, Args)]
pub struct Global {
    /// Print JSON Lines (one compact object per line) instead of text.
    /// `convert` and `watch` emit one event per line, the same for every
    /// format, and `spoof` emits one event.
    /// `inspect` emits one object per file, `preset list` one per preset,
    /// `licenses` one per notice, and other commands one object. Errors are
    /// emitted as error events. Not supported by `gui`.
    #[arg(long, global = true)]
    pub json: bool,
    /// Show more output; use -vv to include ffmpeg logs.
    #[arg(short, long, global = true, action = clap::ArgAction::Count, conflicts_with = "quiet")]
    pub verbose: u8,
    /// Print only errors and warnings on stderr, and output file paths (one
    /// per line) on stdout.
    #[arg(short, long, global = true)]
    pub quiet: bool,
    /// Path to the ffmpeg executable or a directory containing ffmpeg and
    /// ffprobe. Overrides TGRADISH_FFMPEG and the config file.
    #[arg(long, global = true, value_name = "PATH", conflicts_with = "ffmpeg_from")]
    pub ffmpeg: Option<PathBuf>,
    /// Choose which ffmpeg to use: auto (built-in if this build has it,
    /// otherwise the one on PATH), builtin, or system. Overrides
    /// TGRADISH_FFMPEG and the config file. [default: config file, or auto]
    #[arg(long, global = true, value_name = "WHERE", value_enum)]
    pub ffmpeg_from: Option<FfmpegFrom>,
    /// Path to a config file. Overrides TGRADISH_CONFIG and the default
    /// path (`tgradish config path`).
    #[arg(long, global = true, value_name = "FILE")]
    pub config: Option<PathBuf>,
    /// Control colours, file hyperlinks, progress in the terminal's tab or
    /// taskbar, and inline previews. Respects NO_COLOR and CLICOLOR_FORCE
    /// when set to auto. [default: auto]
    #[arg(
        long = "colour",
        alias = "color",
        global = true,
        value_enum,
        default_value_t = ColourChoice::Auto,
        hide_default_value = true,
        value_name = "WHEN"
    )]
    pub colour: ColourChoice,
}

/// The shells `tgradish completions` writes scripts for.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ShellArg {
    Bash,
    Zsh,
    Fish,
    Powershell,
    Nushell,
}

/// [`FfmpegChoice`] for clap.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum FfmpegFrom {
    Auto,
    Builtin,
    System,
}

impl From<FfmpegFrom> for FfmpegChoice {
    fn from(value: FfmpegFrom) -> Self {
        match value {
            FfmpegFrom::Auto => FfmpegChoice::Auto,
            FfmpegFrom::Builtin => FfmpegChoice::Builtin,
            FfmpegFrom::System => FfmpegChoice::System,
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Convert videos or images into stickers or custom emoji.
    #[command(after_help = convert_help())]
    Convert(ConvertArgs),
    /// Watch a directory and convert new videos and images automatically.
    #[command(after_help = exit_status_help())]
    Watch(WatchArgs),
    /// Spoof the duration of an existing WebM file so Telegram accepts it.
    #[command(after_help = exit_status_help())]
    Spoof(SpoofArgs),
    /// Show the properties of WebM, WebP, TGS or source media files and
    /// check them against Telegram's limits.
    #[command(after_help = exit_status_help())]
    Inspect(InspectArgs),
    /// List and inspect presets.
    #[command(subcommand)]
    Preset(PresetCommand),
    /// Show, locate or create the config file.
    #[command(subcommand, after_help = config_help())]
    Config(ConfigCommand),
    /// Show which ffmpeg is active and whether it supports video stickers.
    ///
    /// Running `tgradish ffmpeg` without a subcommand runs `status`.
    Ffmpeg {
        #[command(subcommand)]
        command: Option<FfmpegCommand>,
    },
    /// Print the licences of tgradish and everything built into it.
    #[command(alias = "licences")]
    Licenses,
    /// Print a shell completion script for tgradish.
    ///
    /// For bash: `tgradish completions bash >
    /// ~/.local/share/bash-completion/completions/tgradish`.
    /// For nushell: `tgradish completions nushell | save -f
    /// ($nu.user-autoload-dirs.0 | path join tgradish.nu)`.
    /// Packages (.deb, .rpm, AUR and Nix) install completions for every
    /// shell except PowerShell.
    Completions {
        /// Target shell.
        shell: ShellArg,
    },

    /// Open the GUI, optionally loading FILES (or adding them to a running
    /// window).
    ///
    /// The GUI also opens automatically when tgradish is launched without
    /// arguments outside a terminal.
    #[cfg(feature = "gui")]
    Gui {
        /// Images, videos or frame directories to open.
        #[arg(value_name = "FILES")]
        files: Vec<PathBuf>,
    },
}

impl Cli {
    /// Checks for CLI misuse that clap cannot catch: `--json` with `gui`
    /// (which prints no output), `-` for stdin/stdout, legacy `-i`, or a
    /// non-existent input named `true` or `false` after a boolean flag (such
    /// as `--watermark false`).
    pub fn misused(&self) -> Option<String> {
        if self.command.is_gui() && self.global.json {
            return Some("--json is not supported by gui".into());
        }
        // `-` (used for stdin/stdout in other tools) would otherwise be treated
        // as a literal filename.
        let dash = |path: &Option<PathBuf>| path.as_deref() == Some(std::path::Path::new("-"));
        let paths: Vec<Option<PathBuf>> = match &self.command {
            Command::Convert(args) => args
                .inputs
                .iter()
                .cloned()
                .map(Some)
                .chain([args.output.clone(), args.conversion.output_dir.clone()])
                .collect(),
            Command::Watch(args) => {
                vec![Some(args.dir.clone()), args.conversion.output_dir.clone()]
            }
            Command::Spoof(args) => vec![Some(args.input.clone()), args.output(), args.to.clone()],
            Command::Inspect(args) => args.files.iter().cloned().map(Some).collect(),
            _ => Vec::new(),
        };
        if paths.iter().any(dash) {
            return Some(
                "reading from stdin or writing to stdout via '-' is not supported; pass a file path"
                    .into(),
            );
        }
        let Command::Convert(args) = &self.command else { return None };
        if !args.old_input.is_empty() {
            let inputs: Vec<String> = args
                .old_input
                .iter()
                .chain(&args.inputs)
                .map(|path| {
                    let path = path.to_string_lossy();
                    shlex::try_quote(&path)
                        .map_or_else(|_| path.to_string(), |quoted| quoted.into_owned())
                })
                .collect();
            return Some(format!(
                "tgradish 2.0 takes input paths without -i: tgradish convert {}",
                inputs.join(" ")
            ));
        }
        let word = args
            .inputs
            .iter()
            .find(|input| matches!(input.to_str(), Some("true" | "false")) && !input.exists())?;
        Some(format!(
            "no such input '{}': boolean flags take no value (use --watermark or --no-watermark)",
            word.display()
        ))
    }
}

impl Command {
    /// Returns whether this command uses the config file. Commands that do not
    /// need it skip loading it so a broken config never blocks them (the GUI
    /// loads its own config).
    pub fn reads_config(&self) -> bool {
        match self {
            Command::Convert(_)
            | Command::Watch(_)
            | Command::Inspect(_)
            | Command::Preset(PresetCommand::List)
            | Command::Config(ConfigCommand::Show)
            | Command::Ffmpeg { .. } => true,
            Command::Spoof(_)
            | Command::Preset(_)
            | Command::Config(_)
            | Command::Licenses
            | Command::Completions { .. } => false,
            #[cfg(feature = "gui")]
            Command::Gui { .. } => false,
        }
    }

    /// Returns whether this command launches the GUI (which handles Ctrl-C by
    /// closing its window and sets its own client marker in output stickers).
    pub fn is_gui(&self) -> bool {
        #[cfg(feature = "gui")]
        {
            matches!(self, Command::Gui { .. })
        }
        #[cfg(not(feature = "gui"))]
        {
            false
        }
    }
}

#[derive(Debug, Args)]
pub struct ConvertArgs {
    /// Videos or images to convert (directories only with --sequence).
    #[arg(required_unless_present_any = ["clipboard", "old_input"])]
    pub inputs: Vec<PathBuf>,
    /// Legacy 1.x input flag, kept hidden to give a clear error message.
    #[arg(short = 'i', hide = true, value_name = "FILE")]
    pub old_input: Vec<PathBuf>,
    /// Convert files, a file path or an image from the clipboard.
    #[arg(long, conflicts_with = "inputs")]
    pub clipboard: bool,
    /// Output file path (single input or --sequence only). [default: named
    /// after the input, beside it or in --output-dir]
    #[arg(short, long, value_name = "FILE", conflicts_with = "output_dir")]
    pub output: Option<PathBuf>,
    /// Join input images (or directories of images) into one animation in
    /// natural filename order. Without -o, the output is named after the
    /// first input (walk/ produces walk.sticker.webm; walk/1.png walk/2.png
    /// produces walk/1.sticker.webm).
    #[arg(long, conflicts_with = "clipboard")]
    pub sequence: bool,
    /// Output format: webm (VP9 video sticker or emoji, from any video or
    /// image), tgs (animated vector sticker or emoji, for pixel art or a
    /// video of it) or webp (static sticker or emoji from an image or the
    /// frame at --start; aliases: static, image). [default: inferred from
    /// -o extension, or webm]
    #[arg(short, long, value_enum)]
    pub format: Option<FormatArg>,
    #[command(flatten)]
    pub conversion: ConversionArgs,
}

#[derive(Debug, Args)]
pub struct WatchArgs {
    /// Directory to watch.
    pub dir: PathBuf,
    /// Watch subdirectories as well.
    #[arg(short, long)]
    pub recursive: bool,
    /// Convert existing files that do not already have an output.
    #[arg(long)]
    pub existing: bool,
    /// Polling interval in seconds (0.05 to 86400). [default: 1]
    #[arg(long, value_name = "SECONDS", default_value_t = 1.0, hide_default_value = true,
          value_parser = parse_interval)]
    pub interval: f64,
    /// Output format: webm (VP9 video sticker or emoji, from any video or
    /// image), tgs (animated vector sticker or emoji, for pixel art or a
    /// video of it) or webp (static sticker or emoji from an image or the
    /// frame at --start; aliases: static, image). [default: webm]
    #[arg(short, long, value_enum)]
    pub format: Option<FormatArg>,
    #[command(flatten)]
    pub conversion: ConversionArgs,
}

/// Flags shared by `convert` and `watch`.
#[derive(Debug, Args)]
pub struct ConversionArgs {
    /// Directory for output files (named NAME.sticker.EXT or
    /// NAME.emoji.EXT). [default: beside each input]
    #[arg(short = 'O', long, value_name = "DIR")]
    pub output_dir: Option<PathBuf>,
    /// Overwrite existing output files.
    #[arg(short = 'y', long)]
    pub overwrite: bool,
    /// Base preset: fast, balanced, best or a custom preset (see `tgradish
    /// preset list`). [default: balanced for webm and webp, best for tgs,
    /// unless the config's preset or tgs-preset names another]
    #[arg(short, long)]
    pub preset: Option<String>,
    /// JSON object of options applied after the preset and before CLI
    /// flags. Keys are the option flags' names without the leading dashes.
    #[arg(long, value_name = "JSON")]
    pub options_json: Option<String>,
    /// Keep intermediate WebM files (such as first-pass logs) and print
    /// their directory.
    #[arg(long)]
    pub keep_temp: bool,
    /// Maximum threads per conversion (for ffmpeg and TGS fitting), or
    /// auto (as many as each part makes use of). Files are still converted
    /// one at a time. Overrides the config file. [default: config file, or
    /// auto]
    #[arg(long, value_name = "N|auto", value_parser = |text: &str| text.parse::<Threads>())]
    pub threads: Option<Threads>,
    /// Stop, as Ctrl-C does, when stdin receives a line reading `cancel`
    /// (for front-ends, which cannot always send Ctrl-C). A line reading
    /// `settle` stops fitting a WebM once an encode fits and keeps the best
    /// so far. Without this flag stdin is left untouched for caller scripts.
    #[arg(long)]
    pub cancel_on_stdin: bool,
    #[command(flatten)]
    pub options: OptionArgs,
}

/// One flag per [`Options`] and [`tgs::Options`] field, with the same name.
#[derive(Debug, Args)]
pub struct OptionArgs {
    /// Sticker or custom emoji. Sets dimensions and size limits: stickers
    /// are 512 px on the longer side (256 KiB max as WebM), emoji are
    /// 100×100 (64 KiB max as WebM), WebP is 512 KiB max either way, and
    /// TGS is 512×512 (64 KiB max) either way. [default: sticker]
    #[arg(long, value_enum, help_heading = "Output")]
    pub target: Option<TargetArg>,
    /// Region of the input to keep, in source pixels: WIDTHxHEIGHT+X+Y from
    /// the top-left corner (for example, 640x360+100+50). [default: full
    /// frame]
    #[arg(long, value_name = "WxH+X+Y", help_heading = "Output")]
    pub crop: Option<Crop>,
    /// How to fit the input into the target dimensions. TGS requires a
    /// square output (pad, crop, stretch, or contain if the input or crop
    /// is already square). [default: contain for stickers, pad for emoji
    /// and TGS]
    #[arg(long, value_enum, help_heading = "Output")]
    pub resize: Option<ResizeArg>,
    /// Scaling algorithm: smooth, sharp (nearest-neighbour for pixel art),
    /// or integer (whole-number multiples with transparent padding).
    /// [default: auto (sharp for pixel art upscaled 2x or more)]
    #[arg(long, value_enum, help_heading = "Output")]
    pub scaling: Option<ScalingArg>,
    /// Scale by an exact integer N (or 1/N) so input pixels line up with
    /// output pixels, with transparent padding. The crop must fit within
    /// the target size (at 3x: max 170 px on the longer side for stickers,
    /// 33 px for emoji). Pass --integer-scale= to clear a preset value.
    /// [default: none]
    #[arg(long, value_name = "N", alias = "exact-scale", help_heading = "Output")]
    pub integer_scale: Option<IntegerScale>,
    /// Start offset in seconds (or the timestamp of the frame to extract
    /// for WebP). [default: 0]
    #[arg(short = 's', long, value_name = "SECONDS", value_parser = checked("start"),
          help_heading = "Output")]
    pub start: Option<f64>,
    /// Maximum duration in seconds to read from the input. [default: full
    /// remaining duration]
    #[arg(short = 't', long, value_name = "SECONDS", value_parser = checked("length"),
          help_heading = "Output")]
    pub length: Option<f64>,
    /// Playback speed multiplier (0.1 to 10). [default: 1 for WebM; for
    /// TGS, sped up to fit within 3 seconds and trimmed if still longer]
    #[arg(long, value_name = "TIMES", value_parser = checked("tempo"), help_heading = "Output")]
    pub tempo: Option<f64>,
    /// Output frame rate for WebM (max 60; values above the input rate only
    /// duplicate frames), or playback frame rate for image sequences and
    /// TGS sprite sheets. [default: input frame rate, or 12 for sequences]
    #[arg(long, value_parser = checked("fps"), help_heading = "Output")]
    pub fps: Option<f64>,

    /// Parameter to adjust to get close to the WebM size limit (256 KiB for
    /// stickers, 64 KiB for emoji). [default: auto]
    #[arg(long, value_enum, help_heading = "Size fitting")]
    pub fit: Option<FitArg>,
    /// Maximum encoding attempts during size fitting (1 to 50). [default:
    /// 8]
    #[arg(long, value_name = "N",
          value_parser = clap::value_parser!(u32).range(1..=i64::from(webm::MAX_ATTEMPTS)),
          help_heading = "Size fitting")]
    pub attempts: Option<u32>,
    /// Search range for size fitting, in the units of --fit (kbit/s, CRF,
    /// fps or seconds). [default: depends on --fit]
    #[arg(long, value_name = "MIN..MAX", help_heading = "Size fitting")]
    pub fit_range: Option<Range>,

    /// Target video bitrate in kbit/s when not fitting bitrate. [default:
    /// calculated from the size limit]
    #[arg(short, long, value_name = "KBPS", value_parser = checked("bitrate"),
          help_heading = "Encoding")]
    pub bitrate: Option<f64>,
    /// Constant quality level, 0 (best) to 63 (worst), used when fitting
    /// fps or length or with --fit off. [default: 32]
    #[arg(long, value_parser = clap::value_parser!(u8).range(0..=63), help_heading = "Encoding")]
    pub crf: Option<u8>,
    /// Encoder effort. Lower effort is faster, but reduces WebM quality at
    /// the same size and produces larger TGS files. [default: preset value
    /// (balanced for WebM, best for TGS)]
    #[arg(long, value_enum, alias = "speed", help_heading = "Encoding")]
    pub effort: Option<EffortArg>,
    /// Enable lossless encoding (only practical for small or static
    /// inputs); use --no-lossless to override a preset. [default: off]
    #[arg(long, overrides_with = "no_lossless", help_heading = "Encoding")]
    pub lossless: bool,
    #[arg(long, overrides_with = "lossless", hide = true)]
    pub no_lossless: bool,
    /// Pass a libvpx-vp9 encoder option (repeatable), such as
    /// tune-content=screen (flat graphics), aq-mode=2, sharpness=4,
    /// arnr-strength=3, g=60 (keyframe interval) or qmax=50. Run `ffmpeg -h
    /// encoder=libvpx-vp9` for the full list. Replaces preset options; pass
    /// --encoder-options= to clear them.
    #[arg(long = "encoder-options", visible_alias = "encoder-option",
          value_name = "NAME=VALUE", value_parser = parse_option_pair,
          help_heading = "Encoding")]
    pub encoder_options: Vec<Option<(String, String)>>,
    /// Extra raw ffmpeg output arguments, split using shell quoting rules.
    /// Requires an external ffmpeg (--ffmpeg-from system or --ffmpeg PATH).
    #[arg(long, value_name = "ARGS", allow_hyphen_values = true, help_heading = "Encoding")]
    pub ffmpeg_args: Option<ShellWords>,

    /// When to spoof WebM duration so Telegram accepts videos longer than 3
    /// seconds. [default: auto, only when needed]
    #[arg(long, value_enum, help_heading = "Metadata")]
    pub spoof: Option<SpoofArg>,
    /// Container duration in seconds written when spoofing. [default: a
    /// short duration accepted by Telegram]
    #[arg(long, value_name = "SECONDS", value_parser = checked("fake-duration"),
          help_heading = "Metadata")]
    pub fake_duration: Option<f64>,
    /// Title metadata to store in the output file.
    #[arg(long, help_heading = "Metadata")]
    pub title: Option<String>,
    /// Write tgradish metadata into the output (the Lottie name for TGS);
    /// use --no-watermark to omit it. [default: on]
    #[arg(long, overrides_with = "no_watermark", help_heading = "Metadata")]
    pub watermark: bool,
    #[arg(long, overrides_with = "watermark", hide = true)]
    pub no_watermark: bool,

    /// Never alter pixel art to fit; report a sticker that stays too large.
    /// Use --no-exact to allow lossy reductions. [default: on]
    #[arg(long, overrides_with = "no_exact", help_heading = TGS)]
    pub exact: bool,
    #[arg(long, overrides_with = "exact", hide = true)]
    pub no_exact: bool,
    /// Comma-separated list of reductions allowed when fitting a TGS file.
    /// [default: all]
    #[arg(long, value_enum, value_delimiter = ',', help_heading = TGS)]
    pub reductions: Vec<KindArg>,
    /// What TGS fitting sacrifices first: motion (merge or drop frames,
    /// keep detail), detail (coarser image, smooth motion), or auto (least
    /// visible change). [default: auto]
    #[arg(long, value_enum, help_heading = TGS)]
    pub compromise: Option<CompromiseArg>,
    /// Keep the full input canvas instead of cropping transparent margins;
    /// use --no-keep-canvas to override a preset. [default: off]
    #[arg(long, overrides_with = "no_keep_canvas", help_heading = TGS)]
    pub keep_canvas: bool,
    #[arg(long, overrides_with = "keep_canvas", hide = true)]
    pub no_keep_canvas: bool,
    /// Size of one art pixel in source pixels; misaligned edges are snapped
    /// to this grid. [default: auto-detected]
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u32).range(1..),
          help_heading = TGS)]
    pub pixel_scale: Option<u32>,
    /// Treat the input as a sprite sheet of equal cells, read row by row.
    /// [default: off]
    #[arg(long, value_name = "COLUMNSxROWS", help_heading = TGS)]
    pub sheet: Option<options::Sheet>,
    /// Number of frames to read from a sprite sheet. [default: all cells
    /// except trailing empty ones]
    #[arg(long, value_name = "N", help_heading = TGS)]
    pub sheet_frames: Option<u32>,

    /// Aseprite tag to export. [default: all frames]
    #[arg(long, help_heading = ASEPRITE)]
    pub tag: Option<String>,
    /// Aseprite layer to show (including hidden layers; repeatable).
    /// [default: visible layers in the file]
    #[arg(long = "show-layers", visible_alias = "show-layer", value_name = "NAME",
          help_heading = ASEPRITE)]
    pub show_layers: Vec<String>,
    /// Aseprite layer to hide (repeatable; hiding a group hides its
    /// children). [default: hidden layers in the file]
    #[arg(long = "hide-layers", visible_alias = "hide-layer", value_name = "NAME",
          help_heading = ASEPRITE)]
    pub hide_layers: Vec<String>,
}

impl OptionArgs {
    /// The options given that `format` doesn't use, as core says, named by
    /// their flags; and those of `json`, the object `--options-json` gave,
    /// named as keys in it.
    pub fn unused(
        &self,
        format: Format,
        json: Option<&serde_json::Map<String, serde_json::Value>>,
    ) -> Vec<String> {
        let mut flags = options::set_keys(&self.to_selection());
        flags.extend(options::set_keys(&self.to_options()));
        flags.extend(options::set_keys(&self.to_tgs_options()));
        let json: Vec<&str> =
            json.map_or(Vec::new(), |json| json.keys().map(String::as_str).collect());
        let mut named: Vec<String> = format
            .unused(flags.iter().map(String::as_str))
            .iter()
            .map(|key| format!("--{key}"))
            .collect();
        named.extend(format.unused(json).iter().map(|key| format!("{key:?} in --options-json")));
        named
    }

    /// What the flags choose of one input: [`OptionArgs::to_options`] and
    /// [`OptionArgs::to_tgs_options`] hold none of this.
    pub fn to_selection(&self) -> Selection {
        Selection {
            target: self.target.map(Into::into),
            tag: self.tag.clone(),
            show_layers: names(&self.show_layers),
            hide_layers: names(&self.hide_layers),
            crop: self.crop,
            start: self.start,
            length: self.length,
            title: self.title.clone(),
            sheet: self.sheet,
            sheet_frames: self.sheet_frames,
        }
    }

    pub fn to_tgs_options(&self) -> tgs::Options {
        tgs::Options {
            tempo: self.tempo,
            effort: self.effort.map(Into::into),
            exact: switch(self.exact, self.no_exact),
            reductions: (!self.reductions.is_empty())
                .then(|| self.reductions.iter().map(|&kind| kind.into()).collect()),
            compromise: self.compromise.map(Into::into),
            resize: self.resize.map(Into::into),
            keep_canvas: switch(self.keep_canvas, self.no_keep_canvas),
            pixel_scale: self.pixel_scale,
            fps: self.fps,
            watermark: switch(self.watermark, self.no_watermark),
            ..tgs::Options::default()
        }
    }

    pub fn to_options(&self) -> Options {
        Options {
            resize: self.resize.map(Into::into),
            scaling: self.scaling.map(Into::into),
            integer_scale: self.integer_scale,
            fit: self.fit.map(Into::into),
            attempts: self.attempts,
            fit_range: self.fit_range,
            tempo: self.tempo,
            fps: self.fps,
            bitrate: self.bitrate,
            crf: self.crf,
            effort: self.effort.map(Into::into),
            lossless: switch(self.lossless, self.no_lossless),
            spoof: self.spoof.map(Into::into),
            fake_duration: self.fake_duration,
            watermark: switch(self.watermark, self.no_watermark),
            encoder_options: (!self.encoder_options.is_empty())
                .then(|| self.encoder_options.iter().flatten().cloned().collect()),
            ffmpeg_args: self.ffmpeg_args.clone().map(|words| words.0),
            ..Options::default()
        }
    }
}

/// What a `--x` / `--no-x` pair says: on, off, or nothing, when neither
/// was given. The later of the two wins, as clap clears the other.
fn switch(on: bool, off: bool) -> Option<bool> {
    match (on, off) {
        (true, _) => Some(true),
        (_, true) => Some(false),
        _ => None,
    }
}

/// Names given, or `None` for none.
fn names(given: &[String]) -> Option<Vec<String>> {
    (!given.is_empty()).then(|| given.to_vec())
}

/// One argument split into several like a shell would.
#[derive(Debug, Clone)]
pub struct ShellWords(pub Vec<String>);

impl std::str::FromStr for ShellWords {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        shlex::split(s).map(ShellWords).ok_or("unbalanced quotes")
    }
}

/// Parses `watch --interval` seconds: at least 0.05 s (the cancellation poll
/// period) and at most 86400 s (one day, well within `Duration` limits).
fn parse_interval(text: &str) -> Result<f64, String> {
    let seconds: f64 = text.parse().map_err(|_| format!("{text:?} is not a number"))?;
    if !(0.05..=86_400.0).contains(&seconds) {
        return Err("must be from 0.05 to 86400 seconds".into());
    }
    Ok(seconds)
}

/// Builds a numeric value parser for option `key` using core's validation
/// rules and error messages.
fn checked(key: &'static str) -> impl Fn(&str) -> Result<f64, String> + Clone + Send + Sync {
    move |text| {
        let value: f64 = text.parse().map_err(|_| format!("{text:?} is not a number"))?;
        match options::wrong_value(key, value) {
            Some(reason) => Err(reason),
            None => Ok(value),
        }
    }
}

/// Parses `NAME=VALUE`, or an empty string to clear all options.
fn parse_option_pair(text: &str) -> Result<Option<(String, String)>, String> {
    if text.is_empty() {
        return Ok(None);
    }
    let (name, value) = text.split_once('=').ok_or("expected NAME=VALUE")?;
    Ok(Some((name.trim().to_string(), value.to_string())))
}

/// Mirrors a core enum as a clap value enum.
macro_rules! value_enum {
    ($name:ident => $core:path { $($(#[$meta:meta])* $variant:ident),* $(,)? }) => {
        #[derive(Debug, Clone, Copy, ValueEnum)]
        pub enum $name { $($(#[$meta])* $variant),* }

        impl From<$name> for $core {
            fn from(value: $name) -> Self {
                match value { $($name::$variant => <$core>::$variant),* }
            }
        }
    };
}

value_enum!(TargetArg => Target { Sticker, Emoji });
value_enum!(ResizeArg => options::Resize { Contain, Pad, Crop, Stretch });
value_enum!(ScalingArg => options::Scaling {
    Auto,
    Smooth,
    Sharp,
    #[value(alias = "pixel-perfect")]
    Integer,
});
value_enum!(FitArg => options::Fit { Auto, Bitrate, Crf, Fps, Length, Off });
value_enum!(EffortArg => options::Effort { Fast, Balanced, Best });
value_enum!(SpoofArg => options::Spoof { Auto, Always, Never });

/// Clap value enum for [`Format`], accepting `static` and legacy `image` as
/// aliases for WebP.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum FormatArg {
    Webm,
    Tgs,
    #[value(aliases = ["static", "image"])]
    Webp,
}

impl From<FormatArg> for Format {
    fn from(value: FormatArg) -> Self {
        match value {
            FormatArg::Webm => Format::Webm,
            FormatArg::Tgs => Format::Tgs,
            FormatArg::Webp => Format::Webp,
        }
    }
}
value_enum!(KindArg => reduce::Kind {
    SnapToGrid,
    MergeColours,
    MergeFrames,
    DropFrames,
    Despeckle,
    Downscale,
});
value_enum!(CompromiseArg => reduce::Compromise { Auto, Motion, Detail });

#[derive(Debug, Args)]
pub struct SpoofArgs {
    /// WebM file to spoof.
    pub input: PathBuf,
    /// Output file path (alternative to -o).
    #[arg(value_name = "OUTPUT", conflicts_with_all = ["output", "in_place"])]
    pub to: Option<PathBuf>,
    /// Output file path. [default: NAME.spoofed.webm beside the input]
    #[arg(short, long, value_name = "FILE", conflicts_with = "in_place")]
    pub output: Option<PathBuf>,
    /// Modify the input file in place.
    #[arg(short, long)]
    pub in_place: bool,
    /// Overwrite an existing output file.
    #[arg(short = 'y', long)]
    pub overwrite: bool,
    /// Container duration in seconds to write. [default: a short duration
    /// accepted by Telegram]
    #[arg(short = 'd', long, value_name = "SECONDS",
          default_value_t = webm::DEFAULT_FAKE_DURATION, hide_default_value = true)]
    pub fake_duration: f64,
    /// Title metadata to store in the file.
    #[arg(long)]
    pub title: Option<String>,
    /// Mark the file as modified by tgradish; use --no-watermark to leave
    /// metadata unchanged. [default: on]
    #[arg(long, overrides_with = "no_watermark")]
    pub watermark: bool,
    #[arg(long, overrides_with = "watermark", hide = true)]
    pub no_watermark: bool,
}

impl SpoofArgs {
    /// Returns the explicit output path from `-o` or positional `OUTPUT`.
    pub fn output(&self) -> Option<PathBuf> {
        self.output.clone().or_else(|| self.to.clone())
    }
}

#[derive(Debug, Args)]
pub struct InspectArgs {
    /// Sticker files (WebM, WebP or TGS) or source media (images,
    /// animations, Aseprite files or videos; videos require ffmpeg). Source
    /// media is reported as rejected by Telegram along with any size,
    /// duration or frame-rate issues a sticker keeping those properties
    /// would have.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Check WebM, WebP and source media as a sticker or custom emoji (TGS
    /// limits are identical for both). [default: inferred from dimensions,
    /// or sticker for source media]
    #[arg(long, value_enum)]
    pub target: Option<TargetArg>,
}

#[derive(Debug, Subcommand)]
pub enum PresetCommand {
    /// List all available presets.
    List,
    /// Show the resolved options of a preset.
    Show {
        /// Preset name (see `tgradish preset list`).
        name: String,
    },
    /// Print the user presets directory.
    Path,
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Print the config file path.
    Path,
    /// Print the effective configuration (config file values merged with
    /// defaults).
    Show,
    /// Print an annotated example config file.
    Example,
    /// Write the example config to the config path (see `config path`),
    /// creating its directory; never overwrites an existing file.
    Init,
}

#[derive(Debug, Subcommand)]
pub enum FfmpegCommand {
    /// Show which ffmpeg is active and whether it supports video stickers.
    Status,
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn cli_is_valid() {
        Cli::command().debug_assert();
    }

    /// `-V` prints only the version; `--version` includes build features.
    #[test]
    fn says_how_it_was_built() {
        let version = |flag: &str| {
            let err = Cli::try_parse_from(["tgradish", flag]).unwrap_err();
            assert_eq!(err.kind(), clap::error::ErrorKind::DisplayVersion);
            err.to_string()
        };
        assert_eq!(version("-V").trim_end(), format!("tgradish {}", env!("CARGO_PKG_VERSION")));
        let long = version("--version");
        let gui = if cfg!(feature = "gui") { "with the GUI" } else { "no GUI" };
        assert!(long.trim_end().ends_with(&format!("; {gui})")), "{long}");
        // Matches core's built-in backend availability regardless of feature
        // flag.
        let built_in = tgradish_core::backend::Backend::select(FfmpegChoice::Builtin, None).is_ok();
        assert_eq!(long.contains("ffmpeg built in"), built_in, "{long}");
        assert_eq!(long.contains("system ffmpeg"), !built_in, "{long}");
    }

    /// Every option states its default at most once (at the end of its help),
    /// and every visible argument has non-empty help text.
    #[test]
    fn says_each_default_once_at_the_end() {
        fn check(command: &clap::Command, path: &str) {
            for arg in command.get_arguments().filter(|arg| !arg.is_hide_set()) {
                let name = format!("{path} {}", arg.get_id());
                let help = arg.get_long_help().or(arg.get_help()).map(ToString::to_string);
                let help = help.unwrap_or_default();
                assert!(!help.trim().is_empty(), "{name} says nothing");
                assert!(help.matches("[default:").count() <= 1, "{name}: {help}");
                if let Some(start) = help.find("[default:") {
                    let tail = &help[start..];
                    assert_eq!(tail.find(']'), Some(tail.len() - 1), "{name}: {help}");
                }
            }
            for sub in command.get_subcommands() {
                check(sub, &format!("{path} {}", sub.get_name()));
            }
        }
        check(&Cli::command(), "tgradish");
    }

    /// Top-level help `about` matches `Cargo.toml`'s description (enforced
    /// across packages by xtask).
    #[test]
    fn says_what_tgradish_is_as_its_manifest_does() {
        let about = Cli::command().get_about().unwrap().to_string();
        assert_eq!(about, env!("CARGO_PKG_DESCRIPTION"));
    }

    /// Dumps every `--help` page into `$TGRADISH_STRINGS/cli.json` for
    /// `cargo xtask strings`.
    #[test]
    #[ignore]
    fn help_pages() {
        fn pages(command: &clap::Command, path: &str, out: &mut Vec<serde_json::Value>) {
            let mut command = command.clone().term_width(100);
            let text = command.render_long_help().to_string();
            out.push(serde_json::json!({ "command": path, "text": text }));
            for sub in command.get_subcommands().filter(|sub| sub.get_name() != "help") {
                pages(sub, &format!("{path} {}", sub.get_name()), out);
            }
        }
        let out = std::path::PathBuf::from(std::env::var_os("TGRADISH_STRINGS").unwrap());
        let mut cli = Cli::command().bin_name("tgradish");
        cli.build();
        let mut all = Vec::new();
        pages(&cli, "tgradish", &mut all);
        let json = serde_json::to_string_pretty(&all).unwrap();
        std::fs::write(out.join("cli.json"), json).unwrap();
    }

    /// `--options-json` names options as their flags do, so every option
    /// of either format has a flag of the same name.
    #[test]
    fn every_option_has_a_flag() {
        let cli = Cli::command();
        for command in ["convert", "watch"] {
            let command = cli.find_subcommand(command).unwrap();
            let flags: Vec<_> = command.get_arguments().filter_map(|a| a.get_long()).collect();
            for key in Selection::keys().chain(Options::keys()).chain(tgs::Options::keys()) {
                assert!(flags.contains(&key.as_str()), "no --{key} flag");
            }
        }
    }

    #[test]
    fn takes_a_number_of_threads_or_auto() {
        let threads = |command: &str, value: &str| {
            let given = match command {
                "convert" => ["tgradish", "convert", "a.gif", "--threads", value],
                _ => ["tgradish", "watch", "dir", "--threads", value],
            };
            Cli::try_parse_from(given).map(|cli| match cli.command {
                Command::Convert(args) => args.conversion.threads,
                Command::Watch(args) => args.conversion.threads,
                _ => unreachable!(),
            })
        };
        for command in ["convert", "watch"] {
            assert_eq!(threads(command, "auto").unwrap(), Some(Threads::Auto));
            assert_eq!(threads(command, "4").unwrap(), Some("4".parse().unwrap()));
            for wrong in ["0", "many", "1.5", ""] {
                let err = threads(command, wrong).unwrap_err();
                assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation, "{wrong:?}");
            }
        }
        let err = threads("convert", "0").unwrap_err().to_string();
        assert!(err.contains(r#"0 threads: expected at least 1, or "auto""#), "{err}");
        // left to the config without it
        let cli = Cli::parse_from(["tgradish", "convert", "a.gif"]);
        let Command::Convert(args) = cli.command else { panic!() };
        assert_eq!(args.conversion.threads, None);
    }

    #[test]
    fn sorts_flags_by_format() {
        let cli = Cli::parse_from([
            "tgradish",
            "convert",
            "a.gif",
            "--crf",
            "20",
            "--reductions",
            "merge-colours,drop-frames",
            "--compromise",
            "motion",
            "--keep-canvas",
            "--fps",
            "12",
            // what is read, for any format
            "--tag",
            "idle",
            "--hide-layer",
            "Back",
        ]);
        let Command::Convert(args) = cli.command else { panic!() };
        let options = &args.conversion.options;
        assert_eq!(options.unused(Format::Tgs, None), ["--crf"]);
        assert_eq!(
            options.unused(Format::Webm, None),
            ["--reductions", "--compromise", "--keep-canvas"]
        );
        let webp = options.unused(Format::Webp, None);
        assert!(!webp.iter().any(|flag| flag.contains("layer") || flag == "--tag"), "{webp:?}");
        // what JSON sets is held to the same
        let crop = serde_json::json!({ "x": 0, "y": 0, "width": 8, "height": 8 });
        let json = serde_json::json!({ "crf": 30, "spoof": "always", "crop": crop });
        let none = Cli::parse_from(["tgradish", "convert", "a.png"]);
        let Command::Convert(none) = none.command else { panic!() };
        assert_eq!(
            none.conversion.options.unused(Format::Webp, json.as_object()),
            ["\"crf\" in --options-json", "\"spoof\" in --options-json"]
        );
        let tgs = options.to_tgs_options();
        assert_eq!(tgs.reductions.unwrap(), [reduce::Kind::MergeColours, reduce::Kind::DropFrames]);
        assert_eq!(tgs.compromise, Some(reduce::Compromise::Motion));
        assert_eq!((tgs.keep_canvas, tgs.fps), (Some(true), Some(12.0)));
    }

    #[test]
    fn keeps_lossless_and_exact_apart() {
        let options = |flag: &str| {
            let cli = Cli::parse_from(["tgradish", "convert", "a.gif", flag]);
            let Command::Convert(args) = cli.command else { panic!() };
            args.conversion.options
        };
        // Lossless VP9 is WebM-only; keeping art exact is TGS-only.
        assert_eq!(options("--lossless").unused(Format::Tgs, None), ["--lossless"]);
        assert!(options("--lossless").unused(Format::Webm, None).is_empty());
        for format in [Format::Webm, Format::Webp] {
            assert_eq!(options("--exact").unused(format, None), ["--exact"]);
        }
        assert_eq!(options("--no-exact").to_tgs_options().exact, Some(false));
    }

    /// `-f` selects `--format` (matching ffmpeg); `--fit` has no short flag.
    #[test]
    fn reads_f_as_the_format() {
        let cli = Cli::parse_from(["tgradish", "convert", "a.png", "-f", "tgs"]);
        let Command::Convert(args) = cli.command else { panic!() };
        assert!(matches!(args.format, Some(FormatArg::Tgs)));
        assert!(Cli::try_parse_from(["tgradish", "convert", "a.png", "-f", "crf"]).is_err());
        let help = Cli::command().find_subcommand_mut("convert").unwrap().render_help();
        assert!(help.to_string().contains("-o, --output <FILE>"), "{help}");
    }

    #[cfg(feature = "gui")]
    #[test]
    fn opens_the_gui_with_the_files_named() {
        let cli = Cli::parse_from(["tgradish", "gui", "a.png", "frames"]);
        let Command::Gui { files } = cli.command else { panic!() };
        assert_eq!(files, [PathBuf::from("a.png"), PathBuf::from("frames")]);
    }

    #[test]
    fn spoofs_with_the_flag_convert_has() {
        for flag in ["--fake-duration=1.5", "-d1.5"] {
            let cli = Cli::parse_from(["tgradish", "spoof", "a.webm", flag]);
            let Command::Spoof(args) = cli.command else { panic!() };
            assert_eq!(args.fake_duration, 1.5, "{flag}");
        }
        assert!(Cli::try_parse_from(["tgradish", "spoof", "a.webm", "--duration=1"]).is_err());
    }

    /// Supports 1.x's positional `spoof INPUT OUTPUT` syntax and gives a clear
    /// migration hint for `convert -i`.
    #[test]
    fn takes_or_explains_what_1x_took() {
        let cli = Cli::parse_from(["tgradish", "spoof", "a.webm", "b.webm"]);
        let Command::Spoof(args) = cli.command else { panic!() };
        assert_eq!(args.output(), Some(PathBuf::from("b.webm")));
        for both in [&["-o", "c.webm"][..], &["-i"]] {
            let args = ["tgradish", "spoof", "a.webm", "b.webm"].iter().chain(both);
            assert!(Cli::try_parse_from(args).is_err(), "{both:?}");
        }
        let cli = Cli::parse_from(["tgradish", "convert", "-i", "my clip.mp4"]);
        let said = cli.misused().unwrap();
        assert_eq!(
            said,
            "tgradish 2.0 takes input paths without -i: tgradish convert 'my clip.mp4'"
        );
    }

    #[test]
    fn takes_intervals_watching_can_wait() {
        let interval = |text: &str| {
            let flag = format!("--interval={text}");
            let parsed = Cli::try_parse_from(["tgradish", "watch", "in", &flag]);
            parsed.map(|cli| match cli.command {
                Command::Watch(args) => args.interval,
                _ => panic!(),
            })
        };
        assert_eq!(interval("0.05").unwrap(), 0.05);
        assert_eq!(interval("86400").unwrap(), 86_400.0);
        for wrong in ["1e100", "inf", "NaN", "0", "-1", "0.01", "86401", "soon"] {
            let err = interval(wrong).unwrap_err();
            assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation, "{wrong}");
        }
    }

    #[test]
    fn parses_bool_flags() {
        let options = |flags: &[&str]| {
            let args = ["tgradish", "convert", "a.mp4"].iter().chain(flags);
            let Command::Convert(args) = Cli::parse_from(args).command else { panic!() };
            args.conversion.options.to_options()
        };
        let set = options(&["--lossless", "--no-watermark"]);
        assert_eq!((set.lossless, set.watermark, set.title), (Some(true), Some(false), None));
        assert_eq!(options(&[]).watermark, None, "neither leaves it to the preset");
        // the later of a pair wins
        assert_eq!(options(&["--no-watermark", "--watermark"]).watermark, Some(true));
        assert_eq!(options(&["--lossless", "--no-lossless"]).lossless, Some(false));
        // a value isn't taken: `--watermark false` used to make false an input
        for flag in ["--watermark=false", "--lossless=true", "--exact=false"] {
            assert!(Cli::try_parse_from(["tgradish", "convert", "a.mp4", flag]).is_err(), "{flag}");
        }
        let spoof = |flags: &[&str]| {
            let args = ["tgradish", "spoof", "a.webm"].iter().chain(flags);
            let Command::Spoof(args) = Cli::parse_from(args).command else { panic!() };
            args.watermark || !args.no_watermark
        };
        assert!(spoof(&[]) && !spoof(&["--no-watermark"]));
        assert!(spoof(&["--no-watermark", "--watermark"]));
    }

    #[test]
    fn splits_ffmpeg_args_like_a_shell() {
        let cli = Cli::parse_from([
            "tgradish",
            "convert",
            "a.mp4",
            "--ffmpeg-args",
            r#"-metadata comment="hello world" -tune-content screen"#,
        ]);
        let Command::Convert(args) = cli.command else { panic!() };
        let extra = args.conversion.options.to_options().ffmpeg_args.unwrap();
        assert_eq!(extra, ["-metadata", "comment=hello world", "-tune-content", "screen"]);
        assert!(Cli::try_parse_from(["tgradish", "convert", "a", "--ffmpeg-args", "'x"]).is_err());
    }

    /// Options parsed from the flags `options::flags` writes.
    fn parsed(flags: Vec<String>) -> OptionArgs {
        let mut args = vec!["tgradish".to_owned(), "convert".into(), "a.mp4".into()];
        args.extend(flags);
        let Command::Convert(convert) = Cli::parse_from(args).command else { panic!() };
        convert.conversion.options
    }

    #[test]
    fn writes_flags_that_read_back() {
        let selection = Selection {
            target: Some(Target::Emoji),
            tag: Some("idle".into()),
            // names with commas and spaces, one flag each, and ones that
            // start as flags do
            show_layers: Some(vec!["Shadow, soft".into(), "Hat".into()]),
            hide_layers: Some(vec!["Back ground".into(), "--old".into()]),
            crop: Some(Crop { x: 1, y: 2, width: 30, height: 40 }),
            start: Some(1.25),
            length: Some(2.0),
            title: Some("-_- a pig's \"sticker\"".into()),
            sheet: "4x2".parse().ok(),
            sheet_frames: Some(7),
        };
        let webm = Options {
            resize: Some(options::Resize::Pad),
            scaling: Some(options::Scaling::Integer),
            integer_scale: Some("1/2".parse().unwrap()),
            fit: Some(options::Fit::Crf),
            attempts: Some(5),
            fit_range: Some(Range { min: 4.0, max: 40.5 }),
            tempo: Some(1.5),
            fps: Some(24.0),
            bitrate: Some(600.0),
            crf: Some(30),
            effort: Some(options::Effort::Best),
            lossless: Some(false),
            spoof: Some(options::Spoof::Never),
            fake_duration: Some(0.5),
            watermark: Some(false),
            encoder_options: Some([("tune-content".into(), "screen".into())].into()),
            ffmpeg_args: Some(vec!["-x".into(), "a b".into()]),
            ..Options::default()
        };
        let read = parsed([options::flags(&selection), options::flags(&webm)].concat());
        assert_eq!((read.to_selection(), read.to_options()), (selection, webm));
        let selection = Selection {
            tag: Some("-run".into()),
            hide_layers: Some(vec!["Sketch".into(), "Guides, old".into()]),
            title: Some("-_-".into()),
            ..Selection::default()
        };
        let tgs = tgs::Options {
            tempo: Some(0.5),
            effort: Some(options::Effort::Fast),
            exact: Some(true),
            reductions: Some(vec![reduce::Kind::MergeColours, reduce::Kind::DropFrames]),
            compromise: Some(reduce::Compromise::Detail),
            resize: Some(options::Resize::Stretch),
            keep_canvas: Some(true),
            pixel_scale: Some(2),
            fps: Some(12.5),
            watermark: Some(true),
            ..tgs::Options::default()
        };
        let read = parsed([options::flags(&selection), options::flags(&tgs)].concat());
        assert_eq!((read.to_selection(), read.to_tgs_options()), (selection, tgs));
    }

    /// Flags renamed since keep their old names, hidden.
    #[test]
    fn reads_flags_by_their_old_names() {
        let old = ["--speed=best", "--scaling=pixel-perfect", "--exact-scale=2"];
        let options = parsed(old.map(String::from).to_vec());
        assert_eq!(options.to_options().effort, Some(options::Effort::Best));
        assert_eq!(options.to_tgs_options().effort, Some(options::Effort::Best));
        assert_eq!(options.to_options().scaling, Some(options::Scaling::Integer));
        assert_eq!(options.to_options().integer_scale, "2".parse().ok());
        let convert = Cli::command().find_subcommand("convert").unwrap().clone();
        let help = convert.clone().render_long_help().to_string();
        for (old, new) in options::RENAMED {
            assert!(!help.contains(&format!("--{old}")), "--{old} shows: {help}");
            assert!(!help.contains("pixel-perfect"), "{help}");
            let arg = convert.get_arguments().find(|arg| arg.get_long() == Some(new)).unwrap();
            assert!(arg.get_all_aliases().unwrap_or_default().contains(&old), "--{old}");
        }
    }

    #[test]
    fn collects_encoder_options() {
        let cli = Cli::parse_from([
            "tgradish",
            "convert",
            "a.mp4",
            "--encoder-options",
            "tune-content=screen",
            "--encoder-option",
            "arnr-strength=3",
        ]);
        let Command::Convert(args) = cli.command else { panic!() };
        let options = args.conversion.options.to_options().encoder_options.unwrap();
        assert_eq!(options["tune-content"], "screen");
        assert_eq!(options["arnr-strength"], "3");
        let bad = ["tgradish", "convert", "a", "--encoder-options", "no-equals"];
        assert!(Cli::try_parse_from(bad).is_err());
        // none, over a preset's: as options::flags writes it
        let none = Options { encoder_options: Some(Default::default()), ..Options::default() };
        assert_eq!(options::flags(&none), ["--encoder-options="]);
        assert_eq!(parsed(options::flags(&none)).to_options(), none);
        // and no integer scale, over a preset's
        let none = Options { integer_scale: Some(IntegerScale::NONE), ..Options::default() };
        assert_eq!(options::flags(&none), ["--integer-scale="]);
        assert_eq!(parsed(options::flags(&none)).to_options(), none);
    }
}
