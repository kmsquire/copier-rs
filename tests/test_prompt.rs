//! Port of `tests/test_prompt.py`.

mod common;

use std::path::{Path, PathBuf};

use common::{
    expect_prompt, file_tree, git, git_save, keyboard, load_answers, spawn, spawn_with, v, Tmp,
    BRACKET_ENVOPS_JSON, SUFFIX_TMPL,
};
use copier::Value;
use indoc::{formatdoc, indoc};
use rstest::rstest;
use serde_json::json;

/// Seconds to wait for output in sessions (`spawn_timeout` in Python).
const SPAWN_TIMEOUT: u64 = common::SPAWN_TIMEOUT;

/// Name of the answers file template in a bracket-syntax template.
const BRACKET_ANSWERS_FILE_TMPL: &str = "[[ _copier_conf.answers_file ]].tmpl";
/// Contents of the answers file template in a bracket-syntax template.
const BRACKET_ANSWERS_TMPL_CONTENT: &str = "[[ _copier_answers|to_nice_yaml ]]";

/// `BRACKET_ENVOPS` of the Python helpers.
fn bracket_envops() -> serde_json::Value {
    serde_json::from_str(BRACKET_ENVOPS_JSON).unwrap()
}

/// `MARIO_TREE` (and `MARIO_TREE_WITH_NEW_FIELD` when `with_new_field`) in `src`.
fn build_mario_tree(src: &Path, with_new_field: bool) {
    let new_field = if with_new_field {
        indoc! {"
            your_sister:
                type: str
                default: Luigi
                help: Your sister's name
        "}
    } else {
        ""
    };
    let copier_yml = formatdoc! {"
        _templates_suffix: {suffix}
        _envops: {envops}
        in_love:
            type: bool
            default: yes
        your_name:
            type: str
            default: Mario
            help: If you have a name, tell me now.
        your_enemy:
            type: str
            default: Bowser
            secret: yes
            help: Secret enemy name
        {new_field}what_enemy_does:
            type: str
            default: \"[[ your_enemy ]] hates [[ your_name ]]\"
        ",
        suffix = SUFFIX_TMPL,
        envops = BRACKET_ENVOPS_JSON,
    };
    file_tree! {
        src.join("copier.yml") => copier_yml,
        src.join(BRACKET_ANSWERS_FILE_TMPL) => "[[_copier_answers|to_nice_yaml]]",
    };
}

/// `PATH_TREE` in `src`.
fn build_path_tree(src: &Path) {
    file_tree! {
        src.join("copier.yml") => formatdoc! {"
            _templates_suffix: {suffix}
            _envops: {envops}
            current_location:
                type: path
                default: /dev/warppipe0

            star_location:
                type: path
                help: \"Location of a bonus star\"
            ",
            suffix = SUFFIX_TMPL,
            envops = BRACKET_ENVOPS_JSON,
        },
        src.join(BRACKET_ANSWERS_FILE_TMPL) => "[[_copier_answers|to_nice_yaml]]",
    };
}

/// Prepend the fixed arguments to the variable ones.
fn args_of<'a>(fixed: &[&'a str], extra: &'a [String]) -> Vec<&'a str> {
    fixed
        .iter()
        .copied()
        .chain(extra.iter().map(String::as_str))
        .collect()
}

#[rstest]
#[case("Mario", &[])] // Default name in the template
#[case("Luigi", &["--data=your_name=Luigi"])]
#[case("None", &["--data=your_name=None"])]
fn test_copy_default_advertised(#[case] name: &str, #[case] args: &[&str]) {
    // Test that the questions for the user are OK
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    build_mario_tree(&src, false);
    git_save(&src, "Test commit", Some("v1"));
    git(&src, &["commit", "--allow-empty", "-m", "v2"]);
    git(&src, &["tag", "v2"]);
    // Copy the v1 template
    let mut copy_args = vec!["copy", src.to_str().unwrap(), ".", "--vcs-ref=v1"];
    copy_args.extend_from_slice(args);
    let mut tui = spawn_with(&copy_args, SPAWN_TIMEOUT, Some(&dst));
    // Check what was captured
    expect_prompt(&mut tui, "in_love", "bool", None);
    tui.expect_exact("(Y/n)");
    tui.sendline("");
    tui.expect_exact("Yes");
    if args.is_empty() {
        expect_prompt(
            &mut tui,
            "your_name",
            "str",
            Some("If you have a name, tell me now."),
        );
        tui.expect_exact(name);
        tui.sendline("");
    }
    expect_prompt(&mut tui, "your_enemy", "str", Some("Secret enemy name"));
    tui.expect_exact("******");
    tui.sendline("");
    expect_prompt(&mut tui, "what_enemy_does", "str", None);
    tui.expect_exact(&format!("Bowser hates {name}"));
    tui.sendline("");
    tui.expect_eof();
    assert_eq!(load_answers(&dst).get("_commit"), Some(&Value::from("v1")));
    // Update subproject
    git_save(&dst, "Test commit", None);
    assert_eq!(load_answers(&dst).get("_commit"), Some(&Value::from("v1")));
    let mut tui = spawn_with(&["update"], SPAWN_TIMEOUT * 3, Some(&dst));
    // Check what was captured
    expect_prompt(&mut tui, "in_love", "bool", None);
    tui.expect_exact("(Y/n)");
    tui.sendline("");
    expect_prompt(
        &mut tui,
        "your_name",
        "str",
        Some("If you have a name, tell me now."),
    );
    tui.expect_exact(name);
    tui.sendline("");
    expect_prompt(&mut tui, "your_enemy", "str", Some("Secret enemy name"));
    tui.expect_exact("******");
    tui.sendline("");
    expect_prompt(&mut tui, "what_enemy_does", "str", None);
    tui.expect_exact(&format!("Bowser hates {name}"));
    tui.sendline("");
    tui.expect_eof();
    assert_eq!(load_answers(&dst).get("_commit"), Some(&Value::from("v2")));
}

