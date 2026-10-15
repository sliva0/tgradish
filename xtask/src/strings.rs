//! A page of the words tgradish shows, for reviewing them and for
//! translators: each of the window's messages from its English Fluent file,
//! with its note and pictures of where it shows, then the command line's
//! help pages and the messages its sources hold, each with a name to refer
//! to it by. The pictures come from `strings` in `crates/gui/src/strings.rs`,
//! which needs a GPU wgpu can use and ffmpeg, and takes a few minutes, so
//! `--no-pictures` keeps those made last time.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::{cargo, run, workspace_root};

/// The sources whose messages the command line prints, as their crates are
/// called on the page. The arguments' own words are in the help pages.
const SOURCES: [(&str, &str); 3] = [
    ("crates/cli/src", "The command line"),
    ("crates/core/src", "tgradish-core, whose errors and warnings the command line prints"),
    ("crates/tgs/src", "tgradish-tgs, whose checks of .tgs files both print"),
];

/// Widest a picture shows on the page, in its pixels.
const SHOWN: f32 = 460.0;

pub fn strings(args: &[String]) -> Result<()> {
    let pictures = match args {
        [] => true,
        [flag] if flag == "--no-pictures" => false,
        _ => bail!("usage: cargo xtask strings [--no-pictures]"),
    };
    let root = workspace_root();
    let out = root.join("target/strings");
    std::fs::create_dir_all(&out)?;
    let test = |package: &str, target: &[&str], name: &str| {
        run(cargo()
            .current_dir(root)
            .env("TGRADISH_STRINGS", &out)
            .args(["test", "-p", package])
            .args(target)
            .args([name, "--", "--ignored", "--exact"]))
    };
    if pictures {
        test("tgradish-gui", &["--lib"], "strings::strings")?;
    }
    test("tgradish", &["--bin", "tgradish"], "args::tests::help_pages")?;
    let read = |name: &str| -> Result<Value> {
        let path = out.join(name);
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("{}: run without --no-pictures first", path.display()))?;
        Ok(serde_json::from_str(&text)?)
    };
    let shown = read("window.json")?;
    let help = read("cli.json")?;
    let english = std::fs::read_to_string(root.join("crates/gui/i18n/en-GB/tgradish.ftl"))?;
    let sections = fluent(&english);
    let mut sources = Vec::new();
    for (dir, name) in SOURCES {
        let mut files = Vec::new();
        for path in rust_files(&root.join(dir))? {
            let relative = path.strip_prefix(root)?.display().to_string();
            // the arguments' words are their help pages
            if relative.ends_with("cli/src/args.rs") {
                continue;
            }
            let found = literals(&std::fs::read_to_string(&path)?);
            if !found.is_empty() {
                files.push((relative, found));
            }
        }
        sources.push((name, files));
    }
    let page = Page { pictures: &out.join("pictures"), sizes: BTreeMap::new() };
    let html = page.html(&sections, &shown, &help, &sources)?;
    let index = out.join("index.html");
    std::fs::write(&index, html)?;
    println!("{}", index.display());
    Ok(())
}

/// A part of the Fluent file, under its `##` heading.
struct Section {
    title: String,
    messages: Vec<Message>,
}

/// A message or term as written: its comment, its value and attributes,
/// each as in the file, without the indentation of the lines after the
/// first.
struct Message {
    id: String,
    comment: String,
    value: String,
    attributes: Vec<(String, String)>,
}

