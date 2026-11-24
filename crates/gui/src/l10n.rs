//! GUI localisation backed by embedded Fluent resources in `i18n/`. British
//! English (`en-GB`) is complete and serves as the fallback for missing strings
//! in other languages.
//!
//! `fl!("message-id")`, `fl!("message-id", "attribute")`, and forms with
//! `name = value` arguments look up a localised string and are checked at
//! compile time against the English resource: the message, the attribute and
//! every argument name must exist.

use std::borrow::Cow;
use std::sync::LazyLock;

use i18n_embed::LanguageLoader;
use i18n_embed::fluent::{FluentLanguageLoader, fluent_language_loader};
use i18n_embed::unic_langid::LanguageIdentifier;

#[derive(rust_embed::RustEmbed)]
#[folder = "i18n"]
struct Files;

/// The messages, in English until `choose` picks another language.
pub(crate) static LOADER: LazyLock<FluentLanguageLoader> = LazyLock::new(|| {
    let loader = fluent_language_loader!();
    loader.load_fallback_language(&Files).expect("English is built in");
    settle(&loader, false);
    loader
});

/// A message in the GUI's language, see the module's docs.
macro_rules! fl {
    ($id:literal) => {
        $crate::l10n::seen(
            $id,
            $crate::l10n::unmarked(i18n_embed_fl::fl!($crate::l10n::LOADER, $id)),
        )
    };
    ($id:literal, $attribute:literal $(, $($args:tt)*)?) => {
        $crate::l10n::seen(
            concat!($id, ".", $attribute),
            $crate::l10n::unmarked(i18n_embed_fl::fl!(
                $crate::l10n::LOADER, $id, $attribute $(, $($args)*)?
            )),
        )
    };
    ($id:literal, $($args:tt)*) => {
        $crate::l10n::seen(
            $id,
            $crate::l10n::unmarked(i18n_embed_fl::fl!($crate::l10n::LOADER, $id, $($args)*)),
        )
    };
}
pub(crate) use fl;

/// A message as `fl!` takes it, kept as a [`Text`] to be worded in the
/// GUI's language each time it is shown, rather than in the one shown
/// when it was made: for what is said and shown later, as notices,
/// failures and a result's problems, which a change of language words
/// again at once. Its arguments are taken when it is made.
macro_rules! later {
    ($id:literal) => {
        $crate::l10n::Text::worded(move || $crate::l10n::fl!($id))
    };
    ($id:literal, $attribute:literal) => {
        $crate::l10n::Text::worded(move || $crate::l10n::fl!($id, $attribute))
    };
    ($id:literal, $attribute:literal, $($name:ident = $value:expr),+ $(,)?) => {{
        $(let $name = $crate::l10n::Keep::keep($value);)+
        $crate::l10n::Text::worded(move || {
            $crate::l10n::fl!($id, $attribute, $($name = $crate::l10n::Kept::give(&$name)),+)
        })
    }};
    ($id:literal, $($name:ident = $value:expr),+ $(,)?) => {{
        $(let $name = $crate::l10n::Keep::keep($value);)+
        $crate::l10n::Text::worded(move || {
            $crate::l10n::fl!($id, $($name = $crate::l10n::Kept::give(&$name)),+)
        })
    }};
}
pub(crate) use later;

/// Words shown, kept to be shown later: tgradish's own, worded in the
/// language shown whenever they are (see [`later!`]), or what came from
/// elsewhere, as a program or the system said it, as it is. Two are the
/// same where they say the same now.
#[derive(Clone)]
pub(crate) enum Text {
    Said(std::sync::Arc<str>),
    Worded(std::sync::Arc<dyn Fn() -> String + Send + Sync>),
}

impl Text {
    /// Words of tgradish's own, as `word` words them when shown.
    pub(crate) fn worded(word: impl Fn() -> String + Send + Sync + 'static) -> Text {
        Text::Worded(std::sync::Arc::new(word))
    }

    /// What came from elsewhere, as a program, the system or core said
    /// it, shown as it is: never tgradish's own words, which [`later!`]
    /// keeps.
    pub(crate) fn said(said: impl Into<String>) -> Text {
        Text::Said(said.into().into())
    }

    /// What it says, in the language shown now.
    pub(crate) fn say(&self) -> String {
        match self {
            Text::Said(said) => said.to_string(),
            Text::Worded(word) => word(),
        }
    }
}

impl Default for Text {
    /// Nothing.
    fn default() -> Text {
        Text::from("")
    }
}

impl std::fmt::Display for Text {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.say())
    }
}

impl std::fmt::Debug for Text {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.say())
    }
}

impl PartialEq for Text {
    fn eq(&self, other: &Text) -> bool {
        self.say() == other.say()
    }
}

impl Eq for Text {}

impl PartialEq<str> for Text {
    fn eq(&self, other: &str) -> bool {
        self.say() == other
    }
}

impl PartialEq<&str> for Text {
    fn eq(&self, other: &&str) -> bool {
        self.say() == *other
    }
}

impl PartialEq<String> for Text {
    fn eq(&self, other: &String) -> bool {
        self.say() == *other
    }
}

// No `From<String>`: what `fl!` gives is in the language shown as it is
// given, and kept, it would stay in it. `Text::said` takes what came from
// elsewhere, and `later!` tgradish's own words.

impl From<&str> for Text {
    fn from(said: &str) -> Text {
        Text::Said(said.into())
    }
}

impl From<&Text> for eframe::egui::WidgetText {
    fn from(text: &Text) -> eframe::egui::WidgetText {
        text.say().into()
    }
}

impl From<Text> for eframe::egui::WidgetText {
    fn from(text: Text) -> eframe::egui::WidgetText {
        text.say().into()
    }
}

/// An argument of [`later!`], as it is kept until worded: owned.
pub(crate) trait Keep {
    type Kept: Kept;
    fn keep(self) -> Self::Kept;
}

/// An argument of [`later!`] as kept, which gives Fluent its value each
/// time it is worded.
pub(crate) trait Kept: Send + Sync + 'static {
    type Given;
    fn give(&self) -> Self::Given;
}

impl Keep for &str {
    type Kept = String;
    fn keep(self) -> String {
        self.to_owned()
    }
}

impl Keep for &String {
    type Kept = String;
    fn keep(self) -> String {
        self.clone()
    }
}

impl Keep for String {
    type Kept = String;
    fn keep(self) -> String {
        self
    }
}

impl Kept for String {
    type Given = String;
    fn give(&self) -> String {
        self.clone()
    }
}

impl Keep for Text {
    type Kept = Text;
    fn keep(self) -> Text {
        self
    }
}

impl Keep for &Text {
    type Kept = Text;
    fn keep(self) -> Text {
        self.clone()
    }
}

impl Kept for Text {
    type Given = String;
    fn give(&self) -> String {
        self.say()
    }
}