#[cfg(unix)]
#[test]
#[ignore = "needs-design: requires a pty"]
fn test_path_completion() {
    // Test that file paths can handle tab completion.
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    let completedir = tmp.mktemp("my-directory");
    build_path_tree(&src);
    git_save(&src, "Test commit", Some("v1"));
    git(&src, &["commit", "--allow-empty", "-m", "v2"]);
    git(&src, &["tag", "v2"]);
    // TODO(port): a pty-backed session is needed (Python uses `pexpect.pty_spawn.spawn`
    // so that prompt_toolkit offers tab completion); the pipe-based `Tui` is used here.
    // Copy the v1 template
    let mut tui = spawn_with(
        &["copy", src.to_str().unwrap(), ".", "--vcs-ref=v1"],
        SPAWN_TIMEOUT,
        Some(&dst),
    );

    // Check that default values are maintained
    expect_prompt(&mut tui, "current_location", "path", None);
    tui.sendline("");

    // Check tab completion of a filesystem path (/path/to/my-direct<TAB> should
    // complete to /path/to/my-directory0)
    expect_prompt(
        &mut tui,
        "star_location",
        "path",
        Some("Location of a bonus star"),
    );
    let completedir_str = completedir.to_str().unwrap();
    tui.send(&completedir_str[..completedir_str.len() - 4]);
    tui.send(keyboard::TAB);
    tui.sendline("");
    tui.sendline("");

    tui.expect_eof();

    let answers = load_answers(&dst);
    assert_eq!(answers.get("_commit"), Some(&Value::from("v1")));
    assert_eq!(
        answers.get("current_location"),
        Some(&Value::from("/dev/warppipe0"))
    );
    assert_eq!(
        answers.get("star_location"),
        Some(&Value::from(completedir_str))
    );
}

#[rstest]
#[case("Mario", &[])] // Default name in the template
#[case("Luigi", &["--data=your_name=Luigi"])]
#[case("None", &["--data=your_name=None"])]
fn test_update_skip_answered(
    #[case] name: &str,
    #[case] args: &[&str],
    #[values("update", "recopy")] update_action: &str,
) {
    // Test that the questions for the user are OK
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    build_mario_tree(&src, false);
    git_save(&src, "Test commit", Some("v1"));
    git(&src, &["commit", "--allow-empty", "-m", "v2"]);
    git(&src, &["tag", "v2"]);
    // Copy the v1 template
    let mut copy_args = vec!["copy", src.to_str().unwrap(), ".", "--vcs-ref=v1"];
    copy_args.extend_from_slice(args);
    let mut tui = spawn_with(&copy_args, SPAWN_TIMEOUT, Some(&dst));
    // Check what was captured
    expect_prompt(&mut tui, "in_love", "bool", None);
    tui.expect_exact("(Y/n)");
    tui.sendline("");
    tui.expect_exact("Yes");
    if args.is_empty() {
        expect_prompt(
            &mut tui,
            "your_name",
            "str",
            Some("If you have a name, tell me now."),
        );
        tui.expect_exact(name);
        tui.sendline("");
    }
    expect_prompt(&mut tui, "your_enemy", "str", Some("Secret enemy name"));
    tui.expect_exact("******");
    tui.sendline("");
    expect_prompt(&mut tui, "what_enemy_does", "str", None);
    tui.expect_exact(&format!("Bowser hates {name}"));
    tui.sendline("");
    tui.expect_eof();
    assert_eq!(load_answers(&dst).get("_commit"), Some(&Value::from("v1")));
    // Update subproject
    git_save(&dst, "Test commit", None);
    let mut tui = spawn_with(
        &[update_action, "--skip-answered"],
        SPAWN_TIMEOUT * 3,
        Some(&dst),
    );
    // Check what was captured
    expect_prompt(&mut tui, "your_enemy", "str", Some("Secret enemy name"));
    tui.expect_exact("******");
    tui.sendline("");
    tui.expect_eof();
    assert_eq!(load_answers(&dst).get("_commit"), Some(&Value::from("v2")));
}

#[rstest]
#[case("Mario", &[])] // Default name in the template
#[case("Luigi", &["--data=your_name=Luigi"])]
#[case("None", &["--data=your_name=None"])]
fn test_update_with_new_field_in_new_version_skip_answered(
    #[case] name: &str,
    #[case] args: &[&str],
) {
    // Test that the questions for the user are OK
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    build_mario_tree(&src, false);
    git_save(&src, "Test commit", Some("v1"));
    build_mario_tree(&src, true);
    git_save(&src, "Test commit", Some("v2"));
    // Copy the v1 template
    let mut copy_args = vec!["copy", src.to_str().unwrap(), ".", "--vcs-ref=v1"];
    copy_args.extend_from_slice(args);
    let mut tui = spawn_with(&copy_args, SPAWN_TIMEOUT, Some(&dst));
    // Check what was captured
    expect_prompt(&mut tui, "in_love", "bool", None);
    tui.expect_exact("(Y/n)");
    tui.sendline("");
    tui.expect_exact("Yes");
    if args.is_empty() {
        expect_prompt(
            &mut tui,
            "your_name",
            "str",
            Some("If you have a name, tell me now."),
        );
        tui.expect_exact(name);
        tui.sendline("");
    }
    expect_prompt(&mut tui, "your_enemy", "str", Some("Secret enemy name"));
    tui.expect_exact("******");
    tui.sendline("");
    expect_prompt(&mut tui, "what_enemy_does", "str", None);
    tui.expect_exact(&format!("Bowser hates {name}"));
    tui.sendline("");
    tui.expect_eof();
    assert_eq!(load_answers(&dst).get("_commit"), Some(&Value::from("v1")));
    // Update subproject
    git_save(&dst, "Test commit", None);
    let mut tui = spawn_with(&["update", "-A"], SPAWN_TIMEOUT * 3, Some(&dst));
    // Check what was captured
    expect_prompt(&mut tui, "your_enemy", "str", Some("Secret enemy name"));
    tui.expect_exact("******");
    tui.sendline("");
    expect_prompt(&mut tui, "your_sister", "str", Some("Your sister's name"));
    tui.expect_exact("Luigi");
    tui.sendline("");
    tui.expect_eof();
    assert_eq!(load_answers(&dst).get("_commit"), Some(&Value::from("v2")));
}

/// Parse YAML text (`yaml.safe_load` in Python).
fn yaml_safe_load(text: &str) -> Value {
    copier::yaml::load(text).unwrap()
}

/// Python's `type(value).__name__` for a scalar JSON value.
fn py_type_name(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(number) if number.is_f64() => "float",
        serde_json::Value::Number(_) => "int",
        serde_json::Value::String(_) => "str",
        other => panic!("unexpected scalar {other}"),
    }
}

