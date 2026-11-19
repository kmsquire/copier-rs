//! Git integration, mirroring `copier/_vcs.py`. All work is delegated to the `git` CLI.

use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::LazyLock;

use regex::Regex;
use sha2::{Digest, Sha256};

use crate::dirs;
use crate::errors::{warn, CopierError, Result, WarningKind};
use crate::output;
use crate::version;

/// Name Copier commits under.
pub const GIT_USER_NAME: &str = "Copier";
/// Email Copier commits under.
pub const GIT_USER_EMAIL: &str = "copier@copier";
/// Prefix of the temporary directories templates are checked out into.
pub const CLONE_PREFIX: &str = "copier._vcs.clone.";
/// Environment variable overriding where git mirrors are cached.
pub const CACHE_DIR_ENV_VAR: &str = "COPIER_CACHE_DIR";

const GIT_PREFIXES: &[&str] = &[
    "git@",
    "git://",
    "git+",
    "https://github.com/",
    "https://gitlab.com/",
];
const GIT_POSTFIX: &str = ".git";
const ALIASES: &[(&str, &str)] = &[("gh:", "https://github.com"), ("gl:", "https://gitlab.com")];

/// A parsed `git --version`, e.g. `(2, 39, 2)`.
pub type GitVersion = (u64, u64, u64);

/// A `git` invocation with Copier's fixed identity and maintenance disabled.
#[derive(Debug, Clone, Default)]
pub struct Git {
    cwd: Option<PathBuf>,
    env: Vec<(OsString, OsString)>,
}

impl Git {
    /// Git running in the process's current directory.
    pub fn new() -> Self {
        Self::default()
    }

    /// Git running in `cwd`.
    pub fn in_dir(cwd: impl AsRef<Path>) -> Self {
        Self {
            cwd: Some(cwd.as_ref().to_path_buf()),
            env: Vec::new(),
        }
    }

    /// Add an environment variable to the invocation.
    pub fn env(mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> Self {
        self.env
            .push((key.as_ref().to_os_string(), value.as_ref().to_os_string()));
        self
    }

    fn command<S: AsRef<OsStr>>(&self, args: &[S]) -> Command {
        let mut command = Command::new("git");
        command
            .env("GIT_AUTHOR_NAME", GIT_USER_NAME)
            .env("GIT_AUTHOR_EMAIL", GIT_USER_EMAIL)
            .env("GIT_COMMITTER_NAME", GIT_USER_NAME)
            .env("GIT_COMMITTER_EMAIL", GIT_USER_EMAIL)
            .envs(self.env.iter().map(|(key, value)| (key, value)))
            .args(["-c", "maintenance.auto=false", "-c", "gc.auto=0"])
            .args(args);
        if let Some(cwd) = &self.cwd {
            command.current_dir(cwd);
        }
        command
    }

    /// Run git and return its raw output, whatever the exit status.
    ///
    /// # Errors
    ///
    /// Fails only when git cannot be started.
    pub fn output<S: AsRef<OsStr>>(&self, args: &[S]) -> Result<Output> {
        Ok(self.command(args).stdin(Stdio::null()).output()?)
    }

    /// Run git and return its stdout.
    ///
    /// # Errors
    ///
    /// Returns [`CopierError::Process`] when git exits with a non-zero status.
    pub fn run<S: AsRef<OsStr>>(&self, args: &[S]) -> Result<String> {
        let output = self.output(args)?;
        check(args, output)
    }

    /// Run git with `input` on stdin and return its stdout.
    ///
    /// # Errors
    ///
    /// Returns [`CopierError::Process`] when git exits with a non-zero status.
    pub fn run_with_stdin<S: AsRef<OsStr>>(&self, args: &[S], input: &[u8]) -> Result<String> {
        check(args, self.output_with_stdin(args, input)?)
    }

    /// Run git with `input` on stdin and return its raw output.
    ///
    /// # Errors
    ///
    /// Fails only when git cannot be started.
    pub fn output_with_stdin<S: AsRef<OsStr>>(&self, args: &[S], input: &[u8]) -> Result<Output> {
        let mut child = self
            .command(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        if let Some(mut stdin) = child.stdin.take() {
            // Git may exit before reading everything; its status tells the real story.
            let _ = stdin.write_all(input);
        }
        Ok(child.wait_with_output()?)
    }

    /// Whether git exits successfully.
    pub fn succeeds<S: AsRef<OsStr>>(&self, args: &[S]) -> bool {
        self.output(args)
            .is_ok_and(|output| output.status.success())
    }
}

fn check<S: AsRef<OsStr>>(args: &[S], output: Output) -> Result<String> {
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    let args: Vec<String> = args
        .iter()
        .map(|arg| arg.as_ref().to_string_lossy().into_owned())
        .collect();
    Err(CopierError::Process(format!(
        "Unexpected exit code: {}\nCommand line: git {}\nStdout: {}\nStderr: {}",
        output
            .status
            .code()
            .map_or_else(|| "signal".to_owned(), |code| code.to_string()),
        args.join(" "),
        String::from_utf8_lossy(&output.stdout).trim_end(),
        String::from_utf8_lossy(&output.stderr).trim_end(),
    )))
}

/// Run `git` with Copier's fixed identity and settings in `cwd`, returning stdout.
///
/// # Errors
///
/// Returns [`CopierError::Process`] when git exits with a non-zero status.
pub fn git(cwd: &Path, args: &[&str]) -> Result<String> {
    Git::in_dir(cwd).run(args)
}

/// Whether a `git` executable is on `PATH`.
pub fn is_git_available() -> bool {
    let names: &[&str] = if cfg!(windows) {
        &["git.exe", "git"]
    } else {
        &["git"]
    };
    std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|dir| names.iter().any(|name| dir.join(name).is_file()))
    })
}

