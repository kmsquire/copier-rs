//! Implicit typing of plain scalars, as PyYAML's `SafeLoader` resolves and
//! constructs them (YAML 1.1).

use std::sync::LazyLock;

use regex::Regex;

use crate::value::Value;

fn anchored(pattern: &str) -> Regex {
    // Patterns are literals from PyYAML's resolver, so they always compile.
    #[allow(clippy::expect_used)]
    Regex::new(&format!("^(?:{pattern})$")).expect("valid resolver regex")
}

static BOOL: LazyLock<Regex> =
    LazyLock::new(|| anchored("yes|Yes|YES|no|No|NO|true|True|TRUE|false|False|FALSE|on|On|ON|off|Off|OFF"));
static INT: LazyLock<Regex> = LazyLock::new(|| {
    anchored(
        r"[-+]?0b[0-1_]+|[-+]?0[0-7_]+|[-+]?(?:0|[1-9][0-9_]*)|[-+]?0x[0-9a-fA-F_]+|[-+]?[1-9][0-9_]*(?::[0-5]?[0-9])+",
    )
});
static FLOAT: LazyLock<Regex> = LazyLock::new(|| {
    anchored(
        r"[-+]?(?:[0-9][0-9_]*)\.[0-9_]*(?:[eE][-+][0-9]+)?|\.[0-9][0-9_]*(?:[eE][-+][0-9]+)?|[-+]?[0-9][0-9_]*(?::[0-5]?[0-9])+\.[0-9_]*|[-+]?\.(?:inf|Inf|INF)|\.(?:nan|NaN|NAN)",
    )
});
static TIMESTAMP: LazyLock<Regex> = LazyLock::new(|| {
    anchored(
        r"([0-9]{4})-([0-9][0-9]?)-([0-9][0-9]?)(?:(?:[Tt]|[ \t]+)([0-9][0-9]?):([0-9][0-9]):([0-9][0-9])(?:\.([0-9]*))?(?:[ \t]*(Z|([-+])([0-9][0-9]?)(?::([0-9][0-9]))?))?)?",
    )
});
static DATE: LazyLock<Regex> = LazyLock::new(|| anchored("[0-9]{4}-[0-9]{2}-[0-9]{2}"));

/// What kind of value a plain scalar denotes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Implicit {
    Null,
    Bool,
    Int,
    Float,
    Timestamp,
    Merge,
    /// The YAML 1.1 "default value" scalar `=`, which the safe loader cannot construct.
    DefaultValue,
    Str,
}

/// Classify a plain scalar by PyYAML's implicit resolvers.
pub(crate) fn implicit_kind(text: &str) -> Implicit {
    match text {
        "" | "~" | "null" | "Null" | "NULL" => return Implicit::Null,
        "<<" => return Implicit::Merge,
        "=" => return Implicit::DefaultValue,
        _ => {}
    }
    if BOOL.is_match(text) {
        Implicit::Bool
    } else if INT.is_match(text) {
        Implicit::Int
    } else if FLOAT.is_match(text) {
        Implicit::Float
    } else if is_timestamp(text) {
        Implicit::Timestamp
    } else {
        Implicit::Str
    }
}

fn is_timestamp(text: &str) -> bool {
    // A bare date needs two-digit month and day; with a time part one digit suffices.
    DATE.is_match(text) || (TIMESTAMP.is_match(text) && text.len() > 10)
}

/// Resolve a plain scalar to a value; `Err` carries PyYAML's complaint.
pub(crate) fn resolve_plain(text: &str) -> Result<Value, String> {
    match implicit_kind(text) {
        Implicit::Null => Ok(Value::Null),
        Implicit::Bool => construct_bool(text),
        Implicit::Int => construct_int(text),
        Implicit::Float => construct_float(text),
        Implicit::Timestamp => construct_timestamp(text),
        Implicit::Merge => Err("could not determine a constructor for the tag 'tag:yaml.org,2002:merge'".to_owned()),
        Implicit::DefaultValue => {
            Err("could not determine a constructor for the tag 'tag:yaml.org,2002:value'".to_owned())
        }
        Implicit::Str => Ok(Value::Str(text.to_owned())),
    }
}

/// Construct a `!!bool` scalar.
pub(crate) fn construct_bool(text: &str) -> Result<Value, String> {
    match text.to_lowercase().as_str() {
        "yes" | "true" | "on" => Ok(Value::Bool(true)),
        "no" | "false" | "off" => Ok(Value::Bool(false)),
        _ => Err(format!("invalid boolean: {text:?}")),
    }
}

fn split_sign(text: &str) -> (bool, &str) {
    match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    }
}

