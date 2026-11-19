//! Conversion of raw answers to the type a question declares, following the
//! Python functions in Copier's `CAST_STR_TO_NATIVE` table.

use crate::errors::{CopierError, Result};
use crate::tools::{cast_to_bool, cast_to_str, py_float, py_int};
use crate::value::Value;
use crate::{json, yaml};

/// Types a question may declare.
pub const TYPE_NAMES: &[&str] = &["bool", "float", "int", "json", "str", "yaml", "path"];

/// Cast `answer` to the question type `type_name`.
///
/// # Errors
///
/// Returns [`CopierError::Value`] where Python raises `ValueError` (unparsable
/// text) and [`CopierError::Type`] where it raises `TypeError` or `AttributeError`
/// (a value of the wrong kind).
pub fn cast(type_name: &str, answer: &Value) -> Result<Value> {
    match type_name {
        "bool" => Ok(Value::Bool(cast_to_bool(answer))),
        "int" => cast_int(answer),
        "float" => cast_float(answer),
        "str" => cast_to_str(answer).map(Value::Str),
        "path" => Ok(Value::Str(answer.py_str())),
        "json" => match answer {
            Value::Str(text) => json::loads(text).map_err(CopierError::Value),
            other => Err(wrong_kind("the JSON object must be str, bytes or bytearray", other)),
        },
        "yaml" => match answer {
            Value::Str(text) => yaml::load(text).map_err(|err| CopierError::Value(err.to_string())),
            other => Err(wrong_kind("a YAML document must be text", other)),
        },
        other => Err(CopierError::InvalidType(format!("Unsupported type \"{other}\""))),
    }
}

fn wrong_kind(message: &str, value: &Value) -> CopierError {
    CopierError::Type(format!("{message}, not {}", value.py_type_name()))
}

fn cast_int(answer: &Value) -> Result<Value> {
    match answer {
        Value::Bool(flag) => Ok(Value::Int(i64::from(*flag))),
        Value::Int(number) => Ok(Value::Int(*number)),
        Value::Float(number) if number.is_finite() => Ok(Value::Int(number.trunc() as i64)),
        Value::Float(number) => Err(CopierError::Value(format!(
            "cannot convert float {} to integer",
            if number.is_nan() { "NaN" } else { "infinity" }
        ))),
        Value::Str(text) => py_int(text)
            .map(Value::Int)
            .ok_or_else(|| CopierError::Value(format!("invalid literal for int() with base 10: {}", answer.py_repr()))),
        other => Err(CopierError::Type(format!(
            "int() argument must be a string, a bytes-like object or a real number, not '{}'",
            other.py_type_name()
        ))),
    }
}

fn cast_float(answer: &Value) -> Result<Value> {
    match answer {
        Value::Bool(flag) => Ok(Value::Float(f64::from(u8::from(*flag)))),
        Value::Int(number) => Ok(Value::Float(*number as f64)),
        Value::Float(number) => Ok(Value::Float(*number)),
        Value::Str(text) => py_float(text)
            .map(Value::Float)
            .ok_or_else(|| CopierError::Value(format!("could not convert string to float: {}", answer.py_repr()))),
        other => Err(CopierError::Type(format!(
            "float() argument must be a string or a real number, not '{}'",
            other.py_type_name()
        ))),
    }
}

/// Python's `==` between answers: numbers compare by value across `bool`, `int`
/// and `float`; everything else needs the same type.
pub fn py_eq(a: &Value, b: &Value) -> bool {
    fn number(value: &Value) -> Option<f64> {
        match value {
            Value::Bool(flag) => Some(f64::from(u8::from(*flag))),
            Value::Int(number) => Some(*number as f64),
            Value::Float(number) => Some(*number),
            _ => None,
        }
    }
    match (a, b) {
        (Value::Seq(a), Value::Seq(b)) => a.len() == b.len() && a.iter().zip(b).all(|(a, b)| py_eq(a, b)),
        (Value::Map(a), Value::Map(b)) => {
            a.len() == b.len() && a.iter().all(|(key, value)| b.get(key).is_some_and(|other| py_eq(value, other)))
        }
        _ => match (number(a), number(b)) {
            (Some(a), Some(b)) => a == b,
            (None, None) => a == b,
            _ => false,
        },
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn value(json: serde_json::Value) -> Value {
        json.into()
    }

    #[test]
    fn casting_follows_python() {
        assert_eq!(cast("int", &value(json!("1_000"))).unwrap(), Value::Int(1000));
        assert_eq!(cast("int", &value(json!(true))).unwrap(), Value::Int(1));
        assert_eq!(cast("int", &value(json!(1.9))).unwrap(), Value::Int(1));
        assert!(matches!(cast("int", &value(json!("1.0"))), Err(CopierError::Value(_))));
        assert!(matches!(cast("int", &value(json!([1]))), Err(CopierError::Type(_))));
        assert_eq!(cast("float", &value(json!("1"))).unwrap(), Value::Float(1.0));
        assert_eq!(cast("str", &value(json!(1.5))).unwrap(), Value::from("1.5"));
        assert_eq!(cast("str", &value(json!(true))).unwrap(), Value::from("True"));
        assert!(matches!(cast("str", &value(json!({"a": 1}))), Err(CopierError::Value(_))));
        assert_eq!(cast("json", &value(json!("[1, 2]"))).unwrap(), value(json!([1, 2])));
        assert!(matches!(cast("json", &value(json!([1]))), Err(CopierError::Type(_))));
        assert_eq!(cast("yaml", &value(json!("a: yes"))).unwrap(), value(json!({"a": true})));
        assert_eq!(cast("bool", &value(json!("no"))).unwrap(), Value::Bool(false));
        assert_eq!(cast("path", &value(json!(3))).unwrap(), Value::from("3"));
    }

    #[test]
    fn python_equality() {
        assert!(py_eq(&Value::Int(1), &Value::Float(1.0)));
        assert!(py_eq(&Value::Bool(true), &Value::Int(1)));
        assert!(!py_eq(&Value::from("1"), &Value::Int(1)));
        assert!(py_eq(&value(json!([1, "a"])), &value(json!([1.0, "a"]))));
    }
}
