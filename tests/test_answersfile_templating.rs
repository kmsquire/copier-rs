//! Port of `tests/test_answersfile_templating.py`.

mod common;

use std::path::PathBuf;

use common::{file_tree, load_answers_from, read, v, worker, Tmp, ANSWERS_FILE_TMPL};
use copier::Value;
use indoc::indoc;
use rstest::rstest;
use serde_json::json;

/// Template whose `_answers_file` setting is itself a template of `module_name`.
fn template_path(tmp: &Tmp) -> PathBuf {
    let root = tmp.mktemp("template");
    file_tree! {
        root.join(ANSWERS_FILE_TMPL) => indoc! {"
            # Changes here will be overwritten by Copier
            {{ _copier_answers|to_nice_yaml }}
        "},
        root.join("copier.yml") => indoc! {"
            _answers_file: \".copier-answers-{{ module_name }}.yml\"

            module_name:
                type: str
        "},
    };
    root
}

#[ignore = "pending"]
#[rstest]
fn test_answersfile_templating(
    #[values(None, Some(".changed-by-user.yml"))] answers_file: Option<&str>,
) {
    // Test copier behaves properly when _answers_file contains a template
    //
    // Checks that template is resolved successfully and that a subsequent
    // copy that resolves to a different answers file doesn't clobber the
    // old answers file.
    let template_tmp = Tmp::new();
    let template = template_path(&template_tmp);
    let tmp = Tmp::new();

    let mut copy = worker(&template, tmp.path())
        .data(v(json!({"module_name": "mymodule"})))
        .defaults(true)
        .overwrite(true)
        .unsafe_(true);
    if let Some(name) = answers_file {
        copy = copy.answers_file(name);
    }
    copy.run_copy().unwrap();
    let first_answers_file = answers_file.unwrap_or(".copier-answers-mymodule.yml");
    assert!(tmp.path().join(first_answers_file).exists());
    let answers = load_answers_from(tmp.path(), first_answers_file);
    assert_eq!(answers["module_name"], Value::from("mymodule"));

    worker(&template, tmp.path())
        .data(v(json!({"module_name": "anothermodule"})))
        .defaults(true)
        .overwrite(true)
        .unsafe_(true)
        .run_copy()
        .unwrap();

    // Assert second one created
    let second_answers_file = ".copier-answers-anothermodule.yml";
    assert!(tmp.path().join(second_answers_file).exists());
    let answers = load_answers_from(tmp.path(), second_answers_file);
    assert_eq!(answers["module_name"], Value::from("anothermodule"));

    // Assert first one still exists
    assert!(tmp.path().join(first_answers_file).exists());
    let answers = load_answers_from(tmp.path(), first_answers_file);
    assert_eq!(answers["module_name"], Value::from("mymodule"));
}

#[test]
#[ignore = "pending"]
fn test_answersfile_templating_with_message_before_copy() {
    // Test templated `_answers_file` setting with `_message_before_copy`.
    //
    // Checks that the templated answers file name is rendered correctly while
    // having printing a message before the "copy" operation, which uses the render
    // context before including any answers from the questionnaire.
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => indoc! {"
            _answers_file: \".copier-answers-{{ module_name }}.yml\"
            _message_before_copy: \"Hello world\"

            module_name:
                type: str
        "},
        src.join(ANSWERS_FILE_TMPL) => indoc! {"
            # Changes here will be overwritten by Copier
            {{ _copier_answers|to_nice_yaml }}
        "},
        src.join("result.txt.jinja") => "{{ module_name }}",
    };
    worker(&src, &dst)
        .data(v(json!({"module_name": "mymodule"})))
        .overwrite(true)
        .unsafe_(true)
        .run_copy()
        .unwrap();
    assert!(dst.join(".copier-answers-mymodule.yml").exists());
    let answers = load_answers_from(&dst, ".copier-answers-mymodule.yml");
    assert_eq!(answers["module_name"], Value::from("mymodule"));
    assert!(dst.join("result.txt").exists());
    assert_eq!(read(dst.join("result.txt")), "mymodule");
}

#[test]
#[ignore = "pending"]
fn test_answersfile_templating_phase() {
    // Ensure `_copier_phase` is available while render `answers_relpath`.
    // Not because it is directly useful, but because some extensions might need it.
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => indoc! {"
            _answers_file: \".copier-answers-{{ _copier_phase }}.yml\"
        "},
        src.join(ANSWERS_FILE_TMPL) => "",
    };
    worker(&src, &dst)
        .overwrite(true)
        .unsafe_(true)
        .run_copy()
        .unwrap();
    assert!(dst.join(".copier-answers-render.yml").exists());
}
