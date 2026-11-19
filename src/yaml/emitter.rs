//! YAML output in PyYAML's style.
//!
//! Layout, key order and scalar quoting match `yaml.dump` for ordinary data. Long
//! lines are not folded and multi-line strings are double-quoted, so the output is
//! structurally (not always byte-for-byte) identical to PyYAML's.

use super::resolver::{implicit_kind, Implicit};
use crate::value::{Mapping, Value};

/// Which collections are written in flow style (`[a, b]`, `{k: v}`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FlowStyle {
    /// Never (`default_flow_style=False`).
    #[default]
    Block,
    /// Always (`default_flow_style=True`).
    Flow,
    /// Only collections holding nothing but scalars (`default_flow_style=None`).
    Auto,
}

/// Options of [`dump`], defaulting to those of `yaml.dump(..., default_flow_style=False)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DumpOptions {
    /// Spaces per nesting level.
    pub indent: usize,
    /// Flow style policy.
    pub flow: FlowStyle,
    /// Whether mapping keys are sorted.
    pub sort_keys: bool,
    /// Whether non-ASCII text is written as is rather than escaped.
    pub allow_unicode: bool,
}

impl Default for DumpOptions {
    fn default() -> Self {
        Self {
            indent: 2,
            flow: FlowStyle::Block,
            sort_keys: true,
            allow_unicode: true,
        }
    }
}

/// Serialize `value` as one YAML document.
pub fn dump(value: &Value, options: &DumpOptions) -> String {
    let mut emitter = Emitter {
        out: String::new(),
        options: *options,
    };
    match value {
        Value::Seq(_) | Value::Map(_) => {
            emitter.node(value, 0, Position::Root);
            emitter.out.push('\n');
        }
        scalar => {
            let text = emitter.scalar(scalar, false);
            let plain = !text.starts_with(['\'', '"']);
            emitter.out.push_str(&text);
            emitter.out.push('\n');
            // PyYAML marks the end of a document whose root is a plain scalar.
            if plain {
                emitter.out.push_str("...\n");
            }
        }
    }
    emitter.out
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Position {
    /// Document root: content starts on the current line.
    Root,
    /// After `key:`.
    MappingValue,
    /// After `-`.
    SequenceItem,
}

struct Emitter {
    out: String,
    options: DumpOptions,
}

impl Emitter {
    fn use_flow(&self, value: &Value) -> bool {
        let empty_or_leaf = |leaf: bool, empty: bool| match self.options.flow {
            FlowStyle::Flow => true,
            FlowStyle::Auto => leaf,
            FlowStyle::Block => empty,
        };
        match value {
            Value::Seq(items) => empty_or_leaf(items.iter().all(is_scalar), items.is_empty()),
            Value::Map(map) => empty_or_leaf(map.values().all(is_scalar), map.is_empty()),
            _ => true,
        }
    }

    fn newline(&mut self, indent: usize) {
        self.out.push('\n');
        self.out.push_str(&" ".repeat(indent));
    }

    /// Write `value` whose block content belongs at column `indent`.
    fn node(&mut self, value: &Value, indent: usize, position: Position) {
        if self.use_flow(value) {
            if position != Position::Root {
                self.out.push(' ');
            }
            if is_scalar(value) {
                let text = self.scalar(value, false);
                self.out.push_str(&text);
            } else {
                self.flow(value);
            }
            return;
        }
        match value {
            Value::Seq(items) => {
                for (index, item) in items.iter().enumerate() {
                    self.block_entry_start(index, indent, position);
                    self.out.push('-');
                    self.node(item, indent + self.options.indent, Position::SequenceItem);
                }
            }
            Value::Map(map) => {
                for (index, (key, item)) in self.ordered(map).into_iter().enumerate() {
                    self.block_entry_start(index, indent, position);
                    let text = self.scalar(key, false);
                    self.out.push_str(&text);
                    self.out.push(':');
                    // A block sequence under a key is not indented further.
                    let child_indent = if matches!(item, Value::Seq(_)) {
                        indent
                    } else {
                        indent + self.options.indent
                    };
                    self.node(item, child_indent, Position::MappingValue);
                }
            }
            // Scalars always take the flow branch above.
            _ => {}
        }
    }

    fn block_entry_start(&mut self, index: usize, indent: usize, position: Position) {
        match (index, position) {
            (0, Position::Root) => {}
            // The first entry of a collection nested in a sequence shares the dash's line.
            (0, Position::SequenceItem) => self
                .out
                .push_str(&" ".repeat(self.options.indent.saturating_sub(1))),
            _ => self.newline(indent),
        }
    }

    fn flow(&mut self, value: &Value) {
        match value {
            Value::Seq(items) => {
                self.out.push('[');
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        self.out.push_str(", ");
                    }
                    self.flow(item);
                }
                self.out.push(']');
            }
            Value::Map(map) => {
                self.out.push('{');
                for (index, (key, item)) in self.ordered(map).into_iter().enumerate() {
                    if index > 0 {
                        self.out.push_str(", ");
                    }
                    let text = self.scalar(key, true);
                    self.out.push_str(&text);
                    self.out.push_str(": ");
                    self.flow(item);
                }
                self.out.push('}');
            }
            scalar => {
                let text = self.scalar(scalar, true);
                self.out.push_str(&text);
            }
        }
    }

    fn scalar(&self, value: &Value, in_flow: bool) -> String {
        match value {
            Value::Null => "null".to_owned(),
            Value::Bool(true) => "true".to_owned(),
            Value::Bool(false) => "false".to_owned(),
            Value::Int(number) => number.to_string(),
            Value::Float(number) => float_text(*number),
            // A time contains `: `-like colons that flow context cannot hold unquoted.
            Value::Timestamp(text) if in_flow && text.contains(':') => {
                format!("!!timestamp '{text}'")
            }
            Value::Timestamp(text) => text.clone(),
            Value::Str(text) => string_text(text, in_flow, self.options.allow_unicode),
            // Collections never reach here: keys are scalars and values are dispatched earlier.
            Value::Seq(_) | Value::Map(_) => String::new(),
        }
    }

    fn ordered<'a>(&self, map: &'a Mapping) -> Vec<(&'a Value, &'a Value)> {
        let mut entries: Vec<(&Value, &Value)> = map.iter().collect();
        if self.options.sort_keys {
            // Python cannot order keys of mixed types; PyYAML then keeps insertion order.
            if entries.iter().all(|(key, _)| matches!(key, Value::Str(_))) {
                entries.sort_by(|(a, _), (b, _)| a.as_str().cmp(&b.as_str()));
            } else if entries.iter().all(|(key, _)| matches!(key, Value::Int(_))) {
                entries.sort_by_key(|(key, _)| key.as_i64());
            }
        }
        entries
    }
}