/// Version of the installed git.
///
/// # Errors
///
/// Fails when git cannot be run or prints no recognizable version.
pub fn get_git_version() -> Result<GitVersion> {
    parse_git_version(&Git::new().run(&["version"])?)
}

/// Extract the version from the output of `git version`.
///
/// # Errors
///
/// Returns [`CopierError::Value`] when no version number is present.
pub fn parse_git_version(output: &str) -> Result<GitVersion> {
    static VERSION: LazyLock<Regex> = LazyLock::new(|| compile(r"(\d+)\.(\d+)(?:\.(\d+))?"));
    let captures = VERSION
        .captures(output)
        .ok_or_else(|| CopierError::Value(format!("Cannot parse git version from {output:?}")))?;
    let number = |index: usize| {
        captures
            .get(index)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0)
    };
    Ok((number(1), number(2), number(3)))
}

fn compile(pattern: &str) -> Regex {
    // Patterns are literals, so they always compile.
    #[allow(clippy::expect_used)]
    Regex::new(pattern).expect("valid vcs regex")
}

fn toplevel(path: &Path) -> Option<PathBuf> {
    let output = Git::in_dir(path)
        .output(&["rev-parse", "--show-toplevel"])
        .ok()?;
    output
        .status
        .success()
        .then(|| PathBuf::from(String::from_utf8_lossy(&output.stdout).trim()))
}

/// Whether `path` is inside a git work tree.
pub fn is_in_git_repo(path: &Path) -> bool {
    toplevel(path).is_some()
}

/// Whether `path` is the top level of a git repository.
pub fn is_git_repo_root(path: &Path) -> bool {
    match (
        toplevel(path).map(|top| top.canonicalize()),
        path.canonicalize(),
    ) {
        (Some(Ok(top)), Ok(resolved)) => top == resolved,
        _ => false,
    }
}

/// Whether `path` is a shallow clone.
pub fn is_git_shallow_repo(path: &Path) -> bool {
    Git::in_dir(path)
        .output(&["rev-parse", "--is-shallow-repository"])
        .is_ok_and(|output| {
            output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "true"
        })
}

