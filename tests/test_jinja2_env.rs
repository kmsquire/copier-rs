//! Port of `tests/test_jinja2_env.py`.

// N/A: every test in the Python module probes Python objects from inside the Jinja
// sandbox (`PurePath.parser.os`, `SettingsModel.from_file`/`parse_file`/`parse_raw`).
// The Rust port has no equivalent objects, so there are no tests to port.
