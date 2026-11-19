//! The engine: copy, recopy and update, mirroring `copier/_main.py`.

use std::path::{Path, PathBuf};

use crate::errors::Result;
use crate::prompt::Prompter;
use crate::questions::AnswersMap;
use crate::settings::Settings;
use crate::template::Template;
use crate::value::{Mapping, Value};

/// How update conflicts are reported.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ConflictMode {
    /// Git-style conflict markers inside the file.
    #[default]
    Inline,
    /// Separate `*.rej` files.
    Rej,
}

/// Which template version to use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VcsRef {
    /// A branch, tag or commit.
    Ref(String),
    /// The version the project was last generated from (`:current:`).
    Current,
}

impl From<&str> for VcsRef {
    fn from(text: &str) -> Self {
        if text == ":current:" {
            Self::Current
        } else {
            Self::Ref(text.to_owned())
        }
    }
}

/// Stage of a run, exposed to templates as `_copier_phase`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Phase {
    /// Asking questions.
    Prompt,
    /// Running tasks.
    Tasks,
    /// Running migrations.
    Migrate,
    /// Rendering files.
    Render,
    /// Outside any phase.
    #[default]
    Undefined,
}

/// Result of [`get_update_data`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateData {
    /// Whether a newer template version exists.
    pub update_available: bool,
    /// Version the project was generated from.
    pub current_version: String,
    /// Newest available version.
    pub latest_version: String,
}

/// Options and state of one Copier run.
///
/// Configure with the chainable setters, then call [`Worker::run_copy`],
/// [`Worker::run_recopy`] or [`Worker::run_update`].
pub struct Worker {
    /// Template location; `None` reuses the one recorded in the answers file.
    pub src_path: Option<String>,
    /// Project directory.
    pub dst_path: PathBuf,
    /// Answers file, relative to `dst_path`; `None` uses the template's setting.
    pub answers_file: Option<PathBuf>,
    /// Template version to use; `None` selects the latest tag.
    pub vcs_ref: Option<VcsRef>,
    /// Answers supplied up front.
    pub data: Mapping,
    /// User settings; `None` loads them from the default location.
    pub settings: Option<Settings>,
    /// Extra exclusion patterns.
    pub exclude: Vec<String>,
    /// Whether prerelease tags count as latest.
    pub use_prereleases: bool,
    /// Extra patterns never overwritten when they exist.
    pub skip_if_exists: Vec<String>,
    /// Delete a newly created destination when the run fails.
    pub cleanup_on_error: bool,
    /// Use default answers instead of prompting.
    pub defaults: bool,
    /// Defaults that override the template's.
    pub user_defaults: Mapping,
    /// Overwrite existing files without asking.
    pub overwrite: bool,
    /// Report what would happen without writing anything.
    pub pretend: bool,
    /// Suppress status output.
    pub quiet: bool,
    /// How update conflicts are reported.
    pub conflict: ConflictMode,
    /// Context lines used when diffing during updates.
    pub context_lines: u32,
    /// Allow tasks, migrations and Jinja extensions.
    pub unsafe_: bool,
    /// Do not re-ask questions answered in a previous run.
    pub skip_answered: bool,
    /// Do not run tasks.
    pub skip_tasks: bool,
    /// Glob patterns of questions to ask even when already answered.
    pub ask: Vec<String>,
    /// Answers collected by the run.
    pub answers: AnswersMap,
    prompter: Option<Box<dyn Prompter>>,
}

impl Default for Worker {
    fn default() -> Self {
        Self {
            src_path: None,
            dst_path: PathBuf::from("."),
            answers_file: None,
            vcs_ref: None,
            data: Mapping::new(),
            settings: None,
            exclude: Vec::new(),
            use_prereleases: false,
            skip_if_exists: Vec::new(),
            cleanup_on_error: true,
            defaults: false,
            user_defaults: Mapping::new(),
            overwrite: false,
            pretend: false,
            quiet: false,
            conflict: ConflictMode::Inline,
            context_lines: 3,
            unsafe_: false,
            skip_answered: false,
            skip_tasks: false,
            ask: Vec::new(),
            answers: AnswersMap::default(),
            prompter: None,
        }
    }
}

fn to_mapping(value: Value) -> Mapping {
    match value {
        Value::Map(map) => map,
        _ => Mapping::new(),
    }
}

fn to_strings<S: Into<String>>(items: impl IntoIterator<Item = S>) -> Vec<String> {
    items.into_iter().map(Into::into).collect()
}

impl Worker {
    /// A worker with default options.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the template location.
    pub fn src_path(mut self, src_path: impl AsRef<Path>) -> Self {
        self.src_path = Some(src_path.as_ref().to_string_lossy().into_owned());
        self
    }

    /// Set the project directory.
    pub fn dst_path(mut self, dst_path: impl AsRef<Path>) -> Self {
        self.dst_path = dst_path.as_ref().to_path_buf();
        self
    }

    /// Set the answers file, relative to the project directory.
    pub fn answers_file(mut self, answers_file: impl AsRef<Path>) -> Self {
        self.answers_file = Some(answers_file.as_ref().to_path_buf());
        self
    }

