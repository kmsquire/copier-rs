//! Port of `tests/test_recopy.py`.

mod common;

use std::path::PathBuf;

use common::{
    file_tree, git, git_save, load_answers, read, run_cli, v, worker, Tmp, ANSWERS_FILE_TMPL,
};
use copier::{Value, Worker};
use indoc::indoc;
use serde_json::json;

/// A simple template that supports updates (the `tpl` fixture).
fn tpl(tmp: &Tmp) -> PathBuf {
    let dst = tmp.mktemp("tpl");
    file_tree! {
        dst.join("copier.yml") => "your_name: Mario",
        dst.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|to_nice_yaml }}",
        dst.join("name.txt.jinja") => "This is your name: {{ your_name }}.",
    };
    git_save(&dst, "Test commit", None);
    dst
}

#[test]
fn test_recopy_discards_evolution_api() {
    let tmp = Tmp::new();
    let tpl = tpl(&tmp);
    let dst = tmp.mktemp("dst");
    // First copy
    worker(&tpl, &dst)
        .data(v(json!({"your_name": "Luigi"})))
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    git_save(&dst, "Test commit", None);
    let name_path = dst.join("name.txt");
    assert_eq!(read(&name_path), "This is your name: Luigi.");
    // Evolve subproject
    std::fs::write(&name_path, "This is your name: Luigi. Welcome.").unwrap();
    git_save(&dst, "Test commit", None);
    // Recopy
    Worker::new()
        .dst_path(&dst)
        .defaults(true)
        .overwrite(true)
        .run_recopy()
        .unwrap();
    assert_eq!(read(&name_path), "This is your name: Luigi.");
}

#[test]
fn test_recopy_discards_evolution_cli() {
    let tmp = Tmp::new();
    let tpl = tpl(&tmp);
    let dst = tmp.mktemp("dst");
    // First copy
    worker(&tpl, &dst)
        .data(v(json!({"your_name": "Peach"})))
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    git_save(&dst, "Test commit", None);
    let name_path = dst.join("name.txt");
    assert_eq!(read(&name_path), "This is your name: Peach.");
    // Evolve subproject
    std::fs::write(&name_path, "This is your name: Peach. Welcome.").unwrap();
    git_save(&dst, "Test commit", None);
    // Recopy; the destination is passed explicitly instead of changing directory
    let (retcode, _) = run_cli(&["recopy", "-f", dst.to_str().unwrap()]);
    assert_eq!(retcode, 0);
    assert_eq!(read(&name_path), "This is your name: Peach.");
}

#[test]
fn test_recopy_works_without_replay() {
    let tmp = Tmp::new();
    let tpl = tpl(&tmp);
    let dst = tmp.mktemp("dst");
    // First copy
    worker(&tpl, &dst)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    git_save(&dst, "Test commit", None);
    assert_eq!(read(dst.join("name.txt")), "This is your name: Mario.");
    // Modify template altering git history
    std::fs::write(
        tpl.join("name.txt.jinja"),
        "This is my name: {{ your_name }}.",
    )
    .unwrap();
    git(&tpl, &["commit", "-a", "--amend", "--no-edit"]);
    // Make sure old dangling commit is lost
    // DOCS https://stackoverflow.com/a/63209363/1468388
    git(&tpl, &["reflog", "expire", "--expire=now", "--all"]);
    git(&tpl, &["gc", "--prune=now", "--aggressive"]);
    // Recopy
    Worker::new()
        .dst_path(&dst)
        .skip_answered(true)
        .overwrite(true)
        .run_recopy()
        .unwrap();
    assert_eq!(read(dst.join("name.txt")), "This is my name: Mario.");
}

#[test]
fn test_recopy_with_skip_answered_and_new_answer() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => "boolean: false",
        src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|to_nice_yaml }}",
    };
    git_save(&src, "Test commit", None);
    // First copy
    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    git_save(&dst, "Test commit", None);
    let answers = load_answers(&dst);
    assert_eq!(answers["boolean"], Value::Bool(false));
    // Recopy with different answer and `skip_answered=True`
    Worker::new()
        .dst_path(&dst)
        .data(v(json!({"boolean": "true"})))
        .skip_answered(true)
        .overwrite(true)
        .run_recopy()
        .unwrap();
    let answers = load_answers(&dst);
    assert_eq!(answers["boolean"], Value::Bool(true));
}

#[test]
fn test_recopy_dont_validate_computed_value() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => indoc! {"
            computed:
                type: str
                default: foo
                when: false
                validator: \"This validator should never be rendered\"
            "},
        src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|to_nice_yaml }}",
    };
    git_save(&src, "Test commit", None);
    // First copy
    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    git_save(&dst, "Test commit", None);
    let answers = load_answers(&dst);
    assert!(!answers.contains_key("computed"));
    // Recopy
    Worker::new()
        .dst_path(&dst)
        .overwrite(true)
        .run_recopy()
        .unwrap();
    let answers = load_answers(&dst);
    assert!(!answers.contains_key("computed"));
}

#[test]
fn test_conditional_computed_value() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    file_tree! {
        src.join("copier.yml") => indoc! {"
            first:
                type: bool

            second:
                type: bool
                default: \"{{ first }}\"
                when: \"{{ first }}\"
            "},
        src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|to_nice_yaml }}",
        src.join("log.txt.jinja") => "{{ first }} {{ second }}",
    };
    git_save(&src, "Test commit", None);

    worker(&src, &dst)
        .data(v(json!({"first": true})))
        .defaults(true)
        .run_copy()
        .unwrap();
    let answers = load_answers(&dst);
    assert_eq!(answers["first"], Value::Bool(true));
    assert_eq!(answers["second"], Value::Bool(true));
    assert_eq!(read(dst.join("log.txt")), "True True");

    git_save(&dst, "v1", None);

    Worker::new()
        .dst_path(&dst)
        .data(v(json!({"first": false})))
        .overwrite(true)
        .run_recopy()
        .unwrap();
    let answers = load_answers(&dst);
    assert_eq!(answers["first"], Value::Bool(false));
    assert!(!answers.contains_key("second"));
    assert_eq!(read(dst.join("log.txt")), "False False");

    git_save(&dst, "v2", None);

    Worker::new()
        .dst_path(&dst)
        .data(v(json!({"first": true})))
        .defaults(true)
        .overwrite(true)
        .run_recopy()
        .unwrap();
    let answers = load_answers(&dst);
    assert_eq!(answers["first"], Value::Bool(true));
    assert_eq!(answers["second"], Value::Bool(true));
    assert_eq!(read(dst.join("log.txt")), "True True");
}