// 17 values of `question_1` times 17 `(question_2_when, asks)` cases = 289 cases.
#[rstest]
#[case(json!(true), true)]
#[case(json!(false), false)]
#[case(json!("trUe"), true)]
#[case(json!("faLse"), false)]
#[case(json!("Yes"), true)]
#[case(json!("nO"), false)]
#[case(json!("Y"), true)]
#[case(json!("N"), false)]
#[case(json!("on"), true)]
#[case(json!("off"), false)]
#[case(json!("~"), false)]
#[case(json!("NonE"), false)]
#[case(json!("nulL"), false)]
#[case(json!("[[ question_1 ]]"), true)]
#[case(json!("[[ not question_1 ]]"), false)]
#[case(json!("[% if question_1 %]YES[% endif %]"), true)]
#[case(json!("[% if question_1 %]FALSE[% endif %]"), false)]
fn test_when(
    // All these values evaluate to true
    #[values(
        json!(true),
        json!(1),
        json!(1.1),
        json!(-1),
        json!(-0.001),
        json!("1"),
        json!("1.1"),
        json!("1st"),
        json!("-1"),
        json!("0.1"),
        json!("00001"),
        json!("000.0001"),
        json!("-0001"),
        json!("-000.001"),
        json!("+001"),
        json!("+000.001"),
        json!("hello! 🤗")
    )]
    question_1: serde_json::Value,
    #[case] question_2_when: serde_json::Value,
    #[case] asks: bool,
) {
    // Test that the 2nd question is skipped or not, properly.
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    let questions = json!({
        "_envops": bracket_envops(),
        "_templates_suffix": SUFFIX_TMPL,
        "question_1": question_1,
        "question_2": {"default": "something", "type": "yaml", "when": question_2_when},
    });
    file_tree! {
        src.join("copier.yml") => questions.to_string(),
        src.join(BRACKET_ANSWERS_FILE_TMPL) => BRACKET_ANSWERS_TMPL_CONTENT,
        src.join("context.yml.tmpl") => indoc! {"
            question_2: [[ question_2 ]]
        "},
    };
    let mut tui = spawn(&["copy", src.to_str().unwrap(), dst.to_str().unwrap()]);
    expect_prompt(&mut tui, "question_1", py_type_name(&question_1), None);
    tui.sendline("");
    if asks {
        expect_prompt(&mut tui, "question_2", "yaml", None);
        tui.sendline("");
    }
    tui.expect_eof();
    let answers = load_answers(&dst);
    let mut expected = json!({
        "_src_path": src.to_str().unwrap(),
        "question_1": question_1,
    });
    if asks {
        expected["question_2"] = json!("something");
    }
    assert_eq!(answers, v(expected));
    let context = yaml_safe_load(&common::read(dst.join("context.yml")));
    assert_eq!(context, v(json!({"question_2": "something"})));
}

#[test]
fn test_placeholder() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => json!({
            "_envops": bracket_envops(),
            "_templates_suffix": SUFFIX_TMPL,
            "question_1": "answer 1",
            "question_2": {
                "type": "str",
                "help": "write a list of answers",
                "placeholder": "Write something like [[ question_1 ]], but better",
            },
        })
        .to_string(),
        src.join(BRACKET_ANSWERS_FILE_TMPL) => BRACKET_ANSWERS_TMPL_CONTENT,
    };
    let mut tui = spawn(&["copy", src.to_str().unwrap(), dst.to_str().unwrap()]);
    expect_prompt(&mut tui, "question_1", "str", None);
    tui.expect_exact("answer 1");
    tui.sendline("");
    expect_prompt(
        &mut tui,
        "question_2",
        "str",
        Some("write a list of answers"),
    );
    tui.expect_exact("Write something like answer 1, but better");
    tui.sendline("");
    tui.expect_eof();
    let answers = load_answers(&dst);
    assert_eq!(
        answers,
        v(json!({
            "_src_path": src.to_str().unwrap(),
            "question_1": "answer 1",
            "question_2": "",
        }))
    );
}

#[test]
fn test_qmark() {
    // Test that custom qmark parameter is displayed for questions.
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => json!({
            "_envops": bracket_envops(),
            "_templates_suffix": SUFFIX_TMPL,
            "favorite_color": {
                "type": "str",
                "default": "blue",
                "qmark": "❤️",
            },
            "api_key": {
                "type": "str",
                "default": "secret123",
                "secret": true,
                "qmark": "🔐",
            },
        })
        .to_string(),
        src.join(BRACKET_ANSWERS_FILE_TMPL) => BRACKET_ANSWERS_TMPL_CONTENT,
    };
    let mut tui = spawn(&["copy", src.to_str().unwrap(), dst.to_str().unwrap()]);
    tui.expect_exact("❤️");
    tui.expect_exact("favorite_color");
    tui.expect_exact("blue");
    tui.sendline("");
    tui.expect_exact("🔐");
    tui.expect_exact("api_key");
    tui.sendline("");
    tui.expect_eof();
    let answers = load_answers(&dst);
    assert_eq!(
        answers,
        v(json!({
            "_src_path": src.to_str().unwrap(),
            "favorite_color": "blue",
        }))
    );
}

#[test]
fn test_qmark_default() {
    // Test that default qmark emojis are used when qmark is not specified.
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => json!({
            "_envops": bracket_envops(),
            "_templates_suffix": SUFFIX_TMPL,
            "name": {
                "type": "str",
                "default": "John",
            },
            "password": {
                "type": "str",
                "default": "secret",
                "secret": true,
            },
        })
        .to_string(),
        src.join(BRACKET_ANSWERS_FILE_TMPL) => BRACKET_ANSWERS_TMPL_CONTENT,
    };
    let mut tui = spawn(&["copy", src.to_str().unwrap(), dst.to_str().unwrap()]);
    tui.expect_exact("🎤");
    tui.expect_exact("name");
    tui.expect_exact("John");
    tui.sendline("");
    tui.expect_exact("🕵️");
    tui.expect_exact("password");
    tui.sendline("");
    tui.expect_eof();
    let answers = load_answers(&dst);
    assert_eq!(
        answers,
        v(json!({
            "_src_path": src.to_str().unwrap(),
            "name": "John",
        }))
    );
}

#[cfg(unix)]
#[rstest]
#[case(json!({"type": "str", "default": "foo"}))]
#[case(json!({"type": "str", "default": "foo", "secret": true}))]
#[case(json!({"type": "bool", "default": true}))]
#[case(json!({"type": "str", "default": "a", "choices": ["a", "b"]}))]
#[case(json!({"type": "str", "default": ["a"], "choices": ["a", "b"], "multiselect": true}))]
#[ignore = "needs-design: requires a pty"]
fn test_prompt_does_not_capture_mouse(#[case] question: serde_json::Value) {
    // Test that prompts leave mouse events (e.g. text selection) to the terminal.
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => json!({"question": question}).to_string(),
    };
    // A real pty is needed; without one, no terminal control sequences are emitted
    // TODO(port): a pty-backed session that records all output is needed; the
    // pipe-based `Tui` only returns the output not consumed by `expect_exact`.
    let mut tui = spawn(&["copy", src.to_str().unwrap(), dst.to_str().unwrap()]);
    tui.expect_exact("question");
    tui.sendline("");
    let output = tui.expect_eof();
    // Escape sequence that enables mouse tracking in the terminal
    let mouse_tracking_enabled = output.contains("\x1b[?1000h");
    assert!(!mouse_tracking_enabled, "{output:?}");
}

