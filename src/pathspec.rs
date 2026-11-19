//! Gitignore-style path matching, a port of the `pathspec` library's `gitignore`
//! patterns as Python Copier uses them for `_exclude` and `_skip_if_exists`.
//!
//! Matching is purely textual: the last pattern that matches a path decides, and a
//! pattern starting with `!` re-includes.

use regex::Regex;
use unicode_normalization::UnicodeNormalization;

use crate::errors::{CopierError, Result};

/// An ordered list of gitignore patterns.
#[derive(Debug, Clone, Default)]
pub struct PathSpec {
    patterns: Vec<(Regex, bool)>,
}

impl PathSpec {
    /// Compile `patterns`; blank lines and comments are skipped.
    ///
    /// Patterns are NFD-normalized first, like Copier does, so they match file
    /// names as macOS stores them.
    ///
    /// # Errors
    ///
    /// Returns [`CopierError::Value`] for patterns pathspec rejects.
    pub fn new<S: AsRef<str>>(patterns: impl IntoIterator<Item = S>) -> Result<Self> {
        let mut compiled = Vec::new();
        for pattern in patterns {
            let normalized: String = pattern.as_ref().nfd().collect();
            if let Some((regex, include)) = pattern_to_regex(&normalized)? {
                let regex = Regex::new(&regex).map_err(|_| {
                    CopierError::Value(format!("Invalid git pattern: {:?}", pattern.as_ref()))
                })?;
                compiled.push((regex, include));
            }
        }
        Ok(Self { patterns: compiled })
    }

    /// Whether `path` (relative, any separator) is matched.
    pub fn matches(&self, path: &str) -> bool {
        let normalized = path.replace('\\', "/");
        let normalized = normalized.strip_prefix("./").unwrap_or(&normalized);
        let mut matched = false;
        for (regex, include) in &self.patterns {
            if regex.is_match(normalized) {
                matched = *include;
            }
        }
        matched
    }
}

fn invalid(pattern: &str) -> CopierError {
    CopierError::Value(format!("Invalid git pattern: {pattern:?}"))
}

/// Translate one pattern into a regex and whether it includes (`true`) or
/// re-excludes (`false`); `None` for blank lines and comments.
fn pattern_to_regex(original: &str) -> Result<Option<(String, bool)>> {
    let pattern = if original.ends_with("\\ ") {
        original
    } else {
        original.trim_end()
    };
    if pattern.is_empty() || pattern.starts_with('#') {
        return Ok(None);
    }
    let (include, pattern) = match pattern.strip_prefix('!') {
        Some(rest) => (false, rest),
        None => (true, pattern),
    };
    let mut segments: Vec<&str> = pattern.split('/').collect();
    let is_dir_pattern = segments.last().is_some_and(|last| last.is_empty()) && pattern != "/";

    // A leading slash anchors the pattern; a pattern without inner slashes matches
    // at any depth.
    if segments[0].is_empty() {
        segments.remove(0);
    } else if (segments.len() == 1 || (segments.len() == 2 && segments[1].is_empty()))
        && segments[0] != "**"
    {
        segments.insert(0, "**");
    }
    if segments.is_empty() {
        return Err(invalid(original));
    }
    if let Some(last) = segments.last_mut().filter(|last| last.is_empty()) {
        *last = "**";
    }
    let mut index = segments.len() - 1;
    while index > 0 {
        if segments[index - 1] == "**" && segments[index] == "**" {
            segments.remove(index);
        }
        index -= 1;
    }
    let regex = match segments.as_slice() {
        ["**"] if is_dir_pattern => "/".to_owned(),
        ["**"] | ["**", "*"] => ".".to_owned(),
        ["**", "*", "**"] => "/".to_owned(),
        _ => translate_segments(&segments).ok_or_else(|| invalid(original))?,
    };
    Ok(Some((regex, include)))
}

fn translate_segments(segments: &[&str]) -> Option<String> {
    let mut out = String::new();
    let mut need_slash = false;
    let end = segments.len() - 1;
    for (index, segment) in segments.iter().enumerate() {
        if *segment == "**" {
            if index == 0 {
                out.push_str("^(?:.+/)?");
            } else if index < end {
                out.push_str("(?:/.+)?");
                need_slash = true;
            } else {
                out.push('/');
            }
            continue;
        }
        if index == 0 {
            out.push('^');
        }
        if need_slash {
            out.push('/');
        }
        if *segment == "*" {
            out.push_str("[^/]+");
        } else {
            out.push_str(&translate_glob(segment)?);
        }
        if index == end {
            out.push_str(if *segment == "*" { "/?$" } else { "(?:/|$)" });
        }
        need_slash = true;
    }
    Some(out)
}

