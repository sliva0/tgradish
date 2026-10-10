use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use tgradish_core::ffmpeg::FfmpegChoice;
use tgradish_core::options::{self, Options, Range};
use tgradish_core::presets::Format;
use tgradish_core::telegram::Target;
use tgradish_core::tgs::{self, TgsOptions};

const TGS: &str = "Animated stickers (.tgs)";

/// Converts videos into Telegram video stickers and emoji, with the ability
/// to bypass the 3 second limit, and pixel art into animated stickers.
#[derive(Debug, Parser)]
#[command(version, max_term_width = 100)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
    #[command(flatten)]
    pub global: Global,
}

#[derive(Debug, Args)]
pub struct Global {
    /// Print machine-readable JSON instead of text. `convert` prints one
    /// event per line, see `tgradish describe`.
    #[arg(long, global = true)]
    pub json: bool,
    /// Show more output, including ffmpeg logs with -vv.
    #[arg(short, long, global = true, action = clap::ArgAction::Count, conflicts_with = "quiet")]
    pub verbose: u8,
    /// Only print errors and results.
    #[arg(short, long, global = true)]
    pub quiet: bool,
    /// ffmpeg executable or directory with ffmpeg and ffprobe.
    #[arg(long, global = true, env = "TGRADISH_FFMPEG", value_name = "PATH")]
    pub ffmpeg: Option<PathBuf>,
    /// Which ffmpeg to use when no path is given. Default: from config, or
    /// auto (built in if this build has it, otherwise system).
    #[arg(long, global = true, value_name = "WHERE", value_enum)]
    pub ffmpeg_from: Option<FfmpegFrom>,
    /// Config file to use instead of the default one.
    #[arg(long, global = true, env = "TGRADISH_CONFIG", value_name = "FILE")]
    pub config: Option<PathBuf>,
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
    /// Convert videos or images into stickers or emoji.
    Convert(ConvertArgs),
    /// Convert every video or image that appears in a directory.
    Watch(WatchArgs),
    /// Spoof the duration of an existing WebM so Telegram accepts it.
    Spoof(SpoofArgs),
    /// Show properties of WebM and .tgs files and check them against
    /// Telegram's requirements.
    Inspect(InspectArgs),
    /// Print a JSON description of options, presets and events for
    /// front-ends.
    Describe,
    /// List and show presets.
    #[command(subcommand)]
    Preset(PresetCommand),
    /// Show the config file.
    #[command(subcommand)]
    Config(ConfigCommand),
    /// Check which ffmpeg is used.
    #[command(subcommand)]
    Ffmpeg(FfmpegCommand),
    /// Open the tgradish window. It also opens when tgradish is started
    /// without arguments outside a terminal.
    #[cfg(feature = "gui")]
    Gui,
}

#[derive(Debug, Args)]
pub struct ConvertArgs {
    /// Videos or images to convert.
    #[arg(required_unless_present = "clipboard")]
    pub inputs: Vec<PathBuf>,
    /// Convert what is on the clipboard: copied files, a copied path, or an
    /// image.
    #[arg(long, conflicts_with = "inputs")]
    pub clipboard: bool,
    /// Output file. Only with a single input, or with --sequence.
    #[arg(short, long, conflicts_with = "output_dir")]
    pub output: Option<PathBuf>,
    /// Join the inputs (images, or directories of them) into one animated
    /// sticker, in name order with numbers counted as numbers.
    #[arg(long, conflicts_with = "clipboard")]
    pub sequence: bool,
    #[command(flatten)]
    pub conversion: ConversionArgs,
}

#[derive(Debug, Args)]
pub struct WatchArgs {
    /// Directory to watch.
    pub dir: PathBuf,
    /// Also watch subdirectories.
    #[arg(short, long)]
    pub recursive: bool,
    /// Also convert files that are already there and have no result yet.
    #[arg(long)]
    pub existing: bool,
    /// Seconds between looks at the directory.
    #[arg(long, value_name = "SECONDS", default_value_t = 1.0)]
    pub interval: f64,
    #[command(flatten)]
    pub conversion: ConversionArgs,
}