/// Numbers are kept as they are.
macro_rules! keep_numbers {
    ($($number:ty),*) => {$(
        impl Keep for $number {
            type Kept = $number;
            fn keep(self) -> $number {
                self
            }
        }

        impl Kept for $number {
            type Given = $number;
            fn give(&self) -> $number {
                *self
            }
        }
    )*};
}
keep_numbers!(u8, u16, u32, u64, usize, i32, i64, f32, f64);

/// The language asked for by `TGRADISH_LANG` instead of the config's and
/// the system's: a tag such as `de`, or `pseudo` for English with its
/// letters accented and its vowels doubled, to see where longer words
/// would not fit.
pub(crate) const VARIABLE: &str = "TGRADISH_LANG";

/// The languages there are translations for, each with what it calls
/// itself, in the order of those names.
pub(crate) fn languages() -> &'static [(LanguageIdentifier, String)] {
    static LANGUAGES: LazyLock<Vec<(LanguageIdentifier, String)>> = LazyLock::new(|| {
        let mut languages: Vec<_> = LOADER
            .available_languages(&Files)
            .unwrap_or_default()
            .into_iter()
            .map(|language| {
                // its own file alone, as `LOADER` loads only those shown
                let loader = FluentLanguageLoader::new(LOADER.domain(), language.clone());
                let name = match loader.load_languages(&Files, std::slice::from_ref(&language)) {
                    Ok(()) => loader.get("language-name"),
                    Err(_) => language.to_string(),
                };
                (language, name)
            })
            .collect();
        // by name, as those who read each know it; Latin letters before
        // Cyrillic ones, as Unicode has them
        languages.sort_by_cached_key(|(_, name)| name.to_lowercase());
        languages
    });
    &LANGUAGES
}

/// What `language` calls itself, if there is a translation into it.
pub(crate) fn name_of(language: &LanguageIdentifier) -> Option<&'static str> {
    languages().iter().find(|(had, _)| had == language).map(|(_, name)| name.as_str())
}

/// What `TGRADISH_LANG` asks for, if it is set.
pub(crate) fn variable() -> Option<String> {
    std::env::var(VARIABLE).ok().filter(|tag| !tag.is_empty())
}

/// The language the system's settings pick of those there are
/// translations for, else British English.
pub(crate) fn system() -> LanguageIdentifier {
    let asked: Vec<LanguageIdentifier> =
        system_locales().iter().filter_map(|tag| asked_language(tag)).collect();
    pick(&asked, &available(), LOADER.fallback_language())
}

/// The languages there are translations for.
fn available() -> Vec<LanguageIdentifier> {
    languages().iter().map(|(language, _)| language.clone()).collect()
}

/// The system's languages, the one it prefers first, as it writes them.
#[cfg(all(unix, not(any(target_vendor = "apple", target_os = "android"))))]
fn system_locales() -> Vec<String> {
    posix_locales(|name| std::env::var(name).ok())
}

#[cfg(any(not(unix), target_vendor = "apple", target_os = "android"))]
fn system_locales() -> Vec<String> {
    sys_locale::get_locales().collect()
}

/// The locales the environment, as `var` reads it, asks for, in the
/// order `sys_locale` reads them: the list in `LANGUAGE`, then `LC_ALL`,
/// `LC_MESSAGES` and `LANG`. Unlike `sys_locale`, it keeps a modifier,
/// as Cyrillic Uzbek's `uz_UZ.UTF-8@cyrillic`, which can name a script.
#[cfg(all(unix, not(any(target_vendor = "apple", target_os = "android"))))]
fn posix_locales(var: impl Fn(&str) -> Option<String>) -> Vec<String> {
    let var = |name: &str| var(name).filter(|value| !value.is_empty());
    let listed = var("LANGUAGE").map(|list| list.split(':').map(str::to_owned).collect::<Vec<_>>());
    let set = ["LC_ALL", "LC_MESSAGES", "LANG"].into_iter().filter_map(var);
    listed.into_iter().flatten().chain(set).filter(|tag| !tag.is_empty()).collect()
}

/// The language `tag` asks for, written as BCP 47 writes it, as `pt-BR`,
/// or as POSIX does, as `pt_BR.UTF-8` or `uz_UZ@cyrillic`. The C locale,
/// whose messages are untranslated, asks for English; what isn't a
/// language asks for none.
fn asked_language(tag: &str) -> Option<LanguageIdentifier> {
    let tag = tag.trim();
    let (tag, modifier) = tag.split_once('@').map_or((tag, None), |(tag, rest)| (tag, Some(rest)));
    let tag = tag.split_once('.').map_or(tag, |(tag, _codeset)| tag);
    if tag == "C" || tag == "POSIX" {
        return "en".parse().ok();
    }
    let mut asked: LanguageIdentifier = tag.parse().ok()?;
    if asked.language.is_empty() {
        return None;
    }
    // the old codes that Java, and so Android, still gives for these
    const RENAMED: [(&str, &str); 3] = [("in", "id"), ("iw", "he"), ("ji", "yi")];
    if let Some((_, now)) = RENAMED.iter().find(|(old, _)| asked.language == *old) {
        asked.language = now.parse().ok()?;
    }
    // POSIX names a script by a modifier
    if asked.script.is_none() {
        let script = match modifier {
            Some("cyrillic") => Some("Cyrl"),
            Some("latin") => Some("Latn"),
            _ => None,
        };
        asked.script = script.and_then(|script| script.parse().ok());
    }
    Some(asked)
}

/// The script each language written in more than one is written in where
/// its tag names none: in a region, then elsewhere. A translation's file
/// with no script in its tag is written in it, and a system asking for
/// the language without one means it.
const USUAL_SCRIPTS: [(&str, Option<&str>, &str); 2] =
    [("uz", Some("AF"), "Arab"), ("uz", None, "Latn")];

/// The script `language` is written in, as its tag names it or else as
/// [`USUAL_SCRIPTS`] has it, where that is known.
fn script_of(language: &LanguageIdentifier) -> Option<&str> {
    if let Some(script) = &language.script {
        return Some(script.as_str());
    }
    let region = language.region.as_ref().map(|region| region.as_str());
    USUAL_SCRIPTS
        .iter()
        .find(|(of, only_in, _)| {
            language.language == *of && only_in.is_none_or(|only_in| region == Some(only_in))
        })
        .map(|(_, _, script)| *script)
}

/// The language the GUI is shown in.
pub(crate) fn current() -> LanguageIdentifier {
    LOADER.current_language()
}

/// Regions where English is spelt as in the United States: the country
/// itself, its territories, and the Philippines. Elsewhere, as in Canada,
/// Australia or India, British spelling is nearer the custom.
const AMERICAN: [&str; 8] = ["US", "AS", "GU", "MP", "PR", "UM", "VI", "PH"];

