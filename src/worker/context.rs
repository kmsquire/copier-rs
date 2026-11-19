//! The variables templates are rendered with (`Worker._render_context`).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use indexmap::IndexMap;
use minijinja::value::{Enumerator, Object, ObjectRepr, Value as JinjaValue};

use super::{Operation, Phase, Worker};
use crate::errors::{CopierError, Result};
use crate::jinja::{map_to_jinja, to_jinja, Context, DeferredError};
use crate::output;
use crate::questions::load_data_file;
use crate::tools;
use crate::value::{Mapping, Value};

/// One `_external_data` file, located but not read yet.
#[derive(Debug)]
enum ExternalFile {
    /// Readable: the project directory and the file's path relative to it.
    Allowed { dst_path: PathBuf, relpath: PathBuf },
    /// Outside the project and not trusted, or impossible to locate; the error is
    /// reported when a template actually reads the entry.
    Refused(Mutex<Option<CopierError>>),
}

/// The `_external_data` variable: files are read when a template first uses them.
#[derive(Debug)]
struct ExternalData {
    files: IndexMap<String, ExternalFile>,
    loaded: Mutex<IndexMap<String, JinjaValue>>,
    deferred: DeferredError,
}

impl Object for ExternalData {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Map
    }

    fn get_value(self: &Arc<Self>, key: &JinjaValue) -> Option<JinjaValue> {
        let name = key.as_str()?;
        if let Some(loaded) = self.loaded.lock().ok()?.get(name) {
            return Some(loaded.clone());
        }
        let value = match self.files.get(name)? {
            ExternalFile::Allowed { dst_path, relpath } => {
                to_jinja(&load_data_file(dst_path, relpath, true))
            }
            ExternalFile::Refused(error) => {
                if let Some(error) = error.lock().ok().and_then(|mut slot| slot.take()) {
                    self.deferred.set(error);
                }
                return None;
            }
        };
        self.loaded
            .lock()
            .ok()?
            .insert(name.to_owned(), value.clone());
        Some(value)
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        Enumerator::Values(
            self.files
                .keys()
                .map(|name| JinjaValue::from(name.as_str()))
                .collect(),
        )
    }
}

impl Worker {
    /// The full render context.
    pub(super) fn context(&self) -> Result<Context> {
        self.build_context(None)
    }

    /// The answers file path relative to the project, rendered.
    pub(super) fn answers_relpath(&self) -> Result<PathBuf> {
        let unrendered = match &self.answers_file {
            Some(path) => path.clone(),
            None => self.template_ref()?.answers_relpath()?,
        };
        // The path may use `_copier_conf`, which would otherwise contain this very path.
        let context = self.build_context(Some(""))?;
        Ok(PathBuf::from(
            self.env()?
                .render_str(&unrendered.to_string_lossy(), &context)?,
        ))
    }

    fn build_context(&self, answers_file: Option<&str>) -> Result<Context> {
        let template = self.template_ref()?;
        let subproject = self.subproject()?;
        let mut context = Context::new();
        context.extend_from(&self.answers.combined());
        context.insert_value("_copier_answers", &Value::Map(self.answers_to_remember()?));
        let answers_file = match answers_file {
            Some(path) => path.to_owned(),
            None => self.answers_relpath()?.to_string_lossy().into_owned(),
        };
        context.insert("_copier_conf", map_to_jinja(&self.conf(&answers_file)?));
        let folder_name = subproject
            .local_abspath
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
        context.insert_value(
            "_folder_name",
            &Value::from(folder_name.unwrap_or_default()),
        );
        context.insert_value("_copier_python", &Value::from(tools::python_executable()));
        context.insert_value("_copier_phase", &Value::from(Phase::current().as_str()));
        let external = template.external_data()?;
        let external_data = if external.is_empty() {
            map_to_jinja(&Mapping::new())
        } else {
            self.external_data(&external, &context)?
        };
        context.insert("_external_data", external_data);
        Ok(context)
    }

