//! Template discovery and configuration, mirroring `copier/_template.py`.

mod config;
mod version;

use std::cell::{OnceCell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

pub use config::{filter_config, load_template_config};

use crate::errors::{warn, CopierError, Result, WarningKind};
use crate::tools::split_nul;
use crate::value::{Mapping, Value};
use crate::vcs::{self, Git};
use crate::version::{parse as parse_version, Version};

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

thread_local! {
    static COPIER_VERSION_OVERRIDE: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Run `f` as if this were Copier `version`, for exercising `_min_copier_version`.
pub fn with_copier_version<T>(version: &str, f: impl FnOnce() -> T) -> T {
    let previous =
        COPIER_VERSION_OVERRIDE.with(|slot| slot.borrow_mut().replace(version.to_owned()));
    let result = f();
    COPIER_VERSION_OVERRIDE.with(|slot| *slot.borrow_mut() = previous);
    result
}

/// The Copier version templates are checked against.
pub fn copier_version() -> String {
    COPIER_VERSION_OVERRIDE
        .with(|slot| slot.borrow().clone())
        .unwrap_or_else(|| crate::COPIER_VERSION.to_owned())
}

/// Check the running Copier against a template's `_min_copier_version`.
///
/// # Errors
///
/// Returns [`CopierError::UnsupportedVersion`] when Copier is too old.
pub fn verify_copier_version(required: &str) -> Result<()> {
    let installed_text = copier_version();
    let invalid = |text: &str| CopierError::Value(format!("Invalid version: '{text}'"));
    let installed = parse_version(&installed_text).ok_or_else(|| invalid(&installed_text))?;
    if parse_version("0.0.0").as_ref() == Some(&installed) {
        warn(
            WarningKind::UnknownCopierVersion,
            "Cannot check Copier version constraint.",
        );
        return Ok(());
    }
    let minimum = parse_version(required).ok_or_else(|| invalid(required))?;
    if installed < minimum {
        return Err(CopierError::UnsupportedVersion(format!(
            "This template requires Copier version >= {required}, while your version of Copier is {installed}."
        )));
    }
    if installed.release().first() > minimum.release().first() {
        warn(
            WarningKind::OldTemplate,
            format!(
                "This template was designed for Copier {required}, but your version of Copier is {installed}. You could find some incompatibilities."
            ),
        );
    }
    Ok(())
}

/// The command of a [`Task`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Run through the shell.
    Shell(String),
    /// Run directly, without a shell.
    Argv(Vec<String>),
}

impl Command {
    fn from_value(value: &Value) -> Result<Self> {
        match value {
            Value::Str(text) => Ok(Self::Shell(text.clone())),
            Value::Seq(items) => Ok(Self::Argv(items.iter().map(Value::py_str).collect())),
            other => Err(CopierError::Validation(format!(
                "Task command must be a string or a list, got {}",
                other.py_type_name()
            ))),
        }
    }
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

impl Task {
    /// A task that always runs in the project root.
    pub fn new(cmd: Command, extra_vars: Mapping) -> Self {
        Self {
            cmd,
            extra_vars,
            condition: Value::Bool(true),
            working_directory: PathBuf::new(),
        }
    }
}

/// When a migration runs relative to the update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Before the new template version is applied.
    Before,
    /// After it.
    After,
}

impl Stage {
    /// The name templates and commands see (`before` / `after`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Before => "before",
            Self::After => "after",
        }
    }
}

/// Deletes a temporary template checkout when the last reference goes away.
#[derive(Debug)]
struct CloneGuard(PathBuf);

impl Drop for CloneGuard {
    fn drop(&mut self) {
        if self.0.exists() {
            // Best effort: a leftover temporary directory must not fail the run.
            let _ = vcs::remove_dir_all(&self.0);
        }
    }
}

#[derive(Debug, Default)]
struct Cache {
    is_git: OnceCell<bool>,
    url_expanded: OnceCell<String>,
    local_abspath: OnceCell<PathBuf>,
    clone_guard: OnceCell<CloneGuard>,
    raw_config: OnceCell<Mapping>,
    config_data: OnceCell<Mapping>,
    commit: OnceCell<Option<String>>,
    version: OnceCell<Option<Version>>,
}

