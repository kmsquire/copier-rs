//! Construction of values from YAML text, following PyYAML's `SafeLoader`.

use std::collections::HashMap;
use std::fmt;
use std::iter::Peekable;

use saphyr_parser::{Event, Parser, ScalarStyle, Span, Tag};

use super::resolver::{
    construct_bool, construct_float, construct_int, construct_timestamp, implicit_kind, resolve_plain, Implicit,
};
use crate::errors::CopierError;
use crate::value::{Mapping, Value};

const CORE_TAG_PREFIX: &str = "tag:yaml.org,2002:";

/// A YAML syntax or construction problem, with its 1-based position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YamlError {
    /// What is wrong.
    pub message: String,
    /// Line of the offending token.
    pub line: usize,
    /// Column of the offending token.
    pub column: usize,
}

impl fmt::Display for YamlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Reader errors already carry their own position text.
        if self.message.contains("\n  in \"") {
            return f.write_str(&self.message);
        }
        write!(f, "{}\n  in \"<unicode string>\", line {}, column {}", self.message, self.line, self.column)
    }
}

impl std::error::Error for YamlError {}

/// Failure to load a YAML stream.
#[derive(Debug)]
pub enum LoadError {
    /// The YAML itself is invalid.
    Yaml(YamlError),
    /// A custom tag handler failed.
    Custom(CopierError),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Yaml(err) => err.fmt(f),
            Self::Custom(err) => err.fmt(f),
        }
    }
}

impl std::error::Error for LoadError {}

impl From<YamlError> for LoadError {
    fn from(err: YamlError) -> Self {
        Self::Yaml(err)
    }
}

/// The node an application tag (such as `!include`) is attached to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaggedNode<'a> {
    /// A scalar with its text.
    Scalar(&'a str),
    /// A sequence.
    Sequence,
    /// A mapping.
    Mapping,
}

/// Handler for application tags: gets the tag (e.g. `!include`) and its node, and
/// returns `None` when it does not know the tag.
pub type TagHandler<'a> = dyn FnMut(&str, TaggedNode<'_>) -> Option<Result<Value, CopierError>> + 'a;

/// Load the single document in `text` (`yaml.safe_load`); an empty stream is `Null`.
///
/// # Errors
///
/// Fails on invalid YAML, unknown tags, or more than one document.
pub fn load(text: &str) -> Result<Value, YamlError> {
    let mut documents = load_all(text)?.into_iter();
    let first = documents.next().unwrap_or(Value::Null);
    if documents.next().is_some() {
        return Err(YamlError {
            message: "expected a single document in the stream\nbut found another document".to_owned(),
            line: 1,
            column: 1,
        });
    }
    Ok(first)
}

/// Load every document in `text` (`yaml.safe_load_all`).
///
/// # Errors
///
/// Fails on invalid YAML or unknown tags.
pub fn load_all(text: &str) -> Result<Vec<Value>, YamlError> {
    load_all_with(text, &mut |_, _| None).map_err(|err| match err {
        LoadError::Yaml(err) => err,
        // The no-op handler never produces custom errors.
        LoadError::Custom(err) => YamlError { message: err.to_string(), line: 1, column: 1 },
    })
}

/// Load every document in `text`, resolving application tags through `handler`.
///
/// # Errors
///
/// Fails on invalid YAML, on tags neither PyYAML's safe loader nor `handler` knows,
/// or when `handler` fails.
pub fn load_all_with(text: &str, handler: &mut TagHandler<'_>) -> Result<Vec<Value>, LoadError> {
    check_printable(text, "<unicode string>")?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut loader = Loader { events: Parser::new_from_str(text).peekable(), anchors: HashMap::new(), handler };
    let mut documents = Vec::new();
    loop {
        match loader.next_event()? {
            (Event::StreamStart, _) => {}
            (Event::StreamEnd, _) => break,
            (Event::DocumentStart(_), _) => {
                loader.anchors.clear();
                let (event, span) = loader.next_event()?;
                documents.push(loader.node(event, span)?);
            }
            (Event::DocumentEnd, _) => {}
            (_, span) => return Err(error_at("unexpected event outside a document", span).into()),
        }
    }
    Ok(documents)
}

