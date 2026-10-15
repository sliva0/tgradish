//! The window's words in its user's language: a Fluent file for each
//! language in `i18n`, built into the binary. English is complete, and what
//! another language lacks is shown in English.
//!
//! `fl!("message-id")`, `fl!("message-id", "attribute")` and either with
//! `name = value` arguments give a message, checked at compile time
//! against the English file: the message, the attribute and the names of
//! the arguments must all be there.

use std::borrow::Cow;
use std::sync::LazyLock;

use i18n_embed::LanguageLoader;
use i18n_embed::fluent::{FluentLanguageLoader, fluent_language_loader};
use i18n_embed::unic_langid::LanguageIdentifier;

#[derive(rust_embed::RustEmbed)]
#[folder = "i18n"]
struct Files;

/// The messages, in English until `choose` picks another language.
pub static LOADER: LazyLock<FluentLanguageLoader> = LazyLock::new(|| {
    let loader = fluent_language_loader!();
    loader.load_fallback_language(&Files).expect("English is built in");
    settle(&loader, false);
    loader
});

/// A message in the window's language, see the module's docs.
macro_rules! fl {
    ($id:literal) => {
        $crate::l10n::seen($id, i18n_embed_fl::fl!($crate::l10n::LOADER, $id))
    };
    ($id:literal, $attribute:literal $(, $($args:tt)*)?) => {
        $crate::l10n::seen(
            concat!($id, ".", $attribute),
            i18n_embed_fl::fl!($crate::l10n::LOADER, $id, $attribute $(, $($args)*)?),
        )
    };
    ($id:literal, $($args:tt)*) => {
        $crate::l10n::seen($id, i18n_embed_fl::fl!($crate::l10n::LOADER, $id, $($args)*))
    };
}
pub(crate) use fl;

/// The language asked for by `TGRADISH_LANG` instead of the config's and
/// the system's: a tag such as `de`, or `pseudo` for English with its
/// letters accented and its vowels doubled, to see where longer words
/// would not fit.
const VARIABLE: &str = "TGRADISH_LANG";

/// The languages there are translations for, English first.
pub fn languages() -> Vec<LanguageIdentifier> {
    LOADER.available_languages(&Files).unwrap_or_default()
}

/// What `language` calls itself.
pub fn name(language: &LanguageIdentifier) -> String {
    LOADER.select_languages(&[language]).get("language-name")
}

/// Shows the window in `language`, as a tag, or in the system's when
/// `None`, as far as there are translations. `TGRADISH_LANG` comes first.
pub fn choose(language: Option<&str>) {
    let variable = std::env::var(VARIABLE).ok().filter(|tag| !tag.is_empty());
    let pseudo = variable.as_deref() == Some("pseudo");
    let asked: Vec<LanguageIdentifier> = match variable.as_deref().filter(|_| !pseudo).or(language)
    {
        Some(tag) => tag.parse().into_iter().collect(),
        None => sys_locale::get_locales().filter_map(|tag| tag.parse().ok()).collect(),
    };
    if pseudo || asked.is_empty() {
        let english = LOADER.fallback_language().clone();
        let _ = LOADER.load_languages(&Files, &[english]);
    } else if i18n_embed::select(&*LOADER, &Files, &asked).is_err() {
        let _ = LOADER.load_fallback_language(&Files);
    }
    settle(&LOADER, pseudo);
}

/// Sets up the messages just loaded: without the marks Fluent puts
/// around arguments for text running both ways, which the window's fonts
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

/// `text`, noted as shown for `id` while `seen` records, for the pictures
/// of where each message is shown.
#[cfg(not(test))]
#[inline(always)]
pub fn seen(_id: &'static str, text: String) -> String {
    text
}

#[cfg(test)]
pub use record::seen;

#[cfg(test)]
pub mod record {
    use std::cell::RefCell;

    thread_local! {
        static SEEN: RefCell<Option<Vec<(&'static str, String)>>> = const { RefCell::new(None) };
    }

    /// `text`, noted for `id` if recording.
    pub fn seen(id: &'static str, text: String) -> String {
        SEEN.with_borrow_mut(|seen| {
            if let Some(seen) = seen {
                seen.push((id, text.clone()));
            }
        });
        text
    }

    /// Starts noting the messages shown on this thread, forgetting those
    /// noted before.
    pub fn start() {
        SEEN.set(Some(Vec::new()));
    }

    /// The messages shown since `start` or the last `take`.
    pub fn take() -> Vec<(&'static str, String)> {
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

    /// What `pattern` holds: its text, the arguments it takes, and the
    /// messages and terms it puts in.
    #[derive(Default)]
    struct Parts {
        text: String,
        arguments: BTreeSet<String>,
        references: BTreeSet<String>,
    }

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
            ast::InlineExpression::MessageReference { id, .. } => {
                into.references.insert(id.name.clone());
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
                for variant in variants {
                    parts(&variant.value, into);
                }
            }
        }
    }

    fn english() -> ast::Resource<String> {
        parse("en-GB", &files()["en-GB"])
    }

    #[test]
    fn every_language_reads_as_fluent_without_clashes() {
        for (tag, text) in files() {
            let resource = fluent_bundle::FluentResource::try_new(text)
                .unwrap_or_else(|(_, errors)| panic!("{tag} doesn't read as Fluent: {errors:?}"));
            // a term and a message of one name hide each other
            let mut bundle = fluent_bundle::FluentBundle::new(vec![tag.parse().unwrap()]);
            if let Err(errors) = bundle.add_resource(resource) {
                panic!("{tag} names something twice: {errors:?}");
            }
        }
    }

