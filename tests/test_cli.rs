//! Port of `tests/test_cli.py`.

mod common;

use std::path::{Path, PathBuf};

use common::{
    copier_cmd, file_tree, git, load_answers, load_answers_from, read, run_cli, v, Tmp,
    ANSWERS_FILE_TMPL,
};
use copier::errors::{capture_warnings, WarningKind};
use copier::Value;
use indoc::indoc;
use rstest::rstest;
use serde_json::json;

const DEPRECATION: &str =
    "Answers file template locations other than the template root directory are deprecated";

/// A template with a `{{ _copier_conf.answers_file }}.jinja` and an `a.txt` file
/// (the `template_path` fixture in Python).
fn template_path(tmp: &Tmp) -> PathBuf {
    let root = tmp.mktemp("template");
    file_tree! {
        root.join(ANSWERS_FILE_TMPL) => indoc! {"
            # Changes here will be overwritten by Copier
            {{ _copier_answers|to_nice_yaml }}
        "},
        root.join("a.txt") => "EXAMPLE_CONTENT",
    };
    root
}

/// A template whose answers file lives in `config_folder` (the
/// `template_path_with_dot_config` fixture in Python).
fn template_path_with_dot_config(tmp: &Tmp, config_folder: &str) -> PathBuf {
    let root = tmp.mktemp("template");
    file_tree! {
        root.join(config_folder).join(ANSWERS_FILE_TMPL) => indoc! {"
            # Changes here will be overwritten by Copier
            {{ _copier_answers|to_nice_yaml }}
        "},
        root.join("a.txt") => "EXAMPLE_CONTENT",
    };
    root
}

#[test]
#[ignore = "pending"]
fn test_good_cli_run() {
    let tmp = Tmp::new();
    let src = template_path(&tmp);
    let dst = tmp.mktemp("dst");
    let (src_s, dst_s) = (src.to_str().unwrap(), dst.to_str().unwrap());
    let (retcode, _) = run_cli(&["copy", "--quiet", "-a", "altered-answers.yml", src_s, dst_s]);
    let a_txt = dst.join("a.txt");
    assert_eq!(retcode, 0);
    assert!(a_txt.exists());
    assert!(a_txt.is_file());
    assert_eq!(read(&a_txt), "EXAMPLE_CONTENT");
    let answers = load_answers_from(&dst, "altered-answers.yml");
    assert_eq!(answers["_src_path"], Value::from(src_s));
}

#[rstest]
#[case("str", "string", json!("string"))]
#[case("str", "1", json!("1"))]
#[case("str", "1.2", json!("1.2"))]
#[case("str", "true", json!("true"))]
#[case("str", "null", json!("null"))]
#[case(
    "str",
    r#"["string", 1, 1.2, true, null]"#,
    json!(r#"["string", 1, 1.2, true, null]"#)
)]
#[case("int", "1", json!(1))]
#[case("float", "1.2", json!(1.2))]
#[case("bool", "true", json!(true))]
#[case("bool", "false", json!(false))]
#[case("yaml", r#""string""#, json!("string"))]
#[case("yaml", "1", json!(1))]
#[case("yaml", "1.2", json!(1.2))]
#[case("yaml", "true", json!(true))]
#[case("yaml", "null", json!(null))]
#[case("yaml", r#"["string", 1, 1.2, true, null]"#, json!(["string", 1, 1.2, true, null]))]
#[case("json", r#""string""#, json!("string"))]
#[case("json", "1", json!(1))]
#[case("json", "1.2", json!(1.2))]
#[case("json", "true", json!(true))]
#[case("json", "null", json!(null))]
#[case("json", r#"["string", 1, 1.2, true, null]"#, json!(["string", 1, 1.2, true, null]))]
#[ignore = "pending"]
fn test_cli_data_parsed_by_question_type(
    #[case] type_: &str,
    #[case] answer: &str,
    #[case] expected: serde_json::Value,
) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    let (src_s, dst_s) = (src.to_str().unwrap(), dst.to_str().unwrap());
    file_tree! {
        src.join("copier.yml") => format!("question:\n    type: {type_}\n"),
        src.join(ANSWERS_FILE_TMPL) => indoc! {"
            # Changes here will be overwritten by Copier
            {{ _copier_answers|to_nice_yaml }}
        "},
    };
    let data_arg = format!("--data=question={answer}");
    let (retcode, _) = run_cli(&["copy", &data_arg, src_s, dst_s, "--quiet"]);
    assert_eq!(retcode, 0);
    let answers = load_answers(&dst);
    assert_eq!(answers["_src_path"], Value::from(src_s));
    assert_eq!(answers["question"], v(expected));
}

#[test]
#[ignore = "pending"]
fn test_data_file_parsed_by_question_type() {
    let tmp = Tmp::new();
    let (src, dst, local) = (tmp.mktemp("src"), tmp.mktemp("dst"), tmp.mktemp("local"));
    let (src_s, dst_s) = (src.to_str().unwrap(), dst.to_str().unwrap());
    file_tree! {
        src.join("copier.yml") => indoc! {"
            question:
                type: str
        "},
        src.join(ANSWERS_FILE_TMPL) => indoc! {"
            # Changes here will be overwritten by Copier
            {{ _copier_answers|to_nice_yaml }}
        "},
        local.join("data.yml") => json!({"question": "answer"}).to_string(),
    };

    let data_file_arg = format!("--data-file={}", local.join("data.yml").display());
    let (retcode, _) = run_cli(&["copy", &data_file_arg, src_s, dst_s, "--quiet"]);
    assert_eq!(retcode, 0);
    let answers = load_answers(&dst);
    assert_eq!(answers["_src_path"], Value::from(src_s));
    assert_eq!(answers["question"], Value::from("answer"));
}

#[test]
#[ignore = "pending"]
fn test_data_cli_takes_precedence_over_data_file() {
    let tmp = Tmp::new();
    let (src, dst, local) = (tmp.mktemp("src"), tmp.mktemp("dst"), tmp.mktemp("local"));
    let (src_s, dst_s) = (src.to_str().unwrap(), dst.to_str().unwrap());
    file_tree! {
        src.join("copier.yml") => indoc! {"
            question:
                type: str
            another_question:
                type: str
            yet_another_question:
                type: str
        "},
        src.join(ANSWERS_FILE_TMPL) => indoc! {"
            # Changes here will be overwritten by Copier
            {{ _copier_answers|to_nice_yaml }}
        "},
        local.join("data.yml") => json!({
            "question": "does not matter",
            "another_question": "another answer!",
            "yet_another_question": "sure, one more for you!",
        })
        .to_string(),
    };

    let answer_override = "fourscore and seven years ago";
    let data_file_arg = format!("--data-file={}", local.join("data.yml").display());
    let data_arg = format!("--data=question={answer_override}");
    let (retcode, _) = run_cli(&["copy", &data_file_arg, &data_arg, src_s, dst_s, "--quiet"]);
    assert_eq!(retcode, 0);
    let actual_answers = load_answers(&dst);
    let expected_answers = v(json!({
        "_src_path": src_s,
        "question": answer_override,
        "another_question": "another answer!",
        "yet_another_question": "sure, one more for you!",
    }));
    assert_eq!(actual_answers, expected_answers);
}

/// Encode a YAML document in `encoding`, prefixing a BOM for UTF-16 like the Python test.
fn encode_data(encoding: &str, data: &str) -> Vec<u8> {
    let dump = if encoding.starts_with("utf-16") {
        format!("\u{feff}{data}")
    } else {
        data.to_owned()
    };
    match encoding {
        "utf-16-le" => dump.encode_utf16().flat_map(u16::to_le_bytes).collect(),
        "utf-16-be" => dump.encode_utf16().flat_map(u16::to_be_bytes).collect(),
        "utf-8-sig" => [&[0xEF, 0xBB, 0xBF][..], dump.as_bytes()].concat(),
        _ => dump.into_bytes(),
    }
}

#[rstest]
#[case("utf-8")]
#[case("utf-8-sig")]
#[case("utf-16-le")]
#[case("utf-16-be")]
#[ignore = "needs-design: monkeypatches io.text_encoding, which has no Rust equivalent"]
fn test_read_utf_data_file(#[case] data_file_encoding: &str) {
    let tmp = Tmp::new();
    let (src, dst, local) = (tmp.mktemp("src"), tmp.mktemp("dst"), tmp.mktemp("local"));
    let (src_s, dst_s) = (src.to_str().unwrap(), dst.to_str().unwrap());

    file_tree! {
        src.join("copier.yml") => indoc! {"
            text1:
                type: str
            text2:
                type: str
        "},
        src.join(ANSWERS_FILE_TMPL) => indoc! {"
            # Changes here will be overwritten by Copier
            {{ _copier_answers|to_nice_yaml }}
        "},
        local.join("data.yml") => encode_data(
            data_file_encoding,
            &json!({
                "text1": "\u{3053}\u{3093}\u{306b}\u{3061}\u{306f}", // japanese hiragana
                "text2": "\u{1f60e}", // smiling face with sunglasses
            })
            .to_string(),
        ),
    };

    // Override the factor that determine the default encoding when opening files.
    // data.yml should be read correctly regardless of this value.
    // TODO(port): `monkeypatch.context()` setting `io.text_encoding` to "cp932" has no
    // Rust equivalent; the override is omitted and the call below runs unchanged.
    // with monkeypatch.context() as m:
    //     m.setattr("io.text_encoding", lambda *_args: "cp932")
    let data_file_arg = format!("--data-file={}", local.join("data.yml").display());
    let (retcode, _) = run_cli(&["copy", &data_file_arg, src_s, dst_s]);

    assert_eq!(retcode, 0);
    let expected_answers = v(json!({
        "_src_path": src_s,
        "text1": "\u{3053}\u{3093}\u{306b}\u{3061}\u{306f}",
        "text2": "\u{1f60e}",
    }));
    let answers = load_answers(&dst);
    assert_eq!(answers, expected_answers);
}

/// Function to test different locations of the answersfile.
///
/// Added based on the discussion: https://github.com/copier-org/copier/discussions/859
#[rstest]
#[case("")]
#[case(".config/")]
#[case(".config/subconfig/")]
#[case(".config/subconfig/deep_nested_config/")]
#[ignore = "pending"]
fn test_good_cli_run_dot_config(#[case] config_folder: &str) {
    let tmp = Tmp::new();
    let dst = tmp.mktemp("dst");
    let src = template_path_with_dot_config(&tmp, config_folder);
    let (src_s, dst_s) = (src.to_str().unwrap(), dst.to_str().unwrap());
    let altered_answers = format!("{config_folder}altered-answers.yml");
    let deprecated = !config_folder.is_empty();

    git_commands(&src);

    let ((retcode, _), warnings) = capture_warnings(|| {
        run_cli(&[
            "copy",
            "--quiet",
            "-a",
            altered_answers.as_str(),
            src_s,
            dst_s,
        ])
    });
    if deprecated {
        assert!(warnings
            .iter()
            .any(|w| w.kind == WarningKind::Deprecation && w.message.contains(DEPRECATION)));
    }
    let a_txt = dst.join("a.txt");
    assert_eq!(retcode, 0);
    assert!(a_txt.exists());
    assert!(a_txt.is_file());
    assert_eq!(read(&a_txt), "EXAMPLE_CONTENT");
    let answers = load_answers_from(dst.join(config_folder), "altered-answers.yml");
    assert_eq!(answers["_src_path"], Value::from(src_s));

    git_commands(&dst);

    let (run_update, update_warnings) = capture_warnings(|| {
        run_cli(&[
            "update",
            dst_s,
            "--answers-file",
            altered_answers.as_str(),
            "--quiet",
        ])
    });
    if deprecated {
        assert!(update_warnings
            .iter()
            .any(|w| w.kind == WarningKind::Deprecation && w.message.contains(DEPRECATION)));
    }
    assert_eq!(run_update.0, 0);
    assert_eq!(answers["_src_path"], Value::from(src_s));
}

fn git_commands(dir: &Path) {
    git(dir, &["init"]);
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-m", "init"]);
}

#[test]
#[ignore = "pending"]
fn test_help() {
    let output = copier_cmd().args(["--help-all"]).output().unwrap();
    let _help = String::from_utf8_lossy(&output.stdout);
    assert!(_help.contains("copier copy [SWITCHES] template_src destination_path"));
    assert!(_help.contains("copier update [SWITCHES] [destination_path=.]"));
}

#[test]
#[ignore = "needs-design: help output is rendered at a fixed width of 80"]
fn test_copy_help() {
    // TODO(port): `patch("plumbum.cli.application.get_terminal_size", return_value=(80, 1))`
    // has no Rust equivalent; the help width is fixed at 80 columns, so the patch is dropped.
    let (status, out) = run_cli(&["copy", "--help"]);
    assert_eq!(status, 0);
    let (header, body) = out.stdout.split_once('\n').unwrap();
    assert!(header.starts_with("copier copy"));
    assert_eq!(
        body,
        indoc! {r#"

        Copy from a template source to a destination.

        Usage:
            copier copy [SWITCHES] template_src destination_path

        Meta-switches:
            --completions SHELL:{bash, fish} Prints shell completion script and quits
            -h, --help                      Prints this help message and quits
            --help-all                      Prints help messages of all sub-commands and
                                            quits
            -v, --version                   Prints the program's version and quits

        Switches:
            -C, --no-cleanup                On error, do not delete destination if it
                                            was created by Copier.
            -T, --skip-tasks                Skip template tasks execution
            --UNSAFE, --trust               Allow templates with unsafe features (Jinja
                                            extensions, migrations, tasks)
            -a, --answers-file VALUE:str    Update using this path (relative to
                                            `destination_path`) to find the answers file
            --ask VALUE:str                 Ask the questions matching the given glob-
                                            pattern, even if they would be skipped by
                                            other options; may be given multiple times
            -d, --data VARIABLE=VALUE:str   Make VARIABLE available as VALUE when
                                            rendering the template; may be given
                                            multiple times
            --data-file PATH:ExistingFile   Load data from a YAML file
            -f, --force                     Same as `--defaults --overwrite`.
            -g, --prereleases               Use prereleases to compare template VCS
                                            tags.
            -l, --defaults                  Use default answers to questions, which
                                            might be null if not specified.
            -n, --pretend                   Run but do not make any changes
            -q, --quiet                     Suppress status output
            -r, --vcs-ref VALUE:str         Git reference to checkout in `template_src`.
                                            If you do not specify it, it will try to
                                            checkout the latest git tag, as sorted using
                                            the PEP 440 algorithm. If you want to
                                            checkout always the latest version, use
                                            `--vcs-ref=HEAD`. Use the special value
                                            `:current:` to refer to the current
                                            reference of the template if it already
                                            exists.
            -s, --skip VALUE:str            Skip specified files if they exist already;
                                            may be given multiple times
            -w, --overwrite                 Overwrite files that already exist, without
                                            asking.
            -x, --exclude VALUE:str         A name or shell-style pattern matching files
                                            or folders that must not be copied; may be
                                            given multiple times

        "#}
    );
}

#[test]
#[ignore = "needs-design: help output is rendered at a fixed width of 80"]
fn test_update_help() {
    // TODO(port): `patch("plumbum.cli.application.get_terminal_size", return_value=(80, 1))`
    // has no Rust equivalent; the help width is fixed at 80 columns, so the patch is dropped.
    let (status, out) = run_cli(&["update", "--help"]);
    assert_eq!(status, 0);
    let (header, body) = out.stdout.split_once('\n').unwrap();
    assert!(header.starts_with("copier update"));
    assert_eq!(
        body,
        indoc! {r#"

        Update a subproject from its original template

        The copy must have a valid answers file which contains info from the last Copier
        execution, including the source template (it must be a key called `_src_path`).

        If that file contains also `_commit`, and `destination_path` is a git
        repository, this command will do its best to respect the diff that you have
        generated since the last `copier` execution. To avoid that, use `copier recopy`
        instead.

        Usage:
            copier update [SWITCHES] [destination_path=.]

        Meta-switches:
            --completions SHELL:{bash, fish} Prints shell completion script and quits
            -h, --help                      Prints this help message and quits
            --help-all                      Prints help messages of all sub-commands and
                                            quits
            -v, --version                   Prints the program's version and quits

        Switches:
            -A, --skip-answered             Skip questions that have already been
                                            answered
            -T, --skip-tasks                Skip template tasks execution
            --UNSAFE, --trust               Allow templates with unsafe features (Jinja
                                            extensions, migrations, tasks)
            -a, --answers-file VALUE:str    Update using this path (relative to
                                            `destination_path`) to find the answers file
            --ask VALUE:str                 Ask the questions matching the given glob-
                                            pattern, even if they would be skipped by
                                            other options; may be given multiple times
            -c, --context-lines VALUE:int   Lines of context to use for detecting
                                            conflicts. Increase for accuracy, decrease
                                            for resilience.; the default is 3
            -d, --data VARIABLE=VALUE:str   Make VARIABLE available as VALUE when
                                            rendering the template; may be given
                                            multiple times
            --data-file PATH:ExistingFile   Load data from a YAML file
            -g, --prereleases               Use prereleases to compare template VCS
                                            tags.
            -l, -f, --defaults              Use default answers to questions, which
                                            might be null if not specified.
            -n, --pretend                   Run but do not make any changes
            -o, --conflict VALUE:{rej, inline} Behavior on conflict: Create .rej files, or
                                            add inline conflict markers.; the default is
                                            inline
            -q, --quiet                     Suppress status output
            -r, --vcs-ref VALUE:str         Git reference to checkout in `template_src`.
                                            If you do not specify it, it will try to
                                            checkout the latest git tag, as sorted using
                                            the PEP 440 algorithm. If you want to
                                            checkout always the latest version, use
                                            `--vcs-ref=HEAD`. Use the special value
                                            `:current:` to refer to the current
                                            reference of the template if it already
                                            exists.
            -s, --skip VALUE:str            Skip specified files if they exist already;
                                            may be given multiple times
            -x, --exclude VALUE:str         A name or shell-style pattern matching files
                                            or folders that must not be copied; may be
                                            given multiple times

        "#}
    );
}

#[test]
#[ignore = "needs-design: help output is rendered at a fixed width of 80"]
fn test_check_update_help() {
    // TODO(port): `patch("plumbum.cli.application.get_terminal_size", return_value=(80, 1))`
    // has no Rust equivalent; the help width is fixed at 80 columns, so the patch is dropped.
    let (status, out) = run_cli(&["check-update", "--help"]);
    assert_eq!(status, 0);
    let (header, body) = out.stdout.split_once('\n').unwrap();
    assert!(header.starts_with("copier check-update"));
    assert_eq!(
        body,
        indoc! {r#"

        Check if a copy is using the latest version of its original template

        The copy must have a valid answers file which contains info from the last Copier
        execution, including the source template (it must be a key called `_src_path`).

        If that file contains also `_commit` and `destination_path` is a git repository,
        this command will do its best to determine whether a newer version is available,
        applying PEP 440 to the template's history.

        Usage:
            copier check-update [SWITCHES] [destination_path=.]

        Meta-switches:
            --completions SHELL:{bash, fish} Prints shell completion script and quits
            -h, --help                      Prints this help message and quits
            --help-all                      Prints help messages of all sub-commands and
                                            quits
            -v, --version                   Prints the program's version and quits

        Switches:
            -a, --answers-file VALUE:str    Check for updates using this path (relative
                                            to `destination_path`) to find the answers
                                            file
            -g, --prereleases               Use prereleases to compare template VCS
                                            tags.
            --output-format VALUE:{plain, json} Output format, either 'plain' (the default)
                                            or 'json'.; the default is plain
            -q, --quiet                     Suppress status output, exit with status 2
                                            when a new template version is available

        "#}
    );
}

#[test]
#[ignore = "pending"]
fn test_python_run() {
    let output = copier_cmd().args(["--help-all"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
}

#[test]
#[ignore = "pending"]
fn test_skip_filenotexists() {
    let tmp = Tmp::new();
    let src = template_path(&tmp);
    let dst = tmp.mktemp("dst");
    let (src_s, dst_s) = (src.to_str().unwrap(), dst.to_str().unwrap());
    let (retcode, _) = run_cli(&[
        "copy",
        "--quiet",
        "-a",
        "altered-answers.yml",
        "--overwrite",
        "--skip=a.txt",
        src_s,
        dst_s,
    ]);
    let a_txt = dst.join("a.txt");
    assert_eq!(retcode, 0);
    assert!(a_txt.exists());
    assert!(a_txt.is_file());
    assert_eq!(read(&a_txt), "EXAMPLE_CONTENT");
    let answers = load_answers_from(&dst, "altered-answers.yml");
    assert_eq!(answers["_src_path"], Value::from(src_s));
}

#[test]
#[ignore = "pending"]
fn test_skip_fileexists() {
    let tmp = Tmp::new();
    let src = template_path(&tmp);
    let dst = tmp.mktemp("dst");
    let (src_s, dst_s) = (src.to_str().unwrap(), dst.to_str().unwrap());
    let a_txt = dst.join("a.txt");
    std::fs::write(&a_txt, "PREVIOUS_CONTENT").unwrap();
    let (retcode, _) = run_cli(&[
        "copy",
        "--quiet",
        "-a",
        "altered-answers.yml",
        "--overwrite",
        "--skip=a.txt",
        src_s,
        dst_s,
    ]);
    assert_eq!(retcode, 0);
    assert!(a_txt.exists());
    assert!(a_txt.is_file());
    assert_eq!(read(&a_txt), "PREVIOUS_CONTENT");
    let answers = load_answers_from(&dst, "altered-answers.yml");
    assert_eq!(answers["_src_path"], Value::from(src_s));
}

#[test]
#[ignore = "pending"]
fn test_data_with_multiselect_choice_question() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    let (src_s, dst_s) = (src.to_str().unwrap(), dst.to_str().unwrap());

    file_tree! {
        src.join("copier.yml") => indoc! {"
            q:
                type: int
                choices: [1, 2, 3]
                multiselect: true
        "},
        src.join(ANSWERS_FILE_TMPL) => indoc! {"
            # Changes here will be overwritten by Copier
            {{ _copier_answers|to_nice_yaml }}
        "},
    };

    let (retcode, _) = run_cli(&["copy", "--data=q=[1, 3]", src_s, dst_s]);
    assert_eq!(retcode, 0);
    assert_eq!(
        load_answers(&dst),
        v(json!({"_src_path": src_s, "q": [1, 3]}))
    );
}
