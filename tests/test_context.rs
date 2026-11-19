//! Port of `tests/test_context.py`.

mod common;

use common::{file_tree, git_save, read, worker, Tmp, ANSWERS_FILE_TMPL};
use copier::Worker;
use indoc::formatdoc;
use serde_json::json;

#[test]
#[ignore = "needs-design: Python Jinja extensions cannot be loaded"]
fn test_no_path_variables() {
    // Test that there are no context variables of type `pathlib.Path`.
    // TODO(port): Python Jinja extensions cannot be loaded. The test writes this
    // module (named by `uuid4()`) into `src` and puts `src` on `sys.path`:
    //
    // from pathlib import Path
    // from typing import Any, Mapping
    //
    // from jinja2 import Environment, pass_context
    // from jinja2.ext import Extension
    // from jinja2.runtime import Context
    // from pydantic import BaseModel
    //
    //
    // class ContextExtension(Extension):
    //     def __init__(self, environment: Environment) -> None:
    //         super().__init__(environment)
    //         environment.globals["__assert"] = self._assert
    //
    //     @pass_context
    //     def _assert(self, ctx: Context) -> None:
    //         items: list[tuple[str, Any]] = list(dict(ctx).items())
    //         for k, v in items:
    //             if isinstance(v, Path):
    //                 raise AssertionError(
    //                     f"{k} must not be a `pathlib.Path` object"
    //                 )
    //             if isinstance(v, BaseModel):
    //                 v = dict(v)
    //             if isinstance(v, Mapping):
    //                 items.extend((f"{k}.{k2}", v2) for k2, v2 in v.items())
    //             elif isinstance(v, (list, tuple, set)):
    //                 items.extend((f"{k}[{i}]", v2) for i, v2 in enumerate(v))
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    let ext_module_name = "ext_module";
    file_tree! {
        src.join("copier.yml") => formatdoc! {"
            _jinja_extensions:
                - {ext_module_name}.ContextExtension
        "},
        src.join("test.txt.jinja") => "{{ __assert() | default('', true) }}",
    };
    // monkeypatch.setattr("sys.path", [str(src), *sys.path])
    worker(&src, &dst).unsafe_(true).run_copy().unwrap();
    assert_eq!(read(dst.join("test.txt")), "");
}

#[test]
#[ignore = "pending"]
fn test_exclude_templating_with_operation() {
    // Ensure it's possible to create one-off boilerplate files that are not
    // managed during updates via `_exclude` using the `_copier_operation`
    // context variable.
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    let template = "{% if _copier_operation == 'update' %}copy-only{% endif %}";
    file_tree! {
        src.join("copier.yml") => format!("_exclude:\n - \"{template}\""),
        src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|to_yaml }}",
        src.join("copy-only") => "foo",
        src.join("copy-and-update") => "foo",
    };
    git_save(&src, "Test commit", Some("1.0.0"));
    file_tree! {
        src.join("copy-only") => "bar",
        src.join("copy-and-update") => "bar",
    };
    git_save(&src, "Test commit", Some("2.0.0"));
    let copy_only = dst.join("copy-only");
    let copy_and_update = dst.join("copy-and-update");

    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .vcs_ref("1.0.0")
        .run_copy()
        .unwrap();
    for file in [&copy_only, &copy_and_update] {
        assert!(file.exists());
        assert_eq!(read(file), "foo");
    }

    git_save(&dst, "Test commit", None);

    Worker::new()
        .dst_path(&dst)
        .overwrite(true)
        .run_update()
        .unwrap();
    assert_eq!(read(&copy_only), "foo");
    assert_eq!(read(&copy_and_update), "bar");
}

#[test]
#[ignore = "pending"]
fn test_exclude_templating_with_operation_added_in_new_version() {
    // Ensure it's possible to create one-off boilerplate files that are not
    // managed during updates via `_exclude` using the `_copier_operation` context
    // variable when the `_exclude` item is added in the new template version.
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    let template = "{% if _copier_operation == 'update' %}copy-only{% endif %}";
    file_tree! {
        src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|to_yaml }}",
        src.join("copy-only") => "foo",
        src.join("copy-and-update") => "foo",
    };
    git_save(&src, "Test commit", Some("1.0.0"));
    file_tree! {
        src.join("copier.yml") => format!("_exclude:\n - \"{template}\""),
        src.join("copy-only") => "bar",
        src.join("copy-and-update") => "bar",
    };
    git_save(&src, "Test commit", Some("2.0.0"));
    let copy_only = dst.join("copy-only");
    let copy_and_update = dst.join("copy-and-update");

    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .vcs_ref("1.0.0")
        .run_copy()
        .unwrap();
    for file in [&copy_only, &copy_and_update] {
        assert!(file.exists());
        assert_eq!(read(file), "foo");
    }

    git_save(&dst, "Test commit", None);

    Worker::new()
        .dst_path(&dst)
        .overwrite(true)
        .run_update()
        .unwrap();
    assert_eq!(read(&copy_only), "foo");
    assert_eq!(read(&copy_and_update), "bar");
}

#[test]
#[ignore = "pending"]
fn test_task_templating_with_operation() {
    // Ensure that it is possible to define tasks that are only executed when copying.
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    // Use a file outside the Copier working directories to ensure accurate tracking
    let task_counter = tmp.path().join("task_calls.txt");
    let task_counter_json = json!(task_counter.to_str().unwrap()).to_string();
    file_tree! {
        src.join("copier.yml") => formatdoc! {r#"
            _tasks:
                -   command: echo {{{{ _copier_operation }}}} >> {task_counter_json}
                    when: "{{{{ _copier_operation == 'copy' }}}}"
        "#},
        src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|to_yaml }}",
    };
    git_save(&src, "Test commit", Some("1.0.0"));

    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .unsafe_(true)
        .run_copy()
        .unwrap();
    assert!(task_counter.exists());
    assert_eq!(read(&task_counter).lines().count(), 1);

    git_save(&dst, "Test commit", None);

    Worker::new()
        .dst_path(&dst)
        .defaults(true)
        .overwrite(true)
        .unsafe_(true)
        .run_recopy()
        .unwrap();
    assert_eq!(read(&task_counter).lines().count(), 2);

    Worker::new()
        .dst_path(&dst)
        .defaults(true)
        .overwrite(true)
        .unsafe_(true)
        .run_update()
        .unwrap();
    assert_eq!(read(&task_counter).lines().count(), 2);
}
