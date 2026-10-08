//! Option forms built from the options' JSON Schemas, so both formats'
//! options show up without a form written for each.
//!
//! A form edits a JSON object of the options the user set. Options left
//! out take the preset's value, or the built-in default, which the field
//! shows greyed.

use std::collections::HashMap;

use eframe::egui;
use serde_json::{Map, Value};

/// One option and how to edit it.
#[derive(Debug, Clone, PartialEq)]
struct Field {
    name: String,
    help: String,
    kind: Kind,
}

#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Bool,
    Number {
        integer: bool,
        min: Option<f64>,
        max: Option<f64>,
    },
    Text {
        pattern: Option<String>,
    },
    /// One of these values, each with a description.
    Choice(Vec<(String, String)>),
    /// Any number of these values.
    Choices(Vec<(String, String)>),
    /// Strings, written like a shell command line.
    Words,
    /// NAME=VALUE pairs, one per line.
    Pairs,
    /// `{min, max}`, written `MIN..MAX`.
    Range,
}

#[derive(Debug, Clone, Default)]
pub struct Form {
    fields: Vec<Field>,
    /// What the user typed into text fields, which may not parse yet.
    texts: HashMap<String, String>,
}

/// The schema `node` points at, following a `$ref` into `$defs`.
fn resolve<'a>(node: &'a Value, root: &'a Value) -> &'a Value {
    match node.get("$ref").and_then(Value::as_str) {
        Some(reference) => reference
            .strip_prefix("#/$defs/")
            .and_then(|name| root.get("$defs")?.get(name))
            .unwrap_or(node),
        None => node,
    }
}

/// The schema of a property, without the `null` that makes it optional.
fn without_null<'a>(node: &'a Value, root: &'a Value) -> (&'a Value, Option<String>) {
    if let Some(options) = node.get("anyOf").and_then(Value::as_array) {
        let other = options.iter().find(|option| option.get("type") != Some(&"null".into()));
        if let Some(other) = other {
            return (resolve(other, root), None);
        }
    }
    let ty = match node.get("type") {
        Some(Value::Array(types)) => {
            types.iter().filter_map(Value::as_str).find(|&ty| ty != "null").map(str::to_owned)
        }
        Some(Value::String(ty)) => Some(ty.clone()),
        _ => None,
    };
    (resolve(node, root), ty)
}

/// Values of an enum: `enum` strings, or `oneOf` constants with their
/// descriptions.
fn choices(node: &Value) -> Option<Vec<(String, String)>> {
    if let Some(values) = node.get("enum").and_then(Value::as_array) {
        return Some(
            values.iter().filter_map(|v| Some((v.as_str()?.to_owned(), String::new()))).collect(),
        );
    }
    let options = node.get("oneOf").and_then(Value::as_array)?;
    options
        .iter()
        .map(|option| {
            // variants without a description come as one-value enums
            let value = match option.get("const") {
                Some(value) => value.as_str()?.to_owned(),
                None => option.get("enum")?.as_array()?.first()?.as_str()?.to_owned(),
            };
            let help = option.get("description").and_then(Value::as_str).unwrap_or("").to_owned();
            Some((value, help))
        })
        .collect()
}

impl Form {
    pub fn from_schema(root: &Value) -> Form {
        let mut fields = Vec::new();
        let properties = root.get("properties").and_then(Value::as_object);
        for (name, node) in properties.into_iter().flatten() {
            let help = node.get("description").and_then(Value::as_str).unwrap_or("").to_owned();
            let (node, ty) = without_null(node, root);
            let number = |integer| Kind::Number {
                integer,
                min: node.get("minimum").or(node.get("exclusiveMinimum")).and_then(Value::as_f64),
                max: node.get("maximum").and_then(Value::as_f64),
            };
            let kind = if let Some(values) = choices(node) {
                Kind::Choice(values)
            } else {
                match ty.as_deref().or(node.get("type").and_then(Value::as_str)) {
                    Some("boolean") => Kind::Bool,
                    Some("integer") => number(true),
                    Some("number") => number(false),
                    Some("string") => Kind::Text {
                        pattern: node.get("pattern").and_then(Value::as_str).map(str::to_owned),
                    },
                    Some("array") => {
                        let items = node.get("items").map(|items| resolve(items, root));
                        match items.and_then(choices) {
                            Some(values) => Kind::Choices(values),
                            None => Kind::Words,
                        }
                    }
                    Some("object") if node.get("additionalProperties").is_some() => Kind::Pairs,
                    Some("object") => Kind::Range,
                    _ => continue,
                }
            };
            fields.push(Field { name: name.clone(), help, kind });
        }
        Form { fields, texts: HashMap::new() }
    }