/// Decode the bytes of a YAML file like PyYAML's reader: UTF-16 when a byte-order
/// mark says so, UTF-8 otherwise.
///
/// # Errors
///
/// Fails when the bytes are not valid in the detected encoding.
pub fn decode(bytes: &[u8]) -> Result<String, YamlError> {
    let utf16 = |units: Vec<u16>, encoding: &str| {
        String::from_utf16(&units).map_err(|_| YamlError {
            message: format!("'{encoding}' codec can't decode bytes: illegal encoding"),
            line: 1,
            column: 1,
        })
    };
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        utf16(
            rest.chunks(2).map(|pair| u16::from_le_bytes([pair[0], *pair.get(1).unwrap_or(&0)])).collect(),
            "utf-16-le",
        )
    } else if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        utf16(
            rest.chunks(2).map(|pair| u16::from_be_bytes([pair[0], *pair.get(1).unwrap_or(&0)])).collect(),
            "utf-16-be",
        )
    } else {
        String::from_utf8(bytes.to_vec()).map_err(|err| {
            let position = err.utf8_error().valid_up_to();
            YamlError {
                message: format!(
                    "'utf-8' codec can't decode byte #x{:02x}: invalid start byte\n  in \"<byte string>\", position {position}",
                    bytes[position]
                ),
                line: 1,
                column: 1,
            }
        })
    }
}

/// PyYAML's reader rejects control characters before any parsing happens.
fn check_printable(text: &str, name: &str) -> Result<(), YamlError> {
    let printable = |ch: char| matches!(ch, '\t' | '\n' | '\r' | '\u{20}'..='\u{7e}' | '\u{85}' | '\u{a0}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..);
    match text.chars().enumerate().find(|(_, ch)| !printable(*ch)) {
        None => Ok(()),
        Some((position, ch)) => Err(YamlError {
            message: format!(
                "unacceptable character #x{:04x}: special characters are not allowed\n  in \"{name}\", position {position}",
                ch as u32
            ),
            line: 1,
            column: 1,
        }),
    }
}

/// Raw source text of each item of the YAML list in `text`, with the quotes of
/// quoted strings removed (Copier's `parse_yaml_list`). `None` when `text` is not
/// a list.
///
/// # Errors
///
/// Fails on invalid YAML.
pub fn list_item_sources(text: &str) -> Result<Option<Vec<String>>, YamlError> {
    check_printable(text, "<unicode string>")?;
    let offsets: Vec<usize> = text.char_indices().map(|(offset, _)| offset).chain([text.len()]).collect();
    let slice =
        |start: usize, end: usize| &text[offsets[start.min(offsets.len() - 1)]..offsets[end.min(offsets.len() - 1)]];
    let mut depth = 0usize;
    let mut item_start = None;
    let mut items = Vec::new();
    let mut is_list = false;
    for event in Parser::new_from_str(text) {
        let (event, span) = event.map_err(|err| YamlError {
            message: err.info().to_owned(),
            line: err.marker().line(),
            column: err.marker().col() + 1,
        })?;
        let (opens, closes, quoted_text) = match &event {
            Event::SequenceStart(..) | Event::MappingStart(..) => (true, false, false),
            Event::SequenceEnd | Event::MappingEnd => (false, true, false),
            Event::Scalar(_, style, _, tag) => {
                let is_text = match tag {
                    Some(tag) => tag_name(tag) == format!("{CORE_TAG_PREFIX}str"),
                    None => true,
                };
                (false, false, is_text && matches!(style, ScalarStyle::SingleQuoted | ScalarStyle::DoubleQuoted))
            }
            Event::Alias(_) => (false, false, false),
            _ => continue,
        };
        if depth == 0 {
            if !opens || !matches!(event, Event::SequenceStart(..)) {
                return Ok(None);
            }
            is_list = true;
            depth = 1;
            continue;
        }
        if closes {
            depth -= 1;
            if depth == 0 {
                break;
            }
        }
        if depth == 1 && !closes {
            item_start = Some(span.start.index());
        }
        if opens {
            depth += 1;
            continue;
        }
        if depth == 1 {
            if let Some(start) = item_start.take() {
                let raw = slice(start, span.end.index()).trim();
                let unquoted = (quoted_text && raw.len() >= 2).then(|| &raw[1..raw.len() - 1]);
                items.push(unquoted.unwrap_or(raw).to_owned());
            }
        }
    }
    Ok(is_list.then_some(items))
}

fn error_at(message: impl Into<String>, span: Span) -> YamlError {
    YamlError { message: message.into(), line: span.start.line(), column: span.start.col() + 1 }
}

struct Loader<'input, 'handler, 'h> {
    events: Peekable<Parser<'input, saphyr_parser::StrInput<'input>>>,
    anchors: HashMap<usize, Value>,
    handler: &'handler mut TagHandler<'h>,
}

