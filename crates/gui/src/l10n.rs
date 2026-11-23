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

/// The language asked for by `TGRADISH_LANG` instead of the config's and
/// the system's: a tag such as `de`, or `pseudo` for English with its
/// letters accented and its vowels doubled, to see where longer words
/// would not fit.
const VARIABLE: &str = "TGRADISH_LANG";

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

/// What `TGRADISH_LANG` asks for, if it is set.
pub(crate) fn variable() -> Option<String> {
    std::env::var(VARIABLE).ok().filter(|tag| !tag.is_empty())
}

/// The language the system's settings pick of those there are
/// translations for, if they pick one.
pub(crate) fn system() -> Option<LanguageIdentifier> {
    let asked: Vec<LanguageIdentifier> =
        sys_locale::get_locales().filter_map(|tag| tag.parse().ok()).collect();
    let available: Vec<_> = languages().iter().map(|(language, _)| language.clone()).collect();
    pick(&asked, &available, LOADER.fallback_language())
}

/// Regions where English is spelt as in the United States: the country
/// itself, its territories, and the Philippines. Elsewhere, as in Canada,
/// Australia or India, British spelling is nearer the custom.
const AMERICAN: [&str; 8] = ["US", "AS", "GU", "MP", "PR", "UM", "VI", "PH"];

/// Which of `available` to show for `asked`, the languages asked for in
/// the order they are preferred: the first asked for that there is a
/// translation into. Of a language's several, as English's, the one of
/// the region asked for, else for English the British or American as the
/// region spells it, else the one `fallback` is, else the first. A script
/// asked for, as Cyrillic Uzbek's, rules out another script's.
fn pick(
    asked: &[LanguageIdentifier],
    available: &[LanguageIdentifier],
    fallback: &LanguageIdentifier,
) -> Option<LanguageIdentifier> {
    for asked in asked {
        let of_language: Vec<&LanguageIdentifier> = available
            .iter()
            .filter(|had| had.language == asked.language)
            .filter(|had| {
                asked.script.is_none() || had.script.is_none_or(|s| asked.script == Some(s))
            })
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
            return Some((*found).clone());
        }
    }
    None
}

/// Shows the GUI in `language`, as a tag, or in the system's when
/// `None`, as far as there are translations, else in English.
/// `TGRADISH_LANG` comes first.
pub(crate) fn choose(language: Option<&str>) {
    let variable = variable();
    let pseudo = variable.as_deref() == Some("pseudo");
    let available: Vec<_> = languages().iter().map(|(language, _)| language.clone()).collect();
    let fallback = LOADER.fallback_language().clone();
    let picked = match variable.as_deref().filter(|_| !pseudo).or(language) {
        _ if pseudo => None,
        Some(tag) => tag.parse().ok().and_then(|tag| pick(&[tag], &available, &fallback)),
        None => system(),
    };
    // English stands in for what a translation lacks
    let language = picked.unwrap_or(fallback);
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
    /// messages and terms it puts in, as `id` or `id.attribute`, and the
    /// plural forms each choice by a count has, as `one` and `other`.
    #[derive(Default)]
    struct Parts {
        text: String,
        arguments: BTreeSet<String>,
        references: BTreeSet<String>,
        plurals: Vec<BTreeSet<String>>,
    }

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
            close = again\n";
        let said = problems("uk", text, &english);
        let has = |part: &str| said.iter().any(|problem| problem.contains(part));
        for part in [
            "line 8:",
            "close takes { $who }, which English's doesn't: it takes none",
            "close puts in { -app }, a term not given here",
            "close puts in { never }, which English hasn't",
            "close puts in { save }, which isn't given here",
            "count-frames counts with [one] [other] but not [few] [many], a plural form of uk",
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

    /// What `fl!` asks for in `code`, as `id` or `id.attribute`.
    fn asked_for(code: &str) -> Vec<String> {
        let literal = |rest: &str| -> Option<String> {
            let rest = rest.trim_start().strip_prefix('"')?;
            Some(rest[..rest.find('"')?].to_owned())
        };
        let mut asked = Vec::new();
        for call in code.split("fl!(").skip(1) {
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
            #[cfg(test)]
            mod tests { #[test] fn it() { fl!("in-tests"); } }
        "##;
        let code = without_tests(code);
        assert!(code.contains("kept: &'static str") && !code.contains("field"), "{code}");
        assert_eq!(asked_for(&code), ["shown", "also.hint", "after"]);
    }

    /// Every message English has, and each of its attributes, is shown
    /// somewhere: asked for by `fl!` in the GUI's code, other than its
    /// tests', or put in another message. `fl!` itself fails to build for
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

    #[test]
    fn picks_the_language_asked_for_and_the_english_its_region_spells() {
        let tags = |tags: &[&str]| -> Vec<LanguageIdentifier> {
            tags.iter().map(|tag| tag.parse().unwrap()).collect()
        };
        let available = tags(&["en-GB", "en-US", "pt-BR", "ru", "uk", "uz"]);
        let fallback = &available[0];
        let picked = |asked: &[&str]| pick(&tags(asked), &available, fallback);
        let cases: [(&[&str], Option<&str>); 17] = [
            (&["en-US"], Some("en-US")),
            (&["en-PR"], Some("en-US")),
            (&["en-PH"], Some("en-US")),
            (&["en-GB"], Some("en-GB")),
            (&["en-CA"], Some("en-GB")),
            (&["en-AU"], Some("en-GB")),
            (&["en-IN"], Some("en-GB")),
            (&["en"], Some("en-GB")),
            (&["uk-UA"], Some("uk")),
            (&["ru-RU"], Some("ru")),
            (&["pt-PT"], Some("pt-BR")),
            (&["pt"], Some("pt-BR")),
            (&["uz-Latn-UZ"], Some("uz")),
            (&["de-DE", "ru-RU", "en-US"], Some("ru")),
            (&["de-DE", "en-US"], Some("en-US")),
            (&["de-DE"], None),
            (&[], None),
        ];
        for (asked, expected) in cases {
            let expected = expected.map(|tag| tag.parse().unwrap());
            assert_eq!(picked(asked), expected, "{asked:?}");
        }
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
