//! JSON output matching Python's `json.dumps`, which templates reach through the
//! `to_json`, `to_nice_json` and `tojson` filters.

use crate::value::{py_float_repr, Mapping, Value};

/// Options of [`dumps`], defaulting to those of `json.dumps(value)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DumpOptions<'a> {
    /// Spaces per nesting level; `None` writes everything on one line.
    pub indent: Option<usize>,
    /// Whether mapping keys are sorted.
    pub sort_keys: bool,
    /// Whether non-ASCII characters are escaped.
    pub ensure_ascii: bool,
    /// Separator between items.
    pub item_separator: &'a str,
    /// Separator between a key and its value.
    pub key_separator: &'a str,
}

impl Default for DumpOptions<'_> {
    fn default() -> Self {
        Self {
            indent: None,
            sort_keys: false,
            ensure_ascii: true,
            item_separator: ", ",
            key_separator: ": ",
        }
    }
}

impl DumpOptions<'_> {
    /// `json.dumps(value, indent=n)`: one item per line, no trailing spaces.
    pub fn indented(indent: usize) -> Self {
        Self {
            indent: Some(indent),
            item_separator: ",",
            ..Self::default()
        }
    }
}

/// Serialize `value` as JSON text.
pub fn dumps(value: &Value, options: &DumpOptions<'_>) -> String {
    let mut out = String::new();
    write_value(&mut out, value, options, 0);
    out
}

fn newline(out: &mut String, options: &DumpOptions<'_>, level: usize) {
    if let Some(indent) = options.indent {
        out.push('\n');
        out.push_str(&" ".repeat(indent * level));
    }
}

fn write_value(out: &mut String, value: &Value, options: &DumpOptions<'_>, level: usize) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
        Value::Int(number) => out.push_str(&number.to_string()),
        Value::Float(number) => out.push_str(&float_text(*number)),
        Value::Str(text) | Value::Timestamp(text) => write_string(out, text, options.ensure_ascii),
        Value::Seq(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push_str(options.item_separator);
                }
                newline(out, options, level + 1);
                write_value(out, item, options, level + 1);
            }
            newline(out, options, level);
            out.push(']');
        }
        Value::Map(map) => {
            if map.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push('{');
            for (index, (key, item)) in ordered(map, options.sort_keys).into_iter().enumerate() {
                if index > 0 {
                    out.push_str(options.item_separator);
                }
                newline(out, options, level + 1);
                write_string(out, &key_text(key), options.ensure_ascii);
                out.push_str(options.key_separator);
                write_value(out, item, options, level + 1);
            }
            newline(out, options, level);
            out.push('}');
        }
    }
}

/// JSON keys are strings; Python converts scalar keys the JSON way.
fn key_text(key: &Value) -> String {
    match key {
        Value::Str(text) | Value::Timestamp(text) => text.clone(),
        Value::Null => "null".to_owned(),
        Value::Bool(flag) => flag.to_string(),
        Value::Float(number) => float_text(*number),
        other => other.py_str(),
    }
}

fn ordered(map: &Mapping, sort_keys: bool) -> Vec<(&Value, &Value)> {
    let mut entries: Vec<(&Value, &Value)> = map.iter().collect();
    if sort_keys {
        entries.sort_by_key(|(key, _)| key_text(key));
    }
    entries
}

fn float_text(number: f64) -> String {
    if number.is_nan() {
        "NaN".to_owned()
    } else if number.is_infinite() {
        if number > 0.0 {
            "Infinity"
        } else {
            "-Infinity"
        }
        .to_owned()
    } else {
        py_float_repr(number)
    }
}

fn write_string(out: &mut String, text: &str, ensure_ascii: bool) {
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c if ensure_ascii && !c.is_ascii() => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    out.push_str(&format!("\\u{unit:04x}"));
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Parse JSON text (`json.loads`).
///
/// # Errors
///
/// Returns the parser's message for invalid JSON.
pub fn loads(text: &str) -> Result<Value, String> {
    serde_json::from_str::<serde_json::Value>(text)
        .map(Value::from)
        .map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn matches_python_defaults() {
        let value: Value = json!({"b": [1, 2.0, "é", null, true], "a": {}}).into();
        assert_eq!(
            dumps(&value, &DumpOptions::default()),
            r#"{"b": [1, 2.0, "\u00e9", null, true], "a": {}}"#
        );
        let sorted = DumpOptions {
            sort_keys: true,
            ensure_ascii: false,
            ..DumpOptions::default()
        };
        assert_eq!(
            dumps(&value, &sorted),
            r#"{"a": {}, "b": [1, 2.0, "é", null, true]}"#
        );
    }

    #[test]
    fn indentation() {
        let value: Value = json!({"a": [1, 2], "b": []}).into();
        assert_eq!(
            dumps(&value, &DumpOptions::indented(2)),
            "{\n  \"a\": [\n    1,\n    2\n  ],\n  \"b\": []\n}"
        );
    }

    #[test]
    fn astral_characters_use_surrogate_pairs() {
        assert_eq!(
            dumps(&Value::from("🎤"), &DumpOptions::default()),
            "\"\\ud83c\\udfa4\""
        );
    }
}