/// Flags shared by `convert` and `watch`.
#[derive(Debug, Args)]
pub struct ConversionArgs {
    /// What to make: webm (video sticker or emoji, from any video or image)
    /// or tgs (animated sticker, from pixel art). [default: by the output's
    /// extension, then the preset, then webm]
    #[arg(long, value_enum)]
    pub format: Option<FormatArg>,
    /// Directory for results. Default: next to each input, as
    /// NAME.sticker.webm, NAME.emoji.webm or the same with .tgs.
    #[arg(short = 'O', long, value_name = "DIR")]
    pub output_dir: Option<PathBuf>,
    /// Replace existing output files.
    #[arg(short = 'y', long)]
    pub overwrite: bool,
    /// Preset to start from, see `tgradish preset list`.
    #[arg(short, long)]
    pub preset: Option<String>,
    /// Options as a JSON object, applied after the preset and before other
    /// flags. Keys are the flag names, see `tgradish describe`.
    #[arg(long, value_name = "JSON")]
    pub options_json: Option<String>,
    /// Keep intermediate files and print where they are.
    #[arg(long)]
    pub keep_temp: bool,
    #[command(flatten)]
    pub options: OptionArgs,
}

/// One flag per [`Options`] and [`TgsOptions`] field, with the same name.
#[derive(Debug, Args)]
pub struct OptionArgs {
    /// What to make. [default: sticker]
    #[arg(long, value_enum, help_heading = "Output")]
    pub target: Option<TargetArg>,
    /// How to scale into the target size. [default: contain for stickers,
    /// pad for emoji]
    #[arg(long, value_enum, help_heading = "Output")]
    pub resize: Option<ResizeArg>,
    /// Seconds to skip at the start of the input.
    #[arg(short = 's', long, value_name = "SECONDS", help_heading = "Output")]
    pub start: Option<f64>,
    /// Length of the result. [default: the rest of the input]
    #[arg(short = 't', long, value_name = "SECONDS", help_heading = "Output")]
    pub length: Option<f64>,
    /// Frame rate: of the result for WebM, at most 60 [default: the input's,
    /// at most 30], of sprite sheets and image sequences for .tgs [default:
    /// 10].
    #[arg(long, help_heading = "Output")]
    pub fps: Option<f64>,

    /// What to tune to get close to the size limit: 256 KB for stickers, 64
    /// KB for emoji. [default: auto]
    #[arg(short, long, value_enum, help_heading = "Size fitting")]
    pub fit: Option<FitArg>,
    /// Maximum number of encodes while fitting. [default: 8]
    #[arg(long, value_name = "N", help_heading = "Size fitting")]
    pub attempts: Option<u32>,
    /// Range to search in the unit of the fitted value: kbit/s, CRF, fps or
    /// seconds.
    #[arg(long, value_name = "MIN..MAX", help_heading = "Size fitting")]
    pub fit_range: Option<Range>,

