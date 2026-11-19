# Agent Guidelines (copier-rs)

Rust port of Copier. The Python original lives one directory up (`../copier`,
`../tests`, `../docs`) and is the behavioral reference. Read `notes/SCRATCHPAD.md`
before starting any work.

## Commands

- `cargo test` – run the active test suite
- `cargo test -- --include-ignored` – also run tests for unimplemented features
- `cargo test --test test_copy` – run one ported module
- `cargo fmt --all && cargo clippy --all-targets -- -D warnings` – required before commit
- `../.venv/bin/copier` – the Python reference implementation (oracle)

## Code conventions

- Modules mirror the Python layout (`worker`, `template`, `questions`, `vcs`, ...)
- Doc comments on every public item; comments explain why, never what
- No `unwrap`/`expect` outside tests; return `CopierError`
- User-facing message text must match Python Copier exactly; tests assert on it
- Git is driven through the `git` CLI, never a library

## Test conventions

- One `tests/test_*.rs` per Python `tests/test_*.py`, same test names
- Never `chdir` in tests; pass directories explicitly
- Never weaken an assertion to make a test pass; fix the code or record the
  difference in `notes/TEST_LEDGER.md`
- A test for an unimplemented feature carries `#[ignore = "pending"]`

## Workflow

- Work on `phase-N-<name>` branches; never commit to `main` directly; never push
- Conventional Commits, one functional chunk per commit
- Record decisions in `notes/DECISIONS.md`, deferred work in `notes/BACKLOG.md`