fn is_scalar(value: &Value) -> bool {
    !matches!(value, Value::Seq(_) | Value::Map(_))
}

fn float_text(number: f64) -> String {
    if number.is_nan() {
        return ".nan".to_owned();
    }
    if number.is_infinite() {
        return if number > 0.0 { ".inf" } else { "-.inf" }.to_owned();
    }
    let repr = crate::value::py_float_repr(number);
    // `1e+16` would be read back as a string under YAML 1.1.
    match repr.split_once('e') {
        Some((mantissa, exponent)) if !mantissa.contains('.') => format!("{mantissa}.0e{exponent}"),
        _ => repr,
    }
}

fn string_text(text: &str, in_flow: bool, allow_unicode: bool) -> String {
    if needs_double_quotes(text, allow_unicode) {
        return double_quoted(text, allow_unicode);
    }
    if text.is_empty() || implicit_kind(text) != Implicit::Str || !plain_allowed(text, in_flow) {
        return format!("'{}'", text.replace('\'', "''"));
    }
    text.to_owned()
}

fn needs_double_quotes(text: &str, allow_unicode: bool) -> bool {
    text.chars().any(|ch| {
        matches!(ch, '\n' | '\u{feff}' | '\u{85}' | '\u{2028}' | '\u{2029}')
            || ch.is_control()
            || (!allow_unicode && !ch.is_ascii())
    })
}

