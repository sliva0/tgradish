//! What a conversion's options come to, one way for every front end: the
//! preset's, then each layer given over them in order, checked before any
//! input is read, with the values that have no say under the rest set
//! aside.
//!
//! ```
//! # use tgradish_core::{config::Config, presets::{Format, FormatOptions, Presets}};
//! # use tgradish_core::resolve::{Origin, resolve};
//! use tgradish_core::options::{Fit, Settings};
//! use tgradish_core::webm::Options;
//!
//! // the built-in fast preset fits a bitrate, which a quality has no say in
//! let mine = Options { crf: Some(20), ..Options::default() };
//! let layers = [Settings { options: mine, ..Settings::default() }.map(FormatOptions::Webm)];
//! let resolved =
//!     resolve(&Presets::builtin(), &Config::default(), Format::Webm, Some("fast"), &layers)?;
//! let FormatOptions::Webm(options) = &resolved.settings.options else { unreachable!() };
//! assert_eq!((options.fit, options.crf), (Some(Fit::Bitrate), None));
//! // set aside, and said why and by whom
//! let aside = &resolved.set_aside[0];
//! assert_eq!((aside.option, aside.why.as_str()), ("crf", "fit is bitrate"));
//! assert_eq!(aside.set_by, Origin::Layer(0));
//! # Ok::<(), tgradish_core::resolve::ResolveError>(())
//! ```

use crate::config::Config;
use crate::error::Error;
use crate::options::{Selection, Settings, first_wrong};
use crate::presets::{Format, FormatOptions, PresetError, Presets};
use crate::{tgs, webm, webp};

/// Who set a value: the preset, or the layer given at this index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// The preset, or one it extends.
    Preset,
    /// The layer at this index of those given to [`resolve`].
    Layer(usize),
}

/// A value left out of what is resolved, as it has no say under the others.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetAside {
    /// The option's key.
    pub option: &'static str,
    /// Why it has no say, a clause naming options by key: `fit is off`.
    pub why: String,
    /// Who set it.
    pub set_by: Origin,
}

/// What a conversion's options come to.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    /// The preset's name, as given or as the config chose it.
    pub preset: String,
    /// Everything the conversion gets.
    pub settings: Settings<FormatOptions>,
    /// The values left out, in the order [`webm::set_aside`] gives them.
    pub set_aside: Vec<SetAside>,
}

/// Why options can't be resolved.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ResolveError {
    /// No preset has the name; `known` are those that do. `named` says
    /// whether it was given rather than chosen by the config.
    #[error("preset {name:?} does not exist; available: {}", known.join(", "))]
    UnknownPreset {
        /// The name.
        name: String,
        /// The presets there are.
        known: Vec<String>,
        /// Given to [`resolve`], rather than the config's.
        named: bool,
    },
    /// The preset, or one it extends, can't be had.
    #[error(transparent)]
    Preset(PresetError),
    /// A value is wrong whatever the input, as [`crate::options::check`]
    /// says.
    #[error("{}", invalid(preset, option, reason, *set_by))]
    Invalid {
        /// The preset's name.
        preset: String,
        /// The option's key.
        option: &'static str,
        /// What completes the message: `must be from 0.1 to 10`.
        reason: String,
        /// The last to set it.
        set_by: Origin,
    },
    /// An option set without one it needs.
    #[error("{option} requires {requires}")]
    Requires {
        /// The option's key.
        option: &'static str,
        /// The key of the one it needs.
        requires: &'static str,
        /// The last to set it.
        set_by: Origin,
    },
    /// A layer holds another format's options than the one resolved.
    #[error("layer {layer} holds {} options, where {} ones are resolved", .format.name(), .wanted.name())]
    OtherFormat {
        /// The layer's index.
        layer: usize,
        /// The format its options are for.
        format: Format,
        /// The format resolved.
        wanted: Format,
    },
}

