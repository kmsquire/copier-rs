# copier-rs

A Rust port of [Copier](https://github.com/copier-org/copier), the tool for
rendering project templates and keeping generated projects up to date. It aims to
be a drop-in replacement for the `copier` command: same sub-commands, same flags
and same template format, with the differences listed below.

It tracks Copier **9.19.1** and reports that version to templates
(`_min_copier_version`).

## Status

- `copy`, `recopy`, `update` and `check-update` are implemented, including the
  update algorithm (three-way merge through git, inline or `.rej` conflicts,
  migrations), tasks, the `yield` tag, `!include`, `_external_data`, settings and
  trust, and interactive prompts.
- The Python test suite was ported first: 368 of its 394 test functions run and
  pass here (`cargo test` reports about 1,220 cases once parametrized tests are
  expanded). `notes/TEST_LEDGER.md` accounts for the rest.
- Only macOS has been exercised so far.

## Build and run

```sh
cargo build --release
target/release/copier copy gh:org/template my-project
target/release/copier update my-project
```

Needs Rust 1.80 or newer to build and `git` on `PATH` to run (2.27 or newer
recommended, as for Python Copier). `copy` asks the template's questions
interactively unless you pass `--defaults` or answer them with `--data`.

## Differences from Python Copier

- **Jinja extensions written in Python cannot be loaded.** A template that lists
  one in `_jinja_extensions` fails with a clear error. `jinja2.ext.loopcontrols`
  is built in.
- **Templates are rendered by [MiniJinja](https://github.com/mitsuhiko/minijinja),
  not Jinja2.** Common Python idioms work (`name.lower()`, `", ".join(items)`,
  `d.items()`, slicing, `'{}'.format(x)`); `%` string formatting and the `{% do %}`
  tag do not.
- **YAML written by `to_yaml`/`to_nice_yaml` is equivalent to PyYAML's but not always
  byte-identical**: long lines are not folded and multi-line strings are
  double-quoted. A project created by Python Copier may see its answers file
  reformatted once.
- A few ansible filters are missing: `password_hash`, `to_datetime`, `extract`,
  `subelements`, `random_mac`, `ans_groupby`.
- Values that are Python objects in templates are plain strings here:
  `_copier_conf.src_path` (no `.name`/`.parent`; `path / 'x'` is supported) and
  `_version_pep440_*` (no `.major`).
- Unexpected errors print a one-line message and exit with status 1 instead of a
  Python traceback. Documented exit codes are unchanged (1 user error, 2 usage
  error or update available with `check-update --quiet`, 4 unsafe template).
- `copier --version` prints `copier 9.19.1 (copier-rs <version>)`.

See `notes/BACKLOG.md` for the full list of deferred work.

## Performance

Measured on one machine while other work was running (macOS, git 2.39, release
build, small template with three questions and one task); treat it as indicative:

| Operation | Python Copier | copier-rs |
|---|---|---|
| copy from a plain directory | 0.71 s | 0.12 s |
| copy from a git repository | 1.23 s | 0.25 s |
| update (one local edit) | 4.6 s | 0.75 s |

Git is still driven through its command line. After removing redundant calls,
what remains of a copy is mostly git's own clone and checkout.

## Development

- `cargo test` runs the ported suite; `cargo test -- --include-ignored` adds the
  tests that clone from GitHub.
- `python3 parity/run.py` renders the fixture templates with both implementations
  and compares the results (needs the Python checkout and its virtualenv in the
  parent directory).
- `AGENTS.md` has the conventions; `notes/` has the working notes, decisions and
  the test ledger.

## License

MIT, like Copier; see `LICENSE`.
