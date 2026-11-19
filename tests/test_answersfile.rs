//! Port of `tests/test_answersfile.py`.

mod common;

use std::path::PathBuf;

use common::{
    assert_err, file_tree, git_save, load_answers, load_answers_from, read, v, worker, Tmp,
    ANSWERS_FILE_TMPL, BRACKET_ENVOPS_JSON, SUFFIX_TMPL,
};
use copier::errors::{capture_warnings, WarningKind};
use copier::{CopierError, Value, Worker};
use indoc::indoc;
use rstest::rstest;
use serde_json::json;

/// Message of the deprecation warning for answers file templates outside the root.
const DEPRECATION: &str =
    "Answers file template locations other than the template root directory are deprecated";

/// Template with a `round` question and secret password questions.
fn template_path(tmp: &Tmp) -> PathBuf {
    let root = tmp.mktemp("template");
    file_tree! {
        root.join("[[ _copier_conf.answers_file ]].tmpl") => indoc! {"
            # Changes here will be overwritten by Copier
            [[ _copier_answers|to_nice_yaml ]]
        "},
        root.join("copier.yml") => format!("
            _answers_file: .answers-file-changed-in-template.yml
            _templates_suffix: {SUFFIX_TMPL}
            _envops: {BRACKET_ENVOPS_JSON}

            round: 1st

            # password_1 and password_2 must not appear in answers file
            _secret_questions:
                - password_1

            password_1: password one
            password_2:
                secret: yes
                default: password two
        "),
        root.join("round.txt.tmpl") => indoc! {"
            It's the [[round]] round.
            password_1=[[password_1]]
            password_2=[[password_2]]
        "},
    };
    root
}

#[ignore = "pending"]
#[rstest]
fn test_answersfile(#[values(None, Some(".changed-by-user.yaml"))] answers_file: Option<&str>) {
    let template_tmp = Tmp::new();
    let template = template_path(&template_tmp);
    let tmp = Tmp::new();
    let round_file = tmp.path().join("round.txt");

    // Check 1st round is properly executed and remembered
    let mut copy = worker(&template, tmp.path()).defaults(true).overwrite(true);
    if let Some(name) = answers_file {
        copy = copy.answers_file(name);
    }
    copy.run_copy().unwrap();
    let answers_file = answers_file.unwrap_or(".answers-file-changed-in-template.yml");
    assert_eq!(
        read(&round_file),
        "It's the 1st round.\npassword_1=password one\npassword_2=password two\n"
    );
    let log = load_answers_from(tmp.path(), answers_file);
    assert_eq!(log["round"], Value::from("1st"));
    assert!(!log.contains_key("password_1"));
    assert!(!log.contains_key("password_2"));

    // Check 2nd round is properly executed and remembered
    worker(&template, tmp.path())
        .answers_file(answers_file)
        .data(v(json!({"round": "2nd"})))
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    assert_eq!(
        read(&round_file),
        "It's the 2nd round.\npassword_1=password one\npassword_2=password two\n"
    );
    let log = load_answers_from(tmp.path(), answers_file);
    assert_eq!(log["round"], Value::from("2nd"));
    assert!(!log.contains_key("password_1"));
    assert!(!log.contains_key("password_2"));

    // Check repeating 2nd is properly executed and remembered
    worker(&template, tmp.path())
        .answers_file(answers_file)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    assert_eq!(
        read(&round_file),
        "It's the 2nd round.\npassword_1=password one\npassword_2=password two\n"
    );
    let log = load_answers_from(tmp.path(), answers_file);
    assert_eq!(log["round"], Value::from("2nd"));
    assert!(!log.contains_key("password_1"));
    assert!(!log.contains_key("password_2"));
}

#[ignore = "pending"]
#[rstest]
fn test_answersfile_with_custom_path(
    #[values(
        None,
        Some(".copier-answers.template.yml"),
        Some(".config/.copier-answers.template.yml")
    )]
    template_answers_file: Option<&str>,
    #[values(
        None,
        Some(".copier-answers.user.yml"),
        Some(".config/.copier-answers.user.yml")
    )]
    user_answers_file: Option<&str>,
) {
    // Test specifying the answers file path via template or user setting.
    let answers_file = user_answers_file
        .or(template_answers_file)
        .unwrap_or(".copier-answers.yml");

    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    file_tree! {
        src.join("copier.yml") => template_answers_file
            .map_or(String::new(), |name| format!("_answers_file: {name}")),
        src.join(ANSWERS_FILE_TMPL) => indoc! {"
            # Changes here will be overwritten by Copier
            {{ _copier_answers|to_nice_yaml }}
        "},
        src.join("version.txt.jinja") => "v1",
    };
    git_save(&src, "v1", Some("v1"));

    file_tree! { src.join("version.txt.jinja") => "v2" };
    git_save(&src, "v2", Some("v2"));

    let mut copy = worker(&src, &dst).vcs_ref("v1").overwrite(true);
    if let Some(name) = user_answers_file {
        copy = copy.answers_file(name);
    }
    copy.run_copy().unwrap();
    assert_eq!(read(dst.join("version.txt")), "v1");
    assert_eq!(
        load_answers_from(&dst, answers_file),
        v(json!({"_src_path": src.to_str().unwrap(), "_commit": "v1"}))
    );
    git_save(&dst, "v1", None);

    Worker::new()
        .dst_path(&dst)
        .vcs_ref("v2")
        .answers_file(answers_file)
        .overwrite(true)
        .run_update()
        .unwrap();
    assert_eq!(read(dst.join("version.txt")), "v2");
    assert_eq!(
        load_answers_from(&dst, answers_file),
        v(json!({"_src_path": src.to_str().unwrap(), "_commit": "v2"}))
    );
}

#[test]
#[ignore = "pending"]
fn test_answersfile_template_in_subdirectory_is_deprecated() {
    // Test that the answers file template in a subdirectory is deprecated.
    //
    // NOTE: The answers file is *not* rendered in the subdirectory but in the location
    // specified by:
    //
    // 1. The user-provided answers file location (via `-a,--answers-file` CLI flag or
    //     equivalent API argument)
    // 2. The `_answers_file` setting in `copier.yml`
    // 3. The default answers file name and location at `.copier-answers.yml`
    let answers_file = ".copier-answers.yml";

    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    file_tree! {
        src.join(".config").join(ANSWERS_FILE_TMPL) => indoc! {"
            # Changes here will be overwritten by Copier
            {{ _copier_answers|to_nice_yaml }}
        "},
        src.join("version.txt.jinja") => "v1",
    };
    git_save(&src, "v1", Some("v1"));

    file_tree! { src.join("version.txt.jinja") => "v2" };
    git_save(&src, "v2", Some("v2"));

    let (result, warnings) =
        capture_warnings(|| worker(&src, &dst).vcs_ref("v1").overwrite(true).run_copy());
    result.unwrap();
    assert!(warnings
        .iter()
        .any(|w| w.kind == WarningKind::Deprecation && w.message.contains(DEPRECATION)));
    assert_eq!(read(dst.join("version.txt")), "v1");
    assert_eq!(
        load_answers_from(&dst, answers_file),
        v(json!({"_src_path": src.to_str().unwrap(), "_commit": "v1"}))
    );
    git_save(&dst, "v1", None);

    let (result, warnings) = capture_warnings(|| {
        Worker::new()
            .dst_path(&dst)
            .vcs_ref("v2")
            .answers_file(answers_file)
            .overwrite(true)
            .run_update()
    });
    result.unwrap();
    assert!(warnings
        .iter()
        .any(|w| w.kind == WarningKind::Deprecation && w.message.contains(DEPRECATION)));
    assert_eq!(read(dst.join("version.txt")), "v2");
    assert_eq!(
        load_answers_from(&dst, answers_file),
        v(json!({"_src_path": src.to_str().unwrap(), "_commit": "v2"}))
    );
}

#[test]
#[ignore = "pending"]
fn test_external_data() {
    let tmp = Tmp::new();
    let (parent1, parent2, child, dst) = (
        tmp.mktemp("parent1"),
        tmp.mktemp("parent2"),
        tmp.mktemp("child"),
        tmp.mktemp("dst"),
    );
    file_tree! {
        parent1.join("copier.yaml") => "{name: P1, child: C1}",
        parent1.join("parent1.txt.jinja") => "{{ name }}",
        parent1.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|to_nice_yaml -}}",
        parent2.join("copier.yaml") => "name: P2",
        parent2.join("parent2.txt.jinja") => "{{ name }}",
        parent2.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|to_nice_yaml -}}",
        child.join("copier.yml") => indoc! {"
            _external_data:
                parent1: .copier-answers.yml
                parent2: \"{{ parent2_answers }}\"
            parent2_answers: .parent2-answers.yml
            name: \"{{ _external_data.parent2.child | d(_external_data.parent1.child) }}\"
        "},
        child.join("combined.json.jinja") => indoc! {"
            {
                \"parent1\": {{ _external_data.parent1.name | tojson }},
                \"parent2\": {{ _external_data.parent2.name | tojson }},
                \"child\": {{ name | tojson }}
            }
        "},
        child.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|to_nice_yaml -}}",
    };
    git_save(&parent1, "Test commit", Some("v1.0+parent1"));
    git_save(&parent2, "Test commit", Some("v1.0+parent2"));
    git_save(&child, "Test commit", Some("v1.0+child"));
    // Apply parent 1. At this point we don't know we'll want more than 1
    // template in the same subproject, so we leave the default answers file.
    worker(&parent1, &dst)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    git_save(&dst, "Test commit", None);
    assert_eq!(read(dst.join("parent1.txt")), "P1");
    let expected_parent1_answers = v(json!({
        "_src_path": parent1.to_str().unwrap(),
        "_commit": "v1.0+parent1",
        "name": "P1",
        "child": "C1",
    }));
    assert_eq!(
        load_answers_from(&dst, ".copier-answers.yml"),
        expected_parent1_answers
    );
    // Apply parent 2. It uses a different answers file.
    worker(&parent2, &dst)
        .defaults(true)
        .overwrite(true)
        .answers_file(".parent2-answers.yml")
        .run_copy()
        .unwrap();
    git_save(&dst, "Test commit", None);
    assert_eq!(read(dst.join("parent2.txt")), "P2");
    let expected_parent2_answers = v(json!({
        "_commit": "v1.0+parent2",
        "_src_path": parent2.to_str().unwrap(),
        "name": "P2",
    }));
    assert_eq!(
        load_answers_from(&dst, ".parent2-answers.yml"),
        expected_parent2_answers
    );
    // Apply child. It can access answers from both parents.
    worker(&child, &dst)
        .defaults(true)
        .overwrite(true)
        .answers_file(".child-answers.yml")
        .run_copy()
        .unwrap();
    git_save(&dst, "Test commit", None);
    assert_eq!(
        load_answers_from(&dst, ".child-answers.yml"),
        v(json!({
            "_commit": "v1.0+child",
            "_src_path": child.to_str().unwrap(),
            "name": "C1",
            "parent2_answers": ".parent2-answers.yml",
        }))
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&read(dst.join("combined.json"))).unwrap(),
        json!({
            "parent1": "P1",
            "parent2": "P2",
            "child": "C1",
        })
    );
}

