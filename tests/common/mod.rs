//! Shared helpers for the ported test suite; the counterpart of Python's
//! `tests/helpers.py` and `tests/conftest.py`.

#![allow(dead_code, unused_macros, unused_imports)]

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use copier::output::Captured;
use copier::{CopierError, Value, Worker};

/// Legacy template suffix used by the demo templates.
pub const SUFFIX_TMPL: &str = ".tmpl";

/// Legacy bracket delimiters, as JSON for inlining into a `copier.yml`.
pub const BRACKET_ENVOPS_JSON: &str = r##"{"autoescape": false, "block_end_string": "%]", "block_start_string": "[%", "comment_end_string": "#]", "comment_start_string": "[#", "keep_trailing_newline": true, "variable_end_string": "]]", "variable_start_string": "[["}"##;

/// Name of the answers file template in a default-syntax template.
pub const ANSWERS_FILE_TMPL: &str = "{{ _copier_conf.answers_file }}.jinja";

/// Identity used for commits made by tests.
pub const GIT_USER_NAME: &str = "John Doe";
/// Email used for commits made by tests.
pub const GIT_USER_EMAIL: &str = "john.doe@example.com";

/// Default seconds to wait for expected output from an interactive session.
pub const SPAWN_TIMEOUT: u64 = 10;

/// Directory holding the static fixture templates (`demo`, `demo_exclude`, ...).
pub fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

/// The base demo template (`PROJECT_TEMPLATE` in Python).
pub fn project_template() -> PathBuf {
    fixtures().join("demo")
}

/// A fixture template by directory name, e.g. `fixture("demo_exclude")`.
pub fn fixture(name: &str) -> PathBuf {
    fixtures().join(name)
}

/// Shorthand for converting a `serde_json::json!` literal into a [`Value`].
pub fn v(json: serde_json::Value) -> Value {
    json.into()
}

/// Temporary directories for one test; removed on drop.
pub struct Tmp {
    guard: tempfile::TempDir,
    root: PathBuf,
}

/// Make `python` resolve to the Python Copier checkout's interpreter (which has
/// PyYAML) for the tasks and migrations that templates in the tests run.
fn ensure_python_on_path() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let venv_bin = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(".venv")
            .join("bin");
        if let (Ok(venv_bin), Some(path)) = (venv_bin.canonicalize(), std::env::var_os("PATH")) {
            let mut dirs = vec![venv_bin];
            dirs.extend(std::env::split_paths(&path));
            if let Ok(joined) = std::env::join_paths(dirs) {
                std::env::set_var("PATH", joined);
            }
        }
    });
}

impl Tmp {
    /// Create an empty temporary root.
    pub fn new() -> Self {
        ensure_python_on_path();
        let guard = tempfile::tempdir().expect("create temp dir");
        // Resolved because copier records resolved paths (macOS /var -> /private/var).
        let root = guard.path().canonicalize().expect("resolve temp dir");
        Self { guard, root }
    }

    /// The temporary root itself (`tmp_path` in pytest).
    pub fn path(&self) -> &Path {
        &self.root
    }

    /// Create and return a fresh subdirectory (`tmp_path_factory.mktemp(name)`).
    pub fn mktemp(&self, name: &str) -> PathBuf {
        let mut dir = self.root.join(name);
        let mut index = 0;
        while dir.exists() {
            index += 1;
            dir = self.root.join(format!("{name}{index}"));
        }
        std::fs::create_dir_all(&dir).expect("create temp subdir");
        dir
    }

    /// Create the usual `src` and `dst` directories.
    pub fn src_dst(&self) -> (PathBuf, PathBuf) {
        (self.mktemp("src"), self.mktemp("dst"))
    }
}

/// Contents of one entry of a [`build_file_tree`] spec.
pub enum Content {
    /// Text, dedented before writing.
    Text(String),
    /// Raw bytes, written as is.
    Bytes(Vec<u8>),
    /// A symlink pointing at this target.
    Symlink(PathBuf),
}

impl From<&str> for Content {
    fn from(text: &str) -> Self {
        Self::Text(text.to_owned())
    }
}

impl From<String> for Content {
    fn from(text: String) -> Self {
        Self::Text(text)
    }
}

impl From<&String> for Content {
    fn from(text: &String) -> Self {
        Self::Text(text.clone())
    }
}

impl From<&[u8]> for Content {
    fn from(bytes: &[u8]) -> Self {
        Self::Bytes(bytes.to_vec())
    }
}

impl<const N: usize> From<&[u8; N]> for Content {
    fn from(bytes: &[u8; N]) -> Self {
        Self::Bytes(bytes.to_vec())
    }
}

impl From<Vec<u8>> for Content {
    fn from(bytes: Vec<u8>) -> Self {
        Self::Bytes(bytes)
    }
}