#[rstest]
fn test_multiline(#[values("str", "yaml", "json")] type_: &str) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => json!({
            "_envops": bracket_envops(),
            "_templates_suffix": SUFFIX_TMPL,
            "question_1": "answer 1",
            "question_2": {"type": type_},
            "question_3": {"type": type_, "multiline": true},
            "question_4": {
                "type": type_,
                "multiline": "[[ question_1 == 'answer 1' ]]",
            },
            "question_5": {
                "type": type_,
                "multiline": "[[ question_1 != 'answer 1' ]]",
            },
        })
        .to_string(),
        src.join(BRACKET_ANSWERS_FILE_TMPL) => BRACKET_ANSWERS_TMPL_CONTENT,
    };
    let mut tui = spawn(&["copy", src.to_str().unwrap(), dst.to_str().unwrap()]);
    expect_prompt(&mut tui, "question_1", "str", None);
    tui.expect_exact("answer 1");
    tui.sendline("");
    expect_prompt(&mut tui, "question_2", type_, None);
    tui.sendline("\"answer 2\"");
    expect_prompt(&mut tui, "question_3", type_, None);
    tui.sendline("\"answer 3\"");
    tui.send(&format!("{}{}", keyboard::ALT, keyboard::ENTER));
    expect_prompt(&mut tui, "question_4", type_, None);
    tui.sendline("\"answer 4\"");
    tui.send(&format!("{}{}", keyboard::ALT, keyboard::ENTER));
    expect_prompt(&mut tui, "question_5", type_, None);
    tui.sendline("\"answer 5\"");
    tui.expect_eof();
    let answers = load_answers(&dst);
    if type_ == "str" {
        assert_eq!(
            answers,
            v(json!({
                "_src_path": src.to_str().unwrap(),
                "question_1": "answer 1",
                "question_2": "\"answer 2\"",
                "question_3": "\"answer 3\"\n",
                "question_4": "\"answer 4\"\n",
                "question_5": "\"answer 5\"",
            }))
        );
    } else {
        assert_eq!(
            answers,
            v(json!({
                "_src_path": src.to_str().unwrap(),
                "question_1": "answer 1",
                "question_2": "answer 2",
                "question_3": "answer 3",
                "question_4": "answer 4",
                "question_5": "answer 5",
            }))
        );
    }
}

#[rstest]
#[case(json!([1, 2, 3]))]
#[case(json!([["one", 1], ["two", 2], ["three", 3]]))]
#[case(json!({"one": 1, "two": 2, "three": 3}))]
fn test_update_choice(#[case] choices: serde_json::Value) {
    // Choices are properly remembered and selected in TUI when updating.
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    // Create template
    file_tree! {
        src.join("copier.yml") => json!({
            "_templates_suffix": SUFFIX_TMPL,
            "_envops": bracket_envops(),
            "pick_one": {"type": "float", "default": 3, "choices": choices},
        })
        .to_string(),
        src.join(BRACKET_ANSWERS_FILE_TMPL) => BRACKET_ANSWERS_TMPL_CONTENT,
    };
    git(&src, &["init"]);
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m one"]);
    git(&src, &["tag", "v1"]);
    // Copy
    let mut tui = spawn(&["copy", src.to_str().unwrap(), dst.to_str().unwrap()]);
    expect_prompt(&mut tui, "pick_one", "float", None);
    tui.sendline(keyboard::UP);
    tui.expect_eof();
    let answers = load_answers(&dst);
    assert_eq!(answers["pick_one"], Value::Float(2.0));
    git(&dst, &["init"]);
    git(&dst, &["add", "."]);
    git(&dst, &["commit", "-m1"]);
    // Update
    let mut tui = spawn(&["update", dst.to_str().unwrap()]);
    expect_prompt(&mut tui, "pick_one", "float", None);
    tui.sendline(keyboard::DOWN);
    tui.expect_eof();
    let answers = load_answers(&dst);
    assert_eq!(answers["pick_one"], Value::Float(3.0));
}

// pytest.mark.skip("TODO: fix this")
#[test]
#[ignore = "skipped upstream: TODO: fix this"]
fn test_multiline_defaults() {
    // Multiline questions with scalar defaults produce multiline defaults.
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yaml") => indoc! {r#"
            yaml_single:
                type: yaml
                default: &default
                    - one
                    - two
                    - three:
                        - four
            yaml_multi:
                type: yaml
                multiline: "{{ 'me' == ('m' + 'e') }}"
                default: *default
            json_single:
                type: json
                default: *default
            json_multi:
                type: json
                multiline: yes
                default: *default
        "#},
        src.join(common::ANSWERS_FILE_TMPL) => "{{ _copier_answers|to_nice_yaml }}",
    };
    let mut tui = spawn(&["copy", src.to_str().unwrap(), dst.to_str().unwrap()]);
    expect_prompt(&mut tui, "yaml_single", "yaml", None);
    // This test will always fail here, because python prompt toolkit gives
    // syntax highlighting to YAML and JSON outputs, encoded into terminal
    // colors and all that stuff.
    // TODO Fix this test some day.
    tui.expect_exact("[one, two, {three: [four]}]");
    tui.send(keyboard::ENTER);
    expect_prompt(&mut tui, "yaml_multi", "yaml", None);
    tui.expect_exact("> - one\n  - two\n  - three:\n    - four");
    tui.send(&format!("{}{}", keyboard::ALT, keyboard::ENTER));
    expect_prompt(&mut tui, "json_single", "json", None);
    tui.expect_exact("[\"one\", \"two\", {\"three\": [\"four\"]}]");
    tui.send(keyboard::ENTER);
    expect_prompt(&mut tui, "json_multi", "json", None);
    tui.expect_exact(
        "> [\n    \"one\",\n    \"two\",\n    {\n      \"three\": [\n        \"four\"\n      ]\n    }\n  ]",
    );
    tui.send(&format!("{}{}", keyboard::ALT, keyboard::ENTER));
    tui.expect_eof();
    let answers = load_answers(&dst);
    let expected = v(json!(["one", "two", {"three": ["four"]}]));
    assert_eq!(answers["yaml_single"], answers["yaml_multi"]);
    assert_eq!(answers["yaml_multi"], answers["json_single"]);
    assert_eq!(answers["json_single"], answers["json_multi"]);
    assert_eq!(answers["json_multi"], expected);
}

#[test]
fn test_partial_interrupt() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => json!({
            "question_1": "answer 1",
            "question_2": "answer 2",
        })
        .to_string(),
    };
    let mut tui = spawn(&["copy", src.to_str().unwrap(), dst.to_str().unwrap()]);
    expect_prompt(&mut tui, "question_1", "str", None);
    tui.expect_exact("answer 1");
    // Answer the first question using the default.
    tui.sendline("");
    // Send a ctrl-c.
    tui.send(&format!("{}{}", keyboard::CONTROL_C, keyboard::ENTER));
    // This string (or similar) should show in the output if we've gracefully caught the
    // interrupt.
    // We won't want our custom exception (CopierAnswersInterrupt) propagating
    // to an uncaught stacktrace.
    tui.expect_exact("Execution stopped by user\n");
    tui.expect_eof();
}

