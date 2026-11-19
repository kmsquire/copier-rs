//! The engine: copy, recopy and update, mirroring `copier/_main.py`.

mod ask;
mod context;
mod copy;
mod render;
mod state;
mod tasks;
mod update;

use std::cell::{OnceCell, RefCell};
use std::fmt;
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub use state::{Operation, Phase};

use crate::errors::{CopierError, Result};
use crate::jinja::JinjaEnv;
use crate::pathspec::PathSpec;
use crate::prompt::{NoPrompter, Prompter};
use crate::questions::AnswersMap;
use crate::settings::{load_settings, Settings};
use crate::subproject::Subproject;
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

impl ConflictMode {
    /// The name used on the command line and in `_copier_conf`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Inline => "inline",
            Self::Rej => "rej",
        }
    }
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

impl From<String> for VcsRef {
    fn from(text: String) -> Self {
        Self::from(text.as_str())
    }
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

/// Values derived from the options, computed once per worker.
#[derive(Default)]
struct Cache {
    settings: OnceCell<Settings>,
    template: OnceCell<Template>,
    subproject: OnceCell<Subproject>,
    env: OnceCell<JinjaEnv>,
    template_copy_root: OnceCell<PathBuf>,
    match_exclude: RefCell<Option<Rc<PathSpec>>>,
    match_skip: OnceCell<PathSpec>,
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
    prompter: Rc<RefCell<dyn Prompter>>,
    cache: Cache,
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
            prompter: Rc::new(RefCell::new(NoPrompter)),
            cache: Cache::default(),
        }
    }
}

impl fmt::Debug for Worker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Worker")
            .field("src_path", &self.src_path)
            .field("dst_path", &self.dst_path)
            .field("answers_file", &self.answers_file)
            .field("vcs_ref", &self.vcs_ref)
            .field("data", &self.data)
            .field("defaults", &self.defaults)
            .field("overwrite", &self.overwrite)
            .field("pretend", &self.pretend)
            .field("quiet", &self.quiet)
            .field("unsafe_", &self.unsafe_)
            .finish_non_exhaustive()
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

