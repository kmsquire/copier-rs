# Decisions

Dated, with reasons. Newest last.

## 2026-11-19

- **Nested standalone repo** at `copier-rs/`, branch `main`, never pushed. The parent
  repo's remote is upstream Copier; keeping the port separate avoids touching it.
- **Follow the Python code over the docs** where they disagree (`context_lines`
  default is 3; `update -f` means `--defaults`).
- **Binary `copier`, library crate `copier`, package `copier-rs`.** Reports Copier
  compatibility version `9.19.1` (`copier::COPIER_VERSION`).
- **Git via the `git` CLI.** The update algorithm is built from `git apply --reject`,
  `merge-file`, `diff-tree` and index manipulation; a library would risk drift.
- **Structurally equivalent YAML output** (owner). The emitter follows PyYAML style
  for common cases but does not port its line folding. A project created by Python
  Copier may get its answers file reformatted once.
- **Python-only features deferred when obscure** (owner). Arbitrary Python Jinja
  extensions cannot be loaded; they raise `ExtensionNotFound`. See `BACKLOG.md`.
- **`_copier_python`** is `python3`/`python` from `PATH`, or empty.
- **Template engine: minijinja 3.1 + minijinja-contrib `pycompat`.** Spike results:
  custom delimiters, `keep_trailing_newline`, `trim_blocks`, `lstrip_blocks`, line
  statements, lenient and strict undefined, slicing, `.items()`, `.get()`,
  `.format()`, `dict()`, `namespace()`, macros all work. Missing, to add through
  our own unknown-method callback or defer: `%` string formatting, `str.center`,
  `str.encode`, `list.append`, `{% do %}`, `lipsum`/`cycler`/`joiner` globals.
  minijinja has no custom tags, so `{% yield %}` is handled by a pre-pass on path
  components.
- **YAML parsing on `saphyr-parser` events** (scalar style, tags, anchors, spans are
  all exposed). Resolution to values (YAML 1.1 rules) and emission are ours.
- **Library API shape.** `Worker` is a plain options struct with chainable setters;
  `run_copy`/`run_recopy`/`run_update` consume and return it, like Python returns the
  worker. Errors are one `CopierError` enum whose variants mirror Python's exception
  classes, plus `Value`/`Type`/`Key` for the builtin exceptions Python raises.
- **Output and warnings are capturable per thread** (`output::capture`,
  `errors::capture_warnings`), replacing pytest's `capsys` and `pytest.warns`.
- **Tests are ported ignored** (`#[ignore = "pending"]`) so `main` is always green
  and `cargo test -- --include-ignored` shows overall progress.
- **Interactive tests use pipes, not a pty**, like Python's `PopenSpawn`; the helper
  is `tests/common::Tui`. The prompt layer must therefore read keys from a plain pipe.

## 2026-11-19 (implementation)

- **Phases 2 to 6 on one branch.** The ported tests are end-to-end, so they could not
  be un-ignored layer by layer; splitting the branch afterwards would have produced
  merges whose tests do not run. Commits stay one per layer.
- **Help text and completions are embedded from Python Copier** (`src/cli/help/`),
  rendered at width 80. They are static, and this guarantees identical text.
  Regenerate with `copier <cmd> --help` if upstream changes them.
- **`copier --version` prints `copier 9.19.1 (copier-rs 0.1.0)`**: scripts that parse
  the first two words keep working, and the port is not passed off as the original.
- **Environment-dependent behavior has thread-scoped overrides** (`settings::with_locations`,
  `tools::with_os`, `tools::with_temp_root`, `vcs::without_git`, `vcs::with_git_env`,
  `template::with_copier_version`) instead of tests mutating the process environment.
- **`_copier_conf.src_path / 'x'`** (pathlib division in templates) is rewritten to a
  `pathjoin` call before rendering; minijinja has no operator overloading.
- **Prompts print plain text when stdin or stdout is not a terminal**, as
  prompt_toolkit does, and redraw in place otherwise.
- **Warnings go to the real stderr**, not through the capturable output channel,
  matching how Python's warnings stay out of `capsys`.
- **Questions without a `when` match and without a prompt take their default**, as
  questionary does when `when` is false.

## 2026-11-19 (interrupts and performance)

- **Ctrl-C is recorded, not fatal** (`src/interrupt.rs`): a process-wide flag set by
  a signal handler, checked at safe points, surfacing as `CopierError::Interrupted`.
  The flag stays pending during a run, so code that ignores a failing git command
  cannot lose it, and is always consumed when a top-level call (`run_*`,
  `get_update_data`) returns: any error of an interrupted run is reported as
  `Interrupted`, a run that completed keeps its result. Repository probes ignore
  the flag so a pending interrupt is not read as "not a repository". A second
  Ctrl-C exits with status 130. Like Python (whose cleanup is
  `except Exception`), an interrupted copy does not delete the destination; only
  temporary directories go.
- **Git stays on the command line.** Profiling showed the time was not in git's work
  but in (a) `git submodule update`, a shell script on older gits, now skipped
  without `.gitmodules`; (b) Apple's `/usr/bin/git` launcher, 21 ms per call versus
  7.5 ms for the binary behind it, now resolved once; (c) cloning each template
  version up to five times per update, now shared between the sub-runs.
- **`tools::resolve` follows `realpath` rules** (2026-11-19): the longest existing
  prefix is canonicalized in one call, then the rest is walked per component so a
  symlink reached after `..` is followed. The lexical shortcut it replaced was a
  path-traversal hole (see `test_rendered_path_cannot_escape_through_a_symlink_after_dot_dot`).
- **Submodule commands are skipped without `.gitmodules`.** One known difference: a
  tree with gitlinks but no `.gitmodules` makes Python fail ("no submodule mapping
  found"); this port copies it without the submodule content.
- **`git` lookup results are cached per process** (`is_git_available`, the resolved
  macOS binary). A host that changes `PATH` mid-process will not see the change.
