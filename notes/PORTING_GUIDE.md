# Porting a Python test module to Rust

You are porting ONE Python test module from `../tests/test_X.py` to
`tests/test_X.rs` in this crate. The port is mechanical: same tests, same names,
same assertions, same comments. The library is not implemented yet, so the tests
only need to COMPILE; do not try to make them pass and do not run them.

## Hard rules

1. Port every `def test_*` function, keeping its name. Never drop or weaken an
   assertion. Keep the original comments.
2. Put `#[ignore = "pending"]` on every test, on the line DIRECTLY ABOVE `fn`
   (after `#[test]`, or after `#[rstest]` and all `#[case(...)]` lines). rstest
   attaches an attribute that precedes a `#[case]` to that single case, so an
   `#[ignore]` placed before the cases leaves the other cases running.
3. If something cannot be expressed with the API below, do NOT invent API and do
   NOT edit anything under `src/` or `tests/common/`. Port as much of the test as
   possible, mark the test `#[ignore = "needs-design: <short reason>"]` instead of
   `pending`, and leave a `// TODO(port): <what is missing>` comment at the spot.
   Still keep the full test body, with the unportable lines as comments.
   A `needs-design` test is not a shortcut: everything that can be expressed must be.
4. A test that only checks a Python-specific thing with no Rust meaning (mypy,
   deprecated import paths, pydantic model internals) is not ported: list it in
   your report as N/A with the reason.
5. Never change the working directory in a test. Python's `with local.cwd(x):`
   becomes passing `x` explicitly (`git(&x, ...)`, `x.join("file")`, and the
   destination as an explicit CLI argument).
6. Read the three reference ports before writing anything and copy their style:
   `tests/test_jinja2_undefined.rs` (API + errors), `tests/test_recopy.rs`
   (fixture function, git, in-process CLI), `tests/test_conditional_file_name.rs`
   (rstest parametrization, interactive session). Read `tests/common/mod.rs` for
   the helper signatures and `src/` for the API (`worker.rs`, `errors.rs`,
   `value.rs`, `template.rs`, `settings.rs`, `vcs.rs`, `tools.rs`).
7. When done, run `cargo test --test test_X --no-run` and fix every compile error
   and warning in YOUR file. Then run `cargo fmt -- tests/test_X.rs`.
8. Do not commit. Do not touch other test files.

## Translation table

