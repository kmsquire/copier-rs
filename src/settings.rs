//! User settings, mirroring `copier/_settings.py`.

use std::path::{Path, PathBuf};

use crate::errors::Result;
use crate::value::Mapping;

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
/// # Errors
///
/// Returns [`crate::CopierError::Settings`] when the file is not valid.
pub fn load_settings(_path: Option<&Path>) -> Result<Settings> {
    todo!("load_settings")
}

/// Location of the settings file: `$COPIER_SETTINGS_PATH` or the platform config dir.
pub fn default_settings_path() -> PathBuf {
    todo!("default_settings_path")
}

/// Whether `repository` matches any entry of `trust`.
pub fn is_trusted_repository<S: AsRef<str>>(_trust: &[S], _repository: &str) -> bool {
    todo!("is_trusted_repository")
}