#[test]
fn test_var_name_value_allowed() {
    // Test that a question var name "value" causes no name collision.
    //
    // See https://github.com/copier-org/copier/pull/780 for more details.
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    // Create template
    file_tree! {
        src.join("copier.yaml") => indoc! {"
            value:
                type: str
                default: string
        "},
        src.join(common::ANSWERS_FILE_TMPL) => "{{ _copier_answers|to_nice_yaml }}",
    };
    // Copy
    let mut tui = spawn(&["copy", src.to_str().unwrap(), dst.to_str().unwrap()]);
    expect_prompt(&mut tui, "value", "str", None);
    tui.expect_exact("string");
    tui.send(&format!("{}{}", keyboard::ALT, keyboard::ENTER));
    tui.expect_eof();
    // Check answers file
    let answers = load_answers(&dst);
    assert_eq!(answers["value"], Value::from("string"));
}

#[rstest]
#[case("str", Some(json!("")))]
#[case("int", None)] // ValueError("Invalid input")
#[case("float", None)] // ValueError("Invalid input")
#[case("json", None)] // ValueError("Invalid input")
#[case("yaml", Some(json!(null)))]
fn test_required_text_question(
    #[case] type_name: &str,
    #[case] expected_answer: Option<serde_json::Value>,
) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => json!({
            "_envops": bracket_envops(),
            "_templates_suffix": SUFFIX_TMPL,
            "question": {"type": type_name},
        })
        .to_string(),
        src.join(BRACKET_ANSWERS_FILE_TMPL) => BRACKET_ANSWERS_TMPL_CONTENT,
    };
    let mut tui = spawn(&["copy", src.to_str().unwrap(), dst.to_str().unwrap()]);
    expect_prompt(&mut tui, "question", type_name, None);
    tui.expect_exact("");
    tui.sendline("");
    match expected_answer {
        None => {
            tui.expect_exact("Invalid input");
            assert!(!dst.join(".copier-answers.yml").exists());
        }
        Some(expected_answer) => {
            tui.expect_eof();
            let answers = load_answers(&dst);
            assert_eq!(
                answers,
                v(json!({
                    "_src_path": src.to_str().unwrap(),
                    "question": expected_answer,
                }))
            );
        }
    }
}

#[test]
fn test_required_bool_question() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => json!({
            "_envops": bracket_envops(),
            "_templates_suffix": SUFFIX_TMPL,
            "question": {"type": "bool"},
        })
        .to_string(),
        src.join(BRACKET_ANSWERS_FILE_TMPL) => BRACKET_ANSWERS_TMPL_CONTENT,
    };
    let mut tui = spawn(&["copy", src.to_str().unwrap(), dst.to_str().unwrap()]);
    expect_prompt(&mut tui, "question", "bool", None);
    tui.expect_exact("(y/N)");
    tui.sendline("");
    tui.expect_eof();
    let answers = load_answers(&dst);
    assert_eq!(
        answers,
        v(json!({
            "_src_path": src.to_str().unwrap(),
            "question": false,
        }))
    );
}

#[rstest]
#[case("str", json!(["one", "two", "three"]), json!("one"))]
#[case("int", json!([1, 2, 3]), json!(1))]
#[case("float", json!([1.0, 2.0, 3.0]), json!(1.0))]
#[case("json", json!(["[1]", "[2]", "[3]"]), json!([1]))]
#[case("yaml", json!(["- 1", "- 2", "- 3"]), json!([1]))]
fn test_required_choice_question(
    #[case] type_name: &str,
    #[case] choices: serde_json::Value,
    #[case] expected_answer: serde_json::Value,
) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => json!({
            "_envops": bracket_envops(),
            "_templates_suffix": SUFFIX_TMPL,
            "question": {
                "type": type_name,
                "choices": choices,
            },
        })
        .to_string(),
        src.join(BRACKET_ANSWERS_FILE_TMPL) => BRACKET_ANSWERS_TMPL_CONTENT,
    };
    let mut tui = spawn(&["copy", src.to_str().unwrap(), dst.to_str().unwrap()]);
    expect_prompt(&mut tui, "question", type_name, None);
    tui.sendline("");
    tui.expect_eof();
    let answers = load_answers(&dst);
    assert_eq!(
        answers,
        v(json!({
            "_src_path": src.to_str().unwrap(),
            "question": expected_answer,
        }))
    );
}

/// Test template with a single `question` (`question_tree` fixture in Python);
/// returns `(src, dst)`.
fn question_tree(tmp: &Tmp, question: serde_json::Value) -> (PathBuf, PathBuf) {
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => json!({
            "_envops": bracket_envops(),
            "_templates_suffix": SUFFIX_TMPL,
            "question": question,
        })
        .to_string(),
        src.join(BRACKET_ANSWERS_FILE_TMPL) => BRACKET_ANSWERS_TMPL_CONTENT,
    };
    (src, dst)
}

/// A list of JSON values as a [`Value`].
fn arr(items: &[serde_json::Value]) -> Value {
    v(serde_json::Value::Array(items.to_vec()))
}

/// Expands to a test parametrized over the 7 `CHOICES` of the Python module
/// (`type_name, choices, values`).
macro_rules! choices_test {
    (fn $name:ident($type_name:ident, $choices:ident, $values:ident) $body:block) => {
        #[rstest]
        #[case::str("str", json!(["one", "two", "three"]), json!(["one", "two", "three"]))]
        #[case::int("int", json!([1, 2, 3]), json!([1, 2, 3]))]
        #[case::int_label_list(
            "int",
            json!([["one", 1], ["two", 2], ["three", 3]]),
            json!([1, 2, 3])
        )]
        #[case::int_label_dict(
            "int",
            json!({"1. one": 1, "2. two": 2, "3. three": 3}),
            json!([1, 2, 3])
        )]
        #[case::float("float", json!([1.0, 2.0, 3.0]), json!([1.0, 2.0, 3.0]))]
        #[case::json("json", json!(["[1]", "[2]", "[3]"]), json!([[1], [2], [3]]))]
        #[case::yaml("yaml", json!(["- 1", "- 2", "- 3"]), json!([[1], [2], [3]]))]
        fn $name(
            #[case] $type_name: &str,
            #[case] $choices: serde_json::Value,
            #[case] $values: serde_json::Value,
        ) $body
    };
}