/// The sections of a Fluent file, read line by line as tgradish writes
/// them: a comment right above a message is its note.
fn fluent(text: &str) -> Vec<Section> {
    let mut sections = vec![Section { title: String::new(), messages: Vec::new() }];
    let mut comment = Vec::new();
    let mut open = false;
    for line in text.lines() {
        let section = sections.last_mut().expect("one at least");
        if let Some(title) = line.strip_prefix("## ") {
            sections.push(Section { title: title.to_owned(), messages: Vec::new() });
            (open, comment) = (false, Vec::new());
        } else if line.starts_with("###") {
            open = false;
        } else if let Some(note) = line.strip_prefix('#') {
            comment.push(note.trim().to_owned());
            open = false;
        } else if line.trim().is_empty() {
            (open, comment) = (false, Vec::new());
        } else if !line.starts_with(' ') && !(open && line.starts_with('}')) {
            let (id, value) = line.split_once('=').unwrap_or((line, ""));
            section.messages.push(Message {
                id: id.trim().to_owned(),
                comment: comment.join(" "),
                value: value.trim().to_owned(),
                attributes: Vec::new(),
            });
            (open, comment) = (true, Vec::new());
        } else if open && let Some(message) = section.messages.last_mut() {
            let trimmed = line.trim_start();
            let attribute = trimmed.strip_prefix('.').and_then(|rest| rest.split_once('='));
            match attribute {
                Some((name, value)) if !name.trim().contains(' ') => {
                    message.attributes.push((name.trim().to_owned(), value.trim().to_owned()));
                }
                _ => {
                    let continued = line.strip_prefix("    ").unwrap_or(line);
                    let text = match message.attributes.last_mut() {
                        Some((_, value)) => value,
                        None => &mut message.value,
                    };
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(continued);
                }
            }
        }
    }
    sections.retain(|section| !section.messages.is_empty());
    sections
}

/// The `.rs` files under `dir`, in order.
fn rust_files(dir: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    let mut dirs = vec![dir.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

/// The string literals of Rust `code` that read as words, with their
/// lines, up to its tests: two words or more, of which one is lower case,
/// and not a flag, path or format alone.
fn literals(code: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    let mut line = 1;
    let mut chars = code.chars().peekable();
    let mut at_line_start = true;
    // the code just before, without spaces, to tell what a literal is for
    let mut recent = String::new();
    while let Some(c) = chars.next() {
        if !c.is_whitespace() && c != '"' {
            recent.push(c);
            if recent.len() > 64 {
                recent.drain(..recent.len() - 32);
            }
        }
        // the tests at the end of a file
        if at_line_start && c == '#' && code_rest_starts(&chars, "[cfg(test)]") {
            break;
        }
        at_line_start = c == '\n';
        match c {
            '\n' => line += 1,
            '/' if chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        line += 1;
                        at_line_start = true;
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                let mut last = ' ';
                for c in chars.by_ref() {
                    if c == '\n' {
                        line += 1;
                    }
                    if last == '*' && c == '/' {
                        break;
                    }
                    last = c;
                }
            }
            '\'' => {
                // a character, not a lifetime
                let mut ahead = chars.clone();
                let first = ahead.next();
                if first == Some('\\') || ahead.next() == Some('\'') {
                    let mut escaped = false;
                    for c in chars.by_ref() {
                        if c == '\'' && !escaped {
                            break;
                        }
                        escaped = c == '\\' && !escaped;
                    }
                }
            }
            'r' if matches!(chars.peek(), Some('"' | '#')) => {
                let mut hashes = 0;
                while chars.peek() == Some(&'#') {
                    chars.next();
                    hashes += 1;
                }
                if chars.next() != Some('"') {
                    continue;
                }
                let start = line;
                let mut text = String::new();
                while let Some(c) = chars.next() {
                    if c == '"' {
                        let mut closing = 0;
                        while closing < hashes && chars.peek() == Some(&'#') {
                            chars.next();
                            closing += 1;
                        }
                        if closing == hashes {
                            break;
                        }
                        text.push('"');
                        text.extend(std::iter::repeat_n('#', closing));
                        continue;
                    }
                    if c == '\n' {
                        line += 1;
                    }
                    text.push(c);
                }
                if !unseen(&recent) {
                    found.push((start, text));
                }
            }
            '"' => {
                let start = line;
                let mut text = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\n' => {
                            line += 1;
                            text.push('\n');
                        }
                        '\\' => match chars.next() {
                            Some('n') => text.push('\n'),
                            Some('t') => text.push('\t'),
                            // a line continued: its next line's indentation goes
                            Some('\n') => {
                                line += 1;
                                while chars.peek().is_some_and(|c| c.is_whitespace()) {
                                    if chars.next() == Some('\n') {
                                        line += 1;
                                    }
                                }
                            }
                            Some('u') => {
                                let code: String =
                                    chars.by_ref().skip(1).take_while(|&c| c != '}').collect();
                                text.extend(
                                    u32::from_str_radix(&code, 16).ok().and_then(char::from_u32),
                                );
                            }
                            Some(other) => text.push(other),
                            None => {}
                        },
                        other => text.push(other),
                    }
                }
                if !unseen(&recent) {
                    found.push((start, text));
                }
            }
            _ => {}
        }
    }
    found.retain(|(_, text)| reads_as_words(text));
    found
}