/// Whether `path` is a valid git bundle file.
pub fn is_git_bundle(path: &Path) -> bool {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let Ok(scratch) = tempfile::Builder::new()
        .prefix("copier._vcs.is_git_bundle.")
        .tempdir()
    else {
        return false;
    };
    let git = Git::in_dir(scratch.path());
    git.succeeds(&["init"])
        && git.succeeds(&[
            OsStr::new("bundle"),
            OsStr::new("verify"),
            OsStr::new("--"),
            path.as_os_str(),
        ])
}

/// Normalize a template location to a clonable git URL.
///
/// Expands `gh:`/`gl:` shortcuts and `git+` prefixes; local repositories and bundles
/// come back as POSIX paths. Returns `None` when `url` is not a git repository
/// (a plain directory template).
pub fn get_repo(url: &str) -> Option<String> {
    let mut url = url.to_owned();
    for (alias, replacement) in ALIASES {
        if let Some(rest) = url.strip_prefix(alias) {
            let rest = rest.strip_prefix('/').unwrap_or(rest);
            let postfix = if rest.ends_with(GIT_POSTFIX) {
                ""
            } else {
                GIT_POSTFIX
            };
            url = format!("{replacement}/{rest}{postfix}");
        }
    }
    if url.ends_with(GIT_POSTFIX) || GIT_PREFIXES.iter().any(|prefix| url.starts_with(prefix)) {
        if let Some(stripped) = url.strip_prefix("git+") {
            return Some(stripped.to_owned());
        }
        if url.starts_with("https://") && !url.ends_with(GIT_POSTFIX) {
            return Some(format!("{url}{GIT_POSTFIX}"));
        }
        return Some(url);
    }
    let path = PathBuf::from(dirs::expand_user(&url));
    if is_git_repo_root(&path) || is_git_bundle(&path) {
        return Some(path.to_string_lossy().replace('\\', "/"));
    }
    None
}

/// Whether `url` refers to a remote repository rather than an existing local path.
pub fn is_remote(url: &str) -> bool {
    !Path::new(url).exists()
}

fn cache_dir() -> PathBuf {
    match std::env::var_os(CACHE_DIR_ENV_VAR).filter(|value| !value.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => dirs::user_cache_dir().join("git"),
    }
}

/// Remove `user:password@` from a URL so credentials never shape the cache layout.
fn strip_credentials(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_owned();
    };
    let host_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    match rest[..host_end].rsplit_once('@') {
        Some((_, host)) => format!("{scheme}://{host}{}", &rest[host_end..]),
        None => url.to_owned(),
    }
}

/// Path of the mirror cache for `url`.
pub fn get_mirror_path(url: &str) -> PathBuf {
    let digest = Sha256::digest(strip_credentials(url).as_bytes());
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    cache_dir().join(format!("{hex}.git"))
}

fn is_valid_mirror(mirror: &Path) -> bool {
    mirror.join("objects").is_dir()
        && Git::new()
            .env("GIT_DIR", mirror)
            .output(&["rev-parse", "--is-bare-repository"])
            .is_ok_and(|output| {
                output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "true"
            })
}

/// Get the cached mirror of `url`, creating or refreshing it.
fn get_or_create_mirror(url: &str) -> Result<PathBuf> {
    let mirror = get_mirror_path(url);
    if is_valid_mirror(&mirror) {
        let git = Git::new().env("GIT_DIR", &mirror);
        git.run(&["remote", "update", "--prune"])?;
        git.run(&["worktree", "prune"])?;
        return Ok(mirror);
    }
    if mirror.exists() {
        remove_dir_all(&mirror)?;
    }
    let parent = mirror.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let staging = tempfile::Builder::new()
        .prefix(CLONE_PREFIX)
        .tempdir_in(parent)?;
    let staging_repo = staging.path().join("repo.git");
    Git::new().run(&[
        OsStr::new("clone"),
        OsStr::new("--mirror"),
        OsStr::new("--"),
        OsStr::new(url),
        staging_repo.as_os_str(),
    ])?;
    if let Err(err) = std::fs::rename(&staging_repo, &mirror) {
        // A concurrent run may have published the same mirror first.
        if !is_valid_mirror(&mirror) {
            return Err(err.into());
        }
    }
    Ok(mirror)
}

