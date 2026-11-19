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

## Status

| Phase | State |
|---|---|
| 0 Setup and spikes | done |
| 1 Port all tests | in progress |
| 2 Foundations (errors, yaml, tools, settings, pathspec, PEP 440, vcs) | not started |
| 3 Template, Jinja, questions | not started |
| 4 Copy, recopy, CLI | not started |
| 5 Update, migrations, check-update | not started |
| 6 Interactive prompts | not started |
| 7 Parity, performance, polish | not started |

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

See the phase table in the plan. Current: finish porting tests (phase 1).
