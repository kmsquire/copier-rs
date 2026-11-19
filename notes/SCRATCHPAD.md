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
| 1 Port all tests | done (merged 2026-11-19; reviews: no blocking findings) |
| 2 Foundations (errors, yaml, tools, settings, pathspec, PEP 440, vcs) | implemented in the worktree; tests not yet un-ignored |
| 3 Template, Jinja, questions | implemented in the worktree; tests not yet un-ignored |
| 4 Copy, recopy, CLI | engine written in the worktree (copy path compiles); CLI not started |
| 5 Update, migrations, check-update | not started |
| 6 Interactive prompts | not started |
| 7 Parity, performance, polish | not started |

## Where the code is right now (2026-11-19)

- `copier-rs/` (this checkout): `main` holds the ported test suite, all ignored.
- `../copier-rs-wt/` is a git worktree on `phase-2-foundations`, branched from
  `main`, holding implementation written while test porting ran: `yaml/`
  (loader, emitter, PyYAML corpus test), `tools`, `dirs`, `settings`, `version`,
  `vcs`, `template/` (config, tasks, migrations, dunamai-style version), `json`,
  `jinja/` (minijinja environment, filters, pycompat, yield pre-pass).
  After phase 1 merges into `main`, rebase that branch onto `main`, un-ignore the
  tests it makes pass, and split it into the phase 2 and phase 3 merges.
- Test porting: Haiku agents, one batch each, following `notes/PORTING_GUIDE.md`.
  After each batch run the two helper checks from the session scratchpad (they are
  small; recreate if lost): move `#[ignore]` directly above `fn` on `#[case]`
  tests, and compare test names and assertion counts with the Python module.
  Do not run `cargo fmt --all` here while agents are writing files.

## Known needs-design hooks (tests ported but blocked on a small API)

- Copier version override: implemented in the worktree as
  `template::with_copier_version(version, || ...)`; port `test_minimum_version` to it.
- Default settings location and `COPIER_SETTINGS_PATH`: needs a thread-local
  override in `settings` (12 tests in `test_settings*`, 2 CLI tests in `test_unsafe`).
- Operating system override (`test_tasks::test_os_specific_tasks`).
- Temp-root redirection and SIGINT injection (`test_tmpdir`, `test_cleanup`).
- Per-test git config (`test_dirty_local` gpg tests, some `test_vcs` tests).
- Help output at a fixed width of 80 (`test_cli` help snapshots).
- Python Jinja extensions (`test_context`, `test_jinja2_extensions`, and the
  `test_unsafe` cases that actually load one): no Rust equivalent; decide per test.

## Review follow-ups to do while un-ignoring modules

- Move duplicated test helpers into `tests/common`: JSON-to-`Mapping`, recursive
  directory copy, `bracket_envops`, and replace the `yaml_safe_load` stubs in
  `test_copy`/`test_prompt` with `copier::yaml::load`.
- Derive `Debug` for `Worker` and drop `test_copy`'s local `expect_err`.
- `tests/common::Tui` reads stdout and stderr on separate threads; if a test flakes
  on ordering, merge the streams at the OS level instead.

- Platform marks: most Python `xfail`/`skipif` marks are comments in the ports.
  When un-ignoring, turn each into `#[cfg(...)]` or a runtime skip; known ones:
  `test_exclude::test_path_filter` (strict xfail on macOS), the Darwin/Windows
  cases in `test_updatediff` (`test_skip_update_deleted`, `test_update_deleted_path`,
  the special-characters test) and its `git >= 2.55` xfail.
- Two ports lost their teeth when a Python monkeypatch was dropped and must not be
  un-ignored as they are: `test_config::test_authorization_does_not_enable_python_yaml_tags`
  (its marker file check depends on the working directory) and
  `test_answersfile::test_external_data_with_unicode_characters` (default-encoding
  dependence cannot occur in Rust; decide whether the test still says anything).
- `test_interrupts` dropped the `CopierAnswersInterrupt` parametrize case.

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

See the phase table in the plan. Current: rebase the worktree branch onto `main`,
un-ignore tests module by module, fix what fails, then cut the phase 2 merge.