/// A template: where it comes from and what its configuration says.
///
/// Derived data (the local checkout, parsed configuration, version) is computed on
/// first use and shared between clones.
#[derive(Debug, Clone)]
pub struct Template {
    /// Location as given by the user (path or URL).
    pub url: String,
    /// Git ref to use; `None` selects the latest tag.
    pub git_ref: Option<String>,
    /// Whether prerelease tags count when selecting the latest tag.
    pub use_prereleases: bool,
    cache: Rc<Cache>,
}

impl PartialEq for Template {
    fn eq(&self, other: &Self) -> bool {
        self.url == other.url
            && self.git_ref == other.git_ref
            && self.use_prereleases == other.use_prereleases
    }
}

fn cached<T: Clone>(cell: &OnceCell<T>, compute: impl FnOnce() -> Result<T>) -> Result<T> {
    if let Some(value) = cell.get() {
        return Ok(value.clone());
    }
    let value = compute()?;
    Ok(cell.get_or_init(|| value).clone())
}

fn strings(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Seq(items)) => items.iter().map(Value::py_str).collect(),
        Some(Value::Str(text)) => text.chars().map(String::from).collect(),
        _ => Vec::new(),
    }
}

impl Template {
    /// Describe the template at `url`.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            git_ref: None,
            use_prereleases: false,
            cache: Rc::default(),
        }
    }

    /// Use a specific git ref.
    pub fn git_ref(mut self, git_ref: impl Into<String>) -> Self {
        self.git_ref = Some(git_ref.into());
        self.cache = Rc::default();
        self
    }

    /// Count prerelease tags when selecting the latest tag.
    pub fn use_prereleases(mut self, yes: bool) -> Self {
        self.use_prereleases = yes;
        self.cache = Rc::default();
        self
    }

    /// Whether the template is a git repository.
    pub fn is_git(&self) -> bool {
        *self
            .cache
            .is_git
            .get_or_init(|| vcs::is_git_available() && vcs::get_repo(&self.url).is_some())
    }

    /// The clonable form of [`Template::url`].
    pub fn url_expanded(&self) -> String {
        self.cache
            .url_expanded
            .get_or_init(|| vcs::get_repo(&self.url).unwrap_or_else(|| self.url.clone()))
            .clone()
    }

    /// Local checkout of the template.
    ///
    /// # Errors
    ///
    /// Fails when cloning fails or the location is not a directory.
    pub fn local_abspath(&self) -> Result<PathBuf> {
        cached(&self.cache.local_abspath, || {
            let mut result = PathBuf::from(&self.url);
            if self.is_git() {
                let location = tempfile::Builder::new()
                    .prefix(vcs::CLONE_PREFIX)
                    .tempdir()?
                    .keep();
                // Registered before cloning so a failed clone is cleaned up too.
                let _ = self.cache.clone_guard.set(CloneGuard(location.clone()));
                let url = self.url_expanded();
                let git_ref = match &self.git_ref {
                    Some(git_ref) => git_ref.clone(),
                    None => vcs::get_latest_tag(&url, self.use_prereleases)?,
                };
                result = vcs::clone(&url, Some(&git_ref), Some(&location))?;
            }
            if !result.is_dir() {
                return Err(CopierError::Value(
                    "Local template must be a directory.".to_owned(),
                ));
            }
            Ok(result.canonicalize().unwrap_or(result))
        })
    }

    fn raw_config(&self) -> Result<Mapping> {
        cached(&self.cache.raw_config, || {
            let root = self.local_abspath()?;
            let mut conf_paths: Vec<PathBuf> = std::fs::read_dir(&root)?
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| {
                    let name = path
                        .file_name()
                        .map(|name| name.to_string_lossy().to_lowercase())
                        .unwrap_or_default();
                    path.is_file()
                        && name.strip_prefix("copier.").is_some_and(|rest| {
                            let suffix = rest.rsplit('.').next().unwrap_or(rest);
                            suffix.starts_with("yml") || suffix.starts_with("yaml")
                        })
                })
                .collect();
            conf_paths.sort();
            match conf_paths.as_slice() {
                [] => Ok(Mapping::new()),
                [conf_path] => match load_template_config(conf_path, false)? {
                    Value::Map(map) => Ok(map),
                    _ => Ok(Mapping::new()),
                },
                _ => {
                    let listing: Vec<String> = conf_paths
                        .iter()
                        .map(|path| path.display().to_string())
                        .collect();
                    crate::output::printf_exception(
                        &listing.join(", "),
                        "MULTIPLE CONFIG FILES",
                        &listing.join(", "),
                    );
                    Err(CopierError::MultipleConfigFiles { conf_paths })
                }
            }
        })
    }

    /// Settings from `copier.yml` (keys starting with `_`, underscore stripped).
    ///
    /// # Errors
    ///
    /// Fails on invalid configuration or an unsatisfied `_min_copier_version`.
    pub fn config_data(&self) -> Result<Mapping> {
        cached(&self.cache.config_data, || {
            let (config, _) = filter_config(&self.raw_config()?);
            if let Some(required) = config.get(&Value::from("min_copier_version")) {
                verify_copier_version(&required.py_str())?;
            }
            Ok(config)
        })
    }

    fn setting(&self, name: &str) -> Result<Option<Value>> {
        Ok(self.config_data()?.get(&Value::from(name)).cloned())
    }

    fn text_setting(&self, name: &str) -> Result<String> {
        Ok(self
            .setting(name)?
            .map(|value| value.py_str())
            .unwrap_or_default())
    }

    /// Questions from `copier.yml`, each normalized to a mapping.
    ///
    /// # Errors
    ///
    /// Fails on invalid configuration.
    pub fn questions_data(&self) -> Result<Mapping> {
        let (config, mut questions) = filter_config(&self.raw_config()?);
        for name in strings(config.get(&Value::from("secret_questions"))) {
            if let Some(Value::Map(question)) = questions.get_mut(&Value::from(name)) {
                question.insert(Value::from("secret"), Value::Bool(true));
            }
        }
        Ok(questions)
    }

    /// Jinja environment options (`_envops`), with `keep_trailing_newline` on by default.
    ///
    /// # Errors
    ///
    /// Fails on invalid configuration.
    pub fn envops(&self) -> Result<Mapping> {
        let mut envops = match self.setting("envops")? {
            Some(Value::Map(map)) => map,
            _ => Mapping::new(),
        };
        envops
            .entry(Value::from("keep_trailing_newline"))
            .or_insert(Value::Bool(true));
        Ok(envops)
    }

    /// Exclusion patterns (`_exclude`); the defaults apply only without a subdirectory.
    ///
    /// # Errors
    ///
    /// Fails on invalid configuration.
    pub fn exclude(&self) -> Result<Vec<String>> {
        match self.setting("exclude")? {
            Some(value) => Ok(strings(Some(&value))),
            None if self.subdirectory()?.is_empty() || self.subdirectory()? == "." => {
                Ok(DEFAULT_EXCLUDE
                    .iter()
                    .map(|pattern| (*pattern).to_owned())
                    .collect())
            }
            None => Ok(Vec::new()),
        }
    }

    /// Paths of external data files by name (`_external_data`).
    ///
    /// # Errors
    ///
    /// Fails on invalid configuration.
    pub fn external_data(&self) -> Result<Mapping> {
        Ok(match self.setting("external_data")? {
            Some(Value::Map(map)) => map,
            _ => Mapping::new(),
        })
    }

    /// Patterns never overwritten when they exist (`_skip_if_exists`).
    ///
    /// # Errors
    ///
    /// Fails on invalid configuration.
    pub fn skip_if_exists(&self) -> Result<Vec<String>> {
        Ok(strings(self.setting("skip_if_exists")?.as_ref()))
    }

    /// Requested Jinja extensions (`_jinja_extensions`).
    ///
    /// # Errors
    ///
    /// Fails on invalid configuration.
    pub fn jinja_extensions(&self) -> Result<Vec<String>> {
        Ok(strings(self.setting("jinja_extensions")?.as_ref()))
    }

    /// Names of questions whose answers are secret, in declaration order.
    ///
    /// # Errors
    ///
    /// Fails on invalid configuration.
    pub fn secret_questions(&self) -> Result<Vec<String>> {
        let mut secrets = strings(self.setting("secret_questions")?.as_ref());
        for (name, question) in &self.questions_data()? {
            let name = name.py_str();
            if question.get("secret").is_some_and(Value::is_truthy) && !secrets.contains(&name) {
                secrets.push(name);
            }
        }
        secrets.dedup();
        Ok(secrets)
    }

    /// Subdirectory holding the files to render (`_subdirectory`), unrendered.
    ///
    /// # Errors
    ///
    /// Fails on invalid configuration.
    pub fn subdirectory(&self) -> Result<String> {
        self.text_setting("subdirectory")
    }

    /// Answers file path relative to the project (`_answers_file`), unrendered.
    ///
    /// # Errors
    ///
    /// Fails on invalid configuration or an absolute path.
    pub fn answers_relpath(&self) -> Result<PathBuf> {
        let path = PathBuf::from(match self.setting("answers_file")? {
            Some(value) => value.py_str(),
            None => ".copier-answers.yml".to_owned(),
        });
        if path.is_absolute() {
            return Err(CopierError::PathNotRelative { path });
        }
        Ok(path)
    }

    /// Templates suffix (`_templates_suffix`).
    ///
    /// # Errors
    ///
    /// Fails on invalid configuration.
    pub fn templates_suffix(&self) -> Result<String> {
        Ok(match self.setting("templates_suffix")? {
            None | Some(Value::Null) => DEFAULT_TEMPLATES_SUFFIX.to_owned(),
            Some(value) => value.py_str(),
        })
    }

    /// Whether symlinks are copied as symlinks (`_preserve_symlinks`).
    ///
    /// # Errors
    ///
    /// Fails on invalid configuration.
    pub fn preserve_symlinks(&self) -> Result<bool> {
        Ok(self
            .setting("preserve_symlinks")?
            .is_some_and(|value| value.is_truthy()))
    }

    /// Message shown at the given point of a run (`_message_before_copy`, ...).
    ///
    /// # Errors
    ///
    /// Fails on invalid configuration.
    pub fn message(&self, when: &str) -> Result<String> {
        self.text_setting(&format!("message_{when}"))
    }

    /// Tasks to run after rendering (`_tasks`).
    ///
    /// # Errors
    ///
    /// Fails on invalid configuration.
    pub fn tasks(&self) -> Result<Vec<Task>> {
        let extra_vars: Mapping = [(Value::from("stage"), Value::from("task"))]
            .into_iter()
            .collect();
        let Some(Value::Seq(items)) = self.setting("tasks")? else {
            return Ok(Vec::new());
        };
        items
            .iter()
            .map(|item| match item {
                Value::Map(_) => Ok(Task {
                    cmd: Command::from_value(
                        item.get("command")
                            .ok_or_else(|| CopierError::Key("'command'".to_owned()))?,
                    )?,
                    extra_vars: extra_vars.clone(),
                    condition: item
                        .get("when")
                        .cloned()
                        .unwrap_or_else(|| Value::from("true")),
                    working_directory: PathBuf::from(
                        item.get("working_directory")
                            .map_or_else(|| ".".to_owned(), Value::py_str),
                    ),
                }),
                other => Ok(Task::new(Command::from_value(other)?, extra_vars.clone())),
            })
            .collect()
    }

    /// Migrations to run at `stage` when updating from `from_template` to this one.
    ///
    /// # Errors
    ///
    /// Fails on invalid configuration.
    pub fn migration_tasks(&self, stage: Stage, from_template: &Template) -> Result<Vec<Task>> {
        let (Some(to_version), Some(from_version)) = (self.version()?, from_template.version()?)
        else {
            return Ok(Vec::new());
        };
        let text = |value: Option<String>| value.map_or(Value::Null, Value::from);
        let mut extra_vars: Mapping = [
            ("stage", Value::from(stage.as_str())),
            ("version_from", text(from_template.commit()?)),
            ("version_to", text(self.commit()?)),
            ("version_pep440_from", Value::from(from_version.to_string())),
            ("version_pep440_to", Value::from(to_version.to_string())),
        ]
        .into_iter()
        .map(|(key, value)| (Value::from(key), value))
        .collect();
        let envops = self.envops()?;
        let delimiter = |key: &str, default: &str| {
            envops
                .get(&Value::from(key))
                .map_or_else(|| default.to_owned(), Value::py_str)
        };
        let default_condition = Value::from(format!(
            "{} _stage == \"after\" {}",
            delimiter("variable_start_string", "{{"),
            delimiter("variable_end_string", "}}")
        ));
        let in_range = |declared: &Value| -> Result<Option<Version>> {
            let current = parse_version(&declared.py_str()).ok_or_else(|| {
                CopierError::Value(format!("Invalid version: '{}'", declared.py_str()))
            })?;
            Ok((to_version >= current && current > from_version).then_some(current))
        };
        let mut result = Vec::new();
        let Some(Value::Seq(migrations)) =
            self.raw_config()?.get(&Value::from("_migrations")).cloned()
        else {
            return Ok(result);
        };
        for migration in &migrations {
            let is_legacy = migration.contains_key("before") || migration.contains_key("after");
            if is_legacy {
                warn(
                    WarningKind::Deprecation,
                    "This migration configuration is deprecated. Please switch to the new format.",
                );
                let declared = migration
                    .get("version")
                    .ok_or_else(|| CopierError::Key("'version'".to_owned()))?;
                if let Some(current) = in_range(declared)? {
                    extra_vars.insert(Value::from("version_current"), declared.clone());
                    extra_vars.insert(
                        Value::from("version_pep440_current"),
                        Value::from(current.to_string()),
                    );
                    if let Some(Value::Seq(commands)) = migration.get(stage.as_str()) {
                        for command in commands {
                            result
                                .push(Task::new(Command::from_value(command)?, extra_vars.clone()));
                        }
                    }
                }
                continue;
            }
            let Value::Map(_) = migration else {
                result.push(Task {
                    condition: default_condition.clone(),
                    ..Task::new(Command::from_value(migration)?, extra_vars.clone())
                });
                continue;
            };
            if let Some(declared) = migration.get("version") {
                let Some(current) = in_range(declared)? else {
                    continue;
                };
                extra_vars.insert(Value::from("version_current"), declared.clone());
                extra_vars.insert(
                    Value::from("version_pep440_current"),
                    Value::from(current.to_string()),
                );
            }
            result.push(Task {
                cmd: Command::from_value(
                    migration
                        .get("command")
                        .ok_or_else(|| CopierError::Key("'command'".to_owned()))?,
                )?,
                extra_vars: extra_vars.clone(),
                condition: migration
                    .get("when")
                    .cloned()
                    .unwrap_or_else(|| default_condition.clone()),
                working_directory: PathBuf::from(
                    migration
                        .get("working_directory")
                        .map_or_else(|| ".".to_owned(), Value::py_str),
                ),
            });
        }
        Ok(result)
    }

    /// `git describe` of the checked-out template, if it is a git repository.
    ///
    /// # Errors
    ///
    /// Fails when the checkout or git fails.
    pub fn commit(&self) -> Result<Option<String>> {
        cached(&self.cache.commit, || {
            if !self.is_git() {
                return Ok(None);
            }
            let describe =
                Git::in_dir(self.local_abspath()?).run(&["describe", "--tags", "--always"])?;
            Ok(Some(describe.trim().to_owned()))
        })
    }

    /// Full commit hash of the checked-out template, if it is a git repository.
    ///
    /// # Errors
    ///
    /// Fails when the checkout or git fails.
    pub fn commit_hash(&self) -> Result<Option<String>> {
        if !self.is_git() {
            return Ok(None);
        }
        Ok(Some(
            Git::in_dir(self.local_abspath()?)
                .run(&["rev-parse", "HEAD"])?
                .trim()
                .to_owned(),
        ))
    }

    /// PEP 440 version of the checked-out template, if it has one.
    ///
    /// # Errors
    ///
    /// Fails when the checkout or git fails.
    pub fn version(&self) -> Result<Option<Version>> {
        cached(&self.cache.version, || match self.commit()? {
            Some(commit) if !commit.is_empty() => {
                Ok(version::template_version(&self.local_abspath()?, &commit))
            }
            _ => Ok(None),
        })
    }

    /// Data recorded in the answers file to find the template again.
    ///
    /// # Errors
    ///
    /// Fails when the checkout or git fails.
    pub fn metadata(&self) -> Result<Mapping> {
        let mut metadata = Mapping::new();
        metadata.insert(Value::from("_src_path"), Value::from(self.url.clone()));
        if let Some(commit) = self.commit()?.filter(|commit| !commit.is_empty()) {
            metadata.insert(Value::from("_commit"), Value::from(commit));
        }
        Ok(metadata)
    }

    /// File modes recorded in the template's git index, by POSIX path. Empty for
    /// non-git templates.
    pub fn git_index_modes(&self) -> HashMap<String, u32> {
        let mut modes = HashMap::new();
        if !self.is_git() {
            return modes;
        }
        let Ok(root) = self.local_abspath() else {
            return modes;
        };
        let Ok(listing) = Git::in_dir(root).run(&["ls-files", "-z", "--stage"]) else {
            return modes;
        };
        for line in split_nul(&listing) {
            if let Some((meta, path)) = line.split_once('\t') {
                let mode = meta
                    .split(' ')
                    .next()
                    .and_then(|mode| u32::from_str_radix(mode, 8).ok());
                if let Some(mode) = mode {
                    modes.insert(path.to_owned(), mode);
                }
            }
        }
        modes
    }
}
