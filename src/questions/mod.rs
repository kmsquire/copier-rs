//! Questions and answers, mirroring `copier/_user_data.py`.

mod cast;
mod question;

use std::path::Path;

pub use cast::{cast, py_eq, TYPE_NAMES};
pub use question::{Choice, DefaultRendered, PromptKind, Question};

use crate::errors::{warn, WarningKind};
use crate::value::{Mapping, Value};
use crate::yaml;

/// Answers from every source, layered by priority.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnswersMap {
    /// Answers given interactively or computed in this run.
    pub user: Mapping,
    /// Answers passed in through `--data` or the API.
    pub init: Mapping,
    /// Template metadata (`_src_path`, `_commit`).
    pub metadata: Mapping,
    /// Answers recorded by the previous run.
    pub last: Mapping,
    /// Defaults supplied by the API caller.
    pub user_defaults: Mapping,
    /// Names of answers that must not be recorded.
    pub hidden: Vec<String>,
}

impl AnswersMap {
    /// All answers merged, highest priority first: `user`, `init`, `metadata`,
    /// `last`, `user_defaults`.
    pub fn combined(&self) -> Mapping {
        let mut combined = Mapping::new();
        for layer in [&self.user_defaults, &self.last, &self.metadata, &self.init, &self.user] {
            for (key, value) in layer {
                combined.insert(key.clone(), value.clone());
            }
        }
        combined
    }

    /// The template version recorded by the previous run.
    pub fn old_commit(&self) -> Option<String> {
        self.last.get(&Value::from("_commit")).map(Value::py_str)
    }

    /// Mark an answer as not to be recorded.
    pub fn hide(&mut self, name: &str) {
        if !self.hidden.iter().any(|hidden| hidden == name) {
            self.hidden.push(name.to_owned());
        }
    }
}

/// Read an answers file relative to `dst_path`; a missing file yields an empty mapping.
///
/// Pass `None` for the default `.copier-answers.yml`.
pub fn load_answersfile_data(dst_path: &Path, answers_file: Option<&Path>) -> Value {
    load_data_file(dst_path, answers_file.unwrap_or(Path::new(".copier-answers.yml")), false)
}

/// Read a YAML data file relative to `dst_path`, optionally warning when it is missing.
pub(crate) fn load_data_file(dst_path: &Path, relpath: &Path, warn_on_missing: bool) -> Value {
    let loaded = std::fs::read(dst_path.join(relpath))
        .ok()
        .and_then(|bytes| yaml::decode(&bytes).ok())
        .map(|text| yaml::load(&text));
    match loaded {
        Some(Ok(value)) => value,
        Some(Err(_)) => Value::Map(Mapping::new()),
        None => {
            if warn_on_missing {
                warn(WarningKind::MissingFile, format!("File not found; returning empty dict: {}", relpath.display()));
            }
            Value::Map(Mapping::new())
        }
    }
}
