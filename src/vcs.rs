//! Git integration, mirroring `copier/_vcs.py`. All work is delegated to the `git` CLI.

use std::path::{Path, PathBuf};

use crate::errors::Result;

/// A parsed `git --version`, e.g. `(2, 39, 2)`.
pub type GitVersion = (u64, u64, u64);

/// Whether a `git` executable is on `PATH`.
pub fn is_git_available() -> bool {
    todo!("is_git_available")
}

/// Version of the installed git.
pub fn get_git_version() -> Result<GitVersion> {
    todo!("get_git_version")
}

/// Extract the version from the output of `git --version`.
pub fn parse_git_version(_output: &str) -> Result<GitVersion> {
    todo!("parse_git_version")
}

/// Run `git` with Copier's fixed identity and settings in `cwd`, returning stdout.
pub fn git(_cwd: &Path, _args: &[&str]) -> Result<String> {
    todo!("git")
}

/// Whether `path` is inside a git work tree.
pub fn is_in_git_repo(_path: &Path) -> bool {
    todo!("is_in_git_repo")
}

/// Whether `path` is the top level of a git repository.
pub fn is_git_repo_root(_path: &Path) -> bool {
    todo!("is_git_repo_root")
}

/// Whether `path` is a shallow clone.
pub fn is_git_shallow_repo(_path: &Path) -> bool {
    todo!("is_git_shallow_repo")
}

/// Whether `path` is a git bundle file.
pub fn is_git_bundle(_path: &Path) -> bool {
    todo!("is_git_bundle")
}

/// Normalize a template location to a clonable git URL.
///
/// Expands `gh:`/`gl:` shortcuts and `git+` prefixes; returns `None` when `url` is
/// not a git repository (a plain directory template).
pub fn get_repo(_url: &str) -> Option<String> {
    todo!("get_repo")
}

/// Whether `url` refers to a remote repository rather than a local path.
pub fn is_remote(_url: &str) -> bool {
    todo!("is_remote")
}

/// Path of the mirror cache for `url`.
pub fn get_mirror_path(_url: &str) -> PathBuf {
    todo!("get_mirror_path")
}

/// Newest PEP 440 tag of the repository at `url`, or `HEAD` when it has no tags.
pub fn get_latest_tag(_url: &str, _use_prereleases: bool) -> Result<String> {
    todo!("get_latest_tag")
}

/// Check out `git_ref` of `url` into a temporary directory (or `location`) and
/// return its path.
pub fn clone(_url: &str, _git_ref: Option<&str>, _location: Option<&Path>) -> Result<PathBuf> {
    todo!("clone")
}