/// A symlink entry for [`build_file_tree`] (a `Path` value in the Python spec).
pub fn symlink(target: impl AsRef<Path>) -> Content {
    Content::Symlink(target.as_ref().to_path_buf())
}

/// Port of Python's `textwrap.dedent`: strip the whitespace prefix common to all
/// non-blank lines, and reduce whitespace-only lines to bare newlines.
pub fn dedent(text: &str) -> String {
    let mut margin: Option<&str> = None;
    for line in text.split('\n') {
        if line.trim_matches([' ', '\t']).is_empty() {
            continue;
        }
        let indent = &line[..line.len() - line.trim_start_matches([' ', '\t']).len()];
        margin = Some(match margin {
            None => indent,
            Some(current) => {
                let common = current
                    .bytes()
                    .zip(indent.bytes())
                    .take_while(|(a, b)| a == b)
                    .count();
                &current[..common]
            }
        });
    }
    let margin = margin.unwrap_or("");
    let lines: Vec<&str> = text
        .split('\n')
        .map(|line| {
            if line.trim_matches([' ', '\t']).is_empty() {
                ""
            } else {
                line.strip_prefix(margin).unwrap_or(line)
            }
        })
        .collect();
    lines.join("\n")
}

/// Create files, symlinks and their parent directories.
pub fn build_file_tree(spec: Vec<(PathBuf, Content)>) {
    build_file_tree_with(spec, true);
}

/// [`build_file_tree`] with control over dedenting (`dedent=False` in Python).
pub fn build_file_tree_with(spec: Vec<(PathBuf, Content)>, dedent_text: bool) {
    for (path, content) in spec {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent dirs");
        }
        match content {
            Content::Text(text) => {
                let text = if dedent_text { dedent(&text) } else { text };
                std::fs::write(&path, text).expect("write file");
            }
            Content::Bytes(bytes) => std::fs::write(&path, bytes).expect("write file"),
            Content::Symlink(target) => {
                #[cfg(unix)]
                std::os::unix::fs::symlink(&target, &path).expect("create symlink");
                #[cfg(windows)]
                std::os::windows::fs::symlink_file(&target, &path).expect("create symlink");
            }
        }
    }
}

/// Build a file tree: `file_tree! { src.join("copier.yml") => "a: 1", ... }`.
///
/// Keys are anything convertible to `PathBuf`; values are `&str`/`String` (dedented),
/// byte strings (raw), or `symlink(target)`.
macro_rules! file_tree {
    ($($path:expr => $content:expr),* $(,)?) => {
        $crate::common::build_file_tree(vec![
            $((::std::path::PathBuf::from($path), $crate::common::Content::from($content))),*
        ])
    };
}
pub(crate) use file_tree;

/// Remove inherited git variables (set when tests run from a git hook) so child
/// processes cannot act on the surrounding repository; the config isolation
/// variables stay.
fn scrub_git_env(command: &mut Command) {
    for (name, _) in std::env::vars_os() {
        let name = name.to_string_lossy();
        if name.starts_with("GIT_") && !name.starts_with("GIT_CONFIG_") {
            command.env_remove(name.as_ref());
        }
    }
}

fn git_command(cwd: &Path) -> Command {
    let mut command = Command::new("git");
    scrub_git_env(&mut command);
    command
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", GIT_USER_NAME)
        .env("GIT_AUTHOR_EMAIL", GIT_USER_EMAIL)
        .env("GIT_COMMITTER_NAME", GIT_USER_NAME)
        .env("GIT_COMMITTER_EMAIL", GIT_USER_EMAIL);
    command
}

/// Run `git` in `cwd`; `Err` carries stderr when the command fails.
pub fn try_git(cwd: impl AsRef<Path>, args: &[&str]) -> Result<String, String> {
    let output = git_command(cwd.as_ref())
        .args(args)
        .output()
        .expect("run git");
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).into_owned())
    }
}

/// Run `git` in `cwd` and return stdout; panics when the command fails.
pub fn git(cwd: impl AsRef<Path>, args: &[&str]) -> String {
    match try_git(cwd, args) {
        Ok(stdout) => stdout,
        Err(stderr) => panic!("git {args:?} failed: {stderr}"),
    }
}

/// `git init`, `git add .`, commit and optionally tag (`git_save` in Python).
pub fn git_save(dst: impl AsRef<Path>, message: &str, tag: Option<&str>) {
    git_save_with(dst, message, tag, false);
}

/// [`git_save`] with control over `--allow-empty`.
pub fn git_save_with(dst: impl AsRef<Path>, message: &str, tag: Option<&str>, allow_empty: bool) {
    let dst = dst.as_ref();
    git(dst, &["init"]);
    git(dst, &["add", "."]);
    let mut args = vec!["commit", "-m", message];
    if allow_empty {
        args.push("--allow-empty");
    }
    git(dst, &args);
    if let Some(tag) = tag {
        git(dst, &["tag", tag]);
    }
}

