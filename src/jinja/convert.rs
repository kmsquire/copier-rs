//! Conversion between Copier's [`Value`] and the template engine's values.

use minijinja::value::{Value as JinjaValue, ValueKind};

use crate::value::{Mapping, Value};

/// Convert a Copier value into a template value.
pub fn to_jinja(value: &Value) -> JinjaValue {
    match value {
        Value::Null => JinjaValue::from(()),
        Value::Bool(flag) => JinjaValue::from(*flag),
        Value::Int(number) => JinjaValue::from(*number),
        Value::Float(number) => JinjaValue::from(*number),
        Value::Str(text) | Value::Timestamp(text) => JinjaValue::from(text.as_str()),
        Value::Seq(items) => JinjaValue::from(items.iter().map(to_jinja).collect::<Vec<_>>()),
        Value::Map(map) => map_to_jinja(map),
    }
}

/// Convert a Copier mapping into a template mapping, keeping key order.
pub fn map_to_jinja(map: &Mapping) -> JinjaValue {
    JinjaValue::from_pairs(
        map.iter()
            .map(|(key, value)| (to_jinja(key), to_jinja(value))),
    )
}

/// Convert a template value back into a Copier value. Undefined becomes `Null`
/// and engine objects become their string form.
pub fn from_jinja(value: &JinjaValue) -> Value {
    match value.kind() {
        ValueKind::Undefined | ValueKind::None => Value::Null,
        ValueKind::Bool => Value::Bool(value.is_true()),
        ValueKind::Number => match value.as_i64() {
            Some(number) if value.is_integer() => Value::Int(number),
            _ => f64::try_from(value.clone())
                .map_or_else(|_| Value::Str(value.to_string()), Value::Float),
        },
        ValueKind::String => Value::Str(value.as_str().unwrap_or_default().to_owned()),
        ValueKind::Bytes => {
            Value::Str(String::from_utf8_lossy(value.as_bytes().unwrap_or_default()).into_owned())
        }
        ValueKind::Seq | ValueKind::Iterable => match value.try_iter() {
            Ok(items) => Value::Seq(items.map(|item| from_jinja(&item)).collect()),
            Err(_) => Value::Str(value.to_string()),
        },
        ValueKind::Map => match value.try_iter() {
            Ok(keys) => Value::Map(
                keys.map(|key| {
                    let item = value.get_item(&key).unwrap_or_default();
                    (from_jinja(&key), from_jinja(&item))
                })
                .collect(),
            ),
            Err(_) => Value::Str(value.to_string()),
        },
        _ => Value::Str(value.to_string()),
    }
}
