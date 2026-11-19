//! User settings, mirroring `copier/_settings.py`.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

use crate::dirs;
use crate::errors::{warn, CopierError, Result, WarningKind};
use crate::value::{Mapping, Value};
use crate::yaml;

/// Name of the environment variable overriding the settings file location.
pub const ENV_VAR: &str = "COPIER_SETTINGS_PATH";

/// User-wide settings loaded from `settings.yml`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Settings {
    /// Default answers applied to every template.
    pub defaults: Mapping,
    /// Trusted template locations; entries ending in `/` are prefixes.
    pub trust: Vec<String>,
}

impl Settings {
    /// Whether `repository` is trusted to use unsafe features.
    pub fn is_trusted(&self, repository: &str) -> bool {
        is_trusted_repository(&self.trust, repository)
    }
}

/// Load settings from `path`, or from the default location when `None`.
///
/// The default location is `$COPIER_SETTINGS_PATH` if set, otherwise
/// [`default_settings_path`]. A missing file yields empty settings.
///
/// # Errors
///
/// Returns [`CopierError::Settings`] when the file is not valid.
pub fn load_settings(path: Option<&Path>) -> Result<Settings> {
    let env_path = std::env::var_os(ENV_VAR).filter(|value| !value.is_empty()).map(PathBuf::from);
    let path = match (path, &env_path) {
        (Some(path), _) => path.to_path_buf(),
        (None, Some(env_path)) => env_path.clone(),
        (None, None) => default_settings_path(),
    };
    if !path.is_file() {
        if let Some(env_path) = env_path {
            warn(WarningKind::MissingSettings, format!("Settings file not found at {}", env_path.display()));
        }
        return Ok(Settings::default());
    }
    let invalid_yaml = |err: yaml::YamlError| CopierError::Settings(format!("Invalid YAML data: {err}"));
    let text = yaml::decode(&std::fs::read(&path)?).map_err(invalid_yaml)?;
    let data = yaml::load(&text).map_err(invalid_yaml)?;
    validate(data).map_err(|problems| {
        let lines: Vec<String> =
            problems.iter().map(|(location, message)| format!("  {location}:\n    {message}")).collect();
        CopierError::Settings(format!("Invalid format:\n{}", lines.join("\n")))
    })
}

/// Check the shape of the settings data, reporting problems as (location, message)
/// in pydantic's wording.
fn validate(data: Value) -> std::result::Result<Settings, Vec<(String, String)>> {
    let Value::Map(data) = data else {
        return Err(vec![(
            String::new(),
            "Input should be a valid dictionary or object to extract fields from".to_owned(),
        )]);
    };
    let mut problems = Vec::new();
    let mut settings = Settings::default();
    match data.get(&Value::from("defaults")) {
        None => {}
        Some(Value::Map(defaults)) => {
            for (key, value) in defaults {
                if key.as_str().is_none() {
                    problems.push((
                        format!("defaults.{}.[key]", key.py_str()),
                        "Input should be a valid string".to_owned(),
                    ));
                }
                settings.defaults.insert(key.clone(), value.clone());
            }
        }
        Some(_) => problems.push(("defaults".to_owned(), "Input should be a valid dictionary".to_owned())),
    }
    match data.get(&Value::from("trust")) {
        None => {}
        Some(Value::Seq(items)) => {
            for (index, item) in items.iter().enumerate() {
                match item.as_str() {
                    Some(text) if settings.trust.iter().any(|known| known == text) => {}
                    Some(text) => settings.trust.push(text.to_owned()),
                    None => problems.push((format!("trust.{index}"), "Input should be a valid string".to_owned())),
                }
            }
        }
        Some(_) => problems.push(("trust".to_owned(), "Input should be a valid set".to_owned())),
    }
    if problems.is_empty() {
        Ok(settings)
    } else {
        Err(problems)
    }
}

/// Location of the settings file in the platform's configuration directory.
pub fn default_settings_path() -> PathBuf {
    dirs::user_config_dir().join("settings.yml")
}

