use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use tgradish_core::ffmpeg::FfmpegChoice;
use tgradish_core::options::{self, Options, Range};
use tgradish_core::telegram::Target;

/// Converts videos into Telegram video stickers and emoji, with the ability
/// to bypass the 3 second limit.
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
    /// auto (bundled, then downloaded, then system).
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
    Bundled,
    Downloaded,
    System,
}

impl From<FfmpegFrom> for FfmpegChoice {
    fn from(value: FfmpegFrom) -> Self {
        match value {
            FfmpegFrom::Auto => FfmpegChoice::Auto,
            FfmpegFrom::Bundled => FfmpegChoice::Bundled,
            FfmpegFrom::Downloaded => FfmpegChoice::Downloaded,
            FfmpegFrom::System => FfmpegChoice::System,
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Convert videos or images into stickers or emoji.
    Convert(ConvertArgs),
    /// Spoof the duration of an existing WebM so Telegram accepts it.
    Spoof(SpoofArgs),
    /// Show properties of WebM files and check them against Telegram's
    /// requirements.
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
    /// Check, download or remove ffmpeg.
    #[command(subcommand)]
    Ffmpeg(FfmpegCommand),
}

#[derive(Debug, Args)]
pub struct ConvertArgs {
    /// Videos or images to convert.
    #[arg(required = true)]
    pub inputs: Vec<PathBuf>,
    /// Output file. Default: next to the input, as NAME.sticker.webm or
    /// NAME.emoji.webm. Only with a single input.
    #[arg(short, long)]
    pub output: Option<PathBuf>,
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

/// One flag per [`Options`] field, with the same name.
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
    /// Frame rate. [default: the input's, at most 30]
    #[arg(short = 'r', long, help_heading = "Output")]
    pub fps: Option<f64>,

    /// What to tune to get close to the 256 KB limit. [default: auto]
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
    /// Encoder speed; faster looks worse at the same size. [default: balanced]
    #[arg(long, value_enum, help_heading = "Encoding")]
    pub speed: Option<SpeedArg>,
    /// Lossless encoding, only useful for tiny or static videos.
    #[arg(long, value_name = "BOOL", num_args = 0..=1, require_equals = true,
          default_missing_value = "true", help_heading = "Encoding")]
    pub lossless: Option<bool>,
    /// Extra ffmpeg output arguments, separated by spaces.
    #[arg(long, value_name = "ARGS", allow_hyphen_values = true, help_heading = "Encoding")]
    pub extra_args: Option<String>,

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
}

impl OptionArgs {
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
            extra_args: self
                .extra_args
                .as_ref()
                .map(|args| args.split_whitespace().map(String::from).collect()),
        }
    }
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
    /// WebM files to inspect.
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
    /// Download the minimal ffmpeg build into tgradish's data directory.
    Download {
        /// Archive to install instead of the build published for this
        /// version: a URL, file:// URL or local path.
        #[arg(long, requires = "sha256")]
        url: Option<String>,
        /// Expected SHA-256 of the archive given with --url.
        #[arg(long, requires = "url")]
        sha256: Option<String>,
    },
    /// Delete the downloaded ffmpeg.
    Remove,
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn cli_is_valid() {
        Cli::command().debug_assert();
    }

    /// Front-ends rely on every option having a flag of the same name.
    #[test]
    fn every_option_has_a_flag() {
        let schema = serde_json::to_value(schemars::schema_for!(Options)).unwrap();
        let properties = schema["properties"].as_object().unwrap();
        let cli = Cli::command();
        let convert = cli.find_subcommand("convert").unwrap();
        let flags: Vec<_> = convert.get_arguments().filter_map(|a| a.get_long()).collect();
        for property in properties.keys() {
            assert!(flags.contains(&property.as_str()), "no --{property} flag");
        }
    }

    #[test]
    fn parses_bool_flags() {
        let cli =
            Cli::parse_from(["tgradish", "convert", "a.mp4", "--lossless", "--watermark=false"]);
        let Command::Convert(args) = cli.command else { panic!() };
        assert_eq!(args.options.lossless, Some(true));
        assert_eq!(args.options.watermark, Some(false));
        assert_eq!(args.options.to_options().title, None);
    }
}
