use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use tgradish_core::ffmpeg::FfmpegChoice;
use tgradish_core::options::{self, Crop, ExactScale, Range};
use tgradish_core::presets::Format;
use tgradish_core::telegram::Target;
use tgradish_core::tgradish_tgs::reduce;
use tgradish_core::tgs;
use tgradish_core::webm::{self, Options};

pub use crate::term::ColourChoice;

/// The help pages' sections after the options, each under its heading,
/// which [`sections`] adds; each line indented as clap indents options.
const EXAMPLES: &str =
    "  tgradish convert clip.mp4                 a video sticker, clip.sticker.webm
  tgradish convert clip.mp4 --target emoji  an emoji, clip.emoji.webm
  tgradish convert walk.ase -f tgs          an animated sticker, walk.sticker.tgs
  tgradish convert photo.jpg -f webp        an image sticker, photo.sticker.webp
  tgradish watch ~/Videos -O ~/Stickers     each new video or picture as it comes
  tgradish inspect clip.sticker.webm        what Telegram would say of it";

const CONVERT_EXAMPLES: &str =
    "  tgradish convert clip.mp4 -s 2 -t 5          5 seconds from 0:02, spoofed to pass
  tgradish convert frames/ --sequence -f tgs   the pictures in frames/ as one sticker
  tgradish convert *.gif -O stickers -y        each into stickers/, replacing results";

const ENVIRONMENT: &str = "  TGRADISH_CONFIG  The config file, as --config names it.
  TGRADISH_FFMPEG  The ffmpeg to use, as --ffmpeg names it, when neither --ffmpeg
                   nor --ffmpeg-from is given; over the config file's.
  TGRADISH_LANG    The window's language, as de, over the system's.
  NO_COLOR         No colours, with --colour auto.
  CLICOLOR_FORCE   Colours even where the terminal isn't one, with --colour auto.
  FORCE_HYPERLINK  1 links file names, 0 doesn't, whatever the terminal.";

const EXIT_STATUS: &str = "  0    Done.
  1    Something failed. With several inputs, this beats 3.
  2    The command line was wrong.
  3    Everything was made or read, but Telegram would refuse a result or
       an inspected file: as a .tgs that --exact keeps too large, WebM
       with --fit off over the limit, or a picture of the wrong size.
  130  convert was cancelled, by Ctrl-C or with --cancel-on-stdin; watch
       stops as done when cancelled, with 0.";

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

fn convert_help() -> String {
    sections(&[("Examples", CONVERT_EXAMPLES), ("Exit status", EXIT_STATUS)])
}

fn inspect_help() -> String {
    sections(&[("Exit status", EXIT_STATUS)])
}

const TGS: &str = "Animated stickers (TGS)";
const ASEPRITE: &str = "Aseprite files";

/// Telegram video, image and animated stickers and emoji from videos,
/// pictures and pixel art.
///
/// Video stickers can run past Telegram's 3 seconds by spoofing the
/// duration they say they last.
#[derive(Debug, Parser)]
#[command(version, max_term_width = 100, styles = crate::ui::help_styles(),
          after_help = top_help())]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
    #[command(flatten, next_help_heading = "Global options")]
    pub global: Global,
}

