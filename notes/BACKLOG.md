# Backlog

Deferred work. Each item says why it was deferred.

- **Python Jinja extensions** (`_jinja_extensions`): cannot be loaded from Rust.
  Candidate native reimplementations, by likely usage: `jinja2_time.TimeExtension`,
  `cookiecutter.extensions.*` (slugify, jsonify, random string), `jinja2.ext.do`,
  `jinja2.ext.loopcontrols` (minijinja feature `loop_controls`), `jinja2.ext.debug`,
  `copier_templates_extensions` context hooks (needs a design).
- **`%` string formatting** in templates (`'%s' % x`): not supported by minijinja.
- **Byte-identical PyYAML output** (line folding at width 80, full scalar analysis).
- **Integers beyond 64 bits** in answers.
- **YAML/JSON syntax highlighting** in input prompts.
- **Windows**: untested; the Python suite's Windows-only cases are not ported.
- **Tab-indented YAML** (`\ta: 1`) is rejected by PyYAML's scanner but accepted here
  (the underlying parser follows YAML 1.2).
- **Ansible filters not implemented**: `password_hash`, `to_datetime`, `extract`,
  `subelements`, `random_mac`, `ans_groupby`.
- **Python-only template features**: `{% do %}`, string `%` formatting, pathlib
  attributes on `_copier_conf` paths (`.name`, `.parent`), `packaging.Version`
  attributes on `_version_pep440_*` (they are plain strings here).
- **Real-terminal prompt rendering** has no automated test (needs a pty helper).
- **Windows and Linux** have not been run.
- **A git library for read-only queries** (`gix`): measured 2026-11-19, a copy from a
  git template runs 10 git processes; 8 are small queries costing about 7 ms each
  on macOS once Apple's launcher is bypassed (about 55 ms of a 250 ms copy). A
  library could remove most of that but not the clone, checkout, or the update's
  `git apply --reject` / `merge-file` machinery, and adds a large dependency and a
  second source of git behavior. Not worth it yet; revisit if profiles change.
