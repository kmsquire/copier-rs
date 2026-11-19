//! Errors and warnings, mirroring `copier/errors.py`.
//!
//! Message texts are part of the observable behavior and match Python Copier.

use std::cell::RefCell;
use std::path::PathBuf;

/// Result alias used across the crate.
pub type Result<T, E = CopierError> = std::result::Result<T, E>;

/// Every failure Copier can report.
///
/// Variants named after Python builtins (`Value`, `Type`, `Key`) stand in for the
/// builtin exceptions the Python implementation raises in the same situations.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CopierError {
    /// Generic error whose message is shown to the user as is.
    #[error("{0}")]
    UserMessage(String),
    /// The template requires a newer Copier.
    #[error("{0}")]
    UnsupportedVersion(String),
    /// Invalid template configuration.
    #[error("{0}")]
    ConfigFile(String),
    /// The config file is not valid YAML.
    #[error("{}", .conf_path.display())]
    InvalidConfigFile {
        /// Offending file.
        conf_path: PathBuf,
    },
    /// Both `copier.yml` and `copier.yaml` exist.
    #[error("{conf_paths:?}")]
    MultipleConfigFiles {
        /// The conflicting files.
        conf_paths: Vec<PathBuf>,
    },
    /// A question declares an unsupported type.
    #[error("{0}")]
    InvalidType(String),
    /// A path that must be absolute is not.
    #[error("\"{}\" is not an absolute path", .path.display())]
    PathNotAbsolute {
        /// Offending path.
        path: PathBuf,
    },
    /// A path that must be relative is not.
    #[error("\"{}\" is not a relative path", .path.display())]
    PathNotRelative {
        /// Offending path.
        path: PathBuf,
    },
    /// A path escapes the directory it must stay in.
    #[error("\"{}\" is forbidden{}", .path.display(), if .hint.is_empty() { String::new() } else { format!("\n\n{}", .hint) })]
    ForbiddenPath {
        /// Offending path.
        path: PathBuf,
        /// Optional advice for the user.
        hint: String,
    },
    /// A Jinja extension requested by the template is unavailable.
    #[error("{0}")]
    ExtensionNotFound(String),
    /// The user interrupted the questionnaire.
    #[error("Execution stopped by user")]
    AnswersInterrupt(Box<AnswersInterrupt>),
    /// The template uses features that require `--trust`.
    #[error("Template uses potentially unsafe feature{}: {}.\nIf you trust this template, consider adding the `--trust` option when running `copier copy/update`.", if .features.len() == 1 { "" } else { "s" }, .features.join(", "))]
    UnsafeTemplate {
        /// Sorted names of the unsafe features in use.
        features: Vec<String>,
    },
    /// A `yield` tag was used in file contents.
    #[error("{0}")]
    YieldTagInFile(String),
    /// More than one `yield` tag in one path component.
    #[error("{0}")]
    MultipleYieldTags(String),
    /// A task or migration command failed.
    #[error("Task {command} returned non-zero exit status {returncode}.")]
    Task {
        /// Command, formatted like Python's `repr`.
        command: String,
        /// Its exit status.
        returncode: i32,
    },
    /// A prompt was needed but no interactive session is available.
    #[error("Interactive session required: {0}")]
    InteractiveSession(String),
    /// The user settings file is invalid.
    #[error("{0}")]
    Settings(String),
    /// Stand-in for Python's `ValueError`.
    #[error("{0}")]
    Value(String),
    /// Stand-in for Python's `TypeError`.
    #[error("{0}")]
    Type(String),
    /// Stand-in for Python's `KeyError`.
    #[error("{0}")]
    Key(String),
    /// Invalid arguments to the API (pydantic `ValidationError` in Python).
    #[error("{0}")]
    Validation(String),
    /// A Jinja template failed to compile or render.
    #[error("{message}")]
    Template {
        /// What went wrong.
        kind: TemplateErrorKind,
        /// Engine message.
        message: String,
    },
    /// An external command (usually git) failed.
    #[error("{0}")]
    Process(String),
    /// Filesystem failure.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Broad class of a [`CopierError::Template`] failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateErrorKind {
    /// An undefined value was used (`jinja2.UndefinedError`).
    Undefined,
    /// The template source is invalid (`jinja2.TemplateSyntaxError`).
    Syntax,
    /// A forbidden operation was attempted (`jinja2.sandbox.SecurityError`).
    Security,
    /// Any other rendering failure.
    Other,
}

/// State captured when the user interrupts the questionnaire.
#[derive(Debug)]
pub struct AnswersInterrupt {
    /// Answers given before the interruption.
    pub answers: crate::questions::AnswersMap,
    /// Name of the question being asked.
    pub last_question: String,
    /// URL of the template in use.
    pub template_url: String,
}

impl CopierError {
    /// Whether Python models this error as a `UserMessageError` subclass.
    pub fn is_user_message(&self) -> bool {
        matches!(
            self,
            Self::UserMessage(_)
                | Self::UnsupportedVersion(_)
                | Self::ExtensionNotFound(_)
                | Self::Task { .. }
                | Self::InteractiveSession(_)
                | Self::AnswersInterrupt(_)
        )
    }

    /// Process exit code the CLI uses for this error.
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::UnsafeTemplate { .. } => 4,
            _ => 1,
        }
    }
}

/// Category of a non-fatal [`Warning`], mirroring Python's warning classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarningKind {
    /// `UnknownCopierVersionWarning`
    UnknownCopierVersion,
    /// `OldTemplateWarning`
    OldTemplate,
    /// `DirtyLocalWarning`
    DirtyLocal,
    /// `ShallowCloneWarning`
    ShallowClone,
    /// `MissingSettingsWarning`
    MissingSettings,
    /// `MissingFileWarning`
    MissingFile,
    /// `CleanupWarning`
    Cleanup,
    /// `DeprecationWarning`
    Deprecation,
    /// `FutureWarning`
    Future,
    /// Plain `UserWarning`
    User,
}

/// A non-fatal diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    /// Category.
    pub kind: WarningKind,
    /// Human-readable text.
    pub message: String,
}

thread_local! {
    static WARNING_CAPTURE: RefCell<Option<Vec<Warning>>> = const { RefCell::new(None) };
}

/// Emit a warning: recorded if [`capture_warnings`] is active on this thread,
/// printed to stderr otherwise.
pub fn warn(kind: WarningKind, message: impl Into<String>) {
    let warning = Warning { kind, message: message.into() };
    let uncaptured = WARNING_CAPTURE.with(|slot| match slot.borrow_mut().as_mut() {
        Some(captured) => {
            captured.push(warning.clone());
            None
        }
        None => Some(warning),
    });
    if let Some(warning) = uncaptured {
        crate::output::eprintln(&format!("{:?}Warning: {}", warning.kind, warning.message));
    }
}

/// Run `f`, returning its result together with every warning it emitted on this thread.
pub fn capture_warnings<T>(f: impl FnOnce() -> T) -> (T, Vec<Warning>) {
    let previous = WARNING_CAPTURE.with(|slot| slot.borrow_mut().replace(Vec::new()));
    let result = f();
    let captured = WARNING_CAPTURE.with(|slot| std::mem::replace(&mut *slot.borrow_mut(), previous));
    (result, captured.unwrap_or_default())
}