#[derive(Debug, Args)]
pub struct Global {
    /// Print JSON Lines instead of text: one compact object per line.
    /// `convert` and `watch` print an event a line, which `tgradish
    /// describe` gives the schemas of, `inspect` a file a line, `preset
    /// list` a preset a line, `licenses` a notice a line, and the rest one
    /// line; an error is an error event. Not for `gui`.
    #[arg(long, global = true)]
    pub json: bool,
    /// Show more output, including ffmpeg logs with -vv.
    #[arg(short, long, global = true, action = clap::ArgAction::Count, conflicts_with = "quiet")]
    pub verbose: u8,
    /// Only print errors, warnings and results: a line on stdout for each
    /// file made.
    #[arg(short, long, global = true)]
    pub quiet: bool,
    /// The ffmpeg executable to use, or a directory with ffmpeg and
    /// ffprobe. Without this or --ffmpeg-from, TGRADISH_FFMPEG can name
    /// one, over the config file's.
    #[arg(long, global = true, value_name = "PATH", conflicts_with = "ffmpeg_from")]
    pub ffmpeg: Option<PathBuf>,
    /// Which ffmpeg to use: auto, built in if this build has it, otherwise
    /// the system's on PATH; builtin; or system. Over TGRADISH_FFMPEG and
    /// the config file. [default: the config's, or auto]
    #[arg(long, global = true, value_name = "WHERE", value_enum)]
    pub ffmpeg_from: Option<FfmpegFrom>,
    /// Config file to use instead of the default one.
    #[arg(long, global = true, env = "TGRADISH_CONFIG", value_name = "FILE")]
    pub config: Option<PathBuf>,
    /// Colours, links on file names, progress in the terminal's tab or
    /// taskbar and pictures of results, where the terminal shows them.
    /// NO_COLOR and CLICOLOR_FORCE are heeded with auto.
    #[arg(
        long = "colour",
        alias = "color",
        global = true,
        value_enum,
        default_value_t = ColourChoice::Auto,
        value_name = "WHEN"
    )]
    pub colour: ColourChoice,
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
    #[command(after_help = convert_help())]
    Convert(ConvertArgs),
    /// Convert every video or image that appears in a directory.
    Watch(WatchArgs),
    /// Spoof the duration of an existing WebM so Telegram accepts it.
    Spoof(SpoofArgs),
    /// Show properties of WebM, WebP and TGS files and check them against
    /// Telegram's limits.
    #[command(after_help = inspect_help())]
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
    /// Check which ffmpeg is used: `status`, which `tgradish ffmpeg` alone
    /// runs.
    Ffmpeg {
        #[command(subcommand)]
        command: Option<FfmpegCommand>,
    },
    /// Print the licences of tgradish and of what it is built from, which
    /// every copy carries.
    #[command(alias = "licences")]
    Licenses,
    /// Open the tgradish window. It also opens when tgradish is started
    /// without arguments outside a terminal.
    #[cfg(feature = "gui")]
    Gui,
}

impl Cli {
    /// What clap can't tell is wrong with the command line: `--json` for
    /// the window, which prints nothing to read, and an input named true
    /// or false that isn't there, as `--watermark false` gives one.
    pub fn misused(&self) -> Option<String> {
        if self.command.is_window() && self.global.json {
            return Some("--json can't be used with gui: the window prints nothing to read".into());
        }
        let Command::Convert(args) = &self.command else { return None };
        let word = args
            .inputs
            .iter()
            .find(|input| matches!(input.to_str(), Some("true" | "false")) && !input.exists())?;
        Some(format!(
            "there is no input {}: switches take no value, --watermark turns one on and \
             --no-watermark off",
            word.display()
        ))
    }
}

impl Command {
    /// Whether it opens the window, which takes Ctrl-C as any program
    /// does and marks what it makes as its own.
    pub fn is_window(&self) -> bool {
        #[cfg(feature = "gui")]
        if matches!(self, Command::Gui) {
            return true;
        }
        false
    }
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
    #[arg(short, long, value_name = "FILE", conflicts_with = "output_dir")]
    pub output: Option<PathBuf>,
    /// Join the inputs (images, or directories of them) into one animation,
    /// in name order with numbers counted as numbers. Unless -o names it,
    /// the result is named after the first input: walk/ makes
    /// walk.sticker.webm beside the folder, walk/1.png walk/2.png makes
    /// walk/1.sticker.webm.
    #[arg(long, conflicts_with = "clipboard")]
    pub sequence: bool,
    /// What to make: webm (video sticker or emoji: VP9 in WebM, from any
    /// video or picture), tgs (animated sticker or emoji: TGS, meant for
    /// pixel art, or a video of it) or webp (image sticker or emoji, from
    /// any picture or the frame at --start; image works too).
    /// [default: by the -o file's extension, then webm]
    #[arg(short, long, value_enum)]
    pub format: Option<FormatArg>,
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
    /// Seconds between looks at the directory, from 0.05 to a day.
    #[arg(long, value_name = "SECONDS", default_value_t = 1.0, value_parser = parse_interval)]
    pub interval: f64,
    /// What to make: webm (video sticker or emoji: VP9 in WebM, from any
    /// video or picture), tgs (animated sticker or emoji: TGS, meant for
    /// pixel art, or a video of it) or webp (image sticker or emoji, from
    /// any picture or the frame at --start; image works too).
    /// [default: webm]
    #[arg(short, long, value_enum)]
    pub format: Option<FormatArg>,
    #[command(flatten)]
    pub conversion: ConversionArgs,
}