/// Whether a literal after `code` is one nobody reads: what a panic says
/// of a broken assumption, or a test's.
fn unseen(code: &str) -> bool {
    ["expect(", "panic!(", "unreachable!(", "assert!(", "assert_eq!(", "debug_assert!("]
        .iter()
        .any(|call| code.ends_with(call))
}

/// Whether what follows in `chars` starts with `text`.
fn code_rest_starts(chars: &std::iter::Peekable<std::str::Chars>, text: &str) -> bool {
    chars.clone().take(text.len()).eq(text.chars())
}

/// Whether a literal is words people read, rather than a name, path, flag
/// or format alone.
fn reads_as_words(text: &str) -> bool {
    let trimmed = text.trim();
    let words = trimmed
        .split_whitespace()
        .filter(|word| word.chars().filter(|c| c.is_alphabetic()).count() >= 2)
        .count();
    words >= 2
        && trimmed.chars().any(|c| c.is_lowercase())
        && !trimmed.starts_with(['-', '/', '.'])
        && !trimmed.contains("::")
        && !trimmed.contains("=")
}

/// A source file, and the messages in it with their lines.
type Source = (String, Vec<(usize, String)>);

/// What writing the page needs: the pictures' folder, and their sizes as
/// read.
struct Page<'a> {
    pictures: &'a Path,
    sizes: BTreeMap<String, (f32, f32)>,
}