/// Delete a directory tree, making read-only entries writable when needed.
pub fn remove_dir_all(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(path) {
        Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
            make_writable(path);
            std::fs::remove_dir_all(path)
        }
        other => other,
    }
}

fn make_writable(path: &Path) {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return;
    };
    if metadata.is_symlink() {
        return;
    }
    let mut permissions = metadata.permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(permissions.mode() | 0o700);
    }
    #[cfg(not(unix))]
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    let _ = std::fs::set_permissions(path, permissions);
    if metadata.is_dir() {
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                make_writable(&entry.path());
            }
        }
    }
}

/// Newest PEP 440 tag of the repository at `url`, or `HEAD` when it has no tags.
///
/// # Errors
///
/// Fails when the repository cannot be listed.
pub fn get_latest_tag(url: &str, use_prereleases: bool) -> Result<String> {
    let url = match Path::new(url).canonicalize() {
        Ok(resolved) => resolved.to_string_lossy().replace('\\', "/"),
        Err(_) => url.to_owned(),
    };
    let listing = Git::new().run(&["ls-remote", "--tags", "--refs", "--", &url])?;
    let mut tags: Vec<(version::Version, &str)> = listing
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(_, name)| name.strip_prefix("refs/tags/").unwrap_or(name))
        .filter_map(|tag| version::parse(tag).map(|parsed| (parsed, tag)))
        .filter(|(parsed, _)| use_prereleases || !version::is_prerelease(parsed))
        .collect();
    // Stable sort, descending: equal versions keep git's listing order, like Python.
    tags.sort_by(|(a, _), (b, _)| b.cmp(a));
    match tags.first() {
        Some((_, tag)) => Ok((*tag).to_owned()),
        None => {
            output::eprintln(&output::style::warn(
                "No git tags found in template; using HEAD as ref",
            ));
            Ok("HEAD".to_owned())
        }
    }
}

/// Resolve `git_ref` to a commit ID in the repository at `repo`.
fn resolve_ref(repo: &Path, git_ref: &str) -> Result<String> {
    static OBJECT_ID: LazyLock<Regex> = LazyLock::new(|| compile(r"^[0-9a-f]{40,64}$"));
    if !git_ref.contains(['\r', '\n']) {
        for candidate in [git_ref.to_owned(), format!("refs/remotes/origin/{git_ref}")] {
            let output = Git::in_dir(repo).run_with_stdin(
                &["cat-file", "--batch-check=%(objectname)"],
                format!("{candidate}^{{commit}}\n").as_bytes(),
            )?;
            if OBJECT_ID.is_match(output.trim()) {
                return Ok(output.trim().to_owned());
            }
        }
    }
    Err(CopierError::Value(format!(
        "Unknown revision: {}",
        crate::value::py_str_repr(git_ref)
    )))
}

fn update_submodules(location: &Path) -> Result<()> {
    Git::in_dir(location).run(&[
        "submodule",
        "update",
        "--checkout",
        "--init",
        "--recursive",
        "--force",
    ])?;
    Ok(())
}