    /// `_copier_conf`: the options of this run.
    fn conf(&self, answers_file: &str) -> Result<Mapping> {
        let template = self.template_ref()?;
        let settings = self.user_settings()?;
        let text = |value: Option<String>| value.map_or(Value::Null, Value::from);
        let entries: Vec<(&str, Value)> = vec![
            (
                "src_path",
                Value::from(template.local_abspath()?.to_string_lossy().into_owned()),
            ),
            (
                "dst_path",
                Value::from(self.dst_path.to_string_lossy().into_owned()),
            ),
            ("answers_file", Value::from(answers_file)),
            ("vcs_ref", text(self.resolved_vcs_ref()?)),
            ("vcs_ref_hash", text(template.commit_hash()?)),
            ("data", Value::Map(self.data.clone())),
            (
                "settings",
                Value::map([
                    ("defaults", Value::Map(settings.defaults.clone())),
                    ("trust", Value::from(settings.trust.clone())),
                ]),
            ),
            ("exclude", Value::from(self.exclude.clone())),
            ("use_prereleases", Value::Bool(self.use_prereleases)),
            ("skip_if_exists", Value::from(self.skip_if_exists.clone())),
            ("cleanup_on_error", Value::Bool(self.cleanup_on_error)),
            ("defaults", Value::Bool(self.defaults)),
            ("user_defaults", Value::Map(self.user_defaults.clone())),
            ("overwrite", Value::Bool(self.overwrite)),
            ("pretend", Value::Bool(self.pretend)),
            ("quiet", Value::Bool(self.quiet)),
            ("conflict", Value::from(self.conflict.as_str())),
            ("context_lines", Value::Int(i64::from(self.context_lines))),
            ("unsafe", Value::Bool(self.unsafe_)),
            ("skip_answered", Value::Bool(self.skip_answered)),
            ("skip_tasks", Value::Bool(self.skip_tasks)),
            ("sep", Value::from(std::path::MAIN_SEPARATOR.to_string())),
            ("os", tools::OS.map_or(Value::Null, Value::from)),
        ];
        Ok(entries
            .into_iter()
            .map(|(key, value)| (Value::from(key), value))
            .collect())
    }

    /// `_copier_answers`: what the answers file records.
    pub(super) fn answers_to_remember(&self) -> Result<Mapping> {
        let template = self.template_ref()?;
        let mut answers = Mapping::new();
        if let Some(commit) = template.commit()? {
            answers.insert(Value::from("_commit"), Value::from(commit));
        }
        answers.insert(Value::from("_src_path"), Value::from(template.url.clone()));
        let secrets = template.secret_questions()?;
        let questions = template.questions_data()?;
        for (key, value) in self.answers.combined() {
            let Some(name) = key.as_str() else { continue };
            let recorded = !name.starts_with('_')
                && !self.answers.hidden.iter().any(|hidden| hidden == name)
                && !secrets.iter().any(|secret| secret == name)
                && questions.contains_key(&key)
                && !matches!(value, Value::Timestamp(_));
            if recorded {
                answers.insert(key, value);
            }
        }
        Ok(answers)
    }

    fn external_data(&self, declared: &Mapping, context: &Context) -> Result<JinjaValue> {
        let env = self.env()?;
        let project = &self.subproject()?.local_abspath;
        let undefined_phase =
            context.with("_copier_phase", JinjaValue::from(Phase::Undefined.as_str()));
        let mut files = IndexMap::new();
        for (name, path) in declared {
            let file = match env.render_str(&path.py_str(), &undefined_phase) {
                Err(error) => ExternalFile::Refused(Mutex::new(Some(error))),
                Ok(rendered) => {
                    let inside = tools::resolve(&self.dst_path.join(&rendered))
                        .starts_with(tools::resolve(project));
                    if inside || self.unsafe_ {
                        ExternalFile::Allowed {
                            dst_path: self.dst_path.clone(),
                            relpath: PathBuf::from(rendered),
                        }
                    } else {
                        ExternalFile::Refused(Mutex::new(Some(CopierError::ForbiddenPath {
                            path: PathBuf::from(rendered),
                            hint: "If you trust this path, you can override the check:\n  - CLI: `--trust`/`--UNSAFE`\n  - API: `trust=True`".to_owned(),
                        })))
                    }
                }
            };
            files.insert(name.py_str(), file);
        }
        Ok(JinjaValue::from_object(ExternalData {
            files,
            loaded: Mutex::new(IndexMap::new()),
            deferred: env.deferred_error(),
        }))
    }

    /// Render `source` with the run's context plus `extra` variables.
    pub(super) fn render_string(
        &self,
        source: &str,
        extra: &[(String, JinjaValue)],
    ) -> Result<String> {
        let mut context = self.context()?;
        for (name, value) in extra {
            context.insert(name.clone(), value.clone());
        }
        self.env()?.render_str(source, &context)
    }

    /// The `_copier_operation` variable, for the few places that expose it.
    pub(super) fn operation_variable() -> (String, JinjaValue) {
        let operation = Operation::current().map_or(JinjaValue::UNDEFINED, |operation| {
            JinjaValue::from(operation.as_str())
        });
        ("_copier_operation".to_owned(), operation)
    }

    /// Print a template-provided message to stderr, rendered, unless quiet.
    pub(super) fn print_message(&self, message: &str) -> Result<()> {
        if !message.is_empty() && !self.quiet {
            output::eprintln(&self.render_string(message, &[])?);
        }
        Ok(())
    }

    /// Whether `path` is the project directory or inside it.
    pub(super) fn is_inside_project(&self, path: &Path) -> Result<bool> {
        Ok(tools::resolve(path).starts_with(tools::resolve(&self.subproject()?.local_abspath)))
    }
}