choices_test! {
    fn test_multiselect_choices_question_single_answer(type_name, choices, values) {
        let tmp = Tmp::new();
        let (src, dst) = question_tree(
            &tmp,
            json!({"type": type_name, "choices": choices, "multiselect": true}),
        );
        let mut tui = spawn(&["copy", src.to_str().unwrap(), dst.to_str().unwrap()]);
        expect_prompt(&mut tui, "question", type_name, None);
        tui.send(" "); // select 1
        tui.sendline("");
        tui.expect_eof();
        let answers = load_answers(&dst);
        assert_eq!(answers["question"], arr(&values.as_array().unwrap()[..1]));
    }
}

choices_test! {
    fn test_multiselect_choices_question_multiple_answers(type_name, choices, values) {
        let tmp = Tmp::new();
        let (src, dst) = question_tree(
            &tmp,
            json!({"type": type_name, "choices": choices, "multiselect": true}),
        );
        let mut tui = spawn(&["copy", src.to_str().unwrap(), dst.to_str().unwrap()]);
        expect_prompt(&mut tui, "question", type_name, None);
        tui.send(" "); // select 0
        tui.send(keyboard::DOWN);
        tui.send(" "); // select 1
        tui.sendline("");
        tui.expect_eof();
        let answers = load_answers(&dst);
        assert_eq!(answers["question"], arr(&values.as_array().unwrap()[..2]));
    }
}

choices_test! {
    fn test_multiselect_choices_question_with_default(type_name, choices, values) {
        let tmp = Tmp::new();
        let (src, dst) = question_tree(
            &tmp,
            json!({
                "type": type_name,
                "choices": choices,
                "multiselect": true,
                "default": values,
            }),
        );
        let mut tui = spawn(&["copy", src.to_str().unwrap(), dst.to_str().unwrap()]);
        expect_prompt(&mut tui, "question", type_name, None);
        tui.send(" "); // toggle first
        tui.sendline("");
        tui.expect_eof();
        let answers = load_answers(&dst);
        assert_eq!(answers["question"], arr(&values.as_array().unwrap()[1..]));
    }
}

choices_test! {
    fn test_update_multiselect_choices(type_name, choices, values) {
        // Multiple choices are properly remembered and selected in TUI when updating.
        let tmp = Tmp::new();
        let (src, dst) = question_tree(
            &tmp,
            json!({
                "type": type_name,
                "choices": choices,
                "multiselect": true,
                "default": values,
            }),
        );

        git(&src, &["init"]);
        git(&src, &["add", "."]);
        git(&src, &["commit", "-m one"]);
        git(&src, &["tag", "v1"]);

        // Copy
        let mut tui = spawn(&["copy", src.to_str().unwrap(), dst.to_str().unwrap()]);
        expect_prompt(&mut tui, "question", type_name, None);
        tui.send(" "); // toggle first
        tui.sendline("");
        tui.expect_eof();
        let answers = load_answers(&dst);
        assert_eq!(answers["question"], arr(&values.as_array().unwrap()[1..]));

        git(&dst, &["init"]);
        git(&dst, &["add", "."]);
        git(&dst, &["commit", "-m1"]);

        // Update
        let mut tui = spawn(&["update", dst.to_str().unwrap()]);
        expect_prompt(&mut tui, "question", type_name, None);
        tui.send(" "); // toggle first
        tui.sendline("");
        tui.expect_eof();
        let answers = load_answers(&dst);
        assert_eq!(answers["question"], v(values));
    }
}

#[test]
fn test_multiselect_choices_validator() {
    // Multiple choices are validated.
    let tmp = Tmp::new();
    let (src, dst) = question_tree(
        &tmp,
        json!({
            "type": "str",
            "choices": ["one", "two", "three"],
            "multiselect": true,
            "validator": "[%- if not question -%]At least one choice required[%- endif -%]",
        }),
    );

    let mut tui = spawn(&["copy", src.to_str().unwrap(), dst.to_str().unwrap()]);
    expect_prompt(&mut tui, "question", "str", None);
    tui.sendline("");
    tui.expect_exact("At least one choice required");
    tui.send(" "); // select "one"
    tui.sendline("");
    tui.expect_eof();
    let answers = load_answers(&dst);
    assert_eq!(answers["question"], v(json!(["one"])));
}

#[rstest]
fn test_secret_validator(#[values("s3cret", "")] default: &str) {
    // Secret question answer is validated.
    let tmp = Tmp::new();
    let (src, dst) = question_tree(
        &tmp,
        json!({
            "type": "str",
            "default": default,
            "secret": true,
            "validator": "[% if question|length < 3 %]too short[% endif %]",
        }),
    );

    let mut tui = spawn(&["copy", src.to_str().unwrap(), dst.to_str().unwrap()]);
    expect_prompt(&mut tui, "question", "str", None);
    // Delete default value to fail validation
    for _ in 0..default.chars().count() {
        tui.send(keyboard::BACKSPACE);
    }
    tui.sendline("");
    tui.expect_exact("too short");
    // Enter the valid answer again to pass validation
    tui.sendline("s3cret");
    tui.expect_exact("******");
    tui.expect_eof();
}

