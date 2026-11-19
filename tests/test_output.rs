//! Port of `tests/test_output.py`.

mod common;

use common::{
    assert_err, demo_worker, expect_prompt, file_tree, git, read, spawn, v, worker, Tmp,
    ANSWERS_FILE_TMPL,
};
use copier::output::capture;
use copier::{CopierError, Worker};
use indoc::indoc;
use rstest::rstest;
use serde_json::json;

#[test]
#[ignore = "pending"]
fn test_output() {
    let tmp = Tmp::new();
    let dst = tmp.path();
    let (_, out) = capture(|| demo_worker(dst).quiet(false).run_copy().unwrap());
    assert!(regex::Regex::new(r"create[^\s]*  config\.py")
        .unwrap()
        .is_match(&out.stderr));
    assert!(regex::Regex::new(r"create[^\s]*  pyproject\.toml")
        .unwrap()
        .is_match(&out.stderr));
    assert!(
        regex::Regex::new(r"create[^\s]*  doc[/\\]images[/\\]nslogo\.gif")
            .unwrap()
            .is_match(&out.stderr)
    );
}

#[test]
#[ignore = "pending"]
fn test_output_pretend() {
    let tmp = Tmp::new();
    let dst = tmp.path();
    let (_, out) = capture(|| {
        demo_worker(dst)
            .quiet(false)
            .pretend(true)
            .run_copy()
            .unwrap()
    });
    assert!(regex::Regex::new(r"create[^\s]*  config\.py")
        .unwrap()
        .is_match(&out.stderr));
    assert!(regex::Regex::new(r"create[^\s]*  pyproject\.toml")
        .unwrap()
        .is_match(&out.stderr));
    assert!(
        regex::Regex::new(r"create[^\s]*  doc[/\\]images[/\\]nslogo\.gif")
            .unwrap()
            .is_match(&out.stderr)
    );
}

#[test]
#[ignore = "pending"]
fn test_output_force() {
    let tmp = Tmp::new();
    let dst = tmp.path();
    demo_worker(dst).run_copy().unwrap();
    let (_, out) = capture(|| {
        demo_worker(dst)
            .quiet(false)
            .defaults(true)
            .overwrite(true)
            .run_copy()
            .unwrap()
    });
    assert!(regex::Regex::new(r"conflict[^\s]*  config\.py")
        .unwrap()
        .is_match(&out.stderr));
    assert!(regex::Regex::new(r"overwrite[^\s]*  config\.py")
        .unwrap()
        .is_match(&out.stderr));
    assert!(regex::Regex::new(r"identical[^\s]*  pyproject\.toml")
        .unwrap()
        .is_match(&out.stderr));
    assert!(
        regex::Regex::new(r"identical[^\s]*  doc[/\\]images[/\\]nslogo\.gif")
            .unwrap()
            .is_match(&out.stderr)
    );
}

#[test]
#[ignore = "pending"]
fn test_output_skip() {
    let tmp = Tmp::new();
    let dst = tmp.path();
    demo_worker(dst).run_copy().unwrap();
    let (_, out) = capture(|| {
        demo_worker(dst)
            .quiet(false)
            .skip_if_exists(["config.py"])
            .run_copy()
            .unwrap()
    });
    assert!(regex::Regex::new(r"conflict[^\s]*  config\.py")
        .unwrap()
        .is_match(&out.stderr));
    assert!(regex::Regex::new(r"skip[^\s]*  config\.py")
        .unwrap()
        .is_match(&out.stderr));
    assert!(regex::Regex::new(r"identical[^\s]*  pyproject\.toml")
        .unwrap()
        .is_match(&out.stderr));
    assert!(
        regex::Regex::new(r"identical[^\s]*  doc[/\\]images[/\\]nslogo\.gif")
            .unwrap()
            .is_match(&out.stderr)
    );
}

#[test]
#[ignore = "pending"]
fn test_output_quiet() {
    let tmp = Tmp::new();
    let dst = tmp.path();
    let (_, out) = capture(|| demo_worker(dst).quiet(true).run_copy().unwrap());
    assert_eq!(out.stdout, "");
    assert_eq!(out.stderr, "");
}