/// What [`ResolveError::Invalid`] says: whose value it is, where it is the
/// preset's, which the front end may not name.
fn invalid(preset: &str, option: &str, reason: &str, set_by: Origin) -> String {
    match set_by {
        Origin::Preset => format!("preset {preset:?} sets {option}, which {reason}"),
        Origin::Layer(_) => format!("{option} {reason}"),
    }
}

/// Resolves the settings of a conversion to `format`: the options of
/// `preset`, or else of the one `config` chooses for `format`, with what it
/// extends; then each of `layers` over them, the last over all, without
/// what of the file's own `format` doesn't take; checked; and, for WebM,
/// without the values [`webm::set_aside`] finds have no say.
///
/// A lossless off is left out of [`Resolved::set_aside`] where it is set
/// aside, as that changes nothing.
pub fn resolve(
    presets: &Presets,
    config: &Config,
    format: Format,
    preset: Option<&str>,
    layers: &[Settings<FormatOptions>],
) -> Result<Resolved, ResolveError> {
    let name = preset.unwrap_or_else(|| config.preset_for(format));
    let base = presets.options(name, format).map_err(|err| match err {
        Error::Preset(PresetError::Unknown { name, known }) => {
            ResolveError::UnknownPreset { name, known, named: preset.is_some() }
        }
        Error::Preset(err) => ResolveError::Preset(err),
        err => ResolveError::Preset(PresetError::Broken(err.to_string())),
    })?;
    if let Some((layer, other)) = layers.iter().enumerate().find(|(_, l)| l.format() != format) {
        return Err(ResolveError::OtherFormat { layer, format: other.format(), wanted: format });
    }
    // worked out only for what is wrong or set aside, as the GUI resolves
    // every item's options each frame
    let set_by = |option: &str| {
        let sets = |layer: &Settings<FormatOptions>| {
            layer.selection.sets(option) || sets(&layer.options, option)
        };
        layers.iter().rposition(sets).map_or(Origin::Preset, Origin::Layer)
    };
    let selection = layers
        .iter()
        .fold(Selection::default(), |selection, layer| selection.merged(&layer.selection))
        .for_format(format);
    let options = match base {
        FormatOptions::Webm(base) => {
            FormatOptions::Webm(layers.iter().filter_map(webm_of).fold(base, webm::Options::merged))
        }
        FormatOptions::Tgs(base) => {
            FormatOptions::Tgs(layers.iter().filter_map(tgs_of).fold(base, tgs::Options::merged))
        }
        FormatOptions::Webp(base) => {
            FormatOptions::Webp(layers.iter().filter_map(webp_of).fold(base, webp::Options::merged))
        }
    };
    let merged = Settings { selection, options };
    let wrong = first_wrong(&merged);
    if let Some((option, reason)) = wrong {
        let preset = name.to_owned();
        return Err(ResolveError::Invalid { preset, option, reason, set_by: set_by(option) });
    }
    // how a sheet is read, of a sheet that isn't one
    if merged.selection.sheet_frames.is_some() && merged.selection.sheet.is_none() {
        let (option, requires) = ("sheet-frames", "sheet");
        return Err(ResolveError::Requires { option, requires, set_by: set_by(option) });
    }
    let Settings { selection, options } = merged;
    let (options, set_aside) = match options {
        FormatOptions::Webm(options) => {
            let aside = webm::set_aside(&options, &options);
            let set_aside = aside
                .iter()
                .filter(|(option, _)| *option != "lossless" || options.lossless == Some(true))
                .map(|&(option, ref why)| SetAside {
                    option,
                    why: why.clone(),
                    set_by: set_by(option),
                })
                .collect();
            let kept = webm::without(options, aside.iter().map(|(option, _)| *option));
            (FormatOptions::Webm(kept), set_aside)
        }
        options => (options, Vec::new()),
    };
    let settings = Settings { selection, options };
    Ok(Resolved { preset: name.to_owned(), settings, set_aside })
}

/// Whether `options` set the option `key`.
fn sets(options: &FormatOptions, key: &str) -> bool {
    match options {
        FormatOptions::Webm(options) => options.sets(key),
        FormatOptions::Tgs(options) => options.sets(key),
        FormatOptions::Webp(options) => options.sets(key),
    }
}

