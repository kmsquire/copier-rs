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
- **Ctrl-C during tasks or clones** does not run Copier's cleanup yet.
- **Ansible filters not implemented**: `password_hash`, `to_datetime`, `extract`,
  `subelements`, `random_mac`, `ans_groupby`.
- **Python-only template features**: `{% do %}`, string `%` formatting, pathlib
  attributes on `_copier_conf` paths (`.name`, `.parent`), `packaging.Version`
  attributes on `_version_pep440_*` (they are plain strings here).
- **Real-terminal prompt rendering** has no automated test (needs a pty helper).
- **Windows and Linux** have not been run.