    /// Forgets half-typed text, after the values changed elsewhere.
    pub fn reset_texts(&mut self) {
        self.texts.clear();
    }

    /// Shows the fields. `values` holds what the user set; `base` what the
    /// preset sets, shown where the user set nothing. Returns whether
    /// anything changed.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        values: &mut Map<String, Value>,
        base: &Map<String, Value>,
    ) -> bool {
        let mut changed = false;
        egui::Grid::new("options").num_columns(3).spacing([8.0, 6.0]).striped(true).show(
            ui,
            |ui| {
                for field in &self.fields {
                    let label = ui.label(label_of(&field.name));
                    if !field.help.is_empty() {
                        label.on_hover_text(&field.help);
                    }
                    let fallback = base.get(&field.name);
                    let text = self.texts.entry(field.name.clone()).or_insert_with(|| {
                        values
                            .get(&field.name)
                            .map(|value| text_of(&field.kind, value))
                            .unwrap_or_default()
                    });
                    changed |= edit(ui, field, values, fallback, text);
                    if values.contains_key(&field.name) {
                        if ui
                            .small_button("↺")
                            .on_hover_text("Back to the preset's value")
                            .clicked()
                        {
                            values.remove(&field.name);
                            text.clear();
                            changed = true;
                        }
                    } else {
                        ui.label("");
                    }
                    ui.end_row();
                }
            },
        );
        changed
    }
}

