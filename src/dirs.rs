//! Platform directories, matching what `platformdirs` gives Python Copier.

use std::path::PathBuf;

fn home() -> PathBuf {
    std::env::home_dir().unwrap_or_default()
}

fn env_dir(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// `platformdirs.user_config_path("copier", appauthor=False)`.
pub(crate) fn user_config_dir() -> PathBuf {
    if cfg!(target_os = "macos") {
        home()
            .join("Library")
            .join("Application Support")
            .join("copier")
    } else if cfg!(windows) {
        env_dir("LOCALAPPDATA")
            .unwrap_or_else(|| home().join("AppData").join("Local"))
            .join("copier")
    } else {
        env_dir("XDG_CONFIG_HOME")
            .unwrap_or_else(|| home().join(".config"))
            .join("copier")
    }
}

/// `platformdirs.user_cache_dir("copier", appauthor=False)`.
pub(crate) fn user_cache_dir() -> PathBuf {
    if cfg!(target_os = "macos") {
        home().join("Library").join("Caches").join("copier")
    } else if cfg!(windows) {
        env_dir("LOCALAPPDATA")
            .unwrap_or_else(|| home().join("AppData").join("Local"))
            .join("copier")
            .join("Cache")
    } else {
        env_dir("XDG_CACHE_HOME")
            .unwrap_or_else(|| home().join(".cache"))
            .join("copier")
    }
}

/// Expand a leading `~` or `~/` to the home directory (`os.path.expanduser`).
pub(crate) fn expand_user(path: &str) -> String {
    match path.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with(['/', std::path::MAIN_SEPARATOR]) => {
            format!("{}{rest}", home().display())
        }
        _ => path.to_owned(),
    }
}