    /// Target bitrate in kbit/s, when not fitting bitrate.
    #[arg(short, long, value_name = "KBPS", help_heading = "Encoding")]
    pub bitrate: Option<f64>,
    /// Constant quality, 0 (best) to 63 (worst), when fitting fps or length
    /// or with --fit off. [default: 32]
    #[arg(long, value_parser = clap::value_parser!(u8).range(0..=63), help_heading = "Encoding")]
    pub crf: Option<u8>,
    /// Encoder speed. Faster looks worse at the same size for WebM
    /// [default: balanced], and comes out larger for .tgs [default: best].
    #[arg(long, value_enum, help_heading = "Encoding")]
    pub speed: Option<SpeedArg>,
    /// WebM: lossless encoding, only useful for tiny or static videos.
    /// .tgs: never change the art to make it fit; report a sticker that is
    /// too large instead.
    #[arg(long, value_name = "BOOL", num_args = 0..=1, require_equals = true,
          default_missing_value = "true", help_heading = "Encoding")]
    pub lossless: Option<bool>,
    /// libvpx-vp9 option for tuning; repeat for more. For example
    /// tune-content=screen (flat graphics), aq-mode=2, sharpness=4,
    /// arnr-strength=3, g=60 (keyframe interval) or qmax=50. `ffmpeg -h
    /// encoder=libvpx-vp9` lists them all.
    #[arg(long = "encoder-options", visible_alias = "encoder-option",
          value_name = "NAME=VALUE", value_parser = parse_option_pair,
          help_heading = "Encoding")]
    pub encoder_options: Vec<(String, String)>,
    /// Raw ffmpeg output arguments, split like a shell would. Only with
    /// ffmpeg as a separate program (--ffmpeg-from system or --ffmpeg PATH).
    #[arg(long, value_name = "ARGS", allow_hyphen_values = true, help_heading = "Encoding")]
    pub extra_args: Option<ShellWords>,

    /// When to spoof the duration so Telegram accepts videos longer than 3 s.
    /// [default: auto, only when needed]
    #[arg(long, value_enum, help_heading = "Metadata")]
    pub spoof: Option<SpoofArg>,
    /// Duration written into the header when spoofing. [default: 0.42069]
    #[arg(long, value_name = "SECONDS", help_heading = "Metadata")]
    pub fake_duration: Option<f64>,
    /// Title stored in the file.
    #[arg(long, help_heading = "Metadata")]
    pub title: Option<String>,
    /// Mark the file as made by tgradish. [default: true]
    #[arg(long, value_name = "BOOL", num_args = 0..=1, require_equals = true,
          default_missing_value = "true", help_heading = "Metadata")]
    pub watermark: Option<bool>,

    /// What to do with more than 3 seconds. [default: speed-up]
    #[arg(long, value_enum, help_heading = TGS)]
    pub long: Option<LongArg>,
    /// Reductions that may make a sticker fit, comma-separated. [default:
    /// all]
    #[arg(long, value_enum, value_delimiter = ',', help_heading = TGS)]
    pub reductions: Vec<KindArg>,
    /// Keep the input's canvas instead of cropping to the visible pixels.
    #[arg(long, value_name = "BOOL", num_args = 0..=1, require_equals = true,
          default_missing_value = "true", help_heading = TGS)]
    pub keep_canvas: Option<bool>,
    /// Size of one art pixel in input pixels; pixels off its grid move onto
    /// it. [default: detected]
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u32).range(1..),
          help_heading = TGS)]
    pub pixel_scale: Option<u32>,
    /// Aseprite tag to export. [default: all frames]
    #[arg(long, help_heading = TGS)]
    pub tag: Option<String>,
    /// Read the input as a sprite sheet of equal cells, frames row by row.
    #[arg(long, value_name = "COLUMNSxROWS", help_heading = TGS)]
    pub sheet: Option<String>,
    /// How many sprite sheet cells hold frames. [default: all but
    /// transparent ones at the end]
    #[arg(long, value_name = "N", help_heading = TGS)]
    pub sheet_frames: Option<u32>,
}

