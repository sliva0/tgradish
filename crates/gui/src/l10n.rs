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
pub(crate) static LOADER: LazyLock<FluentLanguageLoader> = LazyLock::new(|| {
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

/// The languages there are translations for, English first, each with
/// what it calls itself.
pub(crate) fn languages() -> &'static [(LanguageIdentifier, String)] {
    static LANGUAGES: LazyLock<Vec<(LanguageIdentifier, String)>> = LazyLock::new(|| {
        let languages = LOADER.available_languages(&Files).unwrap_or_default();
        languages
            .into_iter()
            .map(|language| {
                // its own file alone, as the window's loads only those shown
                let loader = FluentLanguageLoader::new(LOADER.domain(), language.clone());
                let name = match loader.load_languages(&Files, std::slice::from_ref(&language)) {
                    Ok(()) => loader.get("language-name"),
                    Err(_) => language.to_string(),
                };
                (language, name)
            })
            .collect()
    });
    &LANGUAGES
}

/// Shows the window in `language`, as a tag, or in the system's when
/// `None`, as far as there are translations. `TGRADISH_LANG` comes first.
pub(crate) fn choose(language: Option<&str>) {
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
    /// those the window keeps and shows later, as notices and errors.
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

    /// What `pattern` holds: its text, the arguments it takes, and the
    /// messages and terms it puts in, as `id` or `id.attribute`.
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
    /// somewhere: asked for by `fl!` in the window's code, other than its
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

    /// What a translation puts in from another message, it has itself:
    /// English stands in for whole messages a translation lacks, not for
    /// those another puts in, which would show as `{save}`.
    #[test]
    fn translations_put_in_only_what_they_have() {
        for (tag, text) in files() {
            let resource = parse(&tag, &text);
            let had: BTreeSet<String> = patterns(&resource)
                .into_iter()
                .map(|(id, _)| id.split('.').next().unwrap_or_default().to_owned())
                .collect();
            for (id, pattern) in patterns(&resource) {
                let mut found = Parts::default();
                parts(pattern, &mut found);
                let messages = found.references.iter().map(|id| id.split('.').next().unwrap());
                let lacking: Vec<_> = messages.filter(|id| !had.contains(*id)).collect();
                assert!(lacking.is_empty(), "{tag}'s {id} puts in {lacking:?}, which it lacks");
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
        let close = fl!("close");
        let attribute = fl!("custom", "hint");
        assert_eq!(record::take(), [("close", close), ("custom.hint", attribute)]);
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
