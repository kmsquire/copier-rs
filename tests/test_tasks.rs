//! Port of `tests/test_tasks.py`.

mod common;

use std::path::PathBuf;

use common::{file_tree, run_cli, v, worker, Tmp, BRACKET_ENVOPS_JSON, SUFFIX_TMPL};
use copier::tools::with_os;
use indoc::{formatdoc, indoc};
use rstest::rstest;
use serde_json::json;

/// Template exercising tasks: destination-relative paths, working directories,
/// rendering and `when` conditions.
fn template_path(tmp: &Tmp) -> PathBuf {
    let root = tmp.mktemp("demo_tasks");
    file_tree! {
        root.join("copier.yaml") => formatdoc! {r#"
            _templates_suffix: {SUFFIX_TMPL}
            _envops: {BRACKET_ENVOPS_JSON}

            other_file: bye
            condition: true

            # This tests two things:
            # 1. That the tasks are being executed in the destination folder; and
            # 2. That the tasks are being executed in order, one after another
            _tasks:
                -   mkdir hello
                -   command: touch world
                    working_directory: ./hello
                -   touch [[ other_file ]]
                -   ["[[ _copier_python ]]", "-c", "open('pyfile', 'w').close()"]
                -   command: touch true
                    when: "[[ condition ]]"
                -   command: touch false
                    when: "[[ not condition ]]"
        "#},
    };
    root
}

#[test]
fn test_render_tasks() {
    let tmp = Tmp::new();
    let template = template_path(&tmp);
    let dst = tmp.mktemp("dst");
    worker(&template, &dst)
        .data(v(json!({"other_file": "custom", "condition": "true"})))
        .unsafe_(true)
        .run_copy()
        .unwrap();
    assert!(dst.join("custom").is_file());
}

#[test]
fn test_copy_tasks() {
    let tmp = Tmp::new();
    let template = template_path(&tmp);
    let dst = tmp.mktemp("dst");
    worker(&template, &dst)
        .quiet(true)
        .defaults(true)
        .overwrite(true)
        .unsafe_(true)
        .run_copy()
        .unwrap();
    assert!(dst.join("hello").exists());
    assert!(dst.join("hello").is_dir());
    assert!(dst.join("hello").join("world").exists());
    assert!(dst.join("bye").is_file());
    assert!(dst.join("pyfile").is_file());
    assert!(dst.join("true").is_file());
    assert!(!dst.join("false").exists());
}

#[test]
fn test_task_with_blank_rendered_when_is_skipped() {
    let tmp = Tmp::new();
    let src = tmp.mktemp("src");
    let dst = tmp.mktemp("dst");
    file_tree! {
        src.join("copier.yml") => indoc! {"
            _tasks:
                -   command: touch should-not-exist
                    when: |
                        {% if false %}
                        true
                        {% endif %}
        "},
    };

    worker(&src, &dst)
        .quiet(true)
        .defaults(true)
        .overwrite(true)
        .unsafe_(true)
        .run_copy()
        .unwrap();

    assert!(!dst.join("should-not-exist").exists());
}

#[test]
fn test_copy_skip_tasks() {
    let tmp = Tmp::new();
    let template = template_path(&tmp);
    let dst = tmp.mktemp("dst");
    worker(&template, &dst)
        .quiet(true)
        .defaults(true)
        .overwrite(true)
        .skip_tasks(true)
        .run_copy()
        .unwrap();
    assert!(!dst.join("hello").exists());
    assert!(!dst.join("hello").is_dir());
    assert!(!dst.join("hello").join("world").exists());
    assert!(!dst.join("bye").is_file());
    assert!(!dst.join("pyfile").is_file());
}

#[rstest]
fn test_copy_cli_skip_tasks(#[values(false, true)] skip_tasks: bool) {
    let tmp = Tmp::new();
    let src = tmp.mktemp("src");
    let dst = tmp.mktemp("dst");
    file_tree! {
        src.join("copier.yaml") => json!({"_tasks": ["touch task.txt"]}).to_string(),
    };
    let src_str = src.to_str().unwrap();
    let dst_str = dst.to_str().unwrap();
    let mut args = vec!["copy", "--UNSAFE"];
    if skip_tasks {
        args.push("--skip-tasks");
    }
    args.push(src_str);
    args.push(dst_str);
    let (retcode, _) = run_cli(&args);
    assert_eq!(retcode, 0);
    assert_eq!(dst.join("task.txt").exists(), !skip_tasks);
}

#[test]
fn test_pretend_mode() {
    let tmp = Tmp::new();
    let src = tmp.mktemp("src");
    let dst = tmp.mktemp("dst");
    file_tree! {
        src.join("copier.yml") => indoc! {"
            _tasks:
                - touch created-by-task.txt
        "},
    };
    worker(&src, &dst)
        .pretend(true)
        .unsafe_(true)
        .run_copy()
        .unwrap();
    assert!(!dst.join("created-by-task.txt").exists());
}

#[rstest]
#[case(Some("linux"), "linux.txt")]
#[case(Some("macos"), "macos.txt")]
#[case(Some("windows"), "windows.txt")]
#[case(None, "unsupported.txt")]
fn test_os_specific_tasks(#[case] os: Option<&'static str>, #[case] filename: &str) {
    let tmp = Tmp::new();
    let src = tmp.mktemp("src");
    let dst = tmp.mktemp("dst");
    file_tree! {
        src.join("copier.yml") => indoc! {"
            _tasks:
            -   >-
                {% if _copier_conf.os == 'linux' %}
                touch linux.txt
                {% elif _copier_conf.os == 'macos' %}
                touch macos.txt
                {% elif _copier_conf.os == 'windows' %}
                touch windows.txt
                {% elif _copier_conf.os is none %}
                touch unsupported.txt
                {% else %}
                touch never.txt
                {% endif %}
        "},
    };
    with_os(os, || worker(&src, &dst).unsafe_(true).run_copy()).unwrap();
    assert!(dst.join(filename).exists());
}

#[test]
fn test_copier_phase_variable() {
    let tmp = Tmp::new();
    let src = tmp.mktemp("src");
    let dst = tmp.mktemp("dst");
    file_tree! {
        src.join("copier.yml") => indoc! {"
            _tasks:
                - touch {{ _copier_phase }}
        "},
    };
    worker(&src, &dst).unsafe_(true).run_copy().unwrap();
    assert!(dst.join("tasks").exists());
}
