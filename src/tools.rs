//! Small helpers, mirroring `copier/_tools.py`.

use crate::errors::{CopierError, Result};
use crate::value::Value;

/// Operating system name as templates see it in `_copier_conf.os`.
pub const OS: Option<&str> = if cfg!(target_os = "linux") {
    Some("linux")
} else if cfg!(target_os = "macos") {
    Some("macos")
} else if cfg!(windows) {
    Some("windows")
} else {
    None
};

/// Python's `float(str)`: surrounding whitespace, `_` between digits, `inf`,
/// `infinity` and `nan` are accepted.
pub fn py_float(text: &str) -> Option<f64> {
    let trimmed = text.trim();
    let digits = trimmed.trim_start_matches(['+', '-']);
    if digits.is_empty()
        || digits.starts_with('_')
        || digits.ends_with('_')
        || digits.contains("__")
    {
        return None;
    }
    let bytes = trimmed.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        // An underscore must sit between two digits.
        if *byte == b'_'
            && !(bytes[index - 1].is_ascii_digit()
                && bytes.get(index + 1).is_some_and(u8::is_ascii_digit))
        {
            return None;
        }
    }
    let cleaned = trimmed.replace('_', "");
    // Rust also accepts these spellings, but Python is the reference for the rest.
    if cleaned
        .chars()
        .any(|ch| !(ch.is_ascii_alphanumeric() || "+-.".contains(ch)))
    {
        return None;
    }
    cleaned.parse::<f64>().ok()
}

/// Python's `int(str)`: surrounding whitespace and `_` between digits are accepted;
/// prefixes such as `0x` are not.
pub fn py_int(text: &str) -> Option<i64> {
    let trimmed = text.trim();
    let digits = trimmed.strip_prefix(['+', '-']).unwrap_or(trimmed);
    if digits.is_empty()
        || digits.starts_with('_')
        || digits.ends_with('_')
        || digits.contains("__")
    {
        return None;
    }
    if !digits
        .bytes()
        .all(|byte| byte.is_ascii_digit() || byte == b'_')
    {
        return None;
    }
    trimmed.replace('_', "").parse::<i64>().ok()
}

/// Parse anything into a boolean the way Copier answers do.
///
/// Numbers are true when non-zero; `y/yes/t/true/on` and `n/no/f/false/off/~/null/none`
/// are recognized case-insensitively; anything else uses Python truthiness.
pub fn cast_to_bool(value: &Value) -> bool {
    match value {
        Value::Bool(flag) => *flag,
        Value::Int(number) => *number != 0,
        Value::Float(number) => *number != 0.0,
        Value::Str(text) => {
            if let Some(number) = py_float(text) {
                return number != 0.0;
            }
            match text.trim().to_lowercase().as_str() {
                "" | "n" | "no" | "f" | "false" | "off" | "~" | "null" | "none" => false,
                // Any other non-empty text is truthy, including the explicit yes-words.
                _ => true,
            }
        }
        other => other.is_truthy(),
    }
}

/// Parse a scalar into text; collections and nulls are rejected.
///
/// # Errors
///
/// Returns [`CopierError::Value`] for values Python's `cast_to_str` refuses.
pub fn cast_to_str(value: &Value) -> Result<String> {
    match value {
        Value::Str(text) => Ok(text.clone()),
        Value::Int(_) | Value::Float(_) | Value::Bool(_) => Ok(value.py_str()),
        other => Err(CopierError::Value(format!(
            "Could not convert {} to string",
            other.py_str()
        ))),
    }
}

/// Append `end` unless `text` already ends with it.
pub fn force_str_end(text: &str, end: &str) -> String {
    if text.ends_with(end) {
        text.to_owned()
    } else {
        format!("{text}{end}")
    }
}

/// Split NUL-delimited command output, dropping the trailing empty item.
pub fn split_nul(text: &str) -> Vec<String> {
    let mut items: Vec<String> = text.split('\0').map(str::to_owned).collect();
    if items.last().is_some_and(String::is_empty) {
        items.pop();
    }
    items
}

/// Escape a path so git treats it as a literal wildmatch pattern.
pub fn escape_git_path(path: &str) -> String {
    let mut escaped = String::with_capacity(path.len());
    for ch in path.chars() {
        if "\\[]!*#?".contains(ch) {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    let leading = escaped.len() - escaped.trim_start().len();
    let trailing_start = escaped.trim_end().len().max(leading);
    let quote = |blank: &str| blank.chars().flat_map(|ch| ['\\', ch]).collect::<String>();
    format!(
        "{}{}{}",
        quote(&escaped[..leading]),
        &escaped[leading..trailing_start],
        quote(&escaped[trailing_start..])
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_number_parsing() {
        assert_eq!(py_float(" 1_000.5 "), Some(1000.5));
        assert_eq!(py_float("1e3"), Some(1000.0));
        assert_eq!(py_float("-inf"), Some(f64::NEG_INFINITY));
        assert_eq!(py_float("_1"), None);
        assert_eq!(py_float("1__0"), None);
        assert_eq!(py_float("yes"), None);
        assert_eq!(py_int(" 1_000 "), Some(1000));
        assert_eq!(py_int("0x10"), None);
        assert_eq!(py_int("1.0"), None);
        assert_eq!(py_int("-7"), Some(-7));
    }

    #[test]
    fn git_path_escaping() {
        assert_eq!(escape_git_path("a[b]*.txt"), "a\\[b\\]\\*.txt");
        assert_eq!(escape_git_path(" a b "), "\\ a b\\ ");
        assert_eq!(escape_git_path("back\\slash"), "back\\\\slash");
    }
}