fn label_of(name: &str) -> String {
    let words: Vec<String> = name
        .split('-')
        .map(|word| match word {
            "crf" | "fps" => word.to_uppercase(),
            _ => word.to_owned(),
        })
        .collect();
    let words = words.join(" ");
    let mut chars = words.chars();
    chars.next().map(|first| first.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// How a value is written in a text field.
fn text_of(kind: &Kind, value: &Value) -> String {
    match (kind, value) {
        (Kind::Words, Value::Array(words)) => {
            let words: Vec<&str> = words.iter().filter_map(Value::as_str).collect();
            shlex::try_join(words).unwrap_or_default()
        }
        (Kind::Pairs, Value::Object(pairs)) => pairs
            .iter()
            .map(|(name, value)| format!("{name}={}", value.as_str().unwrap_or_default()))
            .collect::<Vec<_>>()
            .join("\n"),
        (Kind::Range, value) => match (value.get("min"), value.get("max")) {
            (Some(min), Some(max)) => format!("{min}..{max}"),
            _ => String::new(),
        },
        (_, Value::String(text)) => text.clone(),
        (_, value) => value.to_string(),
    }
}

/// The value used when the field is left unset, if the preset or the
/// description says.
fn hint(kind: &Kind, fallback: Option<&Value>, help: &str) -> Option<String> {
    if let Some(value) = fallback {
        return Some(text_of(kind, value));
    }
    // descriptions end like "[default: best]" or "Default: best."
    let bracketed = help.rsplit_once("[default: ").and_then(|(_, rest)| rest.split_once(']'));
    let sentence = || {
        let (_, rest) = help.rsplit_once("Default:")?;
        let rest = rest.trim().strip_suffix('.').unwrap_or(rest.trim());
        Some((rest, ""))
    };
    let (default, _) = bracketed.or_else(sentence)?;
    Some(default.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// "default", with the value it stands for when known.
fn default_label(kind: &Kind, fallback: Option<&Value>, help: &str) -> String {
    match hint(kind, fallback, help) {
        Some(value) => format!("default ({value})"),
        None => "default".to_owned(),
    }
}

/// Parses text typed into a field, `None` when it doesn't parse.
fn parse(kind: &Kind, text: &str) -> Option<Value> {
    let text = text.trim();
    match kind {
        Kind::Number { integer, min, max } => {
            let number: f64 = text.parse().ok()?;
            let fits = min.is_none_or(|min| number >= min) && max.is_none_or(|max| number <= max);
            if !number.is_finite() || !fits || (*integer && number.fract() != 0.0) {
                return None;
            }
            Some(if *integer { Value::from(number as i64) } else { Value::from(number) })
        }
        Kind::Text { pattern } => {
            // the only pattern is COLUMNSxROWS; check its shape loosely
            if pattern.is_some()
                && !text.split_once('x').is_some_and(|(a, b)| {
                    a.parse::<u32>().is_ok_and(|n| n > 0) && b.parse::<u32>().is_ok_and(|n| n > 0)
                })
            {
                return None;
            }
            Some(Value::from(text))
        }
        Kind::Words => Some(Value::from(shlex::split(text)?)),
        Kind::Pairs => {
            let mut pairs = Map::new();
            for line in text.lines().filter(|line| !line.trim().is_empty()) {
                let (name, value) = line.split_once('=')?;
                pairs.insert(name.trim().to_owned(), Value::from(value.trim()));
            }
            Some(Value::Object(pairs))
        }
        Kind::Range => {
            let (min, max) = text.split_once("..")?;
            let (min, max): (f64, f64) = (min.trim().parse().ok()?, max.trim().parse().ok()?);
            (min <= max).then(|| serde_json::json!({ "min": min, "max": max }))
        }
        _ => None,
    }
}

fn edit(
    ui: &mut egui::Ui,
    field: &Field,
    values: &mut Map<String, Value>,
    fallback: Option<&Value>,
    text: &mut String,
) -> bool {
    let name = &field.name;
    let current = values.get(name).cloned();
    match &field.kind {
        Kind::Bool => {
            let shown = match current.as_ref().and_then(Value::as_bool) {
                Some(true) => "yes".to_owned(),
                Some(false) => "no".to_owned(),
                None => default_label(&field.kind, fallback, &field.help),
            };
            let mut picked = current.as_ref().and_then(Value::as_bool);
            egui::ComboBox::from_id_salt(name).selected_text(shown).show_ui(ui, |ui| {
                ui.selectable_value(&mut picked, None, "default");
                ui.selectable_value(&mut picked, Some(true), "yes");
                ui.selectable_value(&mut picked, Some(false), "no");
            });
            set(values, name, picked.map(Value::from))
        }
        Kind::Choice(options) => {
            let mut picked = current.as_ref().and_then(Value::as_str).map(str::to_owned);
            let shown =
                picked.clone().unwrap_or_else(|| default_label(&field.kind, fallback, &field.help));
            egui::ComboBox::from_id_salt(name).selected_text(shown).show_ui(ui, |ui| {
                ui.selectable_value(&mut picked, None, "default");
                for (value, help) in options {
                    let response = ui.selectable_value(&mut picked, Some(value.clone()), value);
                    if !help.is_empty() {
                        response.on_hover_text(help);
                    }
                }
            });
            set(values, name, picked.map(Value::from))
        }
        Kind::Choices(options) => {
            let chosen: Option<Vec<String>> = current
                .as_ref()
                .and_then(Value::as_array)
                .map(|values| values.iter().filter_map(Value::as_str).map(str::to_owned).collect());
            let mut picked = chosen.clone();
            ui.vertical(|ui| {
                let mut all = picked.is_none();
                if ui
                    .checkbox(&mut all, default_label(&field.kind, fallback, &field.help))
                    .changed()
                {
                    picked = if all {
                        None
                    } else {
                        Some(options.iter().map(|(v, _)| v.clone()).collect())
                    };
                }
                if let Some(list) = &mut picked {
                    for (value, help) in options {
                        let mut on = list.contains(value);
                        let response = ui.checkbox(&mut on, value);
                        if !help.is_empty() {
                            response.clone().on_hover_text(help);
                        }
                        if response.changed() {
                            if on {
                                list.push(value.clone());
                            } else {
                                list.retain(|other| other != value);
                            }
                            // keep the schema's order
                            list.sort_by_key(|v| options.iter().position(|(o, _)| o == v));
                        }
                    }
                }
            });
            if picked == chosen {
                return false;
            }
            set(values, name, picked.map(Value::from))
        }
        kind => {
            let hint = hint(kind, fallback, &field.help).unwrap_or_else(|| "default".to_owned());
            let wrong = !text.trim().is_empty() && parse(kind, text).is_none();
            let editor = if matches!(kind, Kind::Pairs) {
                egui::TextEdit::multiline(text).desired_rows(2)
            } else {
                egui::TextEdit::singleline(text)
            };
            let mut editor = editor.hint_text(hint).desired_width(180.0);
            if wrong {
                editor = editor.text_color(ui.visuals().error_fg_color);
            }
            if !ui.add(editor).changed() {
                return false;
            }
            match parse(kind, text) {
                _ if text.trim().is_empty() => set(values, name, None),
                Some(value) => set(values, name, Some(value)),
                // keep the last good value until the text parses
                None => false,
            }
        }
    }
}

/// Sets or removes a value; returns whether it changed.
fn set(values: &mut Map<String, Value>, name: &str, value: Option<Value>) -> bool {
    let before = values.get(name).cloned();
    match value {
        Some(value) => values.insert(name.to_owned(), value),
        None => values.remove(name),
    };
    values.get(name).cloned() != before
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form<T: schemars::JsonSchema>() -> Form {
        Form::from_schema(&serde_json::to_value(schemars::schema_for!(T)).unwrap())
    }

    #[test]
    fn reads_both_formats() {
        let webm = form::<tgradish_core::options::Options>();
        let kind = |form: &Form, name: &str| {
            form.fields.iter().find(|f| f.name == name).unwrap().kind.clone()
        };
        assert!(matches!(kind(&webm, "target"), Kind::Choice(values) if values.len() == 2));
        assert_eq!(
            kind(&webm, "crf"),
            Kind::Number { integer: true, min: Some(0.0), max: Some(63.0) }
        );
        assert_eq!(kind(&webm, "lossless"), Kind::Bool);
        assert_eq!(kind(&webm, "encoder-options"), Kind::Pairs);
        assert_eq!(kind(&webm, "extra-args"), Kind::Words);
        assert_eq!(kind(&webm, "fit-range"), Kind::Range);
        // every option has a field
        let schema =
            serde_json::to_value(schemars::schema_for!(tgradish_core::options::Options)).unwrap();
        assert_eq!(webm.fields.len(), schema["properties"].as_object().unwrap().len());

        let tgs = form::<tgradish_core::tgs::TgsOptions>();
        assert!(matches!(kind(&tgs, "reductions"), Kind::Choices(values) if values.len() == 6));
        assert!(matches!(kind(&tgs, "sheet"), Kind::Text { pattern: Some(_) }));
        assert!(matches!(kind(&tgs, "long"), Kind::Choice(_)));
    }

    #[test]
    fn parses_text() {
        let number = Kind::Number { integer: true, min: Some(1.0), max: Some(50.0) };
        assert_eq!(parse(&number, " 8 "), Some(Value::from(8)));
        assert_eq!(parse(&number, "0"), None);
        assert_eq!(parse(&number, "2.5"), None);
        let words = parse(&Kind::Words, r#"-tune-content "screen x""#).unwrap();
        assert_eq!(words, serde_json::json!(["-tune-content", "screen x"]));
        let pairs = parse(&Kind::Pairs, "g=60\n aq-mode = 2\n").unwrap();
        assert_eq!(pairs, serde_json::json!({ "g": "60", "aq-mode": "2" }));
        assert_eq!(
            parse(&Kind::Range, "100..400"),
            Some(serde_json::json!({ "min": 100.0, "max": 400.0 }))
        );
        assert_eq!(
            parse(&Kind::Text { pattern: Some("x".into()) }, "4x2"),
            Some(Value::from("4x2"))
        );
        assert_eq!(parse(&Kind::Text { pattern: Some("x".into()) }, "4 by 2"), None);
        assert_eq!(hint(&Kind::Bool, None, "Lossless. [default: false]").as_deref(), Some("false"));
        assert_eq!(default_label(&Kind::Bool, None, "Lossless."), "default");
        let help = "How to scale. Default: contain for\nstickers, pad for emoji.";
        assert_eq!(
            hint(&Kind::Bool, None, help).as_deref(),
            Some("contain for stickers, pad for emoji")
        );
        assert_eq!(hint(&Kind::Bool, None, "Fake. Default:\n0.42069.").as_deref(), Some("0.42069"));
        assert_eq!(label_of("fake-duration"), "Fake duration");
        assert_eq!(label_of("crf"), "CRF");
        assert_eq!(
            text_of(&Kind::Range, &serde_json::json!({ "min": 1.0, "max": 2.0 })),
            "1.0..2.0"
        );
    }
}