impl<'input> Loader<'input, '_, '_> {
    fn next_event(&mut self) -> Result<(Event<'input>, Span), YamlError> {
        match self.events.next() {
            Some(Ok(pair)) => Ok(pair),
            Some(Err(err)) => Err(YamlError {
                message: err.info().to_owned(),
                line: err.marker().line(),
                column: err.marker().col() + 1,
            }),
            None => Err(YamlError { message: "unexpected end of stream".to_owned(), line: 1, column: 1 }),
        }
    }

    fn node(&mut self, event: Event<'input>, span: Span) -> Result<Value, LoadError> {
        let (anchor, value) = match event {
            Event::Alias(id) => {
                return self.anchors.get(&id).cloned().ok_or_else(|| error_at("found undefined alias", span).into());
            }
            Event::Scalar(text, style, anchor, tag) => (anchor, self.scalar(&text, style, tag.as_deref(), span)?),
            Event::SequenceStart(anchor, tag) => {
                let items = self.sequence()?;
                (anchor, self.tagged_collection(Value::Seq(items), tag.as_deref(), span)?)
            }
            Event::MappingStart(anchor, tag) => {
                let map = self.mapping(span)?;
                (anchor, self.tagged_collection(Value::Map(map), tag.as_deref(), span)?)
            }
            _ => return Err(error_at("expected a node", span).into()),
        };
        if anchor != 0 {
            self.anchors.insert(anchor, value.clone());
        }
        Ok(value)
    }

    fn scalar(&mut self, text: &str, style: ScalarStyle, tag: Option<&Tag>, span: Span) -> Result<Value, LoadError> {
        let construct_error = |message: String| LoadError::Yaml(error_at(message, span));
        let Some(tag) = tag.filter(|tag| !is_non_specific(tag)) else {
            // The non-specific tag `!` leaves plain scalars to implicit resolution.
            return if style == ScalarStyle::Plain {
                resolve_plain(text).map_err(construct_error)
            } else {
                Ok(Value::Str(text.to_owned()))
            };
        };
        let name = tag_name(tag);
        match name.strip_prefix(CORE_TAG_PREFIX) {
            Some("str") => Ok(Value::Str(text.to_owned())),
            Some("null") => Ok(Value::Null),
            Some("bool") => construct_bool(text).map_err(construct_error),
            Some("int") => construct_int(text).map_err(construct_error),
            Some("float") => construct_float(text).map_err(construct_error),
            Some("timestamp") => construct_timestamp(text).map_err(construct_error),
            _ => self.custom(&name, TaggedNode::Scalar(text), span),
        }
    }