/// Whether `repository` matches any entry of `trust`.
///
/// An entry ending in `/` matches every repository below it; any other entry must
/// match exactly. Prefix matching and normalization only apply when both sides are
/// local paths or remote references whose path uses RFC 3986 unreserved characters.
pub fn is_trusted_repository<S: AsRef<str>>(trust: &[S], repository: &str) -> bool {
    let repository_is_safe = is_safe_url(repository);
    let normalized_repository = normalize(repository);
    trust.iter().map(AsRef::as_ref).any(|entry| {
        if repository_is_safe && is_safe_url(entry) {
            if entry.ends_with('/') {
                normalized_repository.starts_with(&normalize(entry))
            } else {
                normalized_repository == normalize(entry)
            }
        } else {
            repository == entry
        }
    })
}

static SCP_URL: LazyLock<Regex> = LazyLock::new(|| compile(r"^(?:[^/@:\s]+@)?[^/:\s]+:.+$"));
static SAFE_SEGMENT: LazyLock<Regex> = LazyLock::new(|| compile(r"^[A-Za-z0-9._~-]+$"));
static WINDOWS_ABSOLUTE: LazyLock<Regex> =
    LazyLock::new(|| compile(r"^(?:[A-Za-z]:[\\/]|[\\/]{2}[^\\/]+[\\/]+[^\\/]+)"));
static URL_SCHEME: LazyLock<Regex> = LazyLock::new(|| compile(r"^[A-Za-z][A-Za-z0-9+.-]*:"));

fn compile(pattern: &str) -> Regex {
    // Patterns are literals, so they always compile.
    #[allow(clippy::expect_used)]
    Regex::new(pattern).expect("valid settings regex")
}

/// Whether `url` is an absolute URL or uses one of Copier's alias prefixes.
fn is_url_style(url: &str) -> bool {
    url.contains("://") || url.starts_with("gh:") || url.starts_with("gl:")
}

/// Whether `url` uses git's SCP-like syntax (`[user@]host:path`).
fn is_scp_style(url: &str) -> bool {
    !WINDOWS_ABSOLUTE.is_match(url) && SCP_URL.is_match(url)
}

/// The pieces of a URL, as Python's `urllib.parse.urlsplit` returns them.
struct SplitUrl<'a> {
    scheme: &'a str,
    netloc: &'a str,
    path: &'a str,
    query: Option<&'a str>,
    fragment: Option<&'a str>,
}

fn split_url(url: &str) -> SplitUrl<'_> {
    let (scheme, rest) = match URL_SCHEME.find(url) {
        Some(found) => (&url[..found.end() - 1], &url[found.end()..]),
        None => ("", url),
    };
    let (netloc, rest) = match rest.strip_prefix("//") {
        Some(after) => {
            let end = after.find(['/', '?', '#']).unwrap_or(after.len());
            (&after[..end], &after[end..])
        }
        None => ("", rest),
    };
    let (rest, fragment) = match rest.split_once('#') {
        Some((before, fragment)) => (before, Some(fragment)),
        None => (rest, None),
    };
    let (path, query) = match rest.split_once('?') {
        Some((before, query)) => (before, Some(query)),
        None => (rest, None),
    };
    SplitUrl { scheme, netloc, path, query, fragment }
}

/// Schemes for which Python's `urlunsplit` writes `//` even with an empty host.
const USES_NETLOC: &[&str] = &[
    "ftp",
    "http",
    "gopher",
    "nntp",
    "telnet",
    "imap",
    "wais",
    "file",
    "mms",
    "https",
    "shttp",
    "snews",
    "prospero",
    "rtsp",
    "rtsps",
    "rtspu",
    "rsync",
    "svn",
    "svn+ssh",
    "sftp",
    "nfs",
    "git",
    "git+ssh",
    "ws",
    "wss",
    "itms-services",
];