// Related to https://github.com/prompt-toolkit/python-prompt-toolkit/issues/1243#issuecomment-706668723)
// pytest.mark.xfail(platform.system() == "Windows",
//     reason="prompt-toolkit in subprocess call fails on Windows")
#[test]
fn test_interactive_session_required_for_question_prompt() {
    // Answering a question prompt requires an interactive session.
    let tmp = Tmp::new();
    let (src, dst) = question_tree(&tmp, json!({"type": "str"}));
    let output = common::copier_cmd()
        .args(["copy", src.to_str().unwrap(), dst.to_str().unwrap()])
        .stdin(std::process::Stdio::piped()) // Prevents interactive input
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let needle = b"Interactive session required: Use `--defaults` and/or `--data`/`--data-file`";
    assert!(
        output.stderr.windows(needle.len()).any(|w| w == needle),
        "stderr: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// Related to https://github.com/prompt-toolkit/python-prompt-toolkit/issues/1243#issuecomment-706668723)
// pytest.mark.xfail(platform.system() == "Windows",
//     reason="prompt-toolkit in subprocess call fails on Windows")
#[test]
fn test_interactive_session_required_for_overwrite_prompt() {
    // Overwriting a file without `--overwrite` flag requires an interactive session.
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("foo.txt.jinja") => "bar",
        dst.join("foo.txt") => "baz",
    };
    let output = common::copier_cmd()
        .args(["copy", src.to_str().unwrap(), dst.to_str().unwrap()])
        .stdin(std::process::Stdio::piped()) // Prevents interactive input
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let needle = b"Interactive session required: Consider using `--overwrite`";
    assert!(
        output.stderr.windows(needle.len()).any(|w| w == needle),
        "stderr: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// `ANIMAL_TREE` (and `ANIMAL_TREE_WITH_NEW_FIELD` when `with_new_field`) in `src`.
fn build_animal_tree(src: &Path, with_new_field: bool) {
    let new_field = if with_new_field {
        indoc! {"
            can_it_fly:
                type: bool
                default: false
        "}
    } else {
        ""
    };
    let copier_yml = formatdoc! {"
        animal_name:
            type: str
            default: mouse
        animal_sound:
            type: str
            default: squeak
        what_does_it_eat:
            type: str
            default: cheese
        {new_field}",
    };
    file_tree! {
        src.join("copier.yml") => copier_yml,
        src.join(common::ANSWERS_FILE_TMPL) => "{{_copier_answers|to_nice_yaml}}",
    };
}

/// Template with `ANIMAL_TREE` at `v1` and the new field at `v2`, plus a project
/// copied from `v1` with `--data` answers; returns `(src, dst)`.
fn animal_project(tmp: &Tmp) -> (PathBuf, PathBuf) {
    let (src, dst) = tmp.src_dst();
    build_animal_tree(&src, false);
    git_save(&src, "Test commit", Some("v1"));
    build_animal_tree(&src, true);
    git_save(&src, "Test commit", Some("v2"));
    // Copy the v1 template
    let mut tui = spawn_with(
        &[
            "copy",
            src.to_str().unwrap(),
            ".",
            "--vcs-ref=v1",
            "--data=animal_name=dog",
            "--data=animal_sound=woof",
            "--data=what_does_it_eat=meat",
        ],
        SPAWN_TIMEOUT,
        Some(&dst),
    );
    tui.expect_eof();
    assert_eq!(load_answers(&dst).get("_commit"), Some(&Value::from("v1")));
    (src, dst)
}

#[rstest]
fn test_update_skip_answered_with_ask(
    #[values(vec!["*it*"], vec!["what_does_it_eat"], vec!["what_does_it_eat", "can_it_fly"])]
    ask: Vec<&str>,
    #[values("update", "recopy")] update_action: &str,
) {
    // Test that the questions for the user are OK
    let tmp = Tmp::new();
    let (_src, dst) = animal_project(&tmp);
    // Update subproject
    git_save(&dst, "Test commit", None);
    let ask_flags: Vec<String> = ask.iter().map(|a| format!("--ask={a}")).collect();
    let mut tui = spawn_with(
        &args_of(&[update_action, "--skip-answered"], &ask_flags),
        SPAWN_TIMEOUT * 3,
        Some(&dst),
    );
    // Check what was captured
    expect_prompt(&mut tui, "what_does_it_eat", "str", None);
    tui.expect_exact("meat");
    tui.sendline("");
    expect_prompt(&mut tui, "can_it_fly", "bool", None);
    tui.expect_exact("(y/N)");
    tui.sendline("");
    tui.expect_eof();
    assert_eq!(load_answers(&dst).get("_commit"), Some(&Value::from("v2")));
}

#[rstest]
fn test_update_skip_answered_with_ask_and_data(
    #[values(vec!["*it*"], vec!["what_does_it_eat"], vec!["what_does_it_eat", "can_it_fly"])]
    ask: Vec<&str>,
    #[values("update", "recopy")] update_action: &str,
) {
    // Test that the questions for the user are OK
    let tmp = Tmp::new();
    let (_src, dst) = animal_project(&tmp);
    // Update subproject
    git_save(&dst, "Test commit", None);
    let mut ask_flags: Vec<String> = ask.iter().map(|a| format!("--ask={a}")).collect();
    ask_flags.push("--data=what_does_it_eat=chow".to_owned());
    let mut tui = spawn_with(
        &args_of(&[update_action, "--skip-answered"], &ask_flags),
        SPAWN_TIMEOUT * 3,
        Some(&dst),
    );
    // Check what was captured
    expect_prompt(&mut tui, "what_does_it_eat", "str", None);
    tui.expect_exact("chow");
    tui.sendline("");
    expect_prompt(&mut tui, "can_it_fly", "bool", None);
    tui.expect_exact("(y/N)");
    tui.sendline("");
    tui.expect_eof();
    let loaded_answers = load_answers(&dst);
    assert_eq!(loaded_answers.get("_commit"), Some(&Value::from("v2")));
    assert_eq!(
        loaded_answers.get("what_does_it_eat"),
        Some(&Value::from("chow"))
    );
}

#[rstest]
fn test_update_defaults_with_ask(
    #[values(vec!["*it*"], vec!["what_does_it_eat", "can_it_fly"])] ask: Vec<&str>,
    #[values("update", "recopy")] update_action: &str,
) {
    // Test that the questions for the user are OK
    let tmp = Tmp::new();
    let (_src, dst) = animal_project(&tmp);
    // Update subproject
    git_save(&dst, "Test commit", None);
    let ask_flags: Vec<String> = ask.iter().map(|a| format!("--ask={a}")).collect();
    let mut tui = spawn_with(
        &args_of(&[update_action, "--defaults"], &ask_flags),
        SPAWN_TIMEOUT * 3,
        Some(&dst),
    );
    // Check what was captured
    expect_prompt(&mut tui, "what_does_it_eat", "str", None);
    tui.expect_exact("meat");
    tui.sendline("");
    expect_prompt(&mut tui, "can_it_fly", "bool", None);
    tui.expect_exact("(y/N)");
    tui.sendline("");
    tui.expect_eof();
    assert_eq!(load_answers(&dst).get("_commit"), Some(&Value::from("v2")));
}

#[rstest]
fn test_update_defaults_with_ask_and_data(
    #[values(vec!["*it*"], vec!["what_does_it_eat", "can_it_fly"])] ask: Vec<&str>,
    #[values("update", "recopy")] update_action: &str,
) {
    // Test that the questions for the user are OK
    let tmp = Tmp::new();
    let (_src, dst) = animal_project(&tmp);
    // Update subproject
    git_save(&dst, "Test commit", None);
    let mut ask_flags: Vec<String> = ask.iter().map(|a| format!("--ask={a}")).collect();
    ask_flags.push("--data=what_does_it_eat=chow".to_owned());
    let mut tui = spawn_with(
        &args_of(&[update_action, "--defaults"], &ask_flags),
        SPAWN_TIMEOUT * 3,
        Some(&dst),
    );
    // Check what was captured
    expect_prompt(&mut tui, "what_does_it_eat", "str", None);
    tui.expect_exact("chow");
    tui.sendline("");
    expect_prompt(&mut tui, "can_it_fly", "bool", None);
    tui.expect_exact("(y/N)");
    tui.sendline("");
    tui.expect_eof();
    let loaded_answers = load_answers(&dst);
    assert_eq!(loaded_answers.get("_commit"), Some(&Value::from("v2")));
    assert_eq!(
        loaded_answers.get("what_does_it_eat"),
        Some(&Value::from("chow"))
    );
}

#[rstest]
fn test_copy_defaults_with_ask(
    #[values(vec!["*it*"], vec!["what_does_it_eat", "can_it_fly"])] ask: Vec<&str>,
) {
    // Test that the questions for the user are OK
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    build_animal_tree(&src, true);
    git_save(&src, "Test commit", Some("v1"));
    // Copy the v1 template
    let ask_flags: Vec<String> = ask.iter().map(|a| format!("--ask={a}")).collect();
    let mut tui = spawn_with(
        &args_of(
            &["copy", src.to_str().unwrap(), ".", "--defaults"],
            &ask_flags,
        ),
        SPAWN_TIMEOUT,
        Some(&dst),
    );
    expect_prompt(&mut tui, "what_does_it_eat", "str", None);
    tui.expect_exact("cheese");
    tui.sendline(" and milk");
    expect_prompt(&mut tui, "can_it_fly", "str", None);
    tui.expect_exact("(y/N)");
    tui.sendline("");
    tui.expect_eof();
    let loaded_answers = load_answers(&dst);
    assert_eq!(
        loaded_answers.get("what_does_it_eat"),
        Some(&Value::from("cheese and milk"))
    );
}

#[rstest]
fn test_copy_defaults_with_ask_and_data(
    #[values(vec!["*it*"], vec!["what_does_it_eat", "can_it_fly"])] ask: Vec<&str>,
) {
    // Test that the questions for the user are OK
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    build_animal_tree(&src, true);
    git_save(&src, "Test commit", Some("v1"));
    // Copy the v1 template
    let mut ask_flags: Vec<String> = ask.iter().map(|a| format!("--ask={a}")).collect();
    ask_flags.push("--data=what_does_it_eat=milk".to_owned());
    let mut tui = spawn_with(
        &args_of(
            &["copy", src.to_str().unwrap(), ".", "--defaults"],
            &ask_flags,
        ),
        SPAWN_TIMEOUT,
        Some(&dst),
    );
    expect_prompt(&mut tui, "what_does_it_eat", "str", None);
    tui.expect_exact("milk");
    tui.sendline(" and cookies");
    expect_prompt(&mut tui, "can_it_fly", "str", None);
    tui.expect_exact("(y/N)");
    tui.sendline("");
    tui.expect_eof();
    let loaded_answers = load_answers(&dst);
    assert_eq!(
        loaded_answers.get("what_does_it_eat"),
        Some(&Value::from("milk and cookies"))
    );
}

#[rstest]
#[case(true, true, vec![], true)]
#[case(true, true, vec!["--ask=foo"], true)]
#[case(true, true, vec!["--ask=f*"], true)]
#[case(true, true, vec!["--ask=baz"], true)]
#[case(true, false, vec![], false)]
#[case(true, false, vec!["--ask=foo"], true)]
#[case(true, false, vec!["--ask=f*"], true)]
#[case(true, false, vec!["--ask=baz"], false)]
#[case(false, true, vec![], false)]
#[case(false, true, vec!["--ask=foo"], false)]
#[case(false, true, vec!["--ask=f*"], false)]
#[case(false, true, vec!["--ask=baz"], false)]
#[case(false, false, vec![], false)]
#[case(false, false, vec!["--ask=foo"], false)]
#[case(false, false, vec!["--ask=f*"], false)]
#[case(false, false, vec!["--ask=baz"], false)]
fn test_copy_with_ask_setting(
    #[case] when: bool,
    #[case] ask: bool,
    #[case] ask_flag: Vec<&str>,
    #[case] prompted: bool,
) {
    // Test copy prompting with `when`, `ask`, and `--ask` flag.
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => json!({
            "foo": {"type": "str", "default": "bar", "when": when, "ask": ask}
        })
        .to_string(),
        src.join(common::ANSWERS_FILE_TMPL) => "{{ _copier_answers|to_nice_yaml }}",
    };
    let mut copy_args = vec!["copy", src.to_str().unwrap(), dst.to_str().unwrap()];
    copy_args.extend_from_slice(&ask_flag);
    let mut tui = spawn(&copy_args);
    if prompted {
        expect_prompt(&mut tui, "foo", "str", None);
        tui.sendline("x");
    }
    tui.expect_eof();
    let answers = load_answers(&dst);
    if when {
        assert_eq!(
            answers.get("foo"),
            Some(&Value::from(if prompted { "barx" } else { "bar" }))
        );
    } else {
        assert!(!answers.contains_key("foo"));
    }
}

#[rstest]
#[case(true, true, false, vec![], true)]
#[case(true, true, false, vec!["--ask=foo"], true)]
#[case(true, true, false, vec!["--ask=baz"], true)]
#[case(true, true, true, vec![], false)]
#[case(true, true, true, vec!["--ask=foo"], true)]
#[case(true, true, true, vec!["--ask=baz"], false)]
#[case(true, false, false, vec![], false)]
#[case(true, false, false, vec!["--ask=foo"], true)]
#[case(true, false, false, vec!["--ask=baz"], false)]
#[case(true, false, true, vec![], false)]
#[case(true, false, true, vec!["--ask=foo"], true)]
#[case(true, false, true, vec!["--ask=baz"], false)]
#[case(false, true, false, vec![], false)]
#[case(false, false, true, vec!["--ask=foo"], false)]
fn test_update_with_ask_setting(
    #[case] when: bool,
    #[case] ask: bool,
    #[case] skip_answered: bool,
    #[case] ask_flag: Vec<&str>,
    #[case] prompted: bool,
) {
    // Test update prompting with `when`, `ask`, `--ask`, and `--skip-answered`.
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => json!({
            "foo": {"type": "str", "default": "bar", "when": when, "ask": ask}
        })
        .to_string(),
        src.join(common::ANSWERS_FILE_TMPL) => "{{ _copier_answers|to_nice_yaml }}",
    };
    git_save(&src, "Test commit", Some("v1"));
    let mut tui = spawn_with(
        &["copy", src.to_str().unwrap(), ".", "--data=foo=prev"],
        SPAWN_TIMEOUT,
        Some(&dst),
    );
    tui.expect_eof();
    git_save(&dst, "Test commit", None);
    let mut update_args = vec!["update"];
    if skip_answered {
        update_args.push("--skip-answered");
    }
    update_args.extend_from_slice(&ask_flag);
    let mut tui = spawn_with(&update_args, SPAWN_TIMEOUT * 3, Some(&dst));
    if prompted {
        expect_prompt(&mut tui, "foo", "str", None);
        tui.sendline("x");
    }
    tui.expect_eof();
    let answers = load_answers(&dst);
    if when {
        assert_eq!(
            answers.get("foo"),
            Some(&Value::from(if prompted { "prevx" } else { "prev" }))
        );
    } else {
        assert!(!answers.contains_key("foo"));
    }
}