    fn tagged_collection(&mut self, value: Value, tag: Option<&Tag>, span: Span) -> Result<Value, LoadError> {
        let Some(tag) = tag.filter(|tag| !is_non_specific(tag)) else {
            return Ok(value);
        };
        let name = tag_name(tag);
        match (name.strip_prefix(CORE_TAG_PREFIX), &value) {
            (Some("seq"), Value::Seq(_)) | (Some("map"), Value::Map(_)) => Ok(value),
            (_, Value::Seq(_)) => self.custom(&name, TaggedNode::Sequence, span),
            _ => self.custom(&name, TaggedNode::Mapping, span),
        }
    }

    fn custom(&mut self, name: &str, node: TaggedNode<'_>, span: Span) -> Result<Value, LoadError> {
        match (self.handler)(name, node) {
            Some(result) => result.map_err(LoadError::Custom),
            None => Err(error_at(format!("could not determine a constructor for the tag '{name}'"), span).into()),
        }
    }

    fn sequence(&mut self) -> Result<Vec<Value>, LoadError> {
        let mut items = Vec::new();
        loop {
            let (event, span) = self.next_event()?;
            if matches!(event, Event::SequenceEnd) {
                return Ok(items);
            }
            items.push(self.node(event, span)?);
        }
    }

    fn mapping(&mut self, start: Span) -> Result<Mapping, LoadError> {
        let mut own: Vec<(Value, Value)> = Vec::new();
        let mut merged: Vec<(Value, Value)> = Vec::new();
        loop {
            let (event, span) = self.next_event()?;
            if matches!(event, Event::MappingEnd) {
                break;
            }
            let is_merge = matches!(&event, Event::Scalar(text, ScalarStyle::Plain, _, None) if implicit_kind(text) == Implicit::Merge);
            if is_merge {
                let (value_event, value_span) = self.next_event()?;
                let value = self.node(value_event, value_span)?;
                merged.extend(merge_pairs(value, value_span)?);
                continue;
            }
            let key = self.node(event, span)?;
            let (value_event, value_span) = self.next_event()?;
            let value = self.node(value_event, value_span)?;
            if matches!(key, Value::Seq(_) | Value::Map(_)) {
                return Err(LoadError::Yaml(YamlError {
                    message: "while constructing a mapping\nfound unhashable key".to_owned(),
                    line: start.start.line(),
                    column: start.start.col() + 1,
                }));
            } else {
                own.push((key, value));
            }
        }
        // Merged pairs come first and explicit keys override them, keeping the
        // position of the first occurrence, exactly like assigning into a dict.
        let mut map = Mapping::with_capacity(merged.len() + own.len());
        for (key, value) in merged.into_iter().chain(own) {
            match python_equal_key(&map, &key) {
                Some(index) => map[index] = value,
                None => {
                    map.insert(key, value);
                }
            }
        }
        Ok(map)
    }
}

/// Index of an existing key that Python considers equal to `key` although the
/// variants differ: `1 == 1.0 == True` share one dict slot.
fn python_equal_key(map: &Mapping, key: &Value) -> Option<usize> {
    let number = numeric(key)?;
    map.keys().position(|existing| existing != key && numeric(existing) == Some(number))
}

fn numeric(value: &Value) -> Option<f64> {
    match value {
        Value::Bool(flag) => Some(f64::from(u8::from(*flag))),
        Value::Int(number) => Some(*number as f64),
        Value::Float(number) => Some(*number),
        _ => None,
    }
}

fn merge_pairs(value: Value, span: Span) -> Result<Vec<(Value, Value)>, YamlError> {
    match value {
        Value::Map(map) => Ok(map.into_iter().collect()),
        Value::Seq(items) => {
            let mut pairs = Vec::new();
            // Earlier mappings in the list take precedence, so they go last.
            for item in items.into_iter().rev() {
                match item {
                    Value::Map(map) => pairs.extend(map),
                    _ => return Err(error_at("expected a mapping for merging", span)),
                }
            }
            Ok(pairs)
        }
        _ => Err(error_at("expected a mapping or list of mappings for merging", span)),
    }
}