fn cached<T>(cell: &OnceCell<T>, compute: impl FnOnce() -> Result<T>) -> Result<&T> {
    if let Some(value) = cell.get() {
        return Ok(value);
    }
    let value = compute()?;
    Ok(cell.get_or_init(|| value))
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
        self.prompter = Rc::new(RefCell::new(BoxedPrompter(prompter)));
        self
    }

    /// A worker with the same options and prompter but none of the derived state,
    /// like Python's `dataclasses.replace(worker)`.
    fn derive(&self) -> Self {
        Self {
            src_path: self.src_path.clone(),
            dst_path: self.dst_path.clone(),
            answers_file: self.answers_file.clone(),
            vcs_ref: self.vcs_ref.clone(),
            data: self.data.clone(),
            settings: Some(self.user_settings().cloned().unwrap_or_default()),
            exclude: self.exclude.clone(),
            use_prereleases: self.use_prereleases,
            skip_if_exists: self.skip_if_exists.clone(),
            cleanup_on_error: self.cleanup_on_error,
            defaults: self.defaults,
            user_defaults: self.user_defaults.clone(),
            overwrite: self.overwrite,
            pretend: self.pretend,
            quiet: self.quiet,
            conflict: self.conflict,
            context_lines: self.context_lines,
            unsafe_: self.unsafe_,
            skip_answered: self.skip_answered,
            skip_tasks: self.skip_tasks,
            ask: self.ask.clone(),
            answers: AnswersMap::default(),
            prompter: Rc::clone(&self.prompter),
            cache: Cache::default(),
        }
    }

    /// The user settings in effect: the ones given, or those loaded from disk.
    fn user_settings(&self) -> Result<&Settings> {
        cached(&self.cache.settings, || match &self.settings {
            Some(settings) => Ok(settings.clone()),
            None => load_settings(None),
        })
    }

    /// The project being generated or updated.
    fn subproject(&self) -> Result<&Subproject> {
        cached(&self.cache.subproject, || {
            Ok(Subproject::new(
                std::path::absolute(&self.dst_path)?,
                self.answers_file
                    .clone()
                    .unwrap_or_else(|| PathBuf::from(".copier-answers.yml")),
            ))
        })
    }

    fn missing_template() -> CopierError {
        CopierError::Type("Template not found".to_owned())
    }

    /// The git ref to use, with `:current:` resolved to the recorded version.
    fn resolved_vcs_ref(&self) -> Result<Option<String>> {
        match &self.vcs_ref {
            None => Ok(None),
            Some(VcsRef::Ref(git_ref)) => Ok(Some(git_ref.clone())),
            Some(VcsRef::Current) => {
                let template = self
                    .subproject()?
                    .template()
                    .ok_or_else(Self::missing_template)?;
                Ok(template.git_ref.clone())
            }
        }
    }

    fn template_ref(&self) -> Result<&Template> {
        cached(&self.cache.template, || {
            let url = match &self.src_path {
                Some(url) if !url.is_empty() => url.clone(),
                _ => self
                    .subproject()?
                    .template()
                    .ok_or_else(Self::missing_template)?
                    .url
                    .clone(),
            };
            let mut template = Template::new(url).use_prereleases(self.use_prereleases);
            if let Some(git_ref) = self.resolved_vcs_ref()? {
                template = template.git_ref(git_ref);
            }
            Ok(template)
        })
    }

    /// The template this worker renders.
    ///
    /// # Errors
    ///
    /// Fails when no template is given and the project records none.
    pub fn template(&self) -> Result<Template> {
        self.template_ref().cloned()
    }

    fn env(&self) -> Result<&JinjaEnv> {
        cached(&self.cache.env, || {
            let template = self.template_ref()?;
            JinjaEnv::new(
                &template.local_abspath()?,
                &template.envops()?,
                &template.jinja_extensions()?,
            )
        })
    }

    /// Template and user exclusion patterns combined.
    ///
    /// # Errors
    ///
    /// Fails when the template configuration cannot be read.
    pub fn all_exclusions(&self) -> Result<Vec<String>> {
        let mut patterns = self.template_ref()?.exclude()?;
        patterns.extend(self.exclude.iter().cloned());
        Ok(patterns)
    }

    /// User and template skip-if-exists patterns combined.
    fn all_skip_if_exists(&self) -> Result<Vec<String>> {
        let mut patterns = self.skip_if_exists.clone();
        patterns.extend(self.template_ref()?.skip_if_exists()?);
        Ok(patterns)
    }

    /// The Jinja context files are rendered with, as plain data.
    ///
    /// # Errors
    ///
    /// Fails when the template cannot be loaded.
    pub fn render_context(&self) -> Result<Mapping> {
        Ok(self.context()?.to_mapping())
    }

    /// Generate a project from the template.
    ///
    /// # Errors
    ///
    /// Fails on invalid configuration or answers, unsafe features without trust,
    /// failing tasks, or I/O errors.
    pub fn run_copy(mut self) -> Result<Self> {
        self.copy()?;
        Ok(self)
    }

    /// Regenerate the project from its template, keeping answers but discarding
    /// local changes.
    ///
    /// # Errors
    ///
    /// Fails when the project has no recorded template, or as [`Worker::run_copy`].
    pub fn run_recopy(mut self) -> Result<Self> {
        self.recopy()?;
        Ok(self)
    }

    /// Update the project to a newer template version, preserving local changes.
    ///
    /// # Errors
    ///
    /// Fails when the project is not a clean git repository, lacks recorded template
    /// data, would be downgraded, or as [`Worker::run_copy`].
    pub fn run_update(mut self) -> Result<Self> {
        self.update()?;
        Ok(self)
    }
}

/// Adapter so a boxed prompter can live behind the shared trait object.
struct BoxedPrompter(Box<dyn Prompter>);

impl Prompter for BoxedPrompter {
    fn ask(
        &mut self,
        spec: &crate::prompt::PromptSpec<'_>,
    ) -> std::result::Result<Value, crate::prompt::PromptError> {
        self.0.ask(spec)
    }

    fn confirm(
        &mut self,
        message: &str,
        default: bool,
    ) -> std::result::Result<bool, crate::prompt::PromptError> {
        self.0.confirm(message, default)
    }
}

pub use update::get_update_data;