#[test]
#[ignore = "pending"]
fn test_external_data_with_umlaut() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    file_tree! {
        src.join("copier.yml") => indoc! {"
            _external_data:
                data: data.yml
            ext_umlaut: \"{{ _external_data.data.umlaut }}\"
        "},
        src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|to_nice_yaml }}",
    };
    git_save(&src, "Test commit", Some("v1"));

    std::fs::write(dst.join("data.yml"), "umlaut: äöü").unwrap();

    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    let answers = load_answers_from(&dst, ".copier-answers.yml");
    assert_eq!(answers["ext_umlaut"], Value::from("äöü"));
}

/// Encode `data` in `encoding`, prefixing a BOM for the UTF-16 variants.
fn encode(data: &str, encoding: &str) -> Vec<u8> {
    match encoding {
        "utf-8" => data.as_bytes().to_vec(),
        "utf-8-sig" => [&[0xEF, 0xBB, 0xBF][..], data.as_bytes()].concat(),
        "utf-16-le" => format!("\u{feff}{data}")
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect(),
        "utf-16-be" => format!("\u{feff}{data}")
            .encode_utf16()
            .flat_map(u16::to_be_bytes)
            .collect(),
        other => panic!("unknown encoding {other}"),
    }
}

#[ignore = "needs-design: cannot override the default text encoding (monkeypatch of io.text_encoding)"]
#[rstest]
fn test_external_data_with_unicode_characters(
    #[values("utf-8", "utf-8-sig", "utf-16-le", "utf-16-be")] encoding: &str,
) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    file_tree! {
        src.join("copier.yml") => indoc! {"
            _external_data:
                data: data.yml
            foo: \"{{ _external_data.data.foo }}\"
            bar: \"{{ _external_data.data.bar }}\"
        "},
        src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|to_nice_yaml }}",
    };
    git_save(&src, "Test commit", Some("v1"));

    let foo = "\u{3053}\u{3093}\u{306b}\u{3061}\u{306f}"; // japanese hiragana
    let bar = "\u{1f60e}"; // smiling face with sunglasses
    std::fs::write(
        dst.join("data.yml"),
        encode(&format!("foo: {foo}\nbar: {bar}\n"), encoding),
    )
    .unwrap();

    // TODO(port): needs-design: no hook to override the default text encoding, so the
    // monkeypatch below cannot be ported.
    // with monkeypatch.context() as m:
    //     # Override the factor that determines the default encoding when opening files.
    //     m.setattr("io.text_encoding", lambda *_args: "cp932")
    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();

    let answers = load_answers(&dst);
    assert_eq!(answers["foo"], Value::from(foo));
    assert_eq!(answers["bar"], Value::from(bar));
}