/// Initialize a repository in `dir` with a first commit (`git_init` in Python).
pub fn git_init(dir: impl AsRef<Path>, message: &str) {
    git_save(dir, message, None);
}

/// Read the answers file of a generated project (`.copier-answers.yml`).
pub fn load_answers(dst: impl AsRef<Path>) -> Value {
    copier::load_answersfile_data(dst.as_ref(), None)
}

/// Read a custom answers file of a generated project.
pub fn load_answers_from(dst: impl AsRef<Path>, answers_file: impl AsRef<Path>) -> Value {
    copier::load_answersfile_data(dst.as_ref(), Some(answers_file.as_ref()))
}

/// Read a file as UTF-8 text (`Path.read_text()`).
pub fn read(path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    std::fs::read_to_string(path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

/// Shortcut for `Worker::new().src_path(src).dst_path(dst)`.
pub fn worker(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> Worker {
    Worker::new().src_path(src).dst_path(dst)
}

/// Data used to render the demo template (`DATA` in Python, minus `make_secret`,
/// which is a built-in global in this port).
pub fn demo_data() -> Value {
    v(serde_json::json!({
        "py3": true,
        "myvar": "awesome",
        "what": "world",
        "project_name": "Copier",
        "version": "2.0.0",
        "description": "A library for rendering projects templates",
    }))
}

/// Worker that renders the demo template quietly into `dst` (`render()` in Python);
/// chain more options, then call `.run_copy()`.
pub fn demo_worker(dst: impl AsRef<Path>) -> Worker {
    worker(project_template(), dst)
        .data(demo_data())
        .quiet(true)
}

/// Assert that a rendered file is byte-identical to its demo template source.
pub fn assert_file(dst: impl AsRef<Path>, relpath: &str) {
    let rendered = std::fs::read(dst.as_ref().join(relpath)).expect("read rendered file");
    let source = std::fs::read(project_template().join(relpath)).expect("read template file");
    assert_eq!(rendered, source, "{relpath} differs from the template");
}

/// Whether the display text of `err` matches `pattern` (Python `re.search`).
pub fn err_matches(err: &CopierError, pattern: &str) -> bool {
    regex::Regex::new(pattern)
        .expect("valid regex")
        .is_match(&err.to_string())
}

/// Assert that `$result` is an `Err` matching the given pattern
/// (`pytest.raises(Variant, match=regex)`):
/// `assert_err!(result, CopierError::UserMessage(_), r"regex")`; the regex is optional.
macro_rules! assert_err {
    ($result:expr, $pattern:pat) => {
        match $result {
            Ok(_) => panic!("expected an error matching {}", stringify!($pattern)),
            Err(err) => assert!(matches!(err, $pattern), "unexpected error: {err:?}"),
        }
    };
    ($result:expr, $pattern:pat, $regex:expr) => {
        match $result {
            Ok(_) => panic!("expected an error matching {}", stringify!($pattern)),
            Err(err) => {
                assert!(matches!(err, $pattern), "unexpected error: {err:?}");
                assert!(
                    $crate::common::err_matches(&err, $regex),
                    "error {:?} does not match {:?}",
                    err.to_string(),
                    $regex
                );
            }
        }
    };
}
pub(crate) use assert_err;

/// Run the CLI in-process (`CopierApp.run([...], exit=False)`), returning the exit
/// code and captured output. `args` excludes the program name.
pub fn run_cli(args: &[&str]) -> (i32, Captured) {
    let argv: Vec<String> = std::iter::once("copier")
        .chain(args.iter().copied())
        .map(str::to_owned)
        .collect();
    copier::output::capture(|| copier::cli::run(argv))
}

/// The built `copier` binary as a subprocess command (`COPIER_CMD` in Python).
pub fn copier_cmd() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_copier"));
    scrub_git_env(&mut command);
    command
}

/// Raw key sequences, as prompt_toolkit names them.
pub mod keyboard {
    /// Ctrl-H.
    pub const CONTROL_H: &str = "\x08";
    /// Ctrl-I.
    pub const CONTROL_I: &str = "\t";
    /// Ctrl-C.
    pub const CONTROL_C: &str = "\x03";
    /// Enter.
    pub const ENTER: &str = "\r";
    /// Escape.
    pub const ESC: &str = "\x1b";
    /// Home.
    pub const HOME: &str = "\x1b[H";
    /// End.
    pub const END: &str = "\x1b[F";
    /// Arrow up.
    pub const UP: &str = "\x1b[A";
    /// Arrow down.
    pub const DOWN: &str = "\x1b[B";
    /// Arrow right.
    pub const RIGHT: &str = "\x1b[C";
    /// Arrow left.
    pub const LEFT: &str = "\x1b[D";
    /// Alt is sent as an Escape prefix.
    pub const ALT: &str = ESC;
    /// Backspace.
    pub const BACKSPACE: &str = CONTROL_H;
    /// Tab.
    pub const TAB: &str = CONTROL_I;
}

/// An interactive `copier` session over pipes: the counterpart of pexpect's
/// `PopenSpawn`. Stdout and stderr are merged into one stream.
pub struct Tui {
    child: Child,
    stdin: Option<ChildStdin>,
    output: Receiver<Vec<u8>>,
    buffer: Vec<u8>,
    timeout: Duration,
}

/// Start `copier` with `args` as an interactive session (`spawn(COPIER_PATH + args)`).
pub fn spawn(args: &[&str]) -> Tui {
    spawn_with(args, SPAWN_TIMEOUT, None)
}

/// [`spawn`] with a custom timeout in seconds and working directory.
pub fn spawn_with(args: &[&str], timeout_secs: u64, cwd: Option<&Path>) -> Tui {
    let mut command = copier_cmd();
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let mut child = command.spawn().expect("spawn copier");
    let (sender, output) = mpsc::channel();
    let streams: [Box<dyn Read + Send>; 2] = [
        Box::new(child.stdout.take().expect("piped stdout")),
        Box::new(child.stderr.take().expect("piped stderr")),
    ];
    for mut stream in streams {
        let sender = sender.clone();
        std::thread::spawn(move || {
            let mut chunk = [0u8; 4096];
            while let Ok(read) = stream.read(&mut chunk) {
                if read == 0 || sender.send(chunk[..read].to_vec()).is_err() {
                    break;
                }
            }
        });
    }
    let stdin = child.stdin.take();
    Tui {
        child,
        stdin,
        output,
        buffer: Vec::new(),
        timeout: Duration::from_secs(timeout_secs),
    }
}

impl Tui {
    /// Wait until `needle` appears in the output, consuming everything up to and
    /// including it; panics on timeout or end of output.
    pub fn expect_exact(&mut self, needle: &str) {
        let needle = needle.as_bytes();
        let deadline = Instant::now() + self.timeout;
        loop {
            if let Some(position) = find(&self.buffer, needle) {
                self.buffer.drain(..position + needle.len());
                return;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.output.recv_timeout(remaining) {
                Ok(chunk) => self.buffer.extend(chunk),
                Err(RecvTimeoutError::Timeout) => panic!(
                    "timed out waiting for {:?}; pending output: {:?}",
                    String::from_utf8_lossy(needle),
                    String::from_utf8_lossy(&self.buffer)
                ),
                Err(RecvTimeoutError::Disconnected) => panic!(
                    "output ended while waiting for {:?}; pending output: {:?}",
                    String::from_utf8_lossy(needle),
                    String::from_utf8_lossy(&self.buffer)
                ),
            }
        }
    }

    /// Send raw text without a line ending.
    pub fn send(&mut self, text: &str) {
        let stdin = self.stdin.as_mut().expect("stdin already closed");
        stdin
            .write_all(text.as_bytes())
            .expect("write to copier stdin");
        stdin.flush().expect("flush copier stdin");
    }

    /// Send text followed by a newline (`sendline`); pass `""` for a bare Enter.
    pub fn sendline(&mut self, text: &str) {
        self.send(&format!("{text}\n"));
    }

    /// Wait for the process to finish (`expect_exact(pexpect.EOF)`) and return all
    /// output not yet consumed; panics on timeout.
    pub fn expect_eof(&mut self) -> String {
        let deadline = Instant::now() + self.timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.output.recv_timeout(remaining) {
                Ok(chunk) => self.buffer.extend(chunk),
                Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => panic!(
                    "timed out waiting for EOF; pending output: {:?}",
                    String::from_utf8_lossy(&self.buffer)
                ),
            }
        }
        String::from_utf8_lossy(&std::mem::take(&mut self.buffer)).into_owned()
    }

    /// Close stdin, wait for exit and return the exit code.
    pub fn wait(&mut self) -> i32 {
        self.stdin.take();
        self.child
            .wait()
            .expect("wait for copier")
            .code()
            .unwrap_or(-1)
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Check for a prompt in the standard form (`expect_prompt` in Python): the help
/// text when given, otherwise the name followed by `(type)` unless the type is `str`.
pub fn expect_prompt(tui: &mut Tui, name: &str, expected_type: &str, help: Option<&str>) {
    match help.filter(|help| !help.is_empty()) {
        Some(help) => tui.expect_exact(help),
        None => {
            tui.expect_exact(name);
            if expected_type != "str" {
                tui.expect_exact(&format!("({expected_type})"));
            }
        }
    }
}