/// Construct a `!!int` scalar: binary, octal (leading `0`), hex, sexagesimal, decimal.
pub(crate) fn construct_int(text: &str) -> Result<Value, String> {
    let cleaned = text.replace('_', "");
    let (negative, digits) = split_sign(&cleaned);
    let invalid = || format!("invalid literal for int(): {text:?}");
    let magnitude: i128 = if digits == "0" {
        0
    } else if let Some(binary) = digits.strip_prefix("0b") {
        i128::from_str_radix(binary, 2).map_err(|_| invalid())?
    } else if let Some(hex) = digits.strip_prefix("0x") {
        i128::from_str_radix(hex, 16).map_err(|_| invalid())?
    } else if digits.starts_with('0') {
        i128::from_str_radix(digits, 8).map_err(|_| invalid())?
    } else if digits.contains(':') {
        let mut total: i128 = 0;
        for part in digits.split(':') {
            total = total * 60 + part.parse::<i128>().map_err(|_| invalid())?;
        }
        total
    } else {
        match digits.parse::<i128>() {
            Ok(number) => number,
            // Beyond 128 bits only the magnitude survives; see notes/BACKLOG.md.
            Err(_) => {
                return digits
                    .parse::<f64>()
                    .map(|f| Value::Float(if negative { -f } else { f }))
                    .map_err(|_| invalid())
            }
        }
    };
    let signed = if negative { -magnitude } else { magnitude };
    Ok(i64::try_from(signed).map_or(Value::Float(signed as f64), Value::Int))
}

/// Construct a `!!float` scalar: `.inf`, `.nan`, sexagesimal, decimal.
pub(crate) fn construct_float(text: &str) -> Result<Value, String> {
    let cleaned = text.replace('_', "").to_lowercase();
    let (negative, digits) = split_sign(&cleaned);
    let invalid = || format!("could not convert string to float: {text:?}");
    let magnitude = if digits == ".inf" {
        f64::INFINITY
    } else if digits == ".nan" {
        f64::NAN
    } else if digits.contains(':') {
        let mut total = 0.0;
        for part in digits.split(':') {
            total = total * 60.0 + part.parse::<f64>().map_err(|_| invalid())?;
        }
        total
    } else {
        digits.parse::<f64>().map_err(|_| invalid())?
    };
    Ok(Value::Float(if negative { -magnitude } else { magnitude }))
}

/// Construct a `!!timestamp` scalar, normalized to Python's `str()` of the
/// resulting `date` or `datetime`.
pub(crate) fn construct_timestamp(text: &str) -> Result<Value, String> {
    let captures = TIMESTAMP.captures(text).ok_or_else(|| format!("invalid timestamp: {text:?}"))?;
    let number = |index: usize| captures.get(index).and_then(|m| m.as_str().parse::<u32>().ok());
    let (year, month, day) = (number(1).unwrap_or(0), number(2).unwrap_or(0), number(3).unwrap_or(0));
    if !(1..=12).contains(&month) {
        return Err("month must be in 1..12".to_owned());
    }
    if !(1..=31).contains(&day) {
        return Err("day is out of range for month".to_owned());
    }
    let mut out = format!("{year:04}-{month:02}-{day:02}");
    let Some(hour) = number(4) else {
        return Ok(Value::Timestamp(out));
    };
    out.push_str(&format!(" {hour:02}:{:02}:{:02}", number(5).unwrap_or(0), number(6).unwrap_or(0)));
    if let Some(fraction) = captures.get(7) {
        let mut micros: String = fraction.as_str().chars().take(6).collect();
        while micros.len() < 6 {
            micros.push('0');
        }
        if micros != "000000" {
            out.push('.');
            out.push_str(&micros);
        }
    }
    if let Some(zone) = captures.get(8) {
        if zone.as_str() == "Z" {
            out.push_str("+00:00");
        } else {
            let sign = captures.get(9).map_or("+", |m| m.as_str());
            out.push_str(&format!("{sign}{:02}:{:02}", number(10).unwrap_or(0), number(11).unwrap_or(0)));
        }
    }
    Ok(Value::Timestamp(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaml_1_1_scalars() {
        let cases: &[(&str, Value)] = &[
            ("yes", Value::Bool(true)),
            ("Off", Value::Bool(false)),
            ("y", Value::from("y")),
            ("010", Value::Int(8)),
            ("0x1F", Value::Int(31)),
            ("0b11", Value::Int(3)),
            ("1_000", Value::Int(1000)),
            ("1:30", Value::Int(90)),
            ("-12", Value::Int(-12)),
            ("1.5", Value::Float(1.5)),
            ("1e5", Value::from("1e5")),
            ("1.5e3", Value::from("1.5e3")),
            ("1.5e+3", Value::Float(1500.0)),
            (".5", Value::Float(0.5)),
            ("-.inf", Value::Float(f64::NEG_INFINITY)),
            ("~", Value::Null),
            ("", Value::Null),
            ("None", Value::from("None")),
            ("2020-01-02", Value::Timestamp("2020-01-02".to_owned())),
            ("2020-1-2", Value::from("2020-1-2")),
            ("2001-12-14t21:59:43.10-05:00", Value::Timestamp("2001-12-14 21:59:43.100000-05:00".to_owned())),
            ("0o17", Value::from("0o17")),
            ("09", Value::from("09")),
        ];
        for (text, expected) in cases {
            assert_eq!(&resolve_plain(text).unwrap(), expected, "{text}");
        }
    }
}
