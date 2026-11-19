//! Port of `tests/test_complex_questions.py`.

mod common;

use std::path::{Path, PathBuf};

use common::{
    expect_prompt, file_tree, git, keyboard, load_answers, load_answers_from, read, spawn, v,
    worker, Tmp, Tui, ANSWERS_FILE_TMPL, BRACKET_ENVOPS_JSON, SUFFIX_TMPL,
};
use copier::{Value, Worker};
use indoc::indoc;
use rstest::rstest;
use serde_json::json;

/// Parse a generated JSON file (`json.loads(path.read_text())`).
fn read_json(path: impl AsRef<Path>) -> serde_json::Value {
    serde_json::from_str(&read(path)).expect("valid JSON")
}

/// Template with questions of every type, plus results rendered as JSON.
fn template_path(tmp: &Tmp) -> PathBuf {
    let root = tmp.mktemp("template");
    file_tree! {
        root.join("copier.yml") => format!(indoc! {"
            _templates_suffix: {suffix}
            _envops: {envops}
            love_me:
                help: I need to know it. Do you love me?
                type: bool
            your_name:
                help: Please tell me your name.
                type: str
            your_age:
                help: How old are you?
                type: int
                validator: |-
                    [% if your_age <= 0 %]
                        Must be +
                    [% endif %]
            your_height:
                help: What's your height?
                type: float
            more_json_info:
                multiline: true
                type: json
            anything_else:
                multiline: true
                help: Wanna give me any more info?

            # In choices, the user will always write the choice index (1, 2, 3...)
            choose_list:
                help: You see the value of the list items
                default: first
                choices:
                    - first
                    - second
                    - third
            choose_tuple:
                help: You see the 1st tuple item, but I get the 2nd item as a result
                default: second
                choices:
                    - [one, first]
                    - [two, second]
                    - [three, third]
            choose_dict:
                help: You see the dict key, but I get the dict value
                default: third
                choices:
                    one: first
                    two: second
                    three: third
            choose_number:
                help: This must be a number
                default: -1.1
                type: float
                choices:
                    - -1.1
                    - 0
                    - 1

            # Simplified format is still supported
            minutes_under_water: 10
            optional_value: null
        "}, suffix = SUFFIX_TMPL, envops = BRACKET_ENVOPS_JSON),
        root.join("results.txt.tmpl") => indoc! {"
            love_me: [[love_me|tojson]]
            your_name: [[your_name|tojson]]
            your_age: [[your_age|tojson]]
            your_height: [[your_height|tojson]]
            more_json_info: [[more_json_info|tojson]]
            anything_else: [[anything_else|tojson]]
            choose_list: [[choose_list|tojson]]
            choose_tuple: [[choose_tuple|tojson]]
            choose_dict: [[choose_dict|tojson]]
            choose_number: [[choose_number|tojson]]
            minutes_under_water: [[minutes_under_water|tojson]]
            optional_value: [[optional_value|tojson]]
        "},
    };
    root
}

/// Check that invalid input is reported correctly.
fn check_invalid(
    tui: &mut Tui,
    name: &str,
    format: &str,
    invalid_value: &str,
    help: Option<&str>,
    err: &str,
) {
    expect_prompt(tui, name, format, help);
    tui.sendline(invalid_value);
    tui.expect_exact(invalid_value);
    tui.expect_exact(err);
}

#[test]
fn test_api() {
    // Test copier correctly processes advanced questions and answers through API.
    let tmp = Tmp::new();
    let template = template_path(&tmp);
    let dst = tmp.mktemp("dst");
    worker(&template, &dst)
        .data(v(json!({
            "love_me": false,
            "your_name": "LeChuck",
            "your_age": 220,
            "your_height": 1.9,
            "more_json_info": ["bad", "guy"],
            "anything_else": {"hates": "all"},
        })))
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    assert_eq!(
        read(dst.join("results.txt")),
        indoc! {r#"
            love_me: false
            your_name: "LeChuck"
            your_age: 220
            your_height: 1.9
            more_json_info: ["bad", "guy"]
            anything_else: {"hates": "all"}
            choose_list: "first"
            choose_tuple: "second"
            choose_dict: "third"
            choose_number: -1.1
            minutes_under_water: 10
            optional_value: null
        "#}
    );
}

#[test]
fn test_cli_interactive() {
    // Test copier correctly processes advanced questions and answers through CLI.
    let tmp = Tmp::new();
    let template = template_path(&tmp);
    let dst = tmp.mktemp("dst");
    let (template_str, dst_str) = (template.to_str().unwrap(), dst.to_str().unwrap());
    let mut tui = spawn(&["copy", template_str, dst_str]);
    expect_prompt(
        &mut tui,
        "love_me",
        "bool",
        Some("I need to know it. Do you love me?"),
    );
    tui.send("y");
    expect_prompt(
        &mut tui,
        "your_name",
        "str",
        Some("Please tell me your name."),
    );
    tui.sendline("Guybrush Threpwood");
    check_invalid(
        &mut tui,
        "your_age",
        "int",
        "wrong your_age",
        Some("How old are you?"),
        "Invalid input",
    );
    tui.send(&format!("{}{}", keyboard::ALT, keyboard::BACKSPACE).repeat(2));
    tui.send("-1");
    tui.expect_exact("Must be +");
    tui.send(&keyboard::BACKSPACE.repeat(2));
    tui.sendline("22");
    check_invalid(
        &mut tui,
        "your_height",
        "float",
        "wrong your_height",
        Some("What's your height?"),
        "Invalid input",
    );
    tui.send(&format!("{}{}", keyboard::ALT, keyboard::BACKSPACE).repeat(2));
    tui.sendline("1.56");
    check_invalid(
        &mut tui,
        "more_json_info",
        "json",
        "{\"objective\":",
        None,
        "Invalid input",
    );
    tui.sendline("\"be a pirate\"}");
    tui.send(&format!("{}{}", keyboard::ESC, keyboard::ENTER));
    expect_prompt(
        &mut tui,
        "anything_else",
        "yaml",
        Some("Wanna give me any more info?"),
    );
    tui.sendline("- Want some grog?");
    tui.sendline("- I'd love it");
    tui.send(&format!("{}{}", keyboard::ESC, keyboard::ENTER));
    expect_prompt(
        &mut tui,
        "choose_list",
        "str",
        Some("You see the value of the list items"),
    );
    for item in ["first", "second", "third"] {
        tui.expect_exact(item);
    }
    tui.sendline("");
    expect_prompt(
        &mut tui,
        "choose_tuple",
        "str",
        Some("You see the 1st tuple item, but I get the 2nd item as a result"),
    );
    for item in ["one", "two", "three"] {
        tui.expect_exact(item);
    }
    tui.sendline("");
    expect_prompt(
        &mut tui,
        "choose_dict",
        "str",
        Some("You see the dict key, but I get the dict value"),
    );
    for item in ["one", "two", "three"] {
        tui.expect_exact(item);
    }
    tui.sendline("");
    expect_prompt(
        &mut tui,
        "choose_number",
        "float",
        Some("This must be a number"),
    );
    for item in ["-1.1", "0", "1"] {
        tui.expect_exact(item);
    }
    tui.sendline("");
    expect_prompt(&mut tui, "minutes_under_water", "int", None);
    tui.expect_exact("10");
    tui.send(keyboard::BACKSPACE);
    tui.sendline("");
    expect_prompt(&mut tui, "optional_value", "yaml", None);
    tui.sendline("");
    tui.expect_eof();
    assert_eq!(
        read(dst.join("results.txt")),
        indoc! {r#"
            love_me: true
            your_name: "Guybrush Threpwood"
            your_age: 22
            your_height: 1.56
            more_json_info: {"objective": "be a pirate"}
            anything_else: ["Want some grog?", "I\u0027d love it"]
            choose_list: "first"
            choose_tuple: "second"
            choose_dict: "third"
            choose_number: -1.1
            minutes_under_water: 1
            optional_value: null
        "#}
    );
}

#[test]
fn test_api_str_data() {
    // Test copier when all data comes as a string.
    //
    // This happens i.e. when using the --data CLI argument.
    let tmp = Tmp::new();
    let template = template_path(&tmp);
    let dst = tmp.mktemp("dst");
    worker(&template, &dst)
        .data(v(json!({
            "love_me": "false",
            "your_name": "LeChuck",
            "your_age": "220",
            "your_height": "1.9",
            "more_json_info": "[\"bad\", \"guy\"]",
            "anything_else": "{'hates': 'all'}",
            "choose_number": "0",
        })))
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    assert_eq!(
        read(dst.join("results.txt")),
        indoc! {r#"
            love_me: false
            your_name: "LeChuck"
            your_age: 220
            your_height: 1.9
            more_json_info: ["bad", "guy"]
            anything_else: {"hates": "all"}
            choose_list: "first"
            choose_tuple: "second"
            choose_dict: "third"
            choose_number: 0.0
            minutes_under_water: 10
            optional_value: null
        "#}
    );
}

#[test]
fn test_cli_interatively_with_flag_data_and_type_casts() {
    // Assert how choices work when copier is invoked with --data interactively.
    let tmp = Tmp::new();
    let template = template_path(&tmp);
    let dst = tmp.mktemp("dst");
    let (template_str, dst_str) = (template.to_str().unwrap(), dst.to_str().unwrap());
    let mut tui = spawn(&[
        "copy",
        "--data=choose_list=second",
        "--data=choose_dict=first",
        "--data=choose_tuple=third",
        "--data=choose_number=1",
        template_str,
        dst_str,
    ]);
    expect_prompt(
        &mut tui,
        "love_me",
        "bool",
        Some("I need to know it. Do you love me?"),
    );
    tui.send("y");
    expect_prompt(
        &mut tui,
        "your_name",
        "str",
        Some("Please tell me your name."),
    );
    tui.sendline("Guybrush Threpwood");
    check_invalid(
        &mut tui,
        "your_age",
        "int",
        "wrong your_age",
        Some("How old are you?"),
        "Invalid input",
    );
    tui.sendline(""); // Does nothing, "wrong your_age still on screen"
    tui.send(&format!("{}{}", keyboard::ALT, keyboard::BACKSPACE).repeat(2));
    tui.sendline("22");
    check_invalid(
        &mut tui,
        "your_height",
        "float",
        "wrong your_height",
        Some("What's your height?"),
        "Invalid input",
    );
    tui.send(&format!("{}{}", keyboard::ALT, keyboard::BACKSPACE).repeat(2));
    tui.sendline("1.56");
    check_invalid(
        &mut tui,
        "more_json_info",
        "json",
        "{\"objective\":",
        None,
        "Invalid input",
    );
    tui.sendline("\"be a pirate\"}");
    tui.send(&format!("{}{}", keyboard::ESC, keyboard::ENTER));
    expect_prompt(
        &mut tui,
        "anything_else",
        "yaml",
        Some("Wanna give me any more info?"),
    );
    tui.sendline("- Want some grog?");
    tui.sendline("- I'd love it");
    tui.send(&format!("{}{}", keyboard::ESC, keyboard::ENTER));
    expect_prompt(&mut tui, "minutes_under_water", "int", None);
    tui.expect_exact("10");
    tui.sendline("");
    expect_prompt(&mut tui, "optional_value", "yaml", None);
    tui.sendline("");
    tui.expect_eof();
    assert_eq!(
        read(dst.join("results.txt")),
        indoc! {r#"
            love_me: true
            your_name: "Guybrush Threpwood"
            your_age: 22
            your_height: 1.56
            more_json_info: {"objective": "be a pirate"}
            anything_else: ["Want some grog?", "I\u0027d love it"]
            choose_list: "second"
            choose_tuple: "third"
            choose_dict: "first"
            choose_number: 1.0
            minutes_under_water: 10
            optional_value: null
        "#}
    );
}

#[rstest]
#[case(true, "example2")]
#[case(false, "example")]
fn test_tui_inherited_default(#[case] has_2_owners: bool, #[case] owner2: &str) {
    // Make sure a template inherits default as expected.
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yaml") => indoc! {r#"
            owner1:
                type: str
            has_2_owners:
                type: bool
                default: false
            owner2:
                type: str
                default: "{{ owner1 }}"
                when: "{{ has_2_owners }}"
        "#},
        src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|to_nice_yaml }}",
        src.join("answers.json.jinja") => "{{ _copier_answers|to_nice_json }}",
        src.join("context.json.jinja") => r#"{"owner2": "{{ owner2 }}"}"#,
    };
    git(&src, &["init"]);
    git(&src, &["add", "--all"]);
    git(&src, &["commit", "--message", "init template"]);
    git(&src, &["tag", "1"]);
    let (src_str, dst_str) = (src.to_str().unwrap(), dst.to_str().unwrap());
    let mut tui = spawn(&["copy", src_str, dst_str]);
    expect_prompt(&mut tui, "owner1", "str", None);
    tui.sendline("example");
    expect_prompt(&mut tui, "has_2_owners", "bool", None);
    tui.expect_exact("(y/N)");
    tui.send(if has_2_owners { "y" } else { "n" });
    if has_2_owners {
        expect_prompt(&mut tui, "owner2", "str", None);
        tui.expect_exact("example");
        tui.sendline("2");
    }
    tui.expect_eof();
    let mut result = json!({
        "_commit": "1",
        "_src_path": src_str,
        "has_2_owners": has_2_owners,
        "owner1": "example",
    });
    if has_2_owners {
        result["owner2"] = json!(owner2);
    }
    assert_eq!(read_json(dst.join("answers.json")), result);
    assert_eq!(
        read_json(dst.join("context.json")),
        json!({"owner2": owner2})
    );
    git(&dst, &["init"]);
    git(&dst, &["add", "--all"]);
    git(&dst, &["commit", "--message", "init project"]);
    // After a forced update, answers stay the same
    Worker::new()
        .dst_path(&dst)
        .defaults(true)
        .overwrite(true)
        .run_update()
        .unwrap();
    assert_eq!(read_json(dst.join("answers.json")), result);
}

#[test]
fn test_tui_typed_default() {
    // Make sure a template defaults are typed as expected.
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yaml") => indoc! {r#"
            test1:
                type: bool
                default: false
                when: false
            test2:
                type: bool
                default: "{{ 'a' == 'b' }}"
                when: false
        "#},
        src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|to_nice_yaml }}",
        src.join("answers.json.jinja") => "{{ _copier_answers|to_nice_json }}",
        src.join("context.json.jinja") => indoc! {r#"
            {"test1": "{{ test1 }}", "test2": "{{ test2 }}"}
        "#},
    };
    let (src_str, dst_str) = (src.to_str().unwrap(), dst.to_str().unwrap());
    let mut tui = spawn(&["copy", src_str, dst_str]);
    tui.expect_eof();
    assert_eq!(
        read_json(dst.join("answers.json")),
        json!({"_src_path": src_str})
    );
    assert_eq!(
        read_json(dst.join("context.json")),
        json!({
            "test1": "False",
            "test2": "False",
        })
    );
}

#[test]
fn test_selection_type_cast() {
    // Selection question with different types, properly casted.
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yaml") => indoc! {r#"
            postgres1: &pg
                type: yaml
                default: 10
                help: Which PostgreSQL version do you want to deploy?
                choices:
                    I will use an external PostgreSQL server: null
                    "9.6": 9.6
                    "10": 10
            postgres2: *pg
            postgres3: *pg
            nulls1: &nulls
                type: yaml
                choices:
                    - [same as key, null]
                    - [yaml-casted to a real null, "null"]
                    - [becomes array, "[one, 2, -3.0]"]
            nulls2: *nulls
            nulls3: *nulls
        "#},
        src.join("answers.json.jinja") => "{{ _copier_answers|to_json }}",
    };
    let (src_str, dst_str) = (src.to_str().unwrap(), dst.to_str().unwrap());
    let mut tui = spawn(&["copy", src_str, dst_str]);
    expect_prompt(
        &mut tui,
        "postgres1",
        "yaml",
        Some("Which PostgreSQL version do you want to deploy?"),
    );
    tui.sendline(keyboard::DOWN);
    expect_prompt(
        &mut tui,
        "postgres2",
        "yaml",
        Some("Which PostgreSQL version do you want to deploy?"),
    );
    tui.sendline(keyboard::UP);
    expect_prompt(
        &mut tui,
        "postgres3",
        "yaml",
        Some("Which PostgreSQL version do you want to deploy?"),
    );
    tui.sendline("");
    expect_prompt(&mut tui, "nulls1", "yaml", None);
    tui.sendline("");
    expect_prompt(&mut tui, "nulls2", "yaml", None);
    tui.sendline(keyboard::DOWN);
    expect_prompt(&mut tui, "nulls3", "yaml", None);
    tui.sendline(&format!("{}{}", keyboard::DOWN, keyboard::DOWN));
    tui.expect_eof();
    assert_eq!(
        read_json(dst.join("answers.json")),
        json!({
            "_src_path": src_str,
            "nulls1": "same as key",
            "nulls2": null,
            "nulls3": ["one", 2, -3.0],
            "postgres1": "I will use an external PostgreSQL server",
            "postgres2": 9.6,
            "postgres3": 10,
        })
    );
}

#[test]
fn test_multi_template_answers() {
    let tmp = Tmp::new();
    let (tpl1, tpl2, dst) = (tmp.mktemp("0"), tmp.mktemp("1"), tmp.mktemp("2"));
    // Build 2 templates with different questions and answers file defaults
    file_tree! {
        tpl1.join("copier.yml") => "q1: a1",
        tpl1.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|to_nice_yaml }}",
        tpl2.join("copier.yml") => "{_answers_file: two.yml, q2: a2}",
        tpl2.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|to_nice_yaml }}",
    };
    // Make them git-tracked
    for tpl in [&tpl1, &tpl2] {
        git(tpl, &["init"]);
        git(tpl, &["add", "-A"]);
        git(tpl, &["commit", "-m1"]);
    }
    // Apply template 1
    worker(&tpl1, &dst)
        .overwrite(true)
        .defaults(true)
        .run_copy()
        .unwrap();
    git(&dst, &["init"]);
    git(&dst, &["add", "-A"]);
    git(&dst, &["commit", "-m1"]);
    // Apply template 2
    worker(&tpl2, &dst)
        .overwrite(true)
        .defaults(true)
        .run_copy()
        .unwrap();
    git(&dst, &["init"]);
    git(&dst, &["add", "-A"]);
    git(&dst, &["commit", "-m2"]);
    // Check contents
    let answers1 = load_answers(&dst);
    assert_eq!(answers1["_src_path"], Value::from(tpl1.to_str().unwrap()));
    assert_eq!(answers1["q1"], Value::from("a1"));
    assert!(!answers1.contains_key("q2"));
    let answers2 = load_answers_from(&dst, "two.yml");
    assert_eq!(answers2["_src_path"], Value::from(tpl2.to_str().unwrap()));
    assert_eq!(answers2["q2"], Value::from("a2"));
    assert!(!answers2.contains_key("q1"));
}