/// Flags shared by `convert` and `watch`.
#[derive(Debug, Args)]
pub struct ConversionArgs {
    /// Directory for results. Default: next to each input, as
    /// NAME.sticker.webm, NAME.emoji.webm or the same with .tgs or .webp.
    #[arg(short = 'O', long, value_name = "DIR")]
    pub output_dir: Option<PathBuf>,
    /// Replace existing output files.
    #[arg(short = 'y', long)]
    pub overwrite: bool,
    /// Preset to start from: fast, balanced, best or one of your own, see
    /// `tgradish preset list`. [default: balanced for webm and webp, best
    /// for tgs, unless the config's preset or tgs-preset names another]
    #[arg(short, long)]
    pub preset: Option<String>,
    /// Options as a JSON object, applied after the preset and before other
    /// flags. Keys are the flag names, see `tgradish describe`.
    #[arg(long, value_name = "JSON")]
    pub options_json: Option<String>,
    /// Keep intermediate files and print where they are: the first pass's
    /// statistics of WebM output, which alone has any.
    #[arg(long)]
    pub keep_temp: bool,
    /// Stop, as Ctrl-C does, when a line reading `cancel` comes on stdin:
    /// for front-ends, which can't always send Ctrl-C. Without it stdin is
    /// left alone, for a script's loop to read.
    #[arg(long)]
    pub cancel_on_stdin: bool,
    #[command(flatten)]
    pub options: OptionArgs,
}

/// One flag per [`Options`] and [`tgs::Options`] field, with the same name.
#[derive(Debug, Args)]
pub struct OptionArgs {
    /// Sticker or emoji, which sets the size and the size limit: a sticker
    /// is 512 pixels on its longer side, within 256 KiB as WebM; an emoji
    /// is 100x100, within 64 KiB as WebM. Pictures have 512 KiB either
    /// way, and TGS are 512x512 within 64 KiB either way. [default:
    /// sticker]
    #[arg(long, value_enum, help_heading = "Output")]
    pub target: Option<TargetArg>,
    /// The part of the input to use, in its pixels: WIDTHxHEIGHT+X+Y from
    /// the top left, for example 640x360+100+50. [default: all of it]
    #[arg(long, value_name = "WxH+X+Y", help_heading = "Output")]
    pub crop: Option<Crop>,
    /// How to scale into the target size [default: contain for stickers,
    /// pad for emoji]; TGS must be square: pad the pixel art, crop its
    /// middle, or stretch it [default: pad].
    #[arg(long, value_enum, help_heading = "Output")]
    pub resize: Option<ResizeArg>,
    /// How pixels are scaled: smooth, sharp blocks for pixel art, or
    /// pixel-perfect whole multiples with transparent margins. [default:
    /// auto, sharp for pixel art made at least twice as large]
    #[arg(long, value_enum, help_heading = "Output")]
    pub scaling: Option<ScalingArg>,
    /// Scale by exactly N (or 1/N), so input pixels line up with the
    /// result's; transparent margins fill the rest. The crop must fit: with
    /// 3, at most 170 pixels on the longer side for stickers, 33 for emoji.
    /// --exact-scale= with nothing after sets none, over a preset's.
    #[arg(long, value_name = "N", help_heading = "Output")]
    pub exact_scale: Option<ExactScale>,
    /// Seconds to skip at the start of the input.
    #[arg(short = 's', long, value_name = "SECONDS", help_heading = "Output")]
    pub start: Option<f64>,
    /// Seconds of the input to use. [default: the rest of it]
    #[arg(short = 't', long, value_name = "SECONDS", help_heading = "Output")]
    pub length: Option<f64>,
    /// How many times as fast as the input the result plays, 0.1 to 10.
    /// [default: 1 for WebM; for TGS, as fast as fits a long input into 3
    /// seconds; what still lasts longer is cut]
    #[arg(long, value_name = "TIMES", help_heading = "Output")]
    pub tempo: Option<f64>,
    /// Frame rate of the result for WebM, at most 60, where more than the
    /// input's at the tempo only repeats its frames [default: the input's
    /// at the tempo, from 1 to 60]. For image sequences, and sprite sheets
    /// for TGS, it is also the rate their pictures are read at, how fast
    /// they play [default: 12].
    #[arg(long, help_heading = "Output")]
    pub fps: Option<f64>,