impl OptionArgs {
    /// Flags given that `format` doesn't use.
    pub fn foreign(&self, format: Format) -> Vec<&'static str> {
        let given = |set: bool, name: &'static str| set.then_some(name);
        let flags = match format {
            Format::Webm => vec![
                given(self.long.is_some(), "--long"),
                given(!self.reductions.is_empty(), "--reductions"),
                given(self.keep_canvas.is_some(), "--keep-canvas"),
                given(self.pixel_scale.is_some(), "--pixel-scale"),
                given(self.tag.is_some(), "--tag"),
                given(self.sheet.is_some(), "--sheet"),
                given(self.sheet_frames.is_some(), "--sheet-frames"),
            ],
            Format::Tgs => vec![
                given(self.resize.is_some(), "--resize"),
                given(self.fit.is_some(), "--fit"),
                given(self.attempts.is_some(), "--attempts"),
                given(self.fit_range.is_some(), "--fit-range"),
                given(self.bitrate.is_some(), "--bitrate"),
                given(self.crf.is_some(), "--crf"),
                given(!self.encoder_options.is_empty(), "--encoder-options"),
                given(self.extra_args.is_some(), "--extra-args"),
                given(self.spoof.is_some(), "--spoof"),
                given(self.fake_duration.is_some(), "--fake-duration"),
            ],
        };
        flags.into_iter().flatten().collect()
    }

    pub fn to_tgs_options(&self) -> TgsOptions {
        TgsOptions {
            target: self.target.map(Into::into),
            start: self.start,
            length: self.length,
            long: self.long.map(Into::into),
            speed: self.speed.map(Into::into),
            lossless: self.lossless,
            reductions: (!self.reductions.is_empty())
                .then(|| self.reductions.iter().map(|&kind| kind.into()).collect()),
            keep_canvas: self.keep_canvas,
            pixel_scale: self.pixel_scale,
            tag: self.tag.clone(),
            sheet: self.sheet.clone(),
            sheet_frames: self.sheet_frames,
            fps: self.fps,
            title: self.title.clone(),
            watermark: self.watermark,
        }
    }

    pub fn to_options(&self) -> Options {
        Options {
            target: self.target.map(Into::into),
            resize: self.resize.map(Into::into),
            fit: self.fit.map(Into::into),
            attempts: self.attempts,
            fit_range: self.fit_range,
            start: self.start,
            length: self.length,
            fps: self.fps,
            bitrate: self.bitrate,
            crf: self.crf,
            speed: self.speed.map(Into::into),
            lossless: self.lossless,
            spoof: self.spoof.map(Into::into),
            fake_duration: self.fake_duration,
            title: self.title.clone(),
            watermark: self.watermark,
            encoder_options: (!self.encoder_options.is_empty())
                .then(|| self.encoder_options.iter().cloned().collect()),
            extra_args: self.extra_args.clone().map(|words| words.0),
        }
    }
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

fn parse_option_pair(text: &str) -> Result<(String, String), String> {
    let (name, value) = text.split_once('=').ok_or("expected NAME=VALUE")?;
    Ok((name.trim().to_string(), value.to_string()))
}

/// Mirrors a core enum as a clap value enum.
macro_rules! value_enum {
    ($name:ident => $core:path { $($variant:ident),* $(,)? }) => {
        #[derive(Debug, Clone, Copy, ValueEnum)]
        pub enum $name { $($variant),* }

        impl From<$name> for $core {
            fn from(value: $name) -> Self {
                match value { $($name::$variant => <$core>::$variant),* }
            }
        }
    };
}

value_enum!(TargetArg => Target { Sticker, Emoji });
value_enum!(ResizeArg => options::Resize { Contain, Pad, Crop, Stretch });
value_enum!(FitArg => options::Fit { Auto, Bitrate, Crf, Fps, Length, Off });
value_enum!(SpeedArg => options::Speed { Fast, Balanced, Best });
value_enum!(SpoofArg => options::Spoof { Auto, Always, Never });
value_enum!(FormatArg => Format { Webm, Tgs });
value_enum!(LongArg => tgs::Long { SpeedUp, Trim });
value_enum!(KindArg => tgs::Kind {
    SnapToGrid,
    MergeColours,
    MergeFrames,
    DropFrames,
    Despeckle,
    Downscale,
});