| Python | Rust |
|---|---|
| `src, dst = map(tmp_path_factory.mktemp, ("src", "dst"))` | `let tmp = Tmp::new(); let (src, dst) = tmp.src_dst();` |
| `tmp_path` | `let tmp = Tmp::new(); let dst = tmp.mktemp("dst");` (or `tmp.path()`) |
| `tmp_path_factory.mktemp("x")` | `tmp.mktemp("x")` |
| module/function fixture | plain function taking `&Tmp`, called at the top of each test using it |
| `build_file_tree({src / "a": "text", ...})` | `file_tree! { src.join("a") => "text", ... };` |
| bytes content `b"..."` | `b"..."` (or `&[u8]`/`Vec<u8>`) as the value |
| `Path(...)` content (symlink) | `symlink("target")` as the value |
| `build_file_tree(..., dedent=False)` | `build_file_tree_with(vec![(path, content.into())], false)` |
| `"""\` + newline multi-line string (no leading newline) | `indoc! {"` + newline + the same lines + `"}` (`use indoc::indoc;`). NEVER use Rust's `"\` line continuation: it eats the indentation of the next line and corrupts the text |
| `"""` + newline multi-line string (leading newline kept) | ordinary Rust string literal spanning the same lines; the helper dedents it like Python |
| string containing `"` or `\` | escape them, or use a raw string: `indoc! {r#"..."#}` / `r#"..."#` |
| `f"...{x}..."` | `format!("...{x}...")`; double literal braces: `{{ }}` becomes `{{{{ }}}}` |
| `yaml.dump({...})` / `json.dumps({...})` for a config | `json!({...}).to_string()` (JSON is valid YAML) |
| `(dst / "f").read_text()` | `read(dst.join("f"))` |
| `(dst / "f").write_text(s)` | `std::fs::write(dst.join("f"), s).unwrap()` |
| `(dst / "f").exists()` / `.is_file()` / `.is_dir()` / `.is_symlink()` | same method names on `PathBuf` |
| `copier.run_copy(str(src), dst, data={...}, defaults=True)` | `worker(&src, &dst).data(v(json!({...}))).defaults(true).run_copy().unwrap()` |
| `run_update(dst, defaults=True, overwrite=True)` | `Worker::new().dst_path(&dst).defaults(true).overwrite(true).run_update().unwrap()` |
| `run_recopy(dst, ...)` | `Worker::new().dst_path(&dst)....run_recopy().unwrap()` |
| `with Worker(...) as worker: worker.run_copy()` | `let worker = worker(&src, &dst)....run_copy().unwrap();` |
| kwarg `unsafe=True` | `.unsafe_(true)` |
| kwarg `vcs_ref="HEAD"` / `VcsRef.CURRENT` | `.vcs_ref("HEAD")` / `.vcs_ref(VcsRef::Current)` |
| kwarg `conflict="rej"` | `.conflict(ConflictMode::Rej)` |
| kwarg `exclude=[...]`, `skip_if_exists=[...]`, `ask=[...]` | `.exclude(["a", "b"])` etc. |
| kwarg `settings=...` | `.settings(Settings { defaults: ..., trust: vec![...] })` |
| `worker.answers.combined["k"]` | `worker.answers.combined()[&Value::from("k")]` |
| `load_answersfile_data(dst)` | `load_answers(&dst)` (a `Value`) |
| `load_answersfile_data(dst, "file.yml")` | `load_answers_from(&dst, "file.yml")` |
| `answers["k"] == 1` / `is True` / `== "s"` | `assert_eq!(answers["k"], Value::Int(1))` / `Value::Bool(true)` / `Value::from("s")` |
| `answers == {...}` | `assert_eq!(answers, v(json!({...})))` |
| `"k" not in answers` | `assert!(!answers.contains_key("k"))` |
| `with pytest.raises(E, match=r"re"):` body | `assert_err!(<expr returning Result>, CopierError::Variant(..), r"re");` |
| `git("init")` inside `with local.cwd(src)` | `git(&src, &["init"])` |
| `git("rev-parse", "HEAD").strip()` | `git(&src, &["rev-parse", "HEAD"]).trim().to_owned()` |
| `git_save(dst)` / `git_save(dst, tag="v1")` | `git_save(&dst, "Test commit", None)` / `git_save(&dst, "Test commit", Some("v1"))` |
| `git_init("msg")` in cwd `d` | `git_init(&d, "msg")` |
| `CopierApp.run(["copier", "copy", ...], exit=False)` | `let (retcode, out) = run_cli(&["copy", ...]);` |
| `capsys.readouterr()` after an in-process call | wrap the call: `let (result, out) = copier::output::capture(\|\| ...);` then `out.stdout` / `out.stderr` |
| `pytest.warns(DirtyLocalWarning)` | `let (result, warnings) = copier::errors::capture_warnings(\|\| ...);` then `assert!(warnings.iter().any(\|w\| w.kind == WarningKind::DirtyLocal))` |
| `COPIER_CMD(...)` / `subprocess.run(COPIER_PATH + ...)` | `copier_cmd().args([...]).output().unwrap()` |
| `spawn(COPIER_PATH + ("copy", str(src), str(dst)))` | `let mut tui = spawn(&["copy", src.to_str().unwrap(), dst.to_str().unwrap()]);` |
| `spawn(..., timeout=N)` | `spawn_with(&[...], N, None)` |
| `tui.expect_exact("x")` / `tui.expect_exact(pexpect.EOF)` | `tui.expect_exact("x")` / `tui.expect_eof()` |
| `tui.sendline()` / `tui.sendline("x")` / `tui.send("x")` | `tui.sendline("")` / `tui.sendline("x")` / `tui.send("x")` |
| `Keyboard.Down`, `Keyboard.Alt + Keyboard.Enter` | `keyboard::DOWN`, `&format!("{}{}", keyboard::ALT, keyboard::ENTER)` |
| `expect_prompt(tui, "name", "bool")` | `expect_prompt(&mut tui, "name", "bool", None)` |
| `PROJECT_TEMPLATE` / `f"{PROJECT_TEMPLATE}_exclude"` / `"tests/demo_data"` | `project_template()` / `fixture("demo_exclude")` / `fixture("demo_data")` |
| `render(tmp_path, **kw)` | `demo_worker(&dst)...run_copy().unwrap()` |
| `assert_file(tmp_path, "a", "b.txt")` | `assert_file(&dst, "a/b.txt")` |
| `BRACKET_ENVOPS_JSON`, `SUFFIX_TMPL` | same names in `common` |
| `"{{ _copier_conf.answers_file }}.jinja"` key | `ANSWERS_FILE_TMPL` (only for exactly that string) |
| `@pytest.mark.parametrize("a, b", [(1, 2), (3, 4)])` | `#[rstest] #[case(1, 2)] #[case(3, 4)] fn t(#[case] a: i64, #[case] b: i64)` |
| single-argument parametrize over simple values | `#[values(false, true)] flag: bool` |
| stacked `parametrize` decorators | one `#[values(...)]` per argument |
| parametrized arbitrary values (dicts, lists, mixed types) | pass `serde_json::Value` cases: `#[case(json!({"a": 1}))] x: serde_json::Value` |
| `pytest.param(..., marks=pytest.mark.xfail(...))` | keep the case and add a comment with the original mark |
| `@pytest.mark.skipif(platform.system() == "Windows")` | `#[cfg(unix)]` on the test |
| `@pytest.mark.impure` | add `// impure: needs network` comment; keep `#[ignore = "pending"]` |
| `@pytest.mark.xfail(...)`/`skip` | keep the test, add a comment quoting the original mark |
| `monkeypatch.setenv`, patching copier/Python internals | rule 3 (`needs-design`) |
| `platform.system() == "Darwin"` | `cfg!(target_os = "macos")` |
| `os.sep` | `std::path::MAIN_SEPARATOR` |

## Error variant mapping (`pytest.raises`)

| Python exception | Rust pattern |
|---|---|
| `UserMessageError` | `CopierError::UserMessage(_)` |
| `UnsupportedVersionError` | `CopierError::UnsupportedVersion(_)` |
| `ConfigFileError` | `CopierError::ConfigFile(_)` |
| `InvalidConfigFileError` | `CopierError::InvalidConfigFile { .. }` |
| `MultipleConfigFilesError` | `CopierError::MultipleConfigFiles { .. }` |
| `InvalidTypeError` | `CopierError::InvalidType(_)` |
| `ForbiddenPathError` | `CopierError::ForbiddenPath { .. }` |
| `PathNotAbsoluteError` / `PathNotRelativeError` | `CopierError::PathNotAbsolute { .. }` / `PathNotRelative { .. }` |
| `ExtensionNotFoundError` | `CopierError::ExtensionNotFound(_)` |
| `CopierAnswersInterrupt` | `CopierError::AnswersInterrupt(_)` |
| `UnsafeTemplateError` | `CopierError::UnsafeTemplate { .. }` |
| `YieldTagInFileError` / `MultipleYieldTagsError` | `CopierError::YieldTagInFile(_)` / `MultipleYieldTags(_)` |
| `TaskError`, `subprocess.CalledProcessError` | `CopierError::Task { .. }` |
| `InteractiveSessionError` | `CopierError::InteractiveSession(_)` |
| `SettingsError` | `CopierError::Settings(_)` |
| `ValueError` | `CopierError::Value(_)` |
| `TypeError` | `CopierError::Type(_)` |
| `KeyError` | `CopierError::Key(_)` |
| pydantic `ValidationError` | `CopierError::Validation(_)` |
| jinja2 `UndefinedError` | `CopierError::Template { kind: TemplateErrorKind::Undefined, .. }` |
| jinja2 `SecurityError` | `CopierError::Template { kind: TemplateErrorKind::Security, .. }` |
| other jinja2 errors | `CopierError::Template { .. }` |
| plumbum `ProcessExecutionError` | `CopierError::Process(_)` |

Warning classes map to `WarningKind::{UnknownCopierVersion, OldTemplate, DirtyLocal,
ShallowClone, MissingSettings, MissingFile, Cleanup, Deprecation, Future, User}`.

## File header

```rust
//! Port of `tests/test_X.py`.

mod common;

use common::{...only what you use...};
```

## Report format (your final message)

1. `Ported: <n>` test functions, `needs-design: <n>`, `N/A: <n>`.
2. A table with one row per Python test function: name, status
   (`ported` / `needs-design` / `N/A`), and for the last two a one-line reason.
3. Any API you found missing (name what you needed, do not add it).
4. Whether `cargo test --test test_X --no-run` compiles cleanly.