/// Check out `git_ref` (default `HEAD`) of `url` into `location`, or into a new
/// temporary directory, and return the checkout's path.
///
/// Remote repositories go through a cached mirror; local ones are cloned directly,
/// including uncommitted changes when checking out `HEAD`.
///
/// # Errors
///
/// Fails when git fails or the ref does not exist.
pub fn clone(url: &str, git_ref: Option<&str>, location: Option<&Path>) -> Result<PathBuf> {
    let git_ref = git_ref.unwrap_or("HEAD");
    let git_version = get_git_version()?;
    let location = match location {
        Some(location) => location.to_path_buf(),
        None => tempfile::Builder::new()
            .prefix(CLONE_PREFIX)
            .tempdir()?
            .keep(),
    };
    if is_remote(url) {
        let mirror = get_or_create_mirror(url)?;
        // `git worktree add` refuses an existing path, even an empty directory.
        if location.exists() {
            std::fs::remove_dir(&location)?;
        }
        Git::new().env("GIT_DIR", &mirror).run(&[
            OsStr::new("worktree"),
            OsStr::new("add"),
            OsStr::new("--detach"),
            OsStr::new("--force"),
            OsStr::new("--"),
            location.as_os_str(),
            OsStr::new(git_ref),
        ])?;
        Git::in_dir(&location).run(&["submodule", "sync", "--recursive"])?;
        update_submodules(&location)?;
        return Ok(location);
    }
    let mut clone_args: Vec<&OsStr> = vec![OsStr::new("clone"), OsStr::new("--no-checkout")];
    if git_version >= (2, 27, 0) {
        let file_url = url.strip_prefix("file://").unwrap_or(url);
        if is_git_shallow_repo(Path::new(file_url)) {
            warn(
                WarningKind::ShallowClone,
                format!(
                    "The repository '{url}' is a shallow clone, this might lead to unexpected failure or unusually high resource consumption."
                ),
            );
        } else {
            clone_args.push(OsStr::new("--filter=blob:none"));
        }
    }
    clone_args.extend([OsStr::new("--"), OsStr::new(url), location.as_os_str()]);
    Git::new().run(&clone_args)?;
    let url_abspath = std::path::absolute(url)?;
    if git_ref == "HEAD" && url_abspath.is_dir() {
        let is_dirty = !Git::in_dir(&url_abspath)
            .run(&["status", "--porcelain"])?
            .trim()
            .is_empty();
        if is_dirty {
            let git = Git::in_dir(&location).env("GIT_WORK_TREE", &url_abspath);
            git.run(&["--git-dir=.git", "add", "-A"])?;
            git.run(&[
                "--git-dir=.git",
                "commit",
                "-m",
                "Copier automated commit for draft changes",
                "--no-verify",
                "--no-gpg-sign",
            ])?;
            warn(
                WarningKind::DirtyLocal,
                "Dirty template changes included automatically.",
            );
        }
    }
    let commit = resolve_ref(&location, git_ref)?;
    Git::in_dir(&location).run(&["-c", "core.fsmonitor=false", "checkout", "-f", &commit])?;
    update_submodules(&location)?;
    Ok(location)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_url_normalization() {
        for (input, expected) in [
            (
                "gh:copier-org/copier",
                "https://github.com/copier-org/copier.git",
            ),
            (
                "gh:/copier-org/copier.git",
                "https://github.com/copier-org/copier.git",
            ),
            ("gl:group/project", "https://gitlab.com/group/project.git"),
            (
                "git@github.com:copier-org/copier.git",
                "git@github.com:copier-org/copier.git",
            ),
            ("git+https://example.com/repo", "https://example.com/repo"),
            (
                "https://github.com/copier-org/copier",
                "https://github.com/copier-org/copier.git",
            ),
            (
                "https://example.com/repo.git",
                "https://example.com/repo.git",
            ),
        ] {
            assert_eq!(get_repo(input).as_deref(), Some(expected), "{input}");
        }
    }

    #[test]
    fn git_version_parsing() {
        assert_eq!(
            parse_git_version("git version 2.39.2 (Apple Git-143)").unwrap(),
            (2, 39, 2)
        );
        assert_eq!(parse_git_version("git version 2.27").unwrap(), (2, 27, 0));
        assert!(parse_git_version("no digits").is_err());
    }

    #[test]
    fn credentials_do_not_affect_the_mirror_path() {
        assert_eq!(
            get_mirror_path("https://user:secret@example.com/repo.git"),
            get_mirror_path("https://example.com/repo.git")
        );
        assert_eq!(
            strip_credentials("git@github.com:org/repo.git"),
            "git@github.com:org/repo.git"
        );
    }
}