/// Which of `available` to show for `asked`, the languages asked for in
/// the order they are preferred: the first asked for that there is a
/// translation into, else `fallback`. Of a language's several, as
/// English's, the one of the region asked for, else for English the
/// British or American as the region spells it, else the one `fallback`
/// is, else the first. A language written in another script than the one
/// asked for, as Latin Uzbek for Cyrillic, isn't the language asked for.
fn pick(
    asked: &[LanguageIdentifier],
    available: &[LanguageIdentifier],
    fallback: &LanguageIdentifier,
) -> LanguageIdentifier {
    for asked in asked {
        let script = script_of(asked);
        let of_language: Vec<&LanguageIdentifier> = available
            .iter()
            .filter(|had| had.language == asked.language)
            .filter(|had| script.is_none() || script_of(had).is_none_or(|had| script == Some(had)))
            .collect();
        let of_region = |region: &str| {
            of_language
                .iter()
                .find(|had| had.region.as_ref().is_some_and(|had| had.as_str() == region))
        };
        let english = asked.language.as_str() == "en";
        let region = asked.region.as_ref().map(|region| region.as_str());
        let found = region
            .and_then(of_region)
            .or_else(|| {
                let american = region.is_some_and(|region| AMERICAN.contains(&region));
                english.then(|| of_region(if american { "US" } else { "GB" })).flatten()
            })
            .or_else(|| of_language.iter().find(|had| **had == fallback))
            .or_else(|| of_language.first());
        if let Some(found) = found {
            return (*found).clone();
        }
    }
    fallback.clone()
}

/// Shows the GUI in `language`, as a tag, or in the system's when
/// `None`, as far as there are translations, else in English.
/// `TGRADISH_LANG` comes first.
pub(crate) fn choose(language: Option<&str>) {
    let variable = variable();
    let pseudo = variable.as_deref() == Some("pseudo");
    let fallback = LOADER.fallback_language().clone();
    // English stands in for what a translation lacks
    let language = match variable.as_deref().filter(|_| !pseudo).or(language) {
        _ if pseudo => fallback,
        Some(tag) => pick(&Vec::from_iter(asked_language(tag)), &available(), &fallback),
        None => system(),
    };
    if LOADER.load_languages(&Files, &[language]).is_err() {
        let _ = LOADER.load_fallback_language(&Files);
    }
    settle(&LOADER, pseudo);
}

/// Sets up the messages just loaded: without the marks Fluent puts
/// around arguments for text running both ways, which tgradish's fonts
/// lack and its layout doesn't need, and, for `pseudo`, transformed.
fn settle(loader: &FluentLanguageLoader, pseudo: bool) {
    loader.set_use_isolating(false);
    loader.with_bundles_mut(|bundle| bundle.set_transform(pseudo.then_some(pseudo_text)));
}

/// `text` with its letters accented and its vowels doubled: about a third
/// longer, as many languages' words are than English's.
fn pseudo_text(text: &str) -> Cow<'_, str> {
    const PLAIN: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    const ACCENTED: &str = "åƀçďęƒğĥïĵķľmñøþqřšŧůvŵxýžÅƁÇÐĘFĞĤÏĴĶĽMÑØÞQŘŠŦŮVŴXÝŽ";
    let mut out = String::with_capacity(text.len() * 2);
    for c in text.chars() {
        let accented = PLAIN.find(c).and_then(|at| ACCENTED.chars().nth(at)).unwrap_or(c);
        out.push(accented);
        if "aeiouyAEIOUY".contains(c) {
            out.push(accented);
        }
    }
    Cow::Owned(out)
}

/// Shows the GUI in British English, its own language, for the tests that
/// hold English's words to fitting where other languages' may not.
#[cfg(test)]
pub(crate) fn english() {
    let english = LOADER.fallback_language().clone();
    LOADER.load_languages(&Files, &[english]).expect("English is built in");
    settle(&LOADER, false);
    assert_eq!(LOADER.current_language().to_string(), "en-GB");
}

/// `text` without the marks Fluent puts around arguments, which [`settle`]
/// turns off: messages loaded again, as each window opening and a change of
/// language load them, have them until it has, and one given on another
/// thread meanwhile would show them as boxes, or not match the same words.
pub(crate) fn unmarked(text: String) -> String {
    const MARKS: [char; 2] = ['\u{2068}', '\u{2069}'];
    if text.contains(MARKS) { text.replace(MARKS, "") } else { text }
}

/// `text`, noted as shown for `id` while `seen` records, for the pictures
/// of where each message is shown.
#[cfg(not(test))]
#[inline(always)]
pub(crate) fn seen(_id: &'static str, text: String) -> String {
    text
}

#[cfg(test)]
pub(crate) use record::seen;

