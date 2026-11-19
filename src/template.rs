//! Template discovery and configuration, mirroring `copier/_template.py`.

use std::path::{Path, PathBuf};

use crate::errors::Result;
use crate::value::{Mapping, Value};

/// Patterns excluded from rendering unless the template overrides `_exclude`.
pub const DEFAULT_EXCLUDE: &[&str] = &[
    "copier.yaml",
    "copier.yml",
    "~*",
    "*.py[co]",
    "__pycache__",
    ".git",
    ".DS_Store",
    ".svn",
];

/// Suffix marking files to be rendered with Jinja.
pub const DEFAULT_TEMPLATES_SUFFIX: &str = ".jinja";

/// The command of a [`Task`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Run through the shell.
    Shell(String),
    /// Run directly, without a shell.
    Argv(Vec<String>),
}

/// A task or migration to execute.
#[derive(Debug, Clone, PartialEq)]
pub struct Task {
    /// Command to run.
    pub cmd: Command,
    /// Variables exposed to the command as environment and Jinja context.
    pub extra_vars: Mapping,
    /// Jinja condition, or a literal boolean.
    pub condition: Value,
    /// Directory to run in, relative to the project.
    pub working_directory: PathBuf,
}

/// Load a `copier.yml`, resolving `!include` tags and merging multiple documents.
///
/// # Errors
///
/// Returns [`crate::CopierError::InvalidConfigFile`] for invalid YAML.
pub fn load_template_config(_conf_path: &Path, _quiet: bool) -> Result<Value> {
    todo!("load_template_config")
}

/// A template: where it comes from and what its configuration says.
#[derive(Debug, Clone)]
pub struct Template {
    /// Location as given by the user (path or URL).
    pub url: String,
    /// Git ref to use; `None` selects the latest tag.
    pub git_ref: Option<String>,
    /// Whether prerelease tags count when selecting the latest tag.
    pub use_prereleases: bool,
}

impl Template {
    /// Describe the template at `url`.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            git_ref: None,
            use_prereleases: false,
        }
    }

    /// Use a specific git ref.
    pub fn git_ref(mut self, git_ref: impl Into<String>) -> Self {
        self.git_ref = Some(git_ref.into());
        self
    }

    /// Settings from `copier.yml` (keys starting with `_`, underscore stripped).
    pub fn config_data(&self) -> Result<Mapping> {
        todo!("Template::config_data")
    }

    /// Questions from `copier.yml`, each normalized to a mapping.
    pub fn questions_data(&self) -> Result<Mapping> {
        todo!("Template::questions_data")
    }

    /// Jinja environment options (`_envops`).
    pub fn envops(&self) -> Result<Mapping> {
        todo!("Template::envops")
    }

    /// Exclusion patterns (`_exclude`).
    pub fn exclude(&self) -> Result<Vec<String>> {
        todo!("Template::exclude")
    }

    /// Patterns never overwritten when they exist (`_skip_if_exists`).
    pub fn skip_if_exists(&self) -> Result<Vec<String>> {
        todo!("Template::skip_if_exists")
    }

    /// Tasks to run after rendering (`_tasks`).
    pub fn tasks(&self) -> Result<Vec<Task>> {
        todo!("Template::tasks")
    }

    /// Requested Jinja extensions (`_jinja_extensions`).
    pub fn jinja_extensions(&self) -> Result<Vec<String>> {
        todo!("Template::jinja_extensions")
    }

    /// Names of questions whose answers are secret.
    pub fn secret_questions(&self) -> Result<Vec<String>> {
        todo!("Template::secret_questions")
    }

    /// Templates suffix (`_templates_suffix`).
    pub fn templates_suffix(&self) -> Result<String> {
        todo!("Template::templates_suffix")
    }

    /// Local checkout of the template.
    pub fn local_abspath(&self) -> Result<PathBuf> {
        todo!("Template::local_abspath")
    }

    /// `git describe` of the checked-out template, if it is a git repository.
    pub fn commit(&self) -> Result<Option<String>> {
        todo!("Template::commit")
    }

    /// PEP 440 version of the checked-out template, if any.
    pub fn version(&self) -> Result<Option<String>> {
        todo!("Template::version")
    }
}