#[test]
#[ignore = "pending"]
fn test_undefined_phase_in_external_data() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    file_tree! {
        src.join("copier.yml") => indoc! {"
            _external_data:
                data: '{{ _copier_phase }}.yml'
            key: \"{{ _external_data.data.key }}\"
        "},
        src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|to_nice_yaml }}",
    };
    git_save(&src, "Test commit", Some("v1"));

    std::fs::write(dst.join("undefined.yml"), "key: value").unwrap();

    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    let answers = load_answers_from(&dst, ".copier-answers.yml");
    assert_eq!(answers["key"], Value::from("value"));
}

#[ignore = "pending"]
#[rstest]
fn test_external_data_path_outside_destination_root_is_unsafe_on_copy(
    #[values(false, true)] is_absolute: bool,
    #[values(false, true)] unsafe_: bool,
) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    let project = dst.join("project");
    std::fs::create_dir(&project).unwrap();

    let secret_file = dst.join("secret.yml");
    std::fs::write(&secret_file, "secret_key: s3cr3t").unwrap();

    let external_data_path = if is_absolute {
        secret_file
    } else {
        PathBuf::from("..").join("secret.yml")
    };

    let external_data_path = external_data_path.display();
    file_tree! {
        src.join("copier.yml") => format!("
            _external_data:
                secret: {external_data_path}

            secret: \"{{{{ _external_data.secret.secret_key }}}}\"
        "),
        src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers | to_nice_yaml }}",
    };
    git_save(&src, "Test commit", Some("v1"));

    let copy = worker(&src, &project).defaults(true).unsafe_(unsafe_);
    if unsafe_ {
        copy.run_copy().unwrap();
    } else {
        assert_err!(copy.run_copy(), CopierError::ForbiddenPath { .. });
    }
}