fn is_non_specific(tag: &Tag) -> bool {
    tag.handle.is_empty() && tag.suffix == "!"
}

fn tag_name(tag: &Tag) -> String {
    format!("{}{}", tag.handle, tag.suffix)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn loaded(text: &str) -> Value {
        load(text).unwrap()
    }

    #[test]
    fn scalars_and_collections() {
        let value = loaded("a: yes\nb: '1'\nc: [1, 2.5, ~]\nd:\n  e: 0x10\n");
        assert_eq!(value, json!({"a": true, "b": "1", "c": [1, 2.5, null], "d": {"e": 16}}).into());
    }

    #[test]
    fn explicit_tags() {
        let value = loaded("a: !!str 1\nb: !!int '010'\nc: !!float 1\nd: ! 12\ne: ! '12'\n");
        assert_eq!(value, json!({"a": "1", "b": 8, "c": 1.0, "d": 12, "e": "12"}).into());
    }

    #[test]
    fn anchors_and_merge_keys() {
        let value = loaded("base: &b {x: 1, y: 2}\nother: &o {y: 9, z: 3}\nm:\n  <<: [*b, *o]\n  x: 5\n  w: 0\n");
        let merged = &value["m"];
        assert_eq!(merged, &json!({"y": 2, "z": 3, "x": 5, "w": 0}).into());
        let keys: Vec<String> = merged.as_map().unwrap().keys().map(Value::py_str).collect();
        assert_eq!(keys, ["y", "z", "x", "w"]);
    }

    #[test]
    fn duplicate_keys_keep_first_position_and_last_value() {
        let value = loaded("a: 1\nb: 2\na: 3\n");
        assert_eq!(value.py_repr(), "{'a': 3, 'b': 2}");
    }

    #[test]
    fn non_string_keys() {
        assert_eq!(loaded("1: a\ntrue: b\n~: c\n").py_repr(), "{1: 'b', None: 'c'}");
    }

    #[test]
    fn documents() {
        assert_eq!(loaded(""), Value::Null);
        assert_eq!(load_all("a: 1\n---\n- 2\n").unwrap().len(), 2);
        assert!(load("a: 1\n---\nb: 2\n").unwrap_err().message.starts_with("expected a single document"));
    }

    #[test]
    fn unknown_tags_are_rejected() {
        let err = load("a: !!python/name:os.system x\n").unwrap_err();
        assert_eq!(
            err.message,
            "could not determine a constructor for the tag 'tag:yaml.org,2002:python/name:os.system'"
        );
    }

    #[test]
    fn custom_tags_reach_the_handler() {
        let mut seen = Vec::new();
        let value = load_all_with("a: !include x.yml\n", &mut |tag, node| {
            seen.push(format!("{tag} {node:?}"));
            Some(Ok(Value::Int(7)))
        })
        .unwrap();
        assert_eq!(value[0], json!({"a": 7}).into());
        assert_eq!(seen, ["!include Scalar(\"x.yml\")"]);
    }

    #[test]
    fn list_items_keep_their_source_text() {
        let items = list_item_sources("[one, 'two', \"3\", 4, {a: 1}, [x, y], !!int '5']").unwrap().unwrap();
        assert_eq!(items, ["one", "two", "3", "4", "{a: 1}", "[x, y]", "!!int '5'"]);
        assert_eq!(list_item_sources("- a\n- 'b c'\n").unwrap().unwrap(), ["a", "b c"]);
        assert_eq!(list_item_sources("[]").unwrap().unwrap(), Vec::<String>::new());
        assert_eq!(list_item_sources("scalar").unwrap(), None);
    }

    #[test]
    fn syntax_errors_carry_a_position() {
        let err = load("a: b: c").unwrap_err();
        assert_eq!((err.line, err.column), (1, 5));
    }
}
