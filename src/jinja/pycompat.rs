//! Python methods callable on template values (`name.lower()`, `d.items()`, ...).
//!
//! `minijinja-contrib` covers the common ones; this adds string methods templates
//! use that it lacks.

use minijinja::value::{from_args, Value as JinjaValue};
use minijinja::{Error, ErrorKind, State};

/// Resolve a method the engine does not know natively.
pub(super) fn unknown_method_callback(
    state: &mut State,
    value: &JinjaValue,
    method: &str,
    args: &[JinjaValue],
) -> Result<JinjaValue, Error> {
    if let Some(text) = value.as_str() {
        if let Some(result) = string_method(text, method, args) {
            return result;
        }
    }
    minijinja_contrib::pycompat::unknown_method_callback(state, value, method, args)
}

fn pad(text: &str, args: &[JinjaValue], left: usize, right: usize) -> Result<JinjaValue, Error> {
    let (width, fill): (usize, Option<char>) = from_args(args)?;
    let missing = width.saturating_sub(text.chars().count());
    let fill = fill.unwrap_or(' ').to_string();
    // `center` puts the odd extra character on the left, like CPython.
    let before = match (left, right) {
        (1, 1) => missing / 2 + (missing & width & 1),
        (1, 0) => missing,
        _ => 0,
    };
    Ok(JinjaValue::from(format!("{}{text}{}", fill.repeat(before), fill.repeat(missing - before))))
}

fn string_method(text: &str, method: &str, args: &[JinjaValue]) -> Option<Result<JinjaValue, Error>> {
    let one_text = || from_args::<(String,)>(args).map(|(argument,)| argument);
    Some(match method {
        "center" => pad(text, args, 1, 1),
        "rjust" => pad(text, args, 1, 0),
        "ljust" => pad(text, args, 0, 1),
        "zfill" => from_args::<(usize,)>(args).map(|(width,)| {
            let (sign, digits) = match text.strip_prefix(['+', '-']) {
                Some(digits) => (&text[..1], digits),
                None => ("", text),
            };
            let zeros = width.saturating_sub(text.chars().count());
            JinjaValue::from(format!("{sign}{}{digits}", "0".repeat(zeros)))
        }),
        "removeprefix" => one_text().map(|prefix| JinjaValue::from(text.strip_prefix(prefix.as_str()).unwrap_or(text))),
        "removesuffix" => one_text().map(|suffix| JinjaValue::from(text.strip_suffix(suffix.as_str()).unwrap_or(text))),
        "partition" | "rpartition" => one_text().and_then(|separator| {
            if separator.is_empty() {
                return Err(Error::new(ErrorKind::InvalidOperation, "empty separator"));
            }
            let found = if method == "partition" { text.split_once(&separator) } else { text.rsplit_once(&separator) };
            Ok(JinjaValue::from(match (found, method) {
                (Some((head, tail)), _) => vec![head.to_owned(), separator, tail.to_owned()],
                (None, "partition") => vec![text.to_owned(), String::new(), String::new()],
                (None, _) => vec![String::new(), String::new(), text.to_owned()],
            }))
        }),
        "casefold" => Ok(JinjaValue::from(text.to_lowercase())),
        "swapcase" => Ok(JinjaValue::from(
            text.chars()
                .map(|ch| if ch.is_uppercase() { ch.to_lowercase().to_string() } else { ch.to_uppercase().to_string() })
                .collect::<String>(),
        )),
        // Text is always Unicode here, so encoding and decoding change nothing visible.
        "encode" | "decode" => Ok(JinjaValue::from(text)),
        _ => return None,
    })
}