    #[test]
    fn english_says_everything_without_errors() {
        let text = files()["en-GB"].clone();
        let resource = english();
        let mut bundle = fluent_bundle::FluentBundle::new(vec!["en-GB".parse().unwrap()]);
        bundle.set_use_isolating(false);
        bundle.add_resource(fluent_bundle::FluentResource::try_new(text).unwrap()).unwrap();
        for (id, pattern) in patterns(&resource) {
            if id.starts_with('-') {
                continue;
            }
            let mut found = Parts::default();
            parts(pattern, &mut found);
            // a count, which also fills in for words
            let mut args = fluent_bundle::FluentArgs::new();
            for argument in &found.arguments {
                args.set(argument.as_str(), 2);
            }
            let (message, attribute) = id
                .split_once('.')
                .map_or((id.as_str(), None), |(message, attribute)| (message, Some(attribute)));
            let message = bundle.get_message(message).unwrap_or_else(|| panic!("{id} is hidden"));
            let pattern = match attribute {
                Some(attribute) => message.get_attribute(attribute).map(|a| a.value()),
                None => message.value(),
            };
            let mut errors = Vec::new();
            let said = bundle.format_pattern(pattern.unwrap(), Some(&args), &mut errors);
            assert!(errors.is_empty(), "{id}: {errors:?}");
            assert!(!said.trim().is_empty(), "{id} says nothing");
        }
    }

    /// Every message English has is shown somewhere: named by `fl!` in
    /// the window's code, or put in another message.
    #[test]
    fn every_english_message_is_used() {
        let mut used = BTreeSet::new();
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut stack = vec![source];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    let code = std::fs::read_to_string(&path).unwrap();
                    for call in code.split("fl!(").skip(1) {
                        let named = call.trim_start().strip_prefix('"');
                        used.extend(
                            named.and_then(|rest| rest.split('"').next()).map(str::to_owned),
                        );
                    }
                }
            }
        }
        let resource = english();
        for (_, pattern) in patterns(&resource) {
            let mut found = Parts::default();
            parts(pattern, &mut found);
            used.extend(found.references);
        }
        let unused: Vec<String> = resource
            .body
            .iter()
            .filter_map(|entry| match entry {
                ast::Entry::Message(message) => Some(message.id.name.clone()),
                ast::Entry::Term(term) => Some(format!("-{}", term.id.name)),
                _ => None,
            })
            .filter(|id| !used.contains(id) && id != "language-name")
            .collect();
        assert!(unused.is_empty(), "never shown: {unused:?}");
    }

    /// Other languages say only what English does, with its arguments.
    #[test]
    fn translations_say_what_english_does() {
        let arguments = |resource: &ast::Resource<String>| -> BTreeMap<String, BTreeSet<String>> {
            patterns(resource)
                .into_iter()
                .map(|(id, pattern)| {
                    let mut found = Parts::default();
                    parts(pattern, &mut found);
                    (id, found.arguments)
                })
                .collect()
        };
        let english = arguments(&english());
        for (tag, text) in files().into_iter().filter(|(tag, _)| tag != "en-GB") {
            let translated = arguments(&parse(&tag, &text));
            for (id, taken) in translated {
                // terms are a language's own
                if id.starts_with('-') {
                    continue;
                }
                let Some(known) = english.get(&id) else {
                    panic!("{tag} has {id}, which English doesn't");
                };
                let unknown: Vec<_> = taken.difference(known).collect();
                assert!(
                    unknown.is_empty(),
                    "{tag}'s {id} takes {unknown:?}, which English doesn't"
                );
            }
        }
    }

    /// The window's own fonts draw every language it has: the system's
    /// are only for names of files.
    #[test]
    fn the_fonts_draw_every_language() {
        let fonts = crate::fonts::fonts();
        let drawn: BTreeSet<char> = fonts
            .font_data
            .values()
            .flat_map(|data| crate::fallback::characters(&data.font, data.index))
            .collect();
        for (tag, text) in files() {
            let mut found = Parts::default();
            for (_, pattern) in patterns(&parse(&tag, &text)) {
                parts(pattern, &mut found);
            }
            let lacking: BTreeSet<char> =
                found.text.chars().filter(|c| !c.is_whitespace() && !drawn.contains(c)).collect();
            assert!(lacking.is_empty(), "{tag} has characters no font draws: {lacking:?}");
        }
    }

    #[test]
    fn notes_the_messages_shown() {
        record::start();
        let ok = fl!("ok");
        let attribute = fl!("custom", "hint");
        assert_eq!(record::take(), [("ok", ok), ("custom.hint", attribute)]);
    }

    #[test]
    fn pseudo_text_is_longer_and_keeps_what_is_not_a_letter() {
        assert_eq!(pseudo_text("Crop: 4 px"), "Çřøøþ: 4 þx");
        assert!(pseudo_text("Settings").chars().count() > "Settings".chars().count());
        // the window's fonts have every letter it makes
        let drawn: BTreeSet<char> = crate::fonts::fonts()
            .font_data
            .values()
            .flat_map(|data| crate::fallback::characters(&data.font, data.index))
            .collect();
        let accented = pseudo_text("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ");
        assert!(accented.chars().all(|c| drawn.contains(&c)), "{accented}");
    }
}
