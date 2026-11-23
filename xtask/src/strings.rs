//! Generates an HTML review/translation page (`target/strings/index.html`)
//! showing all GUI Fluent strings (with translator notes and screenshots),
//! plus CLI help pages with reference IDs.
//!
//! Screenshots are captured by `strings::strings` in `crates/gui/src/strings.rs`
//! (requires a wgpu-capable GPU and ffmpeg, and takes a few minutes). Pass
//! `--no-pictures` to reuse existing screenshots.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::{cargo, run, workspace_root};

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
    let page = Page { pictures: &out.join("pictures"), sizes: BTreeMap::new() };
    let html = page.html(&sections, &shown, &help)?;
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

/// What writing the page needs: the pictures' folder, and their sizes as
/// read.
struct Page<'a> {
    pictures: &'a Path,
    sizes: BTreeMap<String, (f32, f32)>,
}

impl Page<'_> {
    fn html(mut self, sections: &[Section], shown: &Value, help: &Value) -> Result<String> {
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
            "<section id=window><p class=about>{} strings ({pictured} captured in screenshots). \
             Screenshots are generated by driving the GUI through scripted states with scrolling \
             and hover. Strings without a screenshot only appear in states those scripts do not \
             trigger (such as rare errors) or as fragments of larger messages. Text is shown \
             verbatim from <code>crates/gui/i18n/en-GB/tgradish.ftl</code>: {{ $name }} is a \
             variable and {{ other-message }} inserts another message. Each string is captioned \
             with where it appears. Hover over a screenshot to see its GUI state; click to open \
             the full image.</p>",
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
            "<p class=about>The CLI is English-only. Each line has a reference ID made of its \
             section prefix and line number, such as <code>convert:23</code>. Hover over a line \
             number to see the full ID.</p>\n<h2>Help pages</h2>\n",
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
            _ => html.push_str("<p class=unseen>No screenshot captured</p>"),
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
<title>tgradish strings</title>
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
<h1>tgradish strings</h1>
<button id=show-window class=on>GUI</button>
<button id=show-cli>Command line</button>
<input id=filter type=search placeholder="Filter by text or name">
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
