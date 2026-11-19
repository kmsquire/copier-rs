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
| 7 Parity, performance, polish | first pass done and merged: git-query caching, parity harness, README, LICENSE; the rest is in the work list below |

Phases 2 to 6 were developed together on one branch (`phase-2-foundations`), because
the ported tests exercise the whole stack and could only be turned on once copy,
update, the CLI and the prompts all existed. The commits are still one per layer.

`cargo test`: 1,225 passing, 0 failing, 27 ignored. `notes/TEST_LEDGER.md` lists
every ignored test with its reason (regenerate with `python3 tools/gen_ledger.py`).
A first parity check (copy plus update with a local edit) produced byte-identical
trees to Python Copier; release build is about 2x faster on copy and 1.4x on update,
with git subprocesses dominating.

## Where the code is

- `copier-rs/` is the main checkout (`main`). `../copier-rs-wt/` is a leftover
  worktree of the merged `phase-2-foundations` branch; remove it with
  `git worktree remove ../copier-rs-wt` from `copier-rs/` when convenient.
- `tools/unignore.py <module>...` runs a module with `--include-ignored` and removes
  `#[ignore = "pending"]` from tests that pass.

## Remaining work

1. (done) Ctrl-C handling: see `src/interrupt.rs` and `DECISIONS.md`.
2. A pty-backed test session for `test_prompt::test_path_completion` and
   `test_prompt_does_not_capture_mouse`; exercise the real-terminal redraw path.
3. Native versions of popular Jinja extensions (see `BACKLOG.md`).
4. Run on Linux and Windows.
5. Performance: a copy from a git template now runs 10 git processes (0.25 s; clone
   and checkout are most of it). The update is 0.75 s. See `BACKLOG.md` for the git
   library assessment. Next lever if needed: profile large templates (many files),
   where rendering rather than git should dominate.
6. Extend `parity/run.py` with update round-trips and synthesized templates.
7. Refactors the reviews asked for and that are not done: split
   `worker/update.rs::apply_update` into steps (a `generate_copy` helper and a
   small struct for the three repositories), one shared `cached()` helper returning
   references, move path helpers out of `settings.rs`, narrow `pub` modules in
   `lib.rs` to the real public API.

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

See "Remaining work" above. `python3 parity/run.py` compares against Python Copier.