#[ignore = "pending"]
#[rstest]
fn test_external_data_path_outside_destination_root_is_unsafe_on_update(
    #[values(false, true)] is_absolute: bool,
    #[values(false, true)] unsafe_: bool,
) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    let project = dst.join("project");
    std::fs::create_dir(&project).unwrap();

    file_tree! {
        src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers | to_nice_yaml }}",
    };
    git_save(&src, "Test commit", Some("v1"));

    worker(&src, &project).run_copy().unwrap();
    assert_eq!(
        load_answers(&project).get("_commit"),
        Some(&Value::from("v1"))
    );

    git_save(&project, "Test commit", None);

    let secret_file = dst.join("secret.yml");
    std::fs::write(&secret_file, "secret_key: s3cr3t").unwrap();

    let external_data_path = if is_absolute {
        secret_file
    } else {
        PathBuf::from("..").join("secret.yml")
    };

    let external_data_path = external_data_path.display();
    file_tree! {
        src.join("copier.yml") => format!("
            _external_data:
                secret: {external_data_path}

            secret: \"{{{{ _external_data.secret.secret_key }}}}\"
        "),
    };
    git_save(&src, "Test commit", Some("v2"));

    let update = Worker::new()
        .dst_path(&project)
        .defaults(true)
        .overwrite(true)
        .unsafe_(unsafe_);
    if unsafe_ {
        update.run_update().unwrap();
    } else {
        assert_err!(update.run_update(), CopierError::ForbiddenPath { .. });
    }
}
