//! The project being generated or updated, mirroring `copier/_subproject.py`.

use std::cell::OnceCell;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::errors::Result;
use crate::questions::load_answersfile_data;
use crate::template::Template;
use crate::value::{Mapping, Value};
use crate::vcs::{self, Git};

/// A destination directory and what its answers file says about its origin.
#[derive(Debug)]
pub struct Subproject {
    /// Absolute path of the project.
    pub local_abspath: PathBuf,
    /// Answers file, relative to the project.
    pub answers_relpath: PathBuf,
    last_answers: OnceCell<Mapping>,
    template: OnceCell<Option<Template>>,
}

impl Subproject {
    /// Describe the project at `local_abspath`.
    pub fn new(local_abspath: PathBuf, answers_relpath: PathBuf) -> Self {
        Self {
            local_abspath,
            answers_relpath,
            last_answers: OnceCell::new(),
            template: OnceCell::new(),
        }
    }

    /// Answers recorded by the last run: `_src_path`, `_commit` and every public answer.
    pub fn last_answers(&self) -> &Mapping {
        self.last_answers.get_or_init(|| {
            let Value::Map(raw) =
                load_answersfile_data(&self.local_abspath, Some(&self.answers_relpath))
            else {
                return Mapping::new();
            };
            raw.into_iter()
                .filter(|(key, _)| match key.as_str() {
                    Some(name) => {
                        name == "_src_path" || name == "_commit" || !name.starts_with('_')
                    }
                    None => true,
                })
                .collect()
        })
    }

    /// Forget the cached answers so the file is read again.
    pub fn reload_answers(&mut self) {
        self.last_answers = OnceCell::new();
    }

    /// The template the project was generated from, at the recorded version.
    pub fn template(&self) -> Option<&Template> {
        self.template
            .get_or_init(|| {
                let answers = self.last_answers();
                let url = answers
                    .get(&Value::from("_src_path"))
                    .filter(|url| url.is_truthy())?;
                let template = Template::new(url.py_str());
                Some(
                    match answers
                        .get(&Value::from("_commit"))
                        .filter(|commit| !commit.is_null())
                    {
                        Some(commit) => template.git_ref(commit.py_str()),
                        None => template,
                    },
                )
            })
            .as_ref()
    }

    /// Whether the project lives inside a git repository.
    pub fn is_git(&self) -> bool {
        vcs::is_in_git_repo(&self.local_abspath)
    }

    /// Whether the project has uncommitted changes.
    ///
    /// # Errors
    ///
    /// Fails when git cannot report the status.
    pub fn is_dirty(&self) -> Result<bool> {
        if !self.is_git() {
            return Ok(false);
        }
        let status = Git::in_dir(&self.local_abspath).run(&[
            OsStr::new("status"),
            OsStr::new("--porcelain"),
            OsStr::new("--"),
            self.local_abspath.as_os_str(),
        ])?;
        Ok(!status.trim().is_empty())
    }

    /// The project path.
    pub fn path(&self) -> &Path {
        &self.local_abspath
    }
}