    /// What to tune to get close to the file size limit: 256 KiB for
    /// stickers, 64 KiB for emoji. [default: auto]
    #[arg(long, value_enum, help_heading = "Size fitting")]
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
    /// [default: balanced], and comes out larger for TGS [default: best].
    #[arg(long, value_enum, help_heading = "Encoding")]
    pub speed: Option<SpeedArg>,
    /// Lossless encoding, only small for tiny or still pictures;
    /// --no-lossless turns a preset's off. [default: off]
    #[arg(long, overrides_with = "no_lossless", help_heading = "Encoding")]
    pub lossless: bool,
    #[arg(long, overrides_with = "lossless", hide = true)]
    pub no_lossless: bool,
    /// libvpx-vp9 option for tuning; repeat for more. For example
    /// tune-content=screen (flat graphics), aq-mode=2, sharpness=4,
    /// arnr-strength=3, g=60 (keyframe interval) or qmax=50. `ffmpeg -h
    /// encoder=libvpx-vp9` lists them all. These replace the preset's;
    /// --encoder-options= with nothing after sets none.
    #[arg(long = "encoder-options", visible_alias = "encoder-option",
          value_name = "NAME=VALUE", value_parser = parse_option_pair,
          help_heading = "Encoding")]
    pub encoder_options: Vec<Option<(String, String)>>,
    /// Raw ffmpeg output arguments, split like a shell would. Only with
    /// ffmpeg as a separate program (--ffmpeg-from system or --ffmpeg PATH).
    #[arg(long, value_name = "ARGS", allow_hyphen_values = true, help_heading = "Encoding")]
    pub ffmpeg_args: Option<ShellWords>,

    /// When to spoof the duration so Telegram accepts videos longer than 3 s.
    /// [default: auto, only when needed]
    #[arg(long, value_enum, help_heading = "Metadata")]
    pub spoof: Option<SpoofArg>,
    /// Duration encoded into the metadata when spoofing. [default: a short
    /// one Telegram accepts]
    #[arg(long, value_name = "SECONDS", help_heading = "Metadata")]
    pub fake_duration: Option<f64>,
    /// Title stored in the file.
    #[arg(long, help_heading = "Metadata")]
    pub title: Option<String>,
    /// Mark the file as made by tgradish in its metadata, which for TGS is
    /// the sticker's Lottie name; --no-watermark leaves it out. [default:
    /// on]
    #[arg(long, overrides_with = "no_watermark", help_heading = "Metadata")]
    pub watermark: bool,
    #[arg(long, overrides_with = "watermark", hide = true)]
    pub no_watermark: bool,

    /// Never change the pixel art to make it fit, but report a sticker
    /// that is too large; --no-exact lets fitting change it. [default: on]
    #[arg(long, overrides_with = "no_exact", help_heading = TGS)]
    pub exact: bool,
    #[arg(long, overrides_with = "exact", hide = true)]
    pub no_exact: bool,
    /// Reductions that may make a sticker fit, comma-separated. [default:
    /// all]
    #[arg(long, value_enum, value_delimiter = ',', help_heading = TGS)]
    pub reductions: Vec<KindArg>,
    /// What fitting gives up first: motion (frames merged and dropped, full
    /// detail), detail (smooth motion, a coarser picture), or auto, whatever
    /// changes the pixel art least. [default: auto]
    #[arg(long, value_enum, help_heading = TGS)]
    pub compromise: Option<CompromiseArg>,
    /// Keep the input's canvas instead of cropping to the visible pixels;
    /// --no-keep-canvas crops over a preset that keeps it. [default: off]
    #[arg(long, overrides_with = "no_keep_canvas", help_heading = TGS)]
    pub keep_canvas: bool,
    #[arg(long, overrides_with = "keep_canvas", hide = true)]
    pub no_keep_canvas: bool,
    /// Size of one art pixel in input pixels; pixels off its grid move onto
    /// it. [default: detected]
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u32).range(1..),
          help_heading = TGS)]
    pub pixel_scale: Option<u32>,
    /// Read the input as a sprite sheet of equal cells, frames row by row.
    #[arg(long, value_name = "COLUMNSxROWS", help_heading = TGS)]
    pub sheet: Option<options::Sheet>,
    /// How many sprite sheet cells hold frames. [default: all but
    /// transparent ones at the end]
    #[arg(long, value_name = "N", help_heading = TGS)]
    pub sheet_frames: Option<u32>,

    /// Tag to use. [default: all frames]
    #[arg(long, help_heading = ASEPRITE)]
    pub tag: Option<String>,
    /// Layer to show, hidden ones too; repeat for more. [default: those the
    /// file shows]
    #[arg(long = "show-layers", visible_alias = "show-layer", value_name = "NAME",
          help_heading = ASEPRITE)]
    pub show_layers: Vec<String>,
    /// Layer to hide; repeat for more. A hidden group hides the layers in
    /// it.
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
        let mut flags = options::set_keys(&self.to_options());
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

    pub fn to_tgs_options(&self) -> tgs::Options {
        tgs::Options {
            target: self.target.map(Into::into),
            start: self.start,
            length: self.length,
            tempo: self.tempo,
            speed: self.speed.map(Into::into),
            exact: switch(self.exact, self.no_exact),
            reductions: (!self.reductions.is_empty())
                .then(|| self.reductions.iter().map(|&kind| kind.into()).collect()),
            compromise: self.compromise.map(Into::into),
            crop: self.crop,
            resize: self.resize.map(Into::into),
            keep_canvas: switch(self.keep_canvas, self.no_keep_canvas),
            pixel_scale: self.pixel_scale,
            tag: self.tag.clone(),
            show_layers: names(&self.show_layers),
            hide_layers: names(&self.hide_layers),
            sheet: self.sheet,
            sheet_frames: self.sheet_frames,
            fps: self.fps,
            title: self.title.clone(),
            watermark: switch(self.watermark, self.no_watermark),
        }
    }

    pub fn to_options(&self) -> Options {
        Options {
            target: self.target.map(Into::into),
            tag: self.tag.clone(),
            show_layers: names(&self.show_layers),
            hide_layers: names(&self.hide_layers),
            crop: self.crop,
            resize: self.resize.map(Into::into),
            scaling: self.scaling.map(Into::into),
            exact_scale: self.exact_scale,
            fit: self.fit.map(Into::into),
            attempts: self.attempts,
            fit_range: self.fit_range,
            start: self.start,
            length: self.length,
            tempo: self.tempo,
            fps: self.fps,
            bitrate: self.bitrate,
            crf: self.crf,
            speed: self.speed.map(Into::into),
            lossless: switch(self.lossless, self.no_lossless),
            spoof: self.spoof.map(Into::into),
            fake_duration: self.fake_duration,
            title: self.title.clone(),
            watermark: switch(self.watermark, self.no_watermark),
            encoder_options: (!self.encoder_options.is_empty())
                .then(|| self.encoder_options.iter().flatten().cloned().collect()),
            ffmpeg_args: self.ffmpeg_args.clone().map(|words| words.0),
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

/// Seconds for `watch --interval`: no shorter than watching wakes to see
/// if it was stopped, and no longer than a day, beyond which it may as
/// well not watch, and far below where seconds stop being a duration.
fn parse_interval(text: &str) -> Result<f64, String> {
    let seconds: f64 = text.parse().map_err(|_| format!("{text} isn't a number"))?;
    if !(0.05..=86_400.0).contains(&seconds) {
        return Err("must be from 0.05 to 86400 seconds".into());
    }
    Ok(seconds)
}

/// `NAME=VALUE`, or nothing, which says there are none.
fn parse_option_pair(text: &str) -> Result<Option<(String, String)>, String> {
    if text.is_empty() {
        return Ok(None);
    }
    let (name, value) = text.split_once('=').ok_or("expected NAME=VALUE")?;
    Ok(Some((name.trim().to_string(), value.to_string())))
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
value_enum!(ScalingArg => options::Scaling { Auto, Smooth, Sharp, PixelPerfect });
value_enum!(FitArg => options::Fit { Auto, Bitrate, Crf, Fps, Length, Off });
value_enum!(SpeedArg => options::Speed { Fast, Balanced, Best });
value_enum!(SpoofArg => options::Spoof { Auto, Always, Never });

/// [`Format`] for clap, which also takes `image` for WebP: the kind of
/// sticker it makes.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum FormatArg {
    Webm,
    Tgs,
    #[value(alias = "image")]
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
    /// Output file. Default: NAME.spoofed.webm next to the input.
    #[arg(short, long, value_name = "FILE", conflicts_with = "in_place")]
    pub output: Option<PathBuf>,
    /// Modify the input file.
    #[arg(short, long)]
    pub in_place: bool,
    /// Replace an existing output file.
    #[arg(short = 'y', long)]
    pub overwrite: bool,
    /// Duration to encode into the metadata, as convert's --fake-duration.
    /// [default: a short one Telegram accepts]
    #[arg(short = 'd', long, value_name = "SECONDS",
          default_value_t = webm::DEFAULT_FAKE_DURATION, hide_default_value = true)]
    pub fake_duration: f64,
    /// Title to store in the file.
    #[arg(long)]
    pub title: Option<String>,
    /// Mark the file as modified by tgradish; --no-watermark leaves it as
    /// it is. [default: on]
    #[arg(long, overrides_with = "no_watermark")]
    pub watermark: bool,
    #[arg(long, overrides_with = "watermark", hide = true)]
    pub no_watermark: bool,
}