    /// Set the template version; `":current:"` keeps the current one.
    pub fn vcs_ref(mut self, vcs_ref: impl Into<VcsRef>) -> Self {
        self.vcs_ref = Some(vcs_ref.into());
        self
    }

    /// Set the up-front answers; non-mapping values are treated as empty.
    pub fn data(mut self, data: impl Into<Value>) -> Self {
        self.data = to_mapping(data.into());
        self
    }

    /// Set the user settings.
    pub fn settings(mut self, settings: Settings) -> Self {
        self.settings = Some(settings);
        self
    }

    /// Set extra exclusion patterns.
    pub fn exclude<S: Into<String>>(mut self, patterns: impl IntoIterator<Item = S>) -> Self {
        self.exclude = to_strings(patterns);
        self
    }

    /// Set whether prerelease tags count as latest.
    pub fn use_prereleases(mut self, yes: bool) -> Self {
        self.use_prereleases = yes;
        self
    }

    /// Set extra skip-if-exists patterns.
    pub fn skip_if_exists<S: Into<String>>(
        mut self,
        patterns: impl IntoIterator<Item = S>,
    ) -> Self {
        self.skip_if_exists = to_strings(patterns);
        self
    }

    /// Set whether a failed run deletes a newly created destination.
    pub fn cleanup_on_error(mut self, yes: bool) -> Self {
        self.cleanup_on_error = yes;
        self
    }

    /// Set whether defaults are used instead of prompting.
    pub fn defaults(mut self, yes: bool) -> Self {
        self.defaults = yes;
        self
    }

    /// Set defaults that override the template's; non-mapping values are treated as empty.
    pub fn user_defaults(mut self, user_defaults: impl Into<Value>) -> Self {
        self.user_defaults = to_mapping(user_defaults.into());
        self
    }

    /// Set whether existing files are overwritten without asking.
    pub fn overwrite(mut self, yes: bool) -> Self {
        self.overwrite = yes;
        self
    }

    /// Set dry-run mode.
    pub fn pretend(mut self, yes: bool) -> Self {
        self.pretend = yes;
        self
    }

    /// Set whether status output is suppressed.
    pub fn quiet(mut self, yes: bool) -> Self {
        self.quiet = yes;
        self
    }

    /// Set how update conflicts are reported.
    pub fn conflict(mut self, mode: ConflictMode) -> Self {
        self.conflict = mode;
        self
    }

    /// Set the number of diff context lines used by updates.
    pub fn context_lines(mut self, lines: u32) -> Self {
        self.context_lines = lines;
        self
    }

    /// Set whether unsafe template features are allowed.
    pub fn unsafe_(mut self, yes: bool) -> Self {
        self.unsafe_ = yes;
        self
    }

    /// Set whether previously answered questions are skipped.
    pub fn skip_answered(mut self, yes: bool) -> Self {
        self.skip_answered = yes;
        self
    }

    /// Set whether tasks are skipped.
    pub fn skip_tasks(mut self, yes: bool) -> Self {
        self.skip_tasks = yes;
        self
    }

    /// Set glob patterns of questions to ask even when already answered.
    pub fn ask<S: Into<String>>(mut self, patterns: impl IntoIterator<Item = S>) -> Self {
        self.ask = to_strings(patterns);
        self
    }

    /// Replace the terminal prompts with a custom answer source.
    pub fn prompter(mut self, prompter: Box<dyn Prompter>) -> Self {
        self.prompter = Some(prompter);
        self
    }

    /// The template this worker renders.
    pub fn template(&self) -> Result<Template> {
        todo!("Worker::template")
    }

    /// The Jinja context files are rendered with.
    pub fn render_context(&self) -> Result<Mapping> {
        todo!("Worker::render_context")
    }

    /// Template and user exclusion patterns combined.
    pub fn all_exclusions(&self) -> Result<Vec<String>> {
        todo!("Worker::all_exclusions")
    }

    /// Generate a project from the template.
    ///
    /// # Errors
    ///
    /// Fails on invalid configuration or answers, unsafe features without trust,
    /// failing tasks, or I/O errors.
    pub fn run_copy(self) -> Result<Self> {
        todo!("Worker::run_copy")
    }

    /// Regenerate the project from its template, keeping answers but discarding
    /// local changes.
    ///
    /// # Errors
    ///
    /// Fails when the project has no recorded template, or as [`Worker::run_copy`].
    pub fn run_recopy(self) -> Result<Self> {
        todo!("Worker::run_recopy")
    }

    /// Update the project to a newer template version, preserving local changes.
    ///
    /// # Errors
    ///
    /// Fails when the project is not a clean git repository, lacks recorded template
    /// data, would be downgraded, or as [`Worker::run_copy`].
    pub fn run_update(self) -> Result<Self> {
        todo!("Worker::run_update")
    }
}

/// Report whether the project at `dst_path` can be updated to a newer template version.
///
/// # Errors
///
/// Fails when the project lacks the recorded template data needed to compare versions.
pub fn get_update_data(
    _dst_path: &Path,
    _answers_file: Option<&Path>,
    _use_prereleases: bool,
) -> Result<UpdateData> {
    todo!("get_update_data")
}