/// Translate the glob of one path segment; `None` for a dangling escape.
fn translate_glob(pattern: &str) -> Option<String> {
    let chars: Vec<char> = pattern.chars().collect();
    let end = chars.len();
    let mut regex = String::new();
    let mut escape = false;
    let mut i = 0;
    while i < end {
        let ch = chars[i];
        i += 1;
        if escape {
            escape = false;
            regex.push_str(&regex::escape(&ch.to_string()));
        } else if ch == '\\' {
            escape = true;
        } else if ch == '*' {
            regex.push_str("[^/]*");
        } else if ch == '?' {
            regex.push_str("[^/]");
        } else if ch == '[' {
            let mut j = i;
            if j < end && (chars[j] == '!' || chars[j] == '^') {
                j += 1;
            }
            if j < end && chars[j] == ']' {
                j += 1;
            }
            while j < end && chars[j] != ']' {
                j += 1;
            }
            if j < end {
                j += 1;
                regex.push('[');
                if chars[i] == '!' || chars[i] == '^' {
                    regex.push('^');
                    i += 1;
                }
                let body: String = chars[i..j].iter().collect();
                // This regex engine also treats `[` and `&&`-style operators as
                // special inside a class.
                regex.push_str(
                    &body
                        .replace('\\', "\\\\")
                        .replace('[', "\\[")
                        .replace("&&", "&\\&")
                        .replace("~~", "~\\~")
                        .replace("--", "-\\-"),
                );
                i = j;
            } else {
                // An unterminated class is a literal bracket.
                regex.push_str("\\[");
            }
        } else {
            regex.push_str(&regex::escape(&ch.to_string()));
        }
    }
    if escape {
        return None;
    }
    Some(regex)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn regex_of(pattern: &str) -> Option<(String, bool)> {
        pattern_to_regex(pattern).unwrap()
    }

    #[test]
    fn translation_matches_pathspec() {
        // Expected values produced by pathspec 1.1.1 (`GitIgnoreBasicPattern.pattern_to_regex`).
        let cases: &[(&str, Option<(&str, bool)>)] = &[
            ("", None),
            ("# comment", None),
            ("copier.yml", Some(("^(?:.+/)?copier\\.yml(?:/|$)", true))),
            ("~*", Some(("^(?:.+/)?\\~[^/]*(?:/|$)", true))),
            ("*.py[co]", Some(("^(?:.+/)?[^/]*\\.py[co](?:/|$)", true))),
            ("!keep.txt", Some(("^(?:.+/)?keep\\.txt(?:/|$)", false))),
            ("/root.txt", Some(("^root\\.txt(?:/|$)", true))),
            ("docs/", Some(("^(?:.+/)?docs/", true))),
            ("a/b", Some(("^a/b(?:/|$)", true))),
            ("a/**/b", Some(("^a(?:/.+)?/b(?:/|$)", true))),
            ("**/b", Some(("^(?:.+/)?b(?:/|$)", true))),
            ("a/**", Some(("^a/", true))),
            ("*", Some((".", true))),
            ("**", Some((".", true))),
            ("*/", Some(("/", true))),
            ("a/*", Some(("^a/[^/]+/?$", true))),
            ("[!a]x", Some(("^(?:.+/)?[^a]x(?:/|$)", true))),
            ("a[b", Some(("^(?:.+/)?a\\[b(?:/|$)", true))),
        ];
        for (pattern, expected) in cases {
            let expected = expected.map(|(regex, include)| (regex.to_owned(), include));
            assert_eq!(regex_of(pattern), expected, "{pattern}");
        }
    }

    #[test]
    fn last_match_wins() {
        let spec = PathSpec::new(["*.txt", "!keep.txt", "build/", "/top"]).unwrap();
        assert!(spec.matches("a.txt"));
        assert!(spec.matches("dir/a.txt"));
        assert!(!spec.matches("keep.txt"));
        assert!(!spec.matches("dir/keep.txt"));
        assert!(spec.matches("build/out.bin"));
        assert!(!spec.matches("build"));
        assert!(spec.matches("top"));
        assert!(spec.matches("top/inner"));
        assert!(!spec.matches("nested/top"));
        assert!(!spec.matches("other.md"));
    }
}
