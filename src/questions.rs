//! Questions and answers, mirroring `copier/_user_data.py`.

use std::path::Path;

use crate::value::{Mapping, Value};

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
        todo!("AnswersMap::combined")
    }
}

/// Read an answers file relative to `dst_path`; a missing file yields an empty mapping.
///
/// Pass `None` for the default `.copier-answers.yml`.
pub fn load_answersfile_data(_dst_path: &Path, _answers_file: Option<&Path>) -> Value {
    todo!("load_answersfile_data")
}