impl Page<'_> {
    fn html(
        mut self,
        sections: &[Section],
        shown: &Value,
        help: &Value,
        sources: &[(&str, Vec<Source>)],
    ) -> Result<String> {
        let mut html = String::from(HEAD);
        // each value and attribute, by the name the pictures know it by
        let texts: Vec<String> = sections
            .iter()
            .flat_map(|section| &section.messages)
            .flat_map(|message| {
                let value = (!message.value.is_empty()).then(|| message.id.clone());
                let attributes =
                    message.attributes.iter().map(|(name, _)| format!("{}.{name}", message.id));
                value.into_iter().chain(attributes)
            })
            .collect();
        let pictured = texts.iter().filter(|name| shown.get(name.as_str()).is_some()).count();
        writeln!(
            html,
            "<section id=window><p class=about>{} texts, {pictured} of them seen in the \
             pictures; the rest show only in states the pictures don't reach, as errors. \
             Each says where it shows above its words, which are \
             as written in <code>crates/gui/i18n/en-GB/tgradish.ftl</code>: {{ $name }} is \
             filled in, {{ other-message }} puts in another's words. Hovering a picture shows \
             the state of the window it is of; clicking opens all of it.</p>",
            texts.len()
        )?;
        for section in sections {
            writeln!(html, "<h2>{}</h2>", escape(&section.title))?;
            for message in &section.messages {
                writeln!(html, "<div class=message id=\"{}\">", escape(&message.id))?;
                writeln!(html, "<div class=name><code>{}</code></div>", escape(&message.id))?;
                if !message.comment.is_empty() {
                    writeln!(html, "<p class=note>{}</p>", escape(&message.comment))?;
                }
                if !message.value.is_empty() {
                    self.pattern(&mut html, None, &message.value, shown.get(&message.id))?;
                }
                for (name, value) in &message.attributes {
                    let key = format!("{}.{name}", message.id);
                    self.pattern(&mut html, Some(name), value, shown.get(&key))?;
                }
                html.push_str("</div>\n");
            }
        }
        html.push_str("</section>\n<section id=cli hidden>\n");
        html.push_str(
            "<p class=about>The command line stays in English. Each line has a name to refer \
             to it by: the name over its block, then its number, as <code>convert:23</code> \
             for a help page's line, or <code>core/src/error.rs:34</code> for a message in \
             the sources. Hovering a number shows the whole name.</p>\n<h2>Help pages</h2>\n",
        );
        for page in help.as_array().map(Vec::as_slice).unwrap_or_default() {
            let command = page["command"].as_str().unwrap_or_default();
            let short = command.trim_start_matches("tgradish").trim();
            let short = if short.is_empty() { "tgradish" } else { short };
            let prefix = short.replace(' ', "-");
            writeln!(
                html,
                "<h3>{} <code class=prefix>{}:</code></h3><div class=help>",
                escape(command),
                escape(&prefix)
            )?;
            let text = page["text"].as_str().unwrap_or_default();
            for (number, line) in text.lines().enumerate() {
                let name = format!("{prefix}:{}", number + 1);
                writeln!(
                    html,
                    "<div class=line id=\"help-{0}\"><span class=ln title=\"{0}\">{1}</span>\
                     <span class=text>{2}</span></div>",
                    escape(&name),
                    number + 1,
                    escape(line)
                )?;
            }
            html.push_str("</div>\n");
        }
        html.push_str("<h2>Messages in the sources</h2>\n");
        for (name, files) in sources {
            writeln!(html, "<h3>{}</h3>", escape(name))?;
            for (file, found) in files {
                let short = file.trim_start_matches("crates/");
                writeln!(html, "<h4>{}:</h4><div class=help>", escape(short))?;
                for (line, text) in found {
                    let name = format!("{short}:{line}");
                    writeln!(
                        html,
                        "<div class=line id=\"src-{0}\"><span class=ln title=\"{0}\">{1}</span>\
                         <span class=text>{2}</span></div>",
                        escape(&name),
                        line,
                        escape(text)
                    )?;
                }
                html.push_str("</div>\n");
            }
        }
        html.push_str("</section>\n");
        html.push_str(TAIL);
        Ok(html)
    }

    /// A message's value, or its `attribute`, with pictures of where it
    /// shows from `places`.
    fn pattern(
        &mut self,
        html: &mut String,
        attribute: Option<&str>,
        text: &str,
        places: Option<&Value>,
    ) -> Result<()> {
        html.push_str("<div class=pattern>");
        if let Some(name) = attribute {
            write!(html, "<code class=attribute>.{}</code>", escape(name))?;
        }
        write!(html, "<pre>{}</pre>", escape(text))?;
        let places = places.and_then(|shown| shown["places"].as_array());
        match places {
            Some(places) if !places.is_empty() => {
                html.push_str("<div class=places>");
                for place in places {
                    self.place(html, place)?;
                }
                html.push_str("</div>");
            }
            _ => html.push_str("<p class=unseen>Not seen in the pictures</p>"),
        }
        html.push_str("</div>\n");
        Ok(())
    }

    /// A part of a picture, cut by its view, with the message's outlines.
    fn place(&mut self, html: &mut String, place: &Value) -> Result<()> {
        let picture = place["picture"].as_str().unwrap_or_default();
        let state = place["state"].as_str().unwrap_or_default();
        let numbers = |value: &Value| -> Vec<f32> {
            value
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_f64)
                .map(|n| n as f32)
                .collect()
        };
        let view = numbers(&place["view"]);
        let [x, y, width, height] = view[..] else { return Ok(()) };
        let (full_width, full_height) = self.size(picture)?;
        let scale = (SHOWN / width).min(1.0);
        write!(
            html,
            "<a class=shot href=\"pictures/{picture}\" title=\"{state}\" style=\"width:{:.0}px;\
             height:{:.0}px;background-image:url('pictures/{picture}');background-size:{:.0}px \
             {:.0}px;background-position:{:.0}px {:.0}px\">",
            width * scale,
            height * scale,
            full_width * scale,
            full_height * scale,
            -x * scale,
            -y * scale,
            picture = escape(picture),
            state = escape(state),
        )?;
        for mark in place["marks"].as_array().into_iter().flatten() {
            let [left, top, across, down] = numbers(mark)[..] else { continue };
            write!(
                html,
                "<span class=mark style=\"left:{:.0}px;top:{:.0}px;width:{:.0}px;height:{:.0}px\">\
                 </span>",
                (left - x) * scale - 3.0,
                (top - y) * scale - 3.0,
                across * scale + 6.0,
                down * scale + 6.0,
            )?;
        }
        html.push_str("</a>");
        Ok(())
    }

    /// The size of `picture`, from its PNG header.
    fn size(&mut self, picture: &str) -> Result<(f32, f32)> {
        if let Some(&size) = self.sizes.get(picture) {
            return Ok(size);
        }
        let file = std::fs::File::open(self.pictures.join(picture))?;
        let reader = png::Decoder::new(std::io::BufReader::new(file)).read_info()?;
        let info = reader.info();
        let size = (info.width as f32, info.height as f32);
        self.sizes.insert(picture.to_owned(), size);
        Ok(size)
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

const HEAD: &str = r#"<!doctype html>
<html lang=en-GB>
<meta charset=utf-8>
<title>tgradish's words</title>
<style>
:root { color-scheme: light dark; --mark: #e8590c; --weak: #888; --line: #8884; }
body { font: 15px/1.45 system-ui, sans-serif; margin: 0 auto; max-width: 1180px; padding: 0 24px 80px; }
header { position: sticky; top: 0; background: Canvas; padding: 14px 0 10px; border-bottom: 1px solid var(--line); display: flex; gap: 16px; align-items: center; z-index: 1; }
header h1 { font-size: 19px; margin: 0 12px 0 0; }
header button { font: inherit; padding: 4px 12px; border-radius: 6px; border: 1px solid var(--line); background: none; cursor: pointer; }
header button.on { background: Highlight; color: HighlightText; }
header input { margin-left: auto; font: inherit; padding: 4px 8px; width: 260px; }
h2 { margin: 36px 0 8px; font-size: 18px; }
h3 { margin: 24px 0 6px; font-size: 15px; }
h4 { margin: 16px 0 4px; font: 13px ui-monospace, monospace; color: var(--weak); }
.about { color: var(--weak); }
.message { border-top: 1px solid var(--line); padding: 10px 0; }
.name code { font-size: 13px; font-weight: 600; }
.note { margin: 2px 0 6px; color: var(--weak); font-size: 13.5px; }
.pattern { display: grid; grid-template-columns: minmax(260px, 1fr) auto; gap: 4px 16px; align-items: start; margin: 4px 0 8px; }
.pattern .attribute { grid-column: 1 / -1; font-size: 12.5px; color: var(--weak); }
pre { margin: 0; white-space: pre-wrap; font: 14.5px/1.45 system-ui, sans-serif; }
.places { display: flex; flex-wrap: wrap; gap: 8px; justify-content: flex-end; }
.shot { position: relative; display: block; border-radius: 4px; outline: 1px solid var(--line); background-repeat: no-repeat; }
.mark { position: absolute; border: 2px solid var(--mark); border-radius: 4px; box-shadow: 0 0 0 2px #fff8; }
.unseen { margin: 0; color: var(--weak); font-size: 13px; text-align: right; }
.help { font: 13px/1.5 ui-monospace, monospace; border: 1px solid var(--line); border-radius: 6px; padding: 6px 0; }
.line { display: grid; grid-template-columns: 4.5em 1fr; }
.prefix { font-weight: normal; color: var(--weak); margin-left: 6px; }
.line:hover { background: #8881; }
.ln { color: var(--weak); text-align: right; padding-right: 14px; }
@media (max-width: 760px) {
  body { padding: 0 12px 60px; }
  header { flex-wrap: wrap; }
  header input { width: 100%; margin-left: 0; }
  .pattern { grid-template-columns: 1fr; }
  .places { justify-content: flex-start; }
  .line { grid-template-columns: 3.2em 1fr; }
}
.text { white-space: pre-wrap; }
[hidden] { display: none !important; }
</style>
<header>
<h1>tgradish's words</h1>
<button id=show-window class=on>Window</button>
<button id=show-cli>Command line</button>
<input id=filter type=search placeholder="Filter by words or name">
</header>
"#;

const TAIL: &str = r#"<script>
const tabs = { window: document.getElementById('show-window'), cli: document.getElementById('show-cli') };
for (const [name, button] of Object.entries(tabs)) {
  button.onclick = () => {
    for (const [other, b] of Object.entries(tabs)) {
      b.classList.toggle('on', other === name);
      document.getElementById(other).hidden = other !== name;
    }
  };
}
document.getElementById('filter').oninput = (event) => {
  const words = event.target.value.toLowerCase();
  for (const item of document.querySelectorAll('.message, .line')) {
    item.hidden = words !== '' && !item.textContent.toLowerCase().includes(words);
  }
};
</script>
"#;