fn double_quoted(text: &str, allow_unicode: bool) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            '\u{7}' => out.push_str("\\a"),
            '\u{8}' => out.push_str("\\b"),
            '\u{b}' => out.push_str("\\v"),
            '\u{c}' => out.push_str("\\f"),
            '\u{1b}' => out.push_str("\\e"),
            '\u{85}' => out.push_str("\\N"),
            '\u{a0}' => out.push_str("\\_"),
            '\u{2028}' => out.push_str("\\L"),
            '\u{2029}' => out.push_str("\\P"),
            '\u{feff}' => out.push_str("\\uFEFF"),
            c if c.is_control() || (!allow_unicode && !c.is_ascii()) => {
                let code = c as u32;
                out.push_str(&match code {
                    0..=0xFF => format!("\\x{code:02X}"),
                    0x100..=0xFFFF => format!("\\u{code:04X}"),
                    _ => format!("\\U{code:08X}"),
                });
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// PyYAML's `analyze_scalar`, reduced to "may this be written without quotes?".
fn plain_allowed(text: &str, in_flow: bool) -> bool {
    if text.starts_with("---") || text.starts_with("...") {
        return false;
    }
    if text.starts_with([' ', '\t']) || text.ends_with([' ', '\t']) {
        return false;
    }
    let chars: Vec<char> = text.chars().collect();
    for (index, &ch) in chars.iter().enumerate() {
        let followed_by_space = chars
            .get(index + 1)
            .map_or(true, |next| " \t".contains(*next));
        let preceded_by_space = index == 0 || " \t".contains(chars[index - 1]);
        if index == 0 {
            if "#,[]{}&*!|>'\"%@`".contains(ch) {
                return false;
            }
            if "?:".contains(ch) && (in_flow || followed_by_space) {
                return false;
            }
            if ch == '-' && followed_by_space {
                return false;
            }
        } else {
            if ",?[]{}".contains(ch) && in_flow {
                return false;
            }
            if ch == ':' && (in_flow || followed_by_space) {
                return false;
            }
            if ch == '#' && preceded_by_space {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn nice(value: serde_json::Value) -> String {
        dump(
            &value.into(),
            &DumpOptions {
                indent: 4,
                ..DumpOptions::default()
            },
        )
    }

    #[test]
    fn answers_file_shape() {
        let text = nice(
            json!({"_src_path": "gh:org/tpl", "_commit": "v1.0.0", "name": "x", "flag": true, "n": 3}),
        );
        assert_eq!(
            text,
            "_commit: v1.0.0\n_src_path: gh:org/tpl\nflag: true\nn: 3\nname: x\n"
        );
    }

    #[test]
    fn nested_block_layout() {
        let text = nice(json!({"a": {"b": [1, {"x": 1, "y": [2, 3]}, [4, 5]], "c": {}, "d": []}}));
        let expected = "\
a:
    b:
    - 1
    -   x: 1
        y:
        - 2
        - 3
    -   - 4
        - 5
    c: {}
    d: []
";
        assert_eq!(text, expected);
    }

    #[test]
    fn quoting() {
        let text = nice(json!([
            "yes",
            "1",
            "1.0",
            "",
            "a: b",
            "a #b",
            "- x",
            "-x",
            "it's",
            "null",
            "a\nb",
            "@x",
            "2020-01-01",
            " x",
            "x:"
        ]));
        let expected = "\
- 'yes'
- '1'
- '1.0'
- ''
- 'a: b'
- 'a #b'
- '- x'
- -x
- it's
- 'null'
- \"a\\nb\"
- '@x'
- '2020-01-01'
- ' x'
- 'x:'
";
        assert_eq!(text, expected);
    }

    #[test]
    fn scalars_floats_and_root_documents() {
        assert_eq!(nice(json!("foo")), "foo\n...\n");
        assert_eq!(nice(json!("yes")), "'yes'\n");
        assert_eq!(nice(json!(null)), "null\n...\n");
        assert_eq!(nice(json!([1.0, 1e16, 0.5])), "- 1.0\n- 1.0e+16\n- 0.5\n");
    }

    #[test]
    fn flow_styles() {
        let value: Value = json!({"b": [1, "x, y"], "a": {"k": "v"}}).into();
        let flow = dump(
            &value,
            &DumpOptions {
                flow: FlowStyle::Flow,
                ..DumpOptions::default()
            },
        );
        assert_eq!(flow, "{a: {k: v}, b: [1, 'x, y']}\n");
        let auto = dump(
            &value,
            &DumpOptions {
                flow: FlowStyle::Auto,
                ..DumpOptions::default()
            },
        );
        assert_eq!(auto, "a: {k: v}\nb: [1, 'x, y']\n");
    }
}
