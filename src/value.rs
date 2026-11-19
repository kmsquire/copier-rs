//! Dynamic data model shared by answers, template configuration and YAML/JSON I/O.

use std::fmt;
use std::hash::{Hash, Hasher};

use indexmap::IndexMap;

/// Insertion-ordered mapping with arbitrary [`Value`] keys, like a Python `dict`.
pub type Mapping = IndexMap<Value, Value>;

/// A YAML/JSON-like value.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub enum Value {
    /// `null` / Python `None`.
    #[default]
    Null,
    /// Boolean.
    Bool(bool),
    /// Integer.
    Int(i64),
    /// Floating point number.
    Float(f64),
    /// Text.
    Str(String),
    /// A YAML timestamp, kept as Python's `str()` of the `date`/`datetime` it denotes.
    Timestamp(String),
    /// Ordered list.
    Seq(Vec<Value>),
    /// Insertion-ordered mapping.
    Map(Mapping),
}

impl Value {
    /// Build a mapping value from string keys.
    pub fn map<K: Into<String>, V: Into<Value>>(entries: impl IntoIterator<Item = (K, V)>) -> Self {
        Self::Map(
            entries
                .into_iter()
                .map(|(k, v)| (Self::Str(k.into()), v.into()))
                .collect(),
        )
    }

    /// Look up `key` in a mapping; `None` for other variants or missing keys.
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Self::Map(map) => map.get(&Self::Str(key.to_owned())),
            _ => None,
        }
    }

    /// Whether a mapping contains `key`.
    pub fn contains_key(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    /// The string slice, if this is a string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(text) => Some(text),
            _ => None,
        }
    }

    /// The boolean, if this is a boolean.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(flag) => Some(*flag),
            _ => None,
        }
    }

    /// The integer, if this is an integer.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Int(number) => Some(*number),
            _ => None,
        }
    }

    /// The float, if this is a float.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Float(number) => Some(*number),
            _ => None,
        }
    }

    /// The items, if this is a sequence.
    pub fn as_seq(&self) -> Option<&[Value]> {
        match self {
            Self::Seq(items) => Some(items),
            _ => None,
        }
    }

    /// The mapping, if this is a mapping.
    pub fn as_map(&self) -> Option<&Mapping> {
        match self {
            Self::Map(map) => Some(map),
            _ => None,
        }
    }

    /// Whether this is `Null`.
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// Python truthiness.
    pub fn is_truthy(&self) -> bool {
        match self {
            Self::Null => false,
            Self::Bool(flag) => *flag,
            Self::Int(number) => *number != 0,
            Self::Float(number) => *number != 0.0,
            Self::Str(text) => !text.is_empty(),
            Self::Timestamp(_) => true,
            Self::Seq(items) => !items.is_empty(),
            Self::Map(map) => !map.is_empty(),
        }
    }

    /// Name of the equivalent Python type, as `type(x).__name__` reports it.
    pub fn py_type_name(&self) -> &'static str {
        match self {
            Self::Null => "NoneType",
            Self::Bool(_) => "bool",
            Self::Int(_) => "int",
            Self::Float(_) => "float",
            Self::Str(_) => "str",
            Self::Timestamp(text) => {
                if text.len() > 10 {
                    "datetime"
                } else {
                    "date"
                }
            }
            Self::Seq(_) => "list",
            Self::Map(_) => "dict",
        }
    }

    /// Python `repr()` of the value; error messages embed it.
    pub fn py_repr(&self) -> String {
        match self {
            Self::Null => "None".to_owned(),
            Self::Bool(true) => "True".to_owned(),
            Self::Bool(false) => "False".to_owned(),
            Self::Int(number) => number.to_string(),
            Self::Float(number) => py_float_repr(*number),
            Self::Str(text) => py_str_repr(text),
            Self::Timestamp(text) => text.clone(),
            Self::Seq(items) => {
                let parts: Vec<String> = items.iter().map(Self::py_repr).collect();
                format!("[{}]", parts.join(", "))
            }
            Self::Map(map) => {
                let parts: Vec<String> = map
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k.py_repr(), v.py_repr()))
                    .collect();
                format!("{{{}}}", parts.join(", "))
            }
        }
    }

    /// Python `str()` of the value.
    pub fn py_str(&self) -> String {
        match self {
            Self::Str(text) => text.clone(),
            other => other.py_repr(),
        }
    }
}

