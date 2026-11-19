//! PEP 440 versions, as Python's `packaging.version` understands them.

use std::str::FromStr;

pub use pep440_rs::Version;

/// Parse a PEP 440 version; a leading `v` is accepted. `None` when invalid.
pub fn parse(text: &str) -> Option<Version> {
    Version::from_str(text).ok()
}

/// Whether `text` is a valid PEP 440 version.
pub fn is_valid(text: &str) -> bool {
    parse(text).is_some()
}

/// Whether `version` is a pre-release or development release.
pub fn is_prerelease(version: &Version) -> bool {
    version.any_prerelease()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsing_and_ordering() {
        assert!(is_valid("v1.2.3"));
        assert!(is_valid("1.0.0-alpha"));
        assert!(!is_valid("release-1"));
        assert!(!is_valid("HEAD"));
        assert!(parse("v2.0.0a1").is_some_and(|v| is_prerelease(&v)));
        assert!(parse("1.0.dev1").is_some_and(|v| is_prerelease(&v)));
        assert!(parse("1.0.post1").is_some_and(|v| !is_prerelease(&v)));
        assert!(parse("v1.10") > parse("v1.9"));
        assert_eq!(parse("3.0.0-alpha").map(|v| v.to_string()).as_deref(), Some("3.0.0a0"));
    }
}