fn unsplit_url(parts: &SplitUrl<'_>, path: &str) -> String {
    let mut url = path.to_owned();
    if !parts.netloc.is_empty() || (USES_NETLOC.contains(&parts.scheme) && !url.starts_with("//")) {
        if !url.is_empty() && !url.starts_with('/') {
            url.insert(0, '/');
        }
        url = format!("//{}{url}", parts.netloc);
    }
    if !parts.scheme.is_empty() {
        url = format!("{}:{url}", parts.scheme);
    }
    if let Some(query) = parts.query.filter(|query| !query.is_empty()) {
        url = format!("{url}?{query}");
    }
    if let Some(fragment) = parts.fragment.filter(|fragment| !fragment.is_empty()) {
        url = format!("{url}#{fragment}");
    }
    url
}

/// Whether `url` is a local path, or a remote reference whose path segments contain
/// only RFC 3986 unreserved characters.
fn is_safe_url(url: &str) -> bool {
    let path = if is_url_style(url) {
        split_url(url).path
    } else if is_scp_style(url) {
        url.split_once(':').map_or("", |(_, path)| path)
    } else {
        return true;
    };
    let mut segments: Vec<&str> = path.split('/').collect();
    if segments.first() == Some(&"") {
        segments.remove(0);
    }
    if segments.last() == Some(&"") {
        segments.pop();
    }
    segments.iter().all(|segment| SAFE_SEGMENT.is_match(segment))
}

/// Normalize `url` for trust comparison: resolve `.`/`..` segments, keeping a
/// trailing slash, and expand `~` in local paths.
fn normalize(url: &str) -> String {
    if is_url_style(url) {
        let parts = split_url(url);
        return unsplit_url(&parts, &normalize_url_path(parts.path));
    }
    if is_scp_style(url) {
        if let Some((host, path)) = url.split_once(':') {
            return format!("{host}:{}", normalize_url_path(path));
        }
    }
    let expanded = dirs::expand_user(url);
    let mut normalized = normpath(&expanded);
    let separator = std::path::MAIN_SEPARATOR;
    if expanded.ends_with(['/', separator]) && !normalized.ends_with(separator) {
        normalized.push(separator);
    }
    normalized
}

fn normalize_url_path(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    let mut normalized = normpath(path);
    if path.ends_with('/') && !normalized.ends_with('/') {
        normalized.push('/');
    }
    normalized
}

/// Python's `posixpath.normpath`: collapse separators and resolve `.` and `..`
/// lexically; exactly two leading slashes are preserved.
pub(crate) fn normpath(path: &str) -> String {
    if path.is_empty() {
        return ".".to_owned();
    }
    let leading = match path.len() - path.trim_start_matches('/').len() {
        0 => 0,
        2 => 2,
        _ => 1,
    };
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.last().is_some_and(|last| *last != "..") {
                    parts.pop();
                } else if leading == 0 {
                    parts.push("..");
                }
            }
            other => parts.push(other),
        }
    }
    let joined = format!("{}{}", "/".repeat(leading), parts.join("/"));
    if joined.is_empty() {
        ".".to_owned()
    } else {
        joined
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normpath_matches_posixpath() {
        for (input, expected) in [
            ("", "."),
            ("a/./b/../c/", "a/c"),
            ("/../a", "/a"),
            ("//a//b", "//a/b"),
            ("///a", "/a"),
            ("../../a", "../../a"),
            ("a/..", "."),
        ] {
            assert_eq!(normpath(input), expected, "{input}");
        }
    }

    #[test]
    fn url_normalization() {
        assert_eq!(normalize("https://github.com/user/../evil/repo.git"), "https://github.com/evil/repo.git");
        assert_eq!(normalize("gh:user/./repo/"), "gh:user/repo/");
        assert_eq!(normalize("git@github.com:user//repo.git"), "git@github.com:user/repo.git");
        assert_eq!(normalize("https://github.com"), "https://github.com");
    }

    #[test]
    fn unsafe_urls_only_match_exactly() {
        let trust = ["https://github.com/user/"];
        assert!(is_trusted_repository(&trust, "https://github.com/user/repo.git"));
        assert!(!is_trusted_repository(&trust, "https://github.com/user/%2e%2e/evil.git"));
        assert!(is_trusted_repository(&["https://x/a%20b"], "https://x/a%20b"));
    }
}
