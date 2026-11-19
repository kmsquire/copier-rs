# Scratchpad

Working notes for whoever (human or agent) continues this port. Read this first.
Keep it current: update at every phase end and whenever something here goes stale.

## What this is

`copier-rs` is a Rust port of Copier (Python, v9.19.1, upstream commit `dbb2408`).
Goal: a drop-in, faster `copier` CLI. The Python original is the parent directory
(`../copier`, `../tests`, `../docs`) and is the behavioral reference.

This directory is its own git repository (branch `main`), nested inside the Python
checkout and hidden from it via `../.git/info/exclude`. Never push: the parent's
`origin` is upstream `copier-org/copier`, and this repo has no remote.

The approved plan: `~/.claude/plans/please-create-a-complete-nifty-melody.md`.

## Status (2026-11-19)

| Phase | State |
|---|---|
| 0 Setup and spikes | done |
| 1 Port all tests | done, merged |
| 2 Foundations | done |
| 3 Template, Jinja, questions | done |
| 4 Copy, recopy, CLI | done |
| 5 Update, migrations, check-update | done |
| 6 Interactive prompts | done (pipe mode tested; real-terminal mode only smoke-tested by hand) |
| 7 Parity, performance, polish | not started |

Phases 2 to 6 were developed together on one branch (`phase-2-foundations`), because
the ported tests exercise the whole stack and could only be turned on once copy,
update, the CLI and the prompts all existed. The commits are still one per layer.

`cargo test`: 1,219 passing, 0 failing, 28 ignored. `notes/TEST_LEDGER.md` lists
every ignored test with its reason (regenerate with `python3 tools/gen_ledger.py`).
A first parity check (copy plus update with a local edit) produced byte-identical
trees to Python Copier; release build is about 2x faster on copy and 1.4x on update,
with git subprocesses dominating.

## Where the code is

- `copier-rs/` is the main checkout (`main`); `copier-rs-wt/` is a worktree used for
  the implementation branch. After merging, the worktree can be removed with
  `git worktree remove ../copier-rs-wt` from `copier-rs/`.
- `tools/unignore.py <module>...` runs a module with `--include-ignored` and removes
  `#[ignore = "pending"]` from tests that pass.

## Phase 7 work list

1. Performance: profile a copy. Suspects: the render context is rebuilt for every
   rendered string (`worker/context.rs`), `Template::is_git` and `vcs::get_repo`
   spawn several git processes (including `git bundle verify` in a scratch repo)
   for plain directories, and `sync_git_index_executable_bit` runs git per file.
2. Parity harness (`parity/`): run Python Copier and this binary over the fixture
   templates and synthesized ones, compare trees (YAML/JSON structurally).
3. A pty-backed test session for `test_prompt::test_path_completion` and
   `test_prompt_does_not_capture_mouse`; exercise the real-terminal redraw path.
4. SIGINT handling: a Ctrl-C during a task or clone should clean up like Python
   does (`test_cleanup` has three ignored tests waiting on this).
5. Native versions of popular Jinja extensions (see `BACKLOG.md`).
6. README with usage, differences from Python Copier, and benchmarks.
7. Review follow-ups below.

## Review follow-ups

- Move duplicated test helpers into `tests/common`: JSON-to-`Mapping`, recursive
  directory copy, `bracket_envops`, git-config writers.
- `tests/common::Tui` reads stdout and stderr on separate threads; if a test flakes
  on ordering, merge the streams at the OS level instead.
- Platform marks: Windows-only and macOS-only expectations are `cfg`-gated or
  commented; nothing has been run on Linux or Windows yet.

## How to work here

- Build/test commands and conventions: `AGENTS.md`.
- Python oracle: `../.venv/bin/copier` and `../.venv/bin/python -m pytest -n 0 ../tests/test_x.py`
  (created with `uv sync --frozen` in the parent; devbox is not installed and not needed).
- Tests are ported 1:1 from `../tests`. Each test for unimplemented behavior has
  `#[ignore = "pending"]`; a phase removes the attribute from the modules it makes pass.
  `notes/TEST_LEDGER.md` tracks every Python test function.
- Test isolation comes from `.cargo/config.toml`: a fixed `GIT_CONFIG_GLOBAL`
  (`tests/fixtures/gitconfig`) and a private `COPIER_CACHE_DIR` under `target/`.
- Phase procedure: branch `phase-N-<name>` off `main`; small conventional commits;
  at the end run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, the
  phase's tests, then the `/review-by-jeremy` and `/thermos` skills; fix findings
  into the commits they belong to; fast-forward merge into `main`.
- Cost: the owner has other agents running. Keep at most two subagents active.
  Haiku for mechanical test porting, reviewed against the Python source.

## Things that are easy to get wrong

- User-facing message text must match Python exactly; tests assert on it.
- Where the docs and the Python code disagree, follow the code.
- Tests must never `chdir` (cargo runs tests as threads); pass directories explicitly.
- The answers file is not written by Copier: the template ships
  `{{ _copier_conf.answers_file }}.jinja` containing `{{ _copier_answers|to_nice_yaml }}`.
- YAML loading must follow PyYAML (YAML 1.1). YAML output only needs to be
  structurally equivalent (owner's decision), in PyYAML's style where cheap.
- In-process library output goes through `copier::output` so tests can capture it;
  warnings go through `copier::errors::warn`.

## Next steps

Phase 7; see the work list above.