#[test]
fn test_omit_answer_for_skipped_question() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => indoc! {r#"
            disabled:
                type: str
                when: false

            disabled_with_default:
                type: str
                default: hello
                when: false
        "#},
        src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|to_nice_yaml }}",
        src.join("context.yml.jinja") => json!({
            "disabled": "{{ disabled }}",
            "disabled_with_default": "{{ disabled_with_default }}",
        })
        .to_string(),
    };
    let src_str = src.to_str().unwrap();
    worker(&src, &dst)
        .defaults(true)
        .data(v(json!({"disabled": "hello"})))
        .run_copy()
        .unwrap();
    let answers = load_answers(&dst);
    assert_eq!(answers, v(json!({"_src_path": src_str})));
    // JSON is valid YAML
    assert_eq!(
        read_json(dst.join("context.yml")),
        json!({"disabled": "hello", "disabled_with_default": "hello"})
    );
}

#[test]
fn test_keep_answer_for_unask_question() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => indoc! {r#"
            disabled:
                type: str
                ask: false

            disabled_with_default:
                type: str
                default: hello
                ask: false
        "#},
        src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|to_nice_yaml }}",
        src.join("context.yml.jinja") => json!({
            "disabled": "{{ disabled }}",
            "disabled_with_default": "{{ disabled_with_default }}",
        })
        .to_string(),
    };
    let src_str = src.to_str().unwrap();
    worker(&src, &dst)
        .defaults(true)
        .data(v(json!({"disabled": "hello"})))
        .run_copy()
        .unwrap();
    let answers = load_answers(&dst);
    assert_eq!(
        answers,
        v(json!({
            "_src_path": src_str,
            "disabled": "hello",
            "disabled_with_default": "hello",
        }))
    );
    // JSON is valid YAML
    assert_eq!(
        read_json(dst.join("context.yml")),
        json!({"disabled": "hello", "disabled_with_default": "hello"})
    );
}