#[test]
#[ignore = "pending"]
fn test_question_with_invalid_type() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yaml") => "
            bad:
                type: invalid
                default: 1
            ",
    };
    assert_err!(
        worker(&src, &dst).defaults(true).overwrite(true).run_copy(),
        CopierError::InvalidType(_),
        r#"Unsupported type "invalid" in question "bad""#
    );
}

#[test]
#[ignore = "pending"]
fn test_answer_with_invalid_type() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yaml") => "
            bad:
                type: int
                default: null
            ",
    };
    assert_err!(
        worker(&src, &dst).defaults(true).overwrite(true).run_copy(),
        CopierError::InvalidType(_),
        r#"Invalid answer "None" of type "<class 'NoneType'>" to question "bad" of type "int""#
    );
}

#[rstest]
#[ignore = "pending"]
fn test_messages_with_inline_text(#[values(false, true)] interactive: bool) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    let (src_s, dst_s) = (src.to_str().unwrap(), dst.to_str().unwrap());

    file_tree! {
        src.join("copier.yaml") => indoc! {"
            project_name:
                type: str

            _message_before_copy: Thank you for using our template on {{ _copier_conf.os }}
            _message_after_copy: Project {{ project_name }} successfully created
            _message_before_update: Updating on {{ _copier_conf.os }}
            _message_after_update: Project {{ project_name }} successfully updated
            "},
        src.join(ANSWERS_FILE_TMPL) => indoc! {"
            # Changes here will be overwritten by Copier
            {{ _copier_answers|to_nice_yaml }}
            "},
        src.join("version.txt") => "v1",
    };
    git(&src, &["init"]);
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m1"]);
    git(&src, &["tag", "v1"]);

    file_tree! {
        src.join("version.txt") => "v2",
    };
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m2"]);
    git(&src, &["tag", "v2"]);

    // clear capture output log

    // copy
    let err = if interactive {
        let mut tui = spawn(&["copy", "-r", "v1", src_s, dst_s]);
        expect_prompt(&mut tui, "project_name", "str", None);
        tui.sendline("myproj");
        // The session logs to stderr, so its remaining output stands in for capsys stderr.
        tui.expect_eof()
    } else {
        let (_, out) = capture(|| {
            worker(&src, &dst)
                .data(v(json!({"project_name": "myproj"})))
                .vcs_ref("v1")
                .run_copy()
                .unwrap()
        });
        out.stderr
    };

    assert_eq!(read(dst.join("version.txt")), "v1");
    assert!(regex::Regex::new(
        r"(?sx)
        ^Thank\ you\ for\ using\ our\ template\ on\ (linux|macos|windows).+
        Project\ myproj\ successfully\ created\s*$
        "
    )
    .unwrap()
    .is_match(&err));

    // recopy
    let err = if interactive {
        let mut tui = spawn(&["recopy", "-r", "v1", dst_s]);
        expect_prompt(&mut tui, "project_name", "str", None);
        tui.sendline("_new");
        tui.expect_eof()
    } else {
        let (_, out) = capture(|| {
            Worker::new()
                .dst_path(&dst)
                .data(v(json!({"project_name": "myproj_new"})))
                .vcs_ref("v1")
                .run_recopy()
                .unwrap()
        });
        out.stderr
    };

    assert_eq!(read(dst.join("version.txt")), "v1");
    assert!(regex::Regex::new(
        r"(?sx)
        ^Thank\ you\ for\ using\ our\ template\ on\ (linux|macos|windows).+
        Project\ myproj_new\ successfully\ created\s*$
        "
    )
    .unwrap()
    .is_match(&err));

    git(&dst, &["init"]);
    git(&dst, &["add", "."]);
    git(&dst, &["commit", "-m1"]);

    // clear capture output log

    // update
    let err = if interactive {
        let mut tui = spawn(&["update", dst_s]);
        expect_prompt(&mut tui, "project_name", "str", None);
        tui.sendline("_update");
        tui.expect_eof()
    } else {
        let (_, out) = capture(|| {
            Worker::new()
                .dst_path(&dst)
                .data(v(json!({"project_name": "myproj_new_update"})))
                .overwrite(true)
                .run_update()
                .unwrap()
        });
        out.stderr
    };

    assert_eq!(read(dst.join("version.txt")), "v2");
    assert!(regex::Regex::new(
        r"(?sx)
        ^Updating\ on\ (linux|macos|windows).+
        Project\ myproj_new_update\ successfully\ updated\s*$
        "
    )
    .unwrap()
    .is_match(&err));
}

#[rstest]
#[ignore = "pending"]
fn test_messages_with_included_text(#[values(false, true)] interactive: bool) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    let (src_s, dst_s) = (src.to_str().unwrap(), dst.to_str().unwrap());

    file_tree! {
        src.join("copier.yaml") => indoc! {r#"
            project_name:
                type: str

            _exclude: [".git", "*.md"]
            _message_before_copy: "{% include 'message_before_copy.md.jinja' %}"
            _message_after_copy: "{% include 'message_after_copy.md.jinja' %}"
            _message_before_update: "{% include 'message_before_update.md.jinja' %}"
            _message_after_update: "{% include 'message_after_update.md.jinja' %}"
            "#},
        src.join("message_before_copy.md.jinja") => indoc! {r#"
            Thank you for using our template on {{ _copier_conf.os }}
            "#},
        src.join("message_after_copy.md.jinja") => indoc! {r#"
            Project {{ project_name }} successfully created
            "#},
        src.join("message_before_update.md.jinja") => indoc! {r#"
            Updating on {{ _copier_conf.os }}
            "#},
        src.join("message_after_update.md.jinja") => indoc! {r#"
            Project {{ project_name }} successfully updated
            "#},
        src.join(ANSWERS_FILE_TMPL) => indoc! {"
            # Changes here will be overwritten by Copier
            {{ _copier_answers|to_nice_yaml }}
            "},
        src.join("version.txt") => "v1",
    };
    git(&src, &["init"]);
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m1"]);
    git(&src, &["tag", "v1"]);

    file_tree! {
        src.join("version.txt") => "v2",
    };
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m2"]);
    git(&src, &["tag", "v2"]);

    // clear capture output log

    // copy
    let err = if interactive {
        let mut tui = spawn(&["copy", "-r", "v1", src_s, dst_s]);
        expect_prompt(&mut tui, "project_name", "str", None);
        tui.sendline("myproj");
        // The session logs to stderr, so its remaining output stands in for capsys stderr.
        tui.expect_eof()
    } else {
        let (_, out) = capture(|| {
            worker(&src, &dst)
                .data(v(json!({"project_name": "myproj"})))
                .vcs_ref("v1")
                .run_copy()
                .unwrap()
        });
        out.stderr
    };

    assert_eq!(read(dst.join("version.txt")), "v1");
    assert!(regex::Regex::new(
        r"(?sx)
        ^Thank\ you\ for\ using\ our\ template\ on\ (linux|macos|windows).+
        Project\ myproj\ successfully\ created\s*$
        "
    )
    .unwrap()
    .is_match(&err));

    // recopy
    let err = if interactive {
        let mut tui = spawn(&["recopy", "-r", "v1", dst_s]);
        expect_prompt(&mut tui, "project_name", "str", None);
        tui.sendline("_new");
        tui.expect_eof()
    } else {
        let (_, out) = capture(|| {
            Worker::new()
                .dst_path(&dst)
                .data(v(json!({"project_name": "myproj_new"})))
                .vcs_ref("v1")
                .run_recopy()
                .unwrap()
        });
        out.stderr
    };

    assert_eq!(read(dst.join("version.txt")), "v1");
    assert!(regex::Regex::new(
        r"(?sx)
        ^Thank\ you\ for\ using\ our\ template\ on\ (linux|macos|windows).+
        Project\ myproj_new\ successfully\ created\s*$
        "
    )
    .unwrap()
    .is_match(&err));

    git(&dst, &["init"]);
    git(&dst, &["add", "."]);
    git(&dst, &["commit", "-m1"]);

    // clear capture output log

    // update
    let err = if interactive {
        let mut tui = spawn(&["update", dst_s]);
        expect_prompt(&mut tui, "project_name", "str", None);
        tui.sendline("_update");
        tui.expect_eof()
    } else {
        let (_, out) = capture(|| {
            Worker::new()
                .dst_path(&dst)
                .data(v(json!({"project_name": "myproj_new_update"})))
                .overwrite(true)
                .run_update()
                .unwrap()
        });
        out.stderr
    };

    assert_eq!(read(dst.join("version.txt")), "v2");
    assert!(regex::Regex::new(
        r"(?sx)
        ^Updating\ on\ (linux|macos|windows).+
        Project\ myproj_new_update\ successfully\ updated\s*$
        "
    )
    .unwrap()
    .is_match(&err));
}

#[rstest]
#[ignore = "pending"]
fn test_messages_quiet(#[values(false, true)] interactive: bool) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    let (src_s, dst_s) = (src.to_str().unwrap(), dst.to_str().unwrap());

    file_tree! {
        src.join("copier.yaml") => indoc! {"
            project_name:
                type: str

            _message_before_copy: Thank you for using our template on {{ _copier_conf.os }}
            _message_after_copy: Project {{ project_name }} successfully created
            _message_before_update: Updating on {{ _copier_conf.os }}
            _message_after_update: Project {{ project_name }} successfully updated
            "},
        src.join(ANSWERS_FILE_TMPL) => indoc! {"
            # Changes here will be overwritten by Copier
            {{ _copier_answers|to_nice_yaml }}
            "},
        src.join("version.txt") => "v1",
    };
    git(&src, &["init"]);
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m1"]);
    git(&src, &["tag", "v1"]);

    file_tree! {
        src.join("version.txt") => "v2",
    };
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m2"]);
    git(&src, &["tag", "v2"]);

    // clear capture output log

    // copy
    let err = if interactive {
        let mut tui = spawn(&["copy", "--quiet", "-r", "v1", src_s, dst_s]);
        expect_prompt(&mut tui, "project_name", "str", None);
        tui.sendline("myproj");
        // The session logs to stderr, so its remaining output stands in for capsys stderr.
        tui.expect_eof()
    } else {
        let (_, out) = capture(|| {
            worker(&src, &dst)
                .data(v(json!({"project_name": "myproj"})))
                .vcs_ref("v1")
                .quiet(true)
                .run_copy()
                .unwrap()
        });
        out.stderr
    };

    assert_eq!(read(dst.join("version.txt")), "v1");
    assert!(!err.contains("Thank you for using our template"));
    assert!(!err.contains("Project myproj successfully created"));

    // recopy
    let err = if interactive {
        let mut tui = spawn(&["recopy", "--quiet", "-r", "v1", dst_s]);
        expect_prompt(&mut tui, "project_name", "str", None);
        tui.sendline("_new");
        tui.expect_eof()
    } else {
        let (_, out) = capture(|| {
            Worker::new()
                .dst_path(&dst)
                .data(v(json!({"project_name": "myproj_new"})))
                .vcs_ref("v1")
                .quiet(true)
                .run_recopy()
                .unwrap()
        });
        out.stderr
    };

    assert_eq!(read(dst.join("version.txt")), "v1");
    assert!(!err.contains("Thank you for using our template"));
    assert!(!err.contains("Project myproj_new successfully created"));

    git(&dst, &["init"]);
    git(&dst, &["add", "."]);
    git(&dst, &["commit", "-m1"]);

    // clear capture output log

    // update
    let err = if interactive {
        let mut tui = spawn(&["update", "--quiet", dst_s]);
        expect_prompt(&mut tui, "project_name", "str", None);
        tui.sendline("_update");
        tui.expect_eof()
    } else {
        let (_, out) = capture(|| {
            Worker::new()
                .dst_path(&dst)
                .data(v(json!({"project_name": "myproj_new_update"})))
                .overwrite(true)
                .quiet(true)
                .run_update()
                .unwrap()
        });
        out.stderr
    };

    assert_eq!(read(dst.join("version.txt")), "v2");
    assert!(!err.contains("Updating on"));
    assert!(!err.contains("Project myproj_new_update successfully updated"));
}
