//! Port of `tests/test_conditional_file_name.py`.

mod common;

use common::{
    expect_prompt, file_tree, git, read, spawn, spawn_with, v, worker, Tmp, ANSWERS_FILE_TMPL,
    SPAWN_TIMEOUT,
};
use copier::Worker;
use rstest::rstest;
use serde_json::json;

#[test]
#[ignore = "pending"]
fn test_render_conditional() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("{% if conditional %}file.txt{% endif %}.jinja") => "This is {{ conditional.variable }}.",
    };
    worker(&src, &dst)
        .data(v(json!({"conditional": {"variable": true}})))
        .run_copy()
        .unwrap();
    assert_eq!(read(dst.join("file.txt")), "This is True.");
}

#[test]
#[ignore = "pending"]
fn test_dont_render_conditional() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("{% if conditional %}file.txt{% endif %}.jinja") => "This is {{ conditional.variable }}.",
    };
    worker(&src, &dst).run_copy().unwrap();
    assert!(!dst.join("file.txt").exists());
}

#[test]
#[ignore = "pending"]
fn test_render_conditional_subdir() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("subdir").join("{% if conditional %}file.txt{% endif %}.jinja") => "This is {{ conditional.variable }}.",
    };
    worker(&src, &dst)
        .data(v(json!({"conditional": {"variable": true}})))
        .run_copy()
        .unwrap();
    assert_eq!(read(dst.join("subdir").join("file.txt")), "This is True.");
}

#[test]
#[ignore = "pending"]
fn test_dont_render_conditional_subdir() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("subdir").join("{% if conditional %}file.txt{% endif %}.jinja") => "This is a {{ conditional.variable }}.",
    };
    worker(&src, &dst).run_copy().unwrap();
    assert!(!dst.join("subdir").join("file.txt").exists());
}

#[ignore = "pending"]
#[rstest]
fn test_answer_changes(#[values(false, true)] interactive: bool) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    let (src_str, dst_str) = (src.to_str().unwrap(), dst.to_str().unwrap());

    file_tree! {
        src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|to_nice_yaml }}",
        src.join("copier.yml") => "
            condition:
                type: bool
        ",
        src.join("{% if condition %}file.txt{% endif %}.jinja") => "",
    };
    git(&src, &["init"]);
    git(&src, &["add", "."]);
    git(&src, &["commit", "-mv1"]);
    git(&src, &["tag", "v1"]);

    if interactive {
        let mut tui = spawn(&["copy", src_str, dst_str]);
        expect_prompt(&mut tui, "condition", "bool", None);
        tui.expect_exact("(y/N)");
        tui.sendline("y");
        tui.expect_exact("Yes");
        tui.expect_eof();
    } else {
        worker(&src, &dst)
            .data(v(json!({"condition": true})))
            .run_copy()
            .unwrap();
    }

    assert!(dst.join("file.txt").exists());

    git(&dst, &["init"]);
    git(&dst, &["add", "."]);
    git(&dst, &["commit", "-mv1"]);

    if interactive {
        let mut tui = spawn_with(&["update", dst_str], SPAWN_TIMEOUT, None);
        expect_prompt(&mut tui, "condition", "bool", None);
        tui.expect_exact("(Y/n)");
        tui.sendline("n");
        tui.expect_exact("No");
        tui.expect_eof();
    } else {
        Worker::new()
            .dst_path(&dst)
            .data(v(json!({"condition": false})))
            .overwrite(true)
            .run_update()
            .unwrap();
    }

    assert!(!dst.join("file.txt").exists());
}