fn webm_of(layer: &Settings<FormatOptions>) -> Option<&webm::Options> {
    match &layer.options {
        FormatOptions::Webm(options) => Some(options),
        _ => None,
    }
}

fn tgs_of(layer: &Settings<FormatOptions>) -> Option<&tgs::Options> {
    match &layer.options {
        FormatOptions::Tgs(options) => Some(options),
        _ => None,
    }
}

fn webp_of(layer: &Settings<FormatOptions>) -> Option<&webp::Options> {
    match &layer.options {
        FormatOptions::Webp(options) => Some(options),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::{Effort, Fit, IntegerScale, Range, Spoof};

    /// The built-in presets and `files`, each `(name, text)`.
    fn presets(files: &[(&str, &str)]) -> Presets {
        let dir = tempfile::tempdir().unwrap();
        for (name, text) in files {
            std::fs::write(dir.path().join(format!("{name}.toml")), text).unwrap();
        }
        Presets::load(Some(dir.path())).unwrap()
    }

    /// A layer of `options` alone.
    fn webm(options: webm::Options) -> Settings<FormatOptions> {
        Settings { selection: default(), options: FormatOptions::Webm(options) }
    }

    fn tgs(options: tgs::Options) -> Settings<FormatOptions> {
        Settings { selection: default(), options: FormatOptions::Tgs(options) }
    }

    fn picture(options: webp::Options) -> Settings<FormatOptions> {
        Settings { selection: default(), options: FormatOptions::Webp(options) }
    }

    /// A layer of what `selection` chooses of the file, for `options`.
    fn chosen(selection: Selection, options: FormatOptions) -> Settings<FormatOptions> {
        Settings { selection, options }
    }

    fn default<T: Default>() -> T {
        T::default()
    }

    #[test]
    fn takes_the_preset_named_or_else_the_configs() {
        let presets = Presets::builtin();
        let config = Config { tgs_preset: Some("fast".into()), ..Config::default() };
        let resolved = |format, preset| resolve(&presets, &config, format, preset, &[]).unwrap();
        let balanced = resolved(Format::Webm, None);
        assert_eq!(balanced.preset, "balanced");
        assert_eq!(balanced.settings, webm(presets.webm("balanced").unwrap()));
        let fast = resolved(Format::Tgs, None);
        assert_eq!(
            (fast.preset.as_str(), fast.settings),
            ("fast", tgs(presets.tgs("fast").unwrap()))
        );
        assert_eq!(resolved(Format::Tgs, Some("best")).preset, "best");
        // a picture has none of the presets' options
        assert_eq!(resolved(Format::Webp, None).settings, picture(default()));
        // and one there isn't, said as where it was named
        let none = Config { preset: Some("nope".into()), ..Config::default() };
        let unknown =
            |config: &Config, preset| match resolve(&presets, config, Format::Webm, preset, &[])
                .unwrap_err()
            {
                ResolveError::UnknownPreset { name, named, .. } => (name, named),
                err => panic!("{err}"),
            };
        assert_eq!(unknown(&none, None), ("nope".into(), false));
        assert_eq!(unknown(&Config::default(), Some("nope")), ("nope".into(), true));
        let said = resolve(&presets, &none, Format::Webm, None, &[]).unwrap_err().to_string();
        assert_eq!(said, "preset \"nope\" does not exist; available: balanced, best, fast");
        let broken = presets_with("[webm]\ncrop = { x = 0, y = 0, width = 8, height = 8 }\n");
        let err = resolve(&broken, &Config::default(), Format::Tgs, Some("mine"), &[]);
        assert!(matches!(err, Err(ResolveError::Preset(PresetError::PerFile { .. }))), "{err:?}");
    }

    /// The built-in presets and one named `mine`, of `text`.
    fn presets_with(text: &str) -> Presets {
        presets(&[("mine", text)])
    }

    #[test]
    fn layers_each_over_the_last() {
        let presets = presets_with("[webm]\nfit = 'crf'\nfit-range = { min = 20, max = 40 }\n");
        let config = Config::default();
        let layers = [
            webm(webm::Options { effort: Some(Effort::Fast), fps: Some(10.0), ..default() }),
            webm(webm::Options { fps: Some(12.0), ..default() }),
        ];
        let resolved = resolve(&presets, &config, Format::Webm, Some("mine"), &layers).unwrap();
        let range = Some(Range { min: 20.0, max: 40.0 });
        let expected = webm::Options {
            fit: Some(Fit::Crf),
            fit_range: range,
            effort: Some(Effort::Fast),
            fps: Some(12.0),
            ..default()
        };
        assert_eq!(resolved.settings, webm(expected));
        // as each merges: a range of qualities is none of bitrates, and
        // a scale of nothing clears the preset's
        let other = [webm(webm::Options {
            fit: Some(Fit::Bitrate),
            integer_scale: Some(IntegerScale::NONE),
            ..default()
        })];
        let resolved = resolve(&presets, &config, Format::Webm, Some("mine"), &other).unwrap();
        assert_eq!(resolved.settings, webm(webm::Options { fit: Some(Fit::Bitrate), ..default() }));
        // only of the format resolved
        let err = resolve(&presets, &config, Format::Tgs, None, &[tgs(default()), webm(default())]);
        let wanted = Format::Tgs;
        assert_eq!(err, Err(ResolveError::OtherFormat { layer: 1, format: Format::Webm, wanted }));
    }

    #[test]
    fn blames_the_last_to_set_a_wrong_value() {
        let presets = presets_with("[webm]\ntempo = 50.0\n[tgs]\ntempo = 50.0\n");
        let config = Config::default();
        let invalid = |format, layers: &[Settings<FormatOptions>]| match resolve(
            &presets,
            &config,
            format,
            Some("mine"),
            layers,
        )
        .unwrap_err()
        {
            ResolveError::Invalid { option, set_by, .. } => (option, set_by),
            err => panic!("{err}"),
        };
        assert_eq!(invalid(Format::Webm, &[]), ("tempo", Origin::Preset));
        assert_eq!(invalid(Format::Tgs, &[tgs(default())]), ("tempo", Origin::Preset));
        let slow = webm(webm::Options { tempo: Some(0.0), ..default() });
        let none = webm(default());
        assert_eq!(invalid(Format::Webm, &[slow.clone(), none]), ("tempo", Origin::Layer(0)));
        // set over the preset's, it is what is checked
        let fine = webm(webm::Options { tempo: Some(2.0), ..default() });
        assert!(resolve(&presets, &config, Format::Webm, Some("mine"), &[fine]).is_ok());
        // said as whose it is
        let said = |layers: &[Settings<FormatOptions>]| {
            let err = resolve(&presets, &config, Format::Webm, Some("mine"), layers).unwrap_err();
            err.to_string()
        };
        assert_eq!(said(&[]), "preset \"mine\" sets tempo, which must be from 0.1 to 10");
        assert_eq!(said(&[slow]), "tempo must be from 0.1 to 10");
        // and checked though it has no say
        let late = presets_with("[webm]\nspoof = 'never'\nfake-duration = 5.0\n");
        let err = resolve(&late, &config, Format::Webm, Some("mine"), &[]).unwrap_err();
        assert!(matches!(err, ResolveError::Invalid { option: "fake-duration", .. }), "{err}");
    }

    /// A picture's options are checked, and none it doesn't have.
    #[test]
    fn checks_only_what_a_picture_has() {
        let presets = presets_with("[webm]\ntempo = 50.0\nscaling = 'sharp'\n");
        let config = Config::default();
        let resolved = resolve(&presets, &config, Format::Webp, Some("mine"), &[]).unwrap();
        let sharp = webp::Options { scaling: Some(crate::options::Scaling::Sharp), ..default() };
        assert_eq!(resolved.settings, picture(sharp));
        let start = Selection { start: Some(-1.0), ..default() };
        let early = chosen(start, FormatOptions::Webp(default()));
        let err = resolve(&presets, &config, Format::Webp, Some("mine"), &[early]).unwrap_err();
        assert!(matches!(err, ResolveError::Invalid { option: "start", .. }), "{err}");
    }

    #[test]
    fn reads_a_sheets_frames_only_of_a_sheet() {
        let presets = Presets::builtin();
        let config = Config::default();
        let frames =
            chosen(Selection { sheet_frames: Some(3), ..default() }, tgs(default()).options);
        let layers = [frames.clone(), tgs(default())];
        let err = resolve(&presets, &config, Format::Tgs, None, &layers).unwrap_err();
        let (option, requires) = ("sheet-frames", "sheet");
        assert_eq!(err, ResolveError::Requires { option, requires, set_by: Origin::Layer(0) });
        assert_eq!(err.to_string(), "sheet-frames requires sheet");
        let sheet =
            chosen(Selection { sheet: "4x1".parse().ok(), ..default() }, tgs(default()).options);
        assert!(resolve(&presets, &config, Format::Tgs, None, &[frames, sheet]).is_ok());
    }

    /// What is the file's own is layered as options are, and only what the
    /// format takes of it kept.
    #[test]
    fn takes_of_the_files_own_what_the_format_does() {
        let presets = Presets::builtin();
        let config = Config::default();
        let given = Selection {
            start: Some(1.0),
            length: Some(2.0),
            sheet: "4x1".parse().ok(),
            ..default()
        };
        let later = Selection { start: Some(1.5), ..default() };
        let kept = |format, options: FormatOptions| {
            let layers = [chosen(given.clone(), options.clone()), chosen(later.clone(), options)];
            resolve(&presets, &config, format, None, &layers).unwrap().settings.selection
        };
        let all = Selection { start: Some(1.5), ..given.clone() };
        assert_eq!(kept(Format::Tgs, tgs(default()).options), all);
        let unsheeted = Selection { sheet: None, ..all.clone() };
        assert_eq!(kept(Format::Webm, webm(default()).options), unsheeted);
        let picture = Selection { length: None, ..unsheeted };
        assert_eq!(kept(Format::Webp, FormatOptions::Webp(default())), picture);
        // and checked, blaming the last to set it
        let late = chosen(Selection { length: Some(0.0), ..default() }, webm(default()).options);
        let err = resolve(&presets, &config, Format::Webm, None, &[late, webm(default())]);
        let Err(ResolveError::Invalid { option, set_by, .. }) = err else { panic!("{err:?}") };
        assert_eq!((option, set_by), ("length", Origin::Layer(0)));
    }

    #[test]
    fn sets_aside_what_has_no_say_and_says_who_set_it() {
        let presets = presets_with("[webm]\nfit = 'bitrate'\ncrf = 20\nlossless = true\n");
        let config = Config::default();
        let layer = webm(webm::Options {
            spoof: Some(Spoof::Never),
            fake_duration: Some(1.0),
            ..default()
        });
        let resolved = resolve(&presets, &config, Format::Webm, Some("mine"), &[layer]).unwrap();
        let expected =
            webm::Options { fit: Some(Fit::Bitrate), spoof: Some(Spoof::Never), ..default() };
        assert_eq!(resolved.settings, webm(expected));
        let aside = |option, why: &str, set_by| SetAside { option, why: why.into(), set_by };
        assert_eq!(
            resolved.set_aside,
            [
                aside("crf", "fit is bitrate", Origin::Preset),
                aside("lossless", "fit is bitrate", Origin::Preset),
                aside("fake-duration", "spoof is never", Origin::Layer(0)),
            ]
        );
        // lossless off is left out too, but not said to be, as that
        // changes nothing
        let off = webm(webm::Options { lossless: Some(false), ..default() });
        let resolved = resolve(&presets, &config, Format::Webm, Some("mine"), &[off]).unwrap();
        let FormatOptions::Webm(options) = &resolved.settings.options else { panic!() };
        assert_eq!(options.lossless, None);
        assert_eq!(resolved.set_aside, [aside("crf", "fit is bitrate", Origin::Preset)]);
    }
}