#[cfg(test)]
pub(crate) mod record {
    use std::cell::RefCell;
    use std::collections::BTreeSet;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};

    thread_local! {
        static SEEN: RefCell<Option<Vec<(&'static str, String)>>> = const { RefCell::new(None) };
    }

    /// Every message given on any thread since `remember`, in order, for
    /// those the GUI keeps and shows later, as notices and errors.
    static EVER: Mutex<(Vec<Said>, BTreeSet<Said>)> = Mutex::new((Vec::new(), BTreeSet::new()));

    /// A message given: its name and words.
    type Said = (&'static str, String);
    static REMEMBER: AtomicBool = AtomicBool::new(false);

    /// `text`, noted for `id` if recording.
    pub(crate) fn seen(id: &'static str, text: String) -> String {
        SEEN.with_borrow_mut(|seen| {
            if let Some(seen) = seen {
                seen.push((id, text.clone()));
            }
        });
        if REMEMBER.load(Ordering::Relaxed) {
            let (order, all) = &mut *EVER.lock().unwrap();
            if all.insert((id, text.clone())) {
                order.push((id, text.clone()));
            }
        }
        text
    }

    /// Starts noting every message given, on any thread, for [`since`].
    pub(crate) fn remember() {
        REMEMBER.store(true, Ordering::Relaxed);
    }

    /// The messages given since `remember`, from the `from`th on.
    pub(crate) fn since(from: usize) -> Vec<(&'static str, String)> {
        EVER.lock().unwrap().0.get(from..).map(<[_]>::to_vec).unwrap_or_default()
    }

    /// Starts noting the messages shown on this thread, forgetting those
    /// noted before.
    pub(crate) fn start() {
        SEEN.set(Some(Vec::new()));
    }

    /// The messages shown since `start` or the last `take`.
    pub(crate) fn take() -> Vec<(&'static str, String)> {
        SEEN.with_borrow_mut(|seen| seen.as_mut().map(std::mem::take).unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use fluent_syntax::ast;

    use super::*;

    /// Each language's file, by its tag, as written.
    fn files() -> BTreeMap<String, String> {
        Files::iter()
            .map(|path| {
                let tag = path.split('/').next().unwrap_or_default().to_owned();
                let text = String::from_utf8(Files::get(&path).unwrap().data.into_owned());
                (tag, text.expect("UTF-8"))
            })
            .collect()
    }

    fn parse(tag: &str, text: &str) -> ast::Resource<String> {
        fluent_syntax::parser::parse(text.to_owned()).unwrap_or_else(|(_, errors)| {
            panic!("{tag} doesn't read as Fluent: {errors:?}");
        })
    }

    /// What a message or term says, with its attributes: the patterns and
    /// what each is named, as `id` or `id.attribute`.
    fn patterns(resource: &ast::Resource<String>) -> Vec<(String, &ast::Pattern<String>)> {
        let mut patterns = Vec::new();
        for entry in &resource.body {
            let (id, value, attributes) = match entry {
                ast::Entry::Message(message) => {
                    (message.id.name.clone(), message.value.as_ref(), &message.attributes)
                }
                ast::Entry::Term(term) => {
                    (format!("-{}", term.id.name), Some(&term.value), &term.attributes)
                }
                _ => continue,
            };
            patterns.extend(value.map(|value| (id.clone(), value)));
            for attribute in attributes {
                patterns.push((format!("{id}.{}", attribute.id.name), &attribute.value));
            }
        }
        patterns
    }

    /// What `pattern` holds: its text, the arguments it takes, the
    /// messages and terms it puts in, as `id` or `id.attribute`, the
    /// plural forms each choice by a count has, as `one` and `other`, and
    /// the counts chosen by so that none of the choices shows.
    #[derive(Default)]
    struct Parts {
        text: String,
        arguments: BTreeSet<String>,
        references: BTreeSet<String>,
        plurals: Vec<BTreeSet<String>>,
        unshown: Vec<String>,
    }

    /// Messages that choose plural forms by a count they don't show, as it
    /// is shown beside them: `attempts-times`, after the field of the
    /// number of encodes. Others that don't show it choose by `[1]`,
    /// exactly one, as a plural form such as Ukrainian's `[one]` is 21's
    /// too.
    const SHOWN_BESIDE: [&str; 1] = ["attempts-times"];

    /// The names of plural forms, as Unicode's rules for counting name them.
    const PLURAL_FORMS: [&str; 6] = ["zero", "one", "two", "few", "many", "other"];

    fn parts(pattern: &ast::Pattern<String>, into: &mut Parts) {
        for element in &pattern.elements {
            match element {
                ast::PatternElement::TextElement { value } => into.text.push_str(value),
                ast::PatternElement::Placeable { expression } => expression_parts(expression, into),
            }
        }
    }

    fn expression_parts(expression: &ast::Expression<String>, into: &mut Parts) {
        let inline = |inline: &ast::InlineExpression<String>, into: &mut Parts| match inline {
            ast::InlineExpression::VariableReference { id } => {
                into.arguments.insert(id.name.clone());
            }
            ast::InlineExpression::MessageReference { id, attribute } => {
                into.references.insert(match attribute {
                    Some(attribute) => format!("{}.{}", id.name, attribute.name),
                    None => id.name.clone(),
                });
            }
            ast::InlineExpression::TermReference { id, .. } => {
                into.references.insert(format!("-{}", id.name));
            }
            ast::InlineExpression::Placeable { expression } => expression_parts(expression, into),
            ast::InlineExpression::StringLiteral { value } => into.text.push_str(value),
            _ => {}
        };
        match expression {
            ast::Expression::Inline(expression) => inline(expression, into),
            ast::Expression::Select { selector, variants } => {
                inline(selector, into);
                // a choice by a count, its variants named for plural forms
                let names: BTreeSet<String> = variants
                    .iter()
                    .filter_map(|variant| match &variant.key {
                        ast::VariantKey::Identifier { name } => Some(name.clone()),
                        ast::VariantKey::NumberLiteral { .. } => None,
                    })
                    .collect();
                // [other] alone is what is said otherwise, not a form
                let forms = PLURAL_FORMS.iter().filter(|form| **form != "other");
                if forms.into_iter().any(|form| names.contains(*form)) {
                    into.plurals.push(names);
                    if let ast::InlineExpression::VariableReference { id } = selector {
                        let shows = |variant: &ast::Variant<String>| {
                            let mut shown = Parts::default();
                            parts(&variant.value, &mut shown);
                            shown.arguments.contains(&id.name)
                        };
                        if !variants.iter().any(shows) {
                            into.unshown.push(id.name.clone());
                        }
                    }
                }
                for variant in variants {
                    parts(&variant.value, into);
                }
            }
        }
    }

    fn english() -> ast::Resource<String> {
        parse("en-GB", &files()["en-GB"])
    }

    type Bundle = fluent_bundle::FluentBundle<fluent_bundle::FluentResource>;

    /// `id`, as `message` or `message.attribute`, split in two.
    fn split(id: &str) -> (&str, Option<&str>) {
        id.split_once('.').map_or((id, None), |(message, attribute)| (message, Some(attribute)))
    }

    /// What `bundle` has for `id`, as `message` or `message.attribute`.
    fn pattern_of<'a>(
        bundle: &'a Bundle,
        id: &str,
    ) -> Option<&'a fluent_syntax::ast::Pattern<&'a str>> {
        let (message, attribute) = split(id);
        let message = bundle.get_message(message)?;
        match attribute {
            Some(attribute) => message.get_attribute(attribute).map(|found| found.value()),
            None => message.value(),
        }
    }

    /// What British English says, which every other language is checked
    /// against: each message and attribute, by `id` or `id.attribute`,
    /// with the arguments it takes and what it is written as; and the
    /// messages ready to show, for saying what a translation lacks.
    struct English {
        said: BTreeMap<String, (BTreeSet<String>, ast::Pattern<String>)>,
        bundle: Bundle,
    }

    impl English {
        fn new() -> English {
            let said = patterns(&english())
                .into_iter()
                .filter(|(id, _)| !id.starts_with('-'))
                .map(|(id, pattern)| {
                    let mut found = Parts::default();
                    parts(pattern, &mut found);
                    (id, (found.arguments, pattern.clone()))
                })
                .collect();
            let mut bundle = Bundle::new(vec![LOADER.fallback_language().clone()]);
            bundle.set_use_isolating(false);
            let text = files()["en-GB"].clone();
            bundle.add_resource(fluent_bundle::FluentResource::try_new(text).unwrap()).unwrap();
            English { said, bundle }
        }

        /// What English says for `id`, arguments shown as `{$count}`.
        fn words(&self, id: &str) -> String {
            let pattern = pattern_of(&self.bundle, id).expect("English has it");
            self.bundle.format_pattern(pattern, None, &mut Vec::new()).into_owned()
        }
    }

    /// The plural forms `language` counts with, as Fluent picks them:
    /// `one` and `other` for English, `one`, `few`, `many` and `other`
    /// for Ukrainian.
    fn plural_forms(language: &LanguageIdentifier) -> BTreeSet<String> {
        let variants: Vec<String> = PLURAL_FORMS
            .iter()
            .map(|form| format!("    {}[{form}] {form}\n", if *form == "other" { "*" } else { "" }))
            .collect();
        let forms = format!("forms = {{ $n ->\n{}}}\n", variants.concat());
        let mut bundle = Bundle::new(vec![language.clone()]);
        bundle.add_resource(fluent_bundle::FluentResource::try_new(forms).unwrap()).unwrap();
        let pattern = pattern_of(&bundle, "forms").unwrap();
        let whole = (0..=200).map(fluent_bundle::FluentValue::from);
        let numbers = whole.chain([0.5, 1.5, 2.5].map(fluent_bundle::FluentValue::from));
        numbers
            .map(|number| {
                let mut args = fluent_bundle::FluentArgs::new();
                args.set("n", number);
                bundle.format_pattern(pattern, Some(&args), &mut Vec::new()).into_owned()
            })
            .collect()
    }

    /// What is wrong with the file of the language `tag`, `text`, each as
    /// a line for its translator; nothing for a sound one. A translation
    /// has every message and attribute English has, else English shows
    /// for them, which only a regional variant of English, as `en-US`,
    /// may leave to it; and nothing English hasn't, nor any argument
    /// English's message doesn't take.
    fn problems(tag: &str, text: &str, english: &English) -> Vec<String> {
        let mut problems = Vec::new();
        let fallback = LOADER.fallback_language();
        let language: LanguageIdentifier = tag.parse().unwrap_or_else(|_| {
            problems.push(format!("{tag:?} isn't a language tag, as de or pt-BR"));
            fallback.clone()
        });
        let line = |at: usize| text[..at.min(text.len())].matches('\n').count() + 1;
        let resource =
            fluent_syntax::parser::parse(text.to_owned()).unwrap_or_else(|(resource, errors)| {
                for error in errors {
                    problems.push(format!("line {}: {error}", line(error.pos.start)));
                }
                resource
            });
        let patterns = patterns(&resource);
        let had: BTreeSet<&str> = patterns.iter().map(|(id, _)| id.as_str()).collect();
        // what its messages put in, which it must give itself
        let mut put_in = BTreeSet::new();
        for (_, pattern) in &patterns {
            let mut found = Parts::default();
            parts(pattern, &mut found);
            put_in.extend(found.references);
        }
        // English stands in for what a variant of it lacks
        let variant = language.language == fallback.language && language != *fallback;
        let forms = plural_forms(&language);
        let mut given = BTreeSet::new();
        for (id, pattern) in &patterns {
            if !given.insert(id) {
                problems.push(format!("{id} is given twice"));
            }
            let mut found = Parts::default();
            parts(pattern, &mut found);
            let arguments = |names: &mut dyn Iterator<Item = &String>| -> String {
                let names: Vec<String> = names.map(|name| format!("{{ ${name} }}")).collect();
                if names.is_empty() { "none".to_owned() } else { names.join(", ") }
            };
            match english.said.get(id) {
                _ if id.starts_with('-') => {}
                None => problems.push(format!("{id} isn't a message English has: leave it out")),
                Some((known, _)) if !found.arguments.is_subset(known) => problems.push(format!(
                    "{id} takes {}, which English's doesn't: it takes {}",
                    arguments(&mut found.arguments.difference(known)),
                    arguments(&mut known.iter()),
                )),
                Some((_, english)) if variant && english == *pattern && !put_in.contains(id) => {
                    problems.push(format!(
                        "{id} says just what en-GB does: leave it out, as en-GB stands in for it"
                    ));
                }
                Some(_) => {}
            }
            for reference in &found.references {
                let term = reference.starts_with('-');
                if term && !had.contains(split(reference).0) {
                    problems.push(format!("{id} puts in {{ {reference} }}, a term not given here"));
                } else if !term && !english.said.contains_key(reference) {
                    problems.push(format!("{id} puts in {{ {reference} }}, which English hasn't"));
                } else if !term && !had.contains(reference.as_str()) {
                    // English stands in for whole messages, not for parts
                    problems
                        .push(format!("{id} puts in {{ {reference} }}, which isn't given here"));
                }
            }
            for named in &found.plurals {
                let lacking: Vec<String> =
                    forms.difference(named).map(|form| format!("[{form}]")).collect();
                if !lacking.is_empty() {
                    let named: Vec<String> = named.iter().map(|form| format!("[{form}]")).collect();
                    problems.push(format!(
                        "{id} counts with {} but not {}, a plural form of {tag}",
                        named.join(" "),
                        lacking.join(" ")
                    ));
                }
            }
            for count in found.unshown.iter().filter(|_| !SHOWN_BESIDE.contains(&id.as_str())) {
                problems.push(format!(
                    "{id} chooses plural forms by {{ ${count} }}, which it doesn't show: choose \
                     [1] for exactly one, and *[other] otherwise"
                ));
            }
        }
        if !had.contains("language-name") {
            problems.push("language-name, the language's name for itself, is missing".into());
        }
        if !variant {
            for id in english.said.keys().filter(|id| !had.contains(id.as_str())) {
                problems.push(format!("{id} is missing; English says {:?}", english.words(id)));
            }
        }
        // what it says, as the GUI would show it, with counts and words in
        // for its arguments
        let mut bundle = Bundle::new(vec![language]);
        bundle.set_use_isolating(false);
        let resource = fluent_bundle::FluentResource::try_new(text.to_owned())
            .unwrap_or_else(|(resource, _)| resource);
        if let Err(errors) = bundle.add_resource(resource) {
            // as a term and a message of one name, which hide each other
            problems.extend(errors.iter().map(|error| format!("{error}")));
        }
        for (id, pattern) in patterns.iter().filter(|(id, _)| !id.starts_with('-')) {
            let mut found = Parts::default();
            parts(pattern, &mut found);
            let Some(pattern) = pattern_of(&bundle, id) else { continue };
            let mut said = BTreeSet::new();
            for count in [1, 2, 5] {
                let mut args = fluent_bundle::FluentArgs::new();
                for argument in &found.arguments {
                    args.set(argument.as_str(), count);
                }
                let mut errors = Vec::new();
                let text = bundle.format_pattern(pattern, Some(&args), &mut errors);
                if text.trim().is_empty() {
                    said.insert(format!("{id} says nothing"));
                }
                // what it puts in that isn't there is told above
                let reference = |error: &fluent_bundle::FluentError| {
                    use fluent_bundle::resolver::ResolverError;
                    matches!(
                        error,
                        fluent_bundle::FluentError::ResolverError(ResolverError::Reference(_))
                    )
                };
                said.extend(errors.iter().filter(|e| !reference(e)).map(|e| format!("{id}: {e}")));
            }
            problems.extend(said);
        }
        problems
    }

    /// Every language's file is sound, as [`problems`] tells: it reads as
    /// Fluent, and says what English does, all of it but in a regional
    /// variant of English, with the arguments English's messages take and
    /// the plural forms of its language. A translator sees what is wrong
    /// with every file at once with
    /// `cargo test -p tgradish-gui every_language_file_is_sound`.
    #[test]
    fn every_language_file_is_sound() {
        let english = English::new();
        let mut wrong = Vec::new();
        for path in Files::iter() {
            let Some((tag, "tgradish.ftl")) = path.split_once('/') else {
                wrong.push(format!("i18n/{path}: each language's file is i18n/<tag>/tgradish.ftl"));
                continue;
            };
            let text = String::from_utf8(Files::get(&path).unwrap().data.into_owned());
            let problems = match text {
                Ok(text) => problems(tag, &text, &english),
                Err(_) => vec!["isn't UTF-8".to_owned()],
            };
            if !problems.is_empty() {
                let count = problems.len();
                wrong.push(format!("crates/gui/i18n/{path}, {count} problems:"));
                wrong.extend(problems.into_iter().map(|problem| format!("    {problem}")));
            }
        }
        assert!(wrong.is_empty(), "\n{}\n", wrong.join("\n"));
    }

    #[test]
    fn tells_what_is_wrong_with_a_translation() {
        let english = English::new();
        let text = "language-name = Test\n\
            close = { $who } { -app } { never } { save }\n\
            count-frames = { $count ->\n    [one] a\n   *[other] b\n}\n\
            unknown = x\n\
            broken = { $ }\n\
            close = again\n\
            no-font = { $count ->\n    [one] { $codes }\n    [few] c\n    [many] d\n   *[other] e\n}\n";
        let said = problems("uk", text, &english);
        let has = |part: &str| said.iter().any(|problem| problem.contains(part));
        for part in [
            "line 8:",
            "close takes { $who }, which English's doesn't: it takes none",
            "close puts in { -app }, a term not given here",
            "close puts in { never }, which English hasn't",
            "close puts in { save }, which isn't given here",
            "count-frames counts with [one] [other] but not [few] [many], a plural form of uk",
            "no-font chooses plural forms by { $count }, which it doesn't show",
            "unknown isn't a message English has",
            "close is given twice",
            "save is missing; English says \"Save\"",
        ] {
            assert!(has(part), "no {part:?} in {said:#?}");
        }
        // a variant of English may leave messages to it, but not repeat them
        let variant = problems("en-US", "language-name = Test\nclose = Close\n", &english);
        assert_eq!(variant.len(), 1, "{variant:#?}");
        assert!(variant[0].starts_with("close says just what en-GB does"), "{variant:#?}");
        let unnamed = problems("de", "", &english);
        assert!(unnamed.iter().any(|problem| problem.starts_with("language-name")));
    }

    #[test]
    fn gives_arguments_without_marks_while_the_language_is_chosen_again() {
        // a message given on one thread as another loads the language
        // again, as every window opening and a change of language does
        use std::sync::atomic::{AtomicBool, Ordering};
        let done = std::sync::Arc::new(AtomicBool::new(false));
        let choosing = {
            let done = done.clone();
            std::thread::spawn(move || {
                for _ in 0..200 {
                    super::english();
                }
                done.store(true, Ordering::Relaxed);
            })
        };
        let mut marked = None;
        while !done.load(Ordering::Relaxed) && marked.is_none() {
            let said = fl!("name-frames", name = "dance");
            marked = said.contains(['\u{2068}', '\u{2069}']).then_some(said);
        }
        choosing.join().unwrap();
        assert_eq!(marked, None);
    }

    /// Where a comment, string or character starting at `at` in `code`
    /// ends, if one does.
    fn literal_end(code: &str, at: usize) -> Option<usize> {
        let rest = &code[at..];
        let after = |pattern: &str, from: usize| rest[from..].find(pattern).map(|end| end + from);
        if rest.starts_with("//") {
            return Some(at + after("\n", 0).unwrap_or(rest.len()));
        }
        if rest.starts_with("/*") {
            return Some(at + after("*/", 2).map_or(rest.len(), |end| end + 2));
        }
        // a prefix, as of `r"…"` or `b"…"`, rather than a name's end
        let prefixed = !code[..at].ends_with(|c: char| c.is_alphanumeric() || c == '_');
        let raw = rest.strip_prefix("br").or(rest.strip_prefix('r')).filter(|_| prefixed);
        if let Some(hashes) = raw {
            let count = hashes.len() - hashes.trim_start_matches('#').len();
            if hashes[count..].starts_with('"') {
                let close = format!("\"{}", "#".repeat(count));
                let open = rest.len() - hashes.len() + count + 1;
                return Some(at + after(&close, open).map_or(rest.len(), |end| end + close.len()));
            }
        }
        let start = match rest.as_bytes() {
            [b'"', ..] => Some(1),
            [b'b', b'"', ..] if prefixed => Some(2),
            _ => None,
        };
        if let Some(start) = start {
            let mut chars = rest[start..].char_indices();
            while let Some((index, c)) = chars.next() {
                match c {
                    '\\' => {
                        chars.next();
                    }
                    '"' => return Some(at + start + index + 1),
                    _ => {}
                }
            }
            return Some(code.len());
        }
        // a character, not a lifetime
        let mut chars = rest.strip_prefix('\'')?.chars();
        match chars.next()? {
            '\\' => after("'", 3).map(|end| at + end + 1),
            c => (chars.next() == Some('\'')).then(|| at + 2 + c.len_utf8()),
        }
    }

    /// `code` without its comments, and without what is built only for
    /// tests: each item, field or statement marked `#[cfg(test)]`.
    fn without_tests(code: &str) -> String {
        const MARK: &str = "#[cfg(test)]";
        let mut kept = String::new();
        let (mut at, mut from, mut depth, mut skipping) = (0, 0, 0usize, false);
        while at < code.len() {
            if let Some(end) = literal_end(code, at) {
                let comment = code[at..].starts_with("//") || code[at..].starts_with("/*");
                if comment && !skipping {
                    kept.push_str(&code[from..at]);
                    from = end;
                }
                at = end;
                continue;
            }
            if !skipping && code[at..].starts_with(MARK) {
                kept.push_str(&code[from..at]);
                (skipping, depth) = (true, 0);
                at += MARK.len();
                continue;
            }
            let c = code[at..].chars().next().unwrap();
            at += c.len_utf8();
            if !skipping {
                continue;
            }
            match c {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                _ => {}
            }
            // an item ends with its block, a field with its comma, and
            // anything else with its semicolon
            if depth == 0 && matches!(c, '}' | ';' | ',') {
                (skipping, from) = (false, at);
            }
        }
        if !skipping {
            kept.push_str(&code[from..]);
        }
        kept
    }

    /// What `fl!` and `later!` ask for in `code`, as `id` or
    /// `id.attribute`.
    fn asked_for(code: &str) -> Vec<String> {
        let literal = |rest: &str| -> Option<String> {
            let rest = rest.trim_start().strip_prefix('"')?;
            Some(rest[..rest.find('"')?].to_owned())
        };
        let mut asked = Vec::new();
        // worded at once, or kept and worded as shown
        let calls = code.split("fl!(").skip(1).chain(code.split("later!(").skip(1));
        for call in calls {
            let Some(id) = literal(call) else { continue };
            let rest = call.trim_start()[id.len() + 2..].trim_start();
            match rest.strip_prefix(',').and_then(literal) {
                Some(attribute) => asked.push(format!("{id}.{attribute}")),
                None => asked.push(id),
            }
        }
        asked
    }

    #[test]
    fn tells_test_code_and_what_is_asked_for() {
        let code = r##"
            fn shown() -> String { fl!("shown") + &fl!("also", "hint") }
            // fl!("commented")
            #[cfg(test)]
            fn a_test() { let x = "}"; let y = '{'; let z = r#"{"#; fl!("tested"); }
            struct Fields {
                #[cfg(test)]
                field: fn(*const u8) -> Vec<u8>,
                kept: &'static str,
            }
            #[cfg(test)]
            mod more;
            const AFTER: &str = fl!("after", count = 2);
            fn later() -> Text { later!("later", "hint", count = 2_usize) }
            #[cfg(test)]
            mod tests { #[test] fn it() { fl!("in-tests"); } }
        "##;
        let code = without_tests(code);
        assert!(code.contains("kept: &'static str") && !code.contains("field"), "{code}");
        assert_eq!(asked_for(&code), ["shown", "also.hint", "after", "later.hint"]);
    }

    /// Every message English has, and each of its attributes, is shown
    /// somewhere: asked for by `fl!` or `later!` in the GUI's code, other
    /// than its tests', or put in another message. `fl!` itself fails to build for
    /// a message or attribute English doesn't have.
    #[test]
    fn every_english_message_is_used() {
        let mut used = BTreeSet::new();
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        // modules built only for tests, as lib.rs names them
        let lib = std::fs::read_to_string(source.join("lib.rs")).unwrap();
        let test_modules: Vec<String> = lib
            .split("#[cfg(test)]")
            .skip(1)
            .filter_map(|rest| {
                let rest = rest.trim_start();
                let rest = rest.strip_prefix("pub(crate) ").unwrap_or(rest);
                let name = rest.strip_prefix("mod ")?.split(';').next()?;
                Some(format!("{name}.rs"))
            })
            .collect();
        assert!(test_modules.contains(&"testing.rs".to_owned()), "{test_modules:?}");
        let mut stack = vec![source];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                if path.is_dir() {
                    stack.push(path);
                } else if name.ends_with(".rs") && !test_modules.contains(&name) {
                    let code = std::fs::read_to_string(&path).unwrap();
                    used.extend(asked_for(&without_tests(&code)));
                }
            }
        }
        let resource = english();
        for (_, pattern) in patterns(&resource) {
            let mut found = Parts::default();
            parts(pattern, &mut found);
            used.extend(found.references);
        }
        let unused: Vec<String> = patterns(&resource)
            .into_iter()
            .map(|(id, _)| id)
            // a term's attributes are for its own selectors
            .filter(|id| !(id.starts_with('-') && id.contains('.')))
            .filter(|id| !used.contains(id) && id != "language-name")
            .collect();
        assert!(unused.is_empty(), "never shown: {unused:?}");
    }

    /// The built-in fonts draw every character of every language the GUI
    /// has: the system's are only for names of files. A line's end, and
    /// the thin and narrow no-break spaces, as French puts before its
    /// colons, egui spaces out itself; any other character no font has
    /// would show as a box. To draw more, see `assets/fonts/subset.py`.
    #[test]
    fn the_fonts_draw_every_language() {
        let drawn = crate::fonts::drawn();
        let mut lacking = Vec::new();
        for (tag, text) in files() {
            let mut found = Parts::default();
            for (_, pattern) in patterns(&parse(&tag, &text)) {
                parts(pattern, &mut found);
            }
            let spaced = ['\n', '\u{2009}', '\u{202F}'];
            let missing: BTreeSet<char> =
                found.text.chars().filter(|c| !spaced.contains(c) && !drawn.contains(c)).collect();
            if !missing.is_empty() {
                let missing: Vec<String> =
                    missing.iter().map(|c| format!("{c:?} U+{:04X}", u32::from(*c))).collect();
                lacking.push(format!("{tag}: {}", missing.join(", ")));
            }
        }
        assert!(lacking.is_empty(), "characters no font draws:\n{}", lacking.join("\n"));
    }

    /// What `pick` shows for `asked`, the system's locales as it writes
    /// them, of the languages there are translations for, as tags.
    fn picked(asked: &[&str], available: &[&str]) -> String {
        let asked: Vec<_> = asked.iter().filter_map(|tag| asked_language(tag)).collect();
        let available: Vec<LanguageIdentifier> =
            available.iter().map(|tag| tag.parse().unwrap()).collect();
        let fallback = "en-GB".parse().unwrap();
        assert!(available.contains(&fallback));
        pick(&asked, &available, &fallback).to_string()
    }

    /// The languages there are files for, and those being added, whether
    /// or not their files are here yet.
    const AVAILABLE: [&str; 11] =
        ["de", "en-GB", "en-US", "es", "fr", "id", "pt-BR", "ru", "tr", "uk", "uz"];

    #[test]
    fn picks_the_language_asked_for_and_the_english_its_region_spells() {
        let cases: &[(&[&str], &str)] = &[
            (&["en-US"], "en-US"),
            (&["en-PR"], "en-US"),
            (&["en-PH"], "en-US"),
            (&["en-GB"], "en-GB"),
            (&["en-CA"], "en-GB"),
            (&["en-AU"], "en-GB"),
            (&["en-IN"], "en-GB"),
            (&["en"], "en-GB"),
            (&["pt"], "pt-BR"),
            (&["pt-PT"], "pt-BR"),
            (&["pt-BR"], "pt-BR"),
            (&["es-MX"], "es"),
            (&["es-419"], "es"),
            (&["es-ES"], "es"),
            (&["de-AT"], "de"),
            (&["de-CH"], "de"),
            (&["fr-CA"], "fr"),
            (&["fr-BE"], "fr"),
            (&["id-ID"], "id"),
            (&["in"], "id"),
            (&["in-ID"], "id"),
            (&["uz"], "uz"),
            (&["uz-UZ"], "uz"),
            (&["uz-Latn-UZ"], "uz"),
            (&["tr-TR"], "tr"),
            (&["tr-CY"], "tr"),
            (&["uk-UA"], "uk"),
            (&["ru"], "ru"),
            (&["ru-RU"], "ru"),
            (&["ru-UA"], "ru"),
            (&["ru-KZ"], "ru"),
        ];
        for (asked, expected) in cases {
            assert_eq!(picked(asked, &AVAILABLE), *expected, "{asked:?}");
        }
    }

    #[test]
    fn picks_the_first_language_asked_for_that_there_is_and_else_british_english() {
        let cases: &[(&[&str], &str)] = &[
            (&["ja-JP", "ru-RU", "en-US"], "ru"),
            (&["ja-JP", "en-US", "ru-RU"], "en-US"),
            (&["uk-UA", "ru-RU"], "uk"),
            // languages near another aren't it
            (&["be-BY"], "en-GB"),
            (&["kk-KZ"], "en-GB"),
            (&["be-BY", "kk-KZ", "ru-RU"], "ru"),
            (&["gl-ES", "es-ES"], "es"),
            // Uzbek's file is in Latin letters
            (&["uz-Cyrl"], "en-GB"),
            (&["uz-Cyrl-UZ", "ru-RU"], "ru"),
            (&["uz-Arab-AF", "uz-AF", "tr-TR"], "tr"),
            (&["ja-JP"], "en-GB"),
            (&["zz"], "en-GB"),
            (&[""], "en-GB"),
            (&[], "en-GB"),
        ];
        for (asked, expected) in cases {
            assert_eq!(picked(asked, &AVAILABLE), *expected, "{asked:?}");
        }
        // without the languages being added, as before their files are
        let here = ["en-GB", "en-US", "ru", "uk"];
        assert_eq!(picked(&["pt-BR", "uk-UA"], &here), "uk");
        assert_eq!(picked(&["de-DE", "en-US"], &here), "en-US");
        assert_eq!(picked(&["es-MX"], &here), "en-GB");
    }

    /// Tags as POSIX writes them, as `LANG` and `LANGUAGE` hold them, and
    /// as a system may give them.
    #[test]
    fn reads_the_locales_posix_writes() {
        let cases: &[(&[&str], &str)] = &[
            (&["pt_BR.UTF-8"], "pt-BR"),
            (&["pt_PT.UTF-8@euro"], "pt-BR"),
            (&["de_AT.ISO-8859-1"], "de"),
            (&["en_US.UTF-8"], "en-US"),
            (&["en_GB"], "en-GB"),
            (&["in_ID.UTF-8"], "id"),
            (&["tr_TR.UTF-8"], "tr"),
            (&["uz_UZ.UTF-8"], "uz"),
            (&["uz_UZ.UTF-8@cyrillic", "ru_RU.UTF-8"], "ru"),
            (&["sr_RS@latin", "uk_UA.UTF-8"], "uk"),
            // the C locale's messages are untranslated: English
            (&["C"], "en-GB"),
            (&["C.UTF-8", "ru_RU.UTF-8"], "en-GB"),
            (&["POSIX", "ru_RU.UTF-8"], "en-GB"),
        ];
        for (asked, expected) in cases {
            assert_eq!(picked(asked, &AVAILABLE), *expected, "{asked:?}");
        }
        let asked = |tag: &str| asked_language(tag).map(|tag| tag.to_string());
        assert_eq!(asked("uz_UZ.UTF-8@cyrillic").as_deref(), Some("uz-Cyrl-UZ"));
        assert_eq!(asked("fr_FR@euro").as_deref(), Some("fr-FR"));
        assert_eq!(asked("in").as_deref(), Some("id"));
        assert_eq!(asked("und"), None);
        assert_eq!(asked("not a locale"), None);
    }

    /// The environment's locales in the order `sys_locale` reads them, but
    /// with their modifiers.
    #[cfg(all(unix, not(any(target_vendor = "apple", target_os = "android"))))]
    #[test]
    fn lists_the_environments_locales_in_order() {
        let environment = |pairs: &'static [(&str, &str)]| {
            move |name: &str| {
                pairs.iter().find(|(set, _)| *set == name).map(|(_, value)| (*value).to_owned())
            }
        };
        let all = environment(&[
            ("LANGUAGE", "uz_UZ@cyrillic:pt_BR:"),
            ("LC_ALL", ""),
            ("LC_MESSAGES", "de_DE.UTF-8"),
            ("LANG", "en_US.UTF-8"),
        ]);
        assert_eq!(posix_locales(all), ["uz_UZ@cyrillic", "pt_BR", "de_DE.UTF-8", "en_US.UTF-8"]);
        assert_eq!(posix_locales(environment(&[("LANG", "C.UTF-8")])), ["C.UTF-8"]);
        assert!(posix_locales(environment(&[])).is_empty());
    }

    /// American English says what differs, and British English the rest:
    /// a value where American gives only an attribute, and the reverse.
    #[test]
    fn american_english_falls_back_on_british_for_the_rest() {
        let american: LanguageIdentifier = "en-US".parse().unwrap();
        let loader = FluentLanguageLoader::new(LOADER.domain(), LOADER.fallback_language().clone());
        loader.load_languages(&Files, &[american]).unwrap();
        assert_eq!(loader.get("accent"), "Accent color");
        assert_eq!(loader.get("accent-grey"), "Gray");
        assert_eq!(loader.get("fitting"), "Getting under the limit");
        assert!(loader.get_attr("fitting", "lossless").contains("unchecks"));
        assert_eq!(loader.get("accent-system"), "As the system");
        assert!(loader.get_attr("accent-system", "hint").contains("accent color"));
        assert_eq!(
            loader.get_attr("accent", "hint"),
            "What marks the chosen, the hovered, and the focused"
        );
        assert_eq!(loader.get("close"), "Close");
    }

    #[test]
    fn names_each_language_as_it_calls_itself() {
        let names: Vec<&str> = languages().iter().map(|(_, name)| name.as_str()).collect();
        for name in ["English (UK)", "English (US)", "Русский", "Українська"] {
            assert!(names.contains(&name), "{names:?}");
        }
        // in order, and none named alike
        let mut sorted = names.clone();
        sorted.sort_by_key(|name| name.to_lowercase());
        sorted.dedup();
        assert_eq!(names, sorted);
        assert_eq!(name_of(&"en-US".parse().unwrap()), Some("English (US)"));
    }

    #[test]
    fn notes_the_messages_shown() {
        record::start();
        let close = fl!("close");
        let attribute = fl!("custom", "hint");
        assert_eq!(record::take(), [("close", close), ("custom.hint", attribute)]);
    }

    #[test]
    fn pseudo_text_is_longer_and_keeps_what_is_not_a_letter() {
        assert_eq!(pseudo_text("Crop: 4 px"), "Çřøøþ: 4 þx");
        assert!(pseudo_text("Settings").chars().count() > "Settings".chars().count());
        // the built-in fonts have every letter it makes
        let drawn = crate::fonts::drawn();
        let accented = pseudo_text("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ");
        assert!(accented.chars().all(|c| drawn.contains(&c)), "{accented}");
    }
}