#[derive(Debug, Args)]
pub struct SpoofArgs {
    /// WebM file to spoof.
    pub input: PathBuf,
    /// Output file. Default: NAME.spoofed.webm next to the input.
    #[arg(short, long, conflicts_with = "in_place")]
    pub output: Option<PathBuf>,
    /// Modify the input file.
    #[arg(short, long)]
    pub in_place: bool,
    /// Replace an existing output file.
    #[arg(short = 'y', long)]
    pub overwrite: bool,
    /// Duration to write into the header. [default: 0.42069]
    #[arg(short, long, value_name = "SECONDS", default_value_t = options::DEFAULT_FAKE_DURATION,
          hide_default_value = true)]
    pub duration: f64,
    /// Title to store in the file.
    #[arg(long)]
    pub title: Option<String>,
    /// Mark the file as modified by tgradish.
    #[arg(long, value_name = "BOOL", num_args = 0..=1, require_equals = true,
          default_value_t = true, default_missing_value = "true")]
    pub watermark: bool,
}

#[derive(Debug, Args)]
pub struct InspectArgs {
    /// WebM or .tgs files to inspect.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Check against this target instead of guessing from the size.
    #[arg(long, value_enum)]
    pub target: Option<TargetArg>,
}

#[derive(Debug, Subcommand)]
pub enum PresetCommand {
    /// List all presets.
    List,
    /// Show the options of a preset, with everything it extends applied.
    Show { name: String },
    /// Print the directory for user presets.
    Path,
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Print the config file path.
    Path,
    /// Show the effective config.
    Show,
}

#[derive(Debug, Subcommand)]
pub enum FfmpegCommand {
    /// Show which ffmpeg is used and whether it can encode stickers.
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

    /// Front-ends rely on every option of either format having a flag of
    /// the same name.
    #[test]
    fn every_option_has_a_flag() {
        let cli = Cli::command();
        for schema in [schemars::schema_for!(Options), schemars::schema_for!(TgsOptions)] {
            let schema = serde_json::to_value(schema).unwrap();
            let properties = schema["properties"].as_object().unwrap();
            for command in ["convert", "watch"] {
                let command = cli.find_subcommand(command).unwrap();
                let flags: Vec<_> = command.get_arguments().filter_map(|a| a.get_long()).collect();
                for property in properties.keys() {
                    assert!(flags.contains(&property.as_str()), "no --{property} flag");
                }
            }
        }
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
            "--keep-canvas",
            "--fps",
            "12",
        ]);
        let Command::Convert(args) = cli.command else { panic!() };
        let options = &args.conversion.options;
        assert_eq!(options.foreign(Format::Tgs), ["--crf"]);
        assert_eq!(options.foreign(Format::Webm), ["--reductions", "--keep-canvas"]);
        let tgs = options.to_tgs_options();
        assert_eq!(tgs.reductions.unwrap(), [tgs::Kind::MergeColours, tgs::Kind::DropFrames]);
        assert_eq!((tgs.keep_canvas, tgs.fps), (Some(true), Some(12.0)));
    }

    #[test]
    fn parses_bool_flags() {
        let cli =
            Cli::parse_from(["tgradish", "convert", "a.mp4", "--lossless", "--watermark=false"]);
        let Command::Convert(args) = cli.command else { panic!() };
        let options = &args.conversion.options;
        assert_eq!(options.lossless, Some(true));
        assert_eq!(options.watermark, Some(false));
        assert_eq!(options.to_options().title, None);
    }

    #[test]
    fn splits_extra_args_like_a_shell() {
        let cli = Cli::parse_from([
            "tgradish",
            "convert",
            "a.mp4",
            "--extra-args",
            r#"-metadata comment="hello world" -tune-content screen"#,
        ]);
        let Command::Convert(args) = cli.command else { panic!() };
        let extra = args.conversion.options.to_options().extra_args.unwrap();
        assert_eq!(extra, ["-metadata", "comment=hello world", "-tune-content", "screen"]);
        assert!(Cli::try_parse_from(["tgradish", "convert", "a", "--extra-args", "'x"]).is_err());
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
    }
}