/// Python `repr()` of a float (`1.0`, `1e+16`, `nan`, `inf`).
pub fn py_float_repr(number: f64) -> String {
    if number.is_nan() {
        return "nan".to_owned();
    }
    if number.is_infinite() {
        return if number > 0.0 { "inf" } else { "-inf" }.to_owned();
    }
    let magnitude = number.abs();
    if magnitude != 0.0 && !(1e-4..1e16).contains(&magnitude) {
        let text = format!("{number:e}");
        let (mantissa, exponent) = text.split_once('e').unwrap_or((&text, "0"));
        let exponent: i32 = exponent.parse().unwrap_or(0);
        let sign = if exponent < 0 { '-' } else { '+' };
        return format!("{mantissa}e{sign}{:02}", exponent.abs());
    }
    let text = number.to_string();
    if text.contains('.') {
        text
    } else {
        format!("{text}.0")
    }
}

/// Python `repr()` of a string: single quotes unless the text contains one and no
/// double quote.
pub fn py_str_repr(text: &str) -> String {
    let quote = if text.contains('\'') && !text.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::with_capacity(text.len() + 2);
    out.push(quote);
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\x{:02x}", c as u32))
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Null, Self::Null) => true,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Int(a), Self::Int(b)) => a == b,
            // Bitwise so that `Eq`/`Hash` hold and NaN keys stay addressable.
            (Self::Float(a), Self::Float(b)) => a.to_bits() == b.to_bits(),
            (Self::Str(a), Self::Str(b)) | (Self::Timestamp(a), Self::Timestamp(b)) => a == b,
            (Self::Seq(a), Self::Seq(b)) => a == b,
            (Self::Map(a), Self::Map(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for Value {}

impl Hash for Value {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Self::Null => {}
            Self::Bool(flag) => flag.hash(state),
            Self::Int(number) => number.hash(state),
            Self::Float(number) => number.to_bits().hash(state),
            Self::Str(text) | Self::Timestamp(text) => text.hash(state),
            Self::Seq(items) => items.hash(state),
            Self::Map(map) => {
                for (key, value) in map {
                    key.hash(state);
                    value.hash(state);
                }
            }
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.py_str())
    }
}

impl std::ops::Index<&str> for Value {
    type Output = Value;

    /// Index a mapping by string key; missing keys yield `Null`, like `serde_json`.
    fn index(&self, key: &str) -> &Value {
        static NULL: Value = Value::Null;
        self.get(key).unwrap_or(&NULL)
    }
}

impl From<bool> for Value {
    fn from(flag: bool) -> Self {
        Self::Bool(flag)
    }
}

impl From<i64> for Value {
    fn from(number: i64) -> Self {
        Self::Int(number)
    }
}

impl From<i32> for Value {
    fn from(number: i32) -> Self {
        Self::Int(number.into())
    }
}

impl From<f64> for Value {
    fn from(number: f64) -> Self {
        Self::Float(number)
    }
}

impl From<&str> for Value {
    fn from(text: &str) -> Self {
        Self::Str(text.to_owned())
    }
}

impl From<String> for Value {
    fn from(text: String) -> Self {
        Self::Str(text)
    }
}

impl<T: Into<Value>> From<Vec<T>> for Value {
    fn from(items: Vec<T>) -> Self {
        Self::Seq(items.into_iter().map(Into::into).collect())
    }
}

impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(value: Option<T>) -> Self {
        value.map_or(Self::Null, Into::into)
    }
}

impl From<serde_json::Value> for Value {
    fn from(json: serde_json::Value) -> Self {
        match json {
            serde_json::Value::Null => Self::Null,
            serde_json::Value::Bool(flag) => Self::Bool(flag),
            serde_json::Value::Number(number) => match number.as_i64() {
                Some(int) => Self::Int(int),
                None => Self::Float(number.as_f64().unwrap_or(f64::NAN)),
            },
            serde_json::Value::String(text) => Self::Str(text),
            serde_json::Value::Array(items) => {
                Self::Seq(items.into_iter().map(Into::into).collect())
            }
            serde_json::Value::Object(map) => Self::Map(
                map.into_iter()
                    .map(|(k, v)| (Self::Str(k), v.into()))
                    .collect(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repr_matches_python() {
        let value: Value =
            serde_json::json!({"a": [1, 2.0, "x", null, true], "it's": "q\"'"}).into();
        assert_eq!(
            value.py_repr(),
            r#"{'a': [1, 2.0, 'x', None, True], "it's": 'q"\''}"#
        );
        assert_eq!(py_float_repr(1e16), "1e+16");
        assert_eq!(py_float_repr(0.00001), "1e-05");
        assert_eq!(py_float_repr(-1.5), "-1.5");
    }

    #[test]
    fn index_and_get() {
        let value = Value::map([("a", 1)]);
        assert_eq!(value["a"], Value::Int(1));
        assert!(value["missing"].is_null());
        assert!(!value.contains_key("missing"));
    }
}