#[derive(Debug, Args)]
pub struct InspectArgs {
    /// WebM, WebP or TGS files to inspect.
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Check WebM and WebP files against this target instead of guessing
    /// from the size. TGS stickers and emoji have the same limits.
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

    /// The help's first line is what the manifests and packages say
    /// tgradish is, which xtask holds them to.
    #[test]
    fn says_what_tgradish_is_as_its_manifest_does() {
        let about = Cli::command().get_about().unwrap().to_string();
        assert_eq!(about, env!("CARGO_PKG_DESCRIPTION"));
    }

    /// Every help page, as `--help` prints it, into
    /// `$TGRADISH_STRINGS/cli.json`, for `cargo xtask strings`.
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

    /// Front-ends rely on every option of either format having a flag of
    /// the same name.
    #[test]
    fn every_option_has_a_flag() {
        let cli = Cli::command();
        for schema in [schemars::schema_for!(Options), schemars::schema_for!(tgs::Options)] {
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
        // lossless VP9 is WebM's, keeping the art exact is .tgs's
        assert_eq!(options("--lossless").unused(Format::Tgs, None), ["--lossless"]);
        assert!(options("--lossless").unused(Format::Webm, None).is_empty());
        for format in [Format::Webm, Format::Webp] {
            assert_eq!(options("--exact").unused(format, None), ["--exact"]);
        }
        assert_eq!(options("--no-exact").to_tgs_options().exact, Some(false));
    }

    /// -f is the format, as anyone who knows ffmpeg reads it; fitting has
    /// no short flag.
    #[test]
    fn reads_f_as_the_format() {
        let cli = Cli::parse_from(["tgradish", "convert", "a.png", "-f", "tgs"]);
        let Command::Convert(args) = cli.command else { panic!() };
        assert!(matches!(args.format, Some(FormatArg::Tgs)));
        assert!(Cli::try_parse_from(["tgradish", "convert", "a.png", "-f", "crf"]).is_err());
        let help = Cli::command().find_subcommand_mut("convert").unwrap().render_help();
        assert!(help.to_string().contains("-o, --output <FILE>"), "{help}");
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
        let webm = Options {
            target: Some(Target::Emoji),
            tag: Some("idle".into()),
            // names with commas and spaces, one flag each, and ones that
            // start as flags do
            show_layers: Some(vec!["Shadow, soft".into(), "Hat".into()]),
            hide_layers: Some(vec!["Back ground".into(), "--old".into()]),
            crop: Some(Crop { x: 1, y: 2, width: 30, height: 40 }),
            resize: Some(options::Resize::Pad),
            scaling: Some(options::Scaling::PixelPerfect),
            exact_scale: Some("1/2".parse().unwrap()),
            fit: Some(options::Fit::Crf),
            attempts: Some(5),
            fit_range: Some(Range { min: 4.0, max: 40.5 }),
            start: Some(1.25),
            length: Some(2.0),
            tempo: Some(1.5),
            fps: Some(24.0),
            bitrate: Some(600.0),
            crf: Some(30),
            speed: Some(options::Speed::Best),
            lossless: Some(false),
            spoof: Some(options::Spoof::Never),
            fake_duration: Some(0.5),
            title: Some("-_- a pig's \"sticker\"".into()),
            watermark: Some(false),
            encoder_options: Some([("tune-content".into(), "screen".into())].into()),
            ffmpeg_args: Some(vec!["-x".into(), "a b".into()]),
        };
        assert_eq!(parsed(options::flags(&webm)).to_options(), webm);
        let tgs = tgs::Options {
            target: Some(Target::Emoji),
            start: Some(0.5),
            length: Some(1.5),
            tempo: Some(0.5),
            speed: Some(options::Speed::Fast),
            exact: Some(true),
            reductions: Some(vec![reduce::Kind::MergeColours, reduce::Kind::DropFrames]),
            compromise: Some(reduce::Compromise::Detail),
            crop: Some(Crop { x: 0, y: 0, width: 8, height: 8 }),
            resize: Some(options::Resize::Stretch),
            keep_canvas: Some(true),
            pixel_scale: Some(2),
            tag: Some("-run".into()),
            show_layers: Some(vec!["Outline".into()]),
            hide_layers: Some(vec!["Sketch".into(), "Guides, old".into()]),
            sheet: "4x2".parse().ok(),
            sheet_frames: Some(7),
            fps: Some(12.5),
            title: Some("-_-".into()),
            watermark: Some(true),
        };
        assert_eq!(parsed(options::flags(&tgs)).to_tgs_options(), tgs);
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
        // and no exact scale, over a preset's
        let none = Options { exact_scale: Some(ExactScale::NONE), ..Options::default() };
        assert_eq!(options::flags(&none), ["--exact-scale="]);
        assert_eq!(parsed(options::flags(&none)).to_options(), none);
    }
}
