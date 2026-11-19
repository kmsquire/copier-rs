//! Port of `tests/test_templated_prompt.py`.

mod common;

use std::time::{SystemTime, UNIX_EPOCH};

use common::{
    assert_err, expect_prompt, file_tree, git, git_init, keyboard, load_answers, read, spawn, v,
    worker, Tmp, ANSWERS_FILE_TMPL, BRACKET_ENVOPS_JSON, SUFFIX_TMPL,
};
use copier::errors::{capture_warnings, WarningKind};
use copier::CopierError;
use indoc::indoc;
use rstest::rstest;
use serde_json::json;

/// Default answer of the `main` question (`main_default` in Python).
const MAIN_DEFAULT: &str = "copier";

/// Questions every test template starts with (`main_question` in Python).
fn main_question() -> serde_json::Map<String, serde_json::Value> {
    let envops: serde_json::Value = serde_json::from_str(BRACKET_ENVOPS_JSON).unwrap();
    json!({
        "main": {"default": MAIN_DEFAULT},
        "_envops": envops,
        "_templates_suffix": SUFFIX_TMPL,
    })
    .as_object()
    .unwrap()
    .clone()
}

/// Unix seconds of a `YYYY-MM-DD HH:MM:SS[.f][+00:00]` UTC text (`datetime.fromisoformat`
/// substitute, since no datetime crate is available); `None` when the text does not match.
fn parse_utc_seconds(text: &str) -> Option<i64> {
    let pattern =
        regex::Regex::new(r"^(\d{4})-(\d{2})-(\d{2}) (\d{2}):(\d{2}):(\d{2})(\.\d+)?(\+00:00)?$")
            .unwrap();
    let caps = pattern.captures(text)?;
    let field = |index: usize| caps[index].parse::<i64>().unwrap();
    let (year, month, day) = (field(1), field(2), field(3));
    // Days since the Unix epoch (proleptic Gregorian calendar).
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_index = (month + 9) % 12;
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    Some(days * 86_400 + field(4) * 3_600 + field(5) * 60 + field(6))
}

#[rstest]
#[case(
    json!({"templated_default": {"default": "[[ main ]]default"}}),
    json!("copierdefault"),
    json!([{"name": "templated_default", "format": "str"}, "copierdefault"]),
)]
#[case(
    json!({"templated_type": {"type": "[% if main == 'copier' %]int[% endif %]", "default": "0"}}),
    json!(0),
    json!([{"name": "templated_type", "format": "int"}, "0"]),
)]
#[case(
    json!({"templated_help": {"default": MAIN_DEFAULT, "help": "THIS [[ main ]] HELP IS TEMPLATED"}}),
    json!(MAIN_DEFAULT),
    json!([
        {"name": "templated_help", "format": "str", "help": "THIS copier HELP IS TEMPLATED"},
        "copier",
    ]),
)]
#[case(
    json!({"templated_choices_dict_1": {
        "default": "[[ main ]]",
        "choices": {"choice 1": "[[ main ]]", "[[ main ]]": "value 2"},
    }}),
    json!(MAIN_DEFAULT),
    json!(["(Use arrow keys)", "choice 1", "copier"]),
)]
#[case(
    json!({"templated_choices_dict_2": {
        "default": "value 2",
        "choices": {"choice 1": "[[ main ]]", "[[ main ]]": "value 2"},
    }}),
    json!("value 2"),
    json!(["(Use arrow keys)", "choice 1", "copier"]),
)]
#[case(
    json!({"templated_choices_string_list_1": {
        "default": MAIN_DEFAULT,
        "choices": ["[[ main ]]", "choice 2"],
    }}),
    json!(MAIN_DEFAULT),
    json!(["(Use arrow keys)", "copier", "choice 2"]),
)]
#[case(
    json!({"templated_choices_string_list_2": {
        "default": "choice 1",
        "choices": ["choice 1", "[[ main ]]"],
    }}),
    json!("choice 1"),
    json!(["(Use arrow keys)", "choice 1", "copier"]),
)]
#[case(
    json!({"templated_choices_tuple_list_1": {
        "default": MAIN_DEFAULT,
        "choices": [["name 1", "[[ main ]]"], ["[[ main ]]", "value 2"]],
    }}),
    json!(MAIN_DEFAULT),
    json!(["(Use arrow keys)", "name 1", "copier"]),
)]
#[case(
    json!({"templated_choices_tuple_list_2": {
        "default": "value 2",
        "choices": [["name 1", "[[ main ]]"], ["[[ main ]]", "value 2"]],
    }}),
    json!("value 2"),
    json!(["name 1", "copier"]),
)]
#[case(
    json!({"templated_choices_mixed_list": {
        "default": "value 2",
        "choices": ["[[ main ]]", ["[[ main ]]", "value 2"]],
    }}),
    json!("value 2"),
    json!(["copier", "copier"]),
)]
#[ignore = "pending"]
fn test_templated_prompt(
    #[case] questions_data: serde_json::Value,
    #[case] expected_value: serde_json::Value,
    #[case] expected_outputs: serde_json::Value,
) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    let mut questions_combined = main_question();
    let questions_data = questions_data.as_object().unwrap();
    questions_combined.extend(questions_data.clone());
    // There's always only 1 question; get its name
    let question_name = questions_data.keys().next().unwrap().clone();
    file_tree! {
        src.join("copier.yml") => serde_json::Value::Object(questions_combined).to_string(),
        src.join("[[ _copier_conf.answers_file ]].tmpl") => "[[ _copier_answers|to_nice_yaml ]]",
    };
    let (src_str, dst_str) = (src.to_str().unwrap(), dst.to_str().unwrap());
    let mut tui = spawn(&["copy", src_str, dst_str]);
    expect_prompt(&mut tui, "main", "str", None);
    tui.expect_exact(MAIN_DEFAULT);
    tui.sendline("");
    for output in expected_outputs.as_array().unwrap() {
        match output {
            serde_json::Value::String(text) => tui.expect_exact(text),
            prompt => expect_prompt(
                &mut tui,
                prompt["name"].as_str().unwrap(),
                prompt["format"].as_str().unwrap(),
                prompt["help"].as_str(),
            ),
        }
    }
    tui.sendline("");
    tui.expect_eof();
    let answers = load_answers(&dst);
    assert_eq!(answers[question_name.as_str()], v(expected_value));
}

#[test]
#[ignore = "pending"]
fn test_templated_prompt_custom_envops() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => indoc! {r##"
            _envops:
                block_start_string: "<%"
                block_end_string: "%>"
                comment_start_string: "<#"
                comment_end_string: "#>"
                variable_start_string: "<<"
                variable_end_string: ">>"

            powerlevel:
                type: int
                default: 9000

            sentence:
                type: str
                default: "<% if powerlevel >= 9000 %>It's over 9000!<% else %>It's only << powerlevel >>...<% endif %>"
        "##},
        src.join("result.jinja") => "<<sentence>>",
    };
    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    assert_eq!(read(dst.join("result")), "It's over 9000!");

    worker(&src, &dst)
        .data(v(json!({"powerlevel": 1})))
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    assert_eq!(read(dst.join("result")), "It's only 1...");
}

#[test]
#[ignore = "pending"]
fn test_templated_prompt_builtins() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yaml") => format!(indoc! {r#"
            _templates_suffix: {suffix}
            _envops: {envops}
            question1:
                default: "[[ now() ]]"
            question2:
                default: "[[ make_secret() ]]"
        "#}, suffix = SUFFIX_TMPL, envops = BRACKET_ENVOPS_JSON),
        src.join("now.tmpl") => "[[ question1 ]]",
        src.join("make_secret.tmpl") => "[[ question2 ]]",
    };
    let (result, warnings) =
        capture_warnings(|| worker(&src, &dst).defaults(true).overwrite(true).run_copy());
    result.unwrap();
    assert_eq!(
        warnings
            .iter()
            .filter(|w| w.kind == WarningKind::Future)
            .count(),
        2
    );
    // `datetime.fromisoformat` is replaced by a local parse of the same text.
    let that_now = read(dst.join("now"));
    let that_now_seconds =
        parse_utc_seconds(&that_now).unwrap_or_else(|| panic!("not a UTC datetime: {that_now:?}"));
    let now_seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    assert!(that_now_seconds <= now_seconds);
    assert_eq!(read(dst.join("make_secret")).chars().count(), 128);
}

#[rstest]
#[case(json!({"question": {"default": "{{ not_valid }}"}}), false, "")]
#[case(json!({"question": {"help": "{{ not_valid }}"}}), false, "None")]
#[case(json!({"question": {"type": "{{ not_valid }}"}}), true, "None")]
#[case(json!({"question": {"choices": ["{{ not_valid }}"]}}), false, "None")]
#[ignore = "pending"]
fn test_templated_prompt_invalid(
    #[case] questions: serde_json::Value,
    #[case] raises: bool,
    #[case] returns: &str,
) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => questions.to_string(),
        src.join("result.jinja") => "{{question}}",
    };
    let worker = worker(&src, &dst)
        .data(v(json!({"question": ""})))
        .overwrite(true);
    if raises {
        assert_err!(worker.run_copy(), CopierError::InvalidType(_));
    } else {
        worker.run_copy().unwrap();
        assert_eq!(read(dst.join("result")), returns);
    }
}

#[rstest]
#[case(
    "Any",
    vec![
        "Terraform",
        "Cloud Formation (Requires AWS)",
        "Azure Resource Manager (Requires Azure)",
        "Deployment Manager (Requires GCP)",
    ],
)]
#[case(
    "AWS",
    vec![
        "Terraform",
        "Cloud Formation",
        "Azure Resource Manager (Requires Azure)",
        "Deployment Manager (Requires GCP)",
    ],
)]
#[case(
    "Azure",
    vec![
        "Terraform",
        "Cloud Formation (Requires AWS)",
        "Azure Resource Manager",
        "Deployment Manager (Requires GCP)",
    ],
)]
#[case(
    "GCP",
    vec![
        "Terraform",
        "Cloud Formation (Requires AWS)",
        "Azure Resource Manager (Requires Azure)",
        "Deployment Manager",
    ],
)]
#[ignore = "pending"]
fn test_templated_prompt_with_conditional_choices(
    #[case] cloud: &str,
    #[case] iac_choices: Vec<&str>,
) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => indoc! {r#"
            cloud:
                type: str
                help: Which cloud provider do you use?
                choices:
                    - Any
                    - AWS
                    - Azure
                    - GCP

            iac:
                type: str
                help: Which IaC tool do you use?
                choices:
                    Terraform: tf
                    Cloud Formation:
                        value: cf
                        validator: "{% if cloud != 'AWS' %}Requires AWS{% endif %}"
                    Azure Resource Manager:
                        value: arm
                        validator: "{% if cloud != 'Azure' %}Requires Azure{% endif %}"
                    Deployment Manager:
                        value: dm
                        validator: "{% if cloud != 'GCP' %}Requires GCP{% endif %}"
        "#},
    };
    let (src_str, dst_str) = (src.to_str().unwrap(), dst.to_str().unwrap());
    let mut tui = spawn(&["copy", &format!("--data=cloud={cloud}"), src_str, dst_str]);
    expect_prompt(&mut tui, "iac", "str", Some("Which IaC tool do you use?"));
    for iac in iac_choices {
        tui.expect_exact(iac);
    }
    tui.sendline("");
}

#[rstest]
#[case("Any", vec!["Terraform"])]
#[case("AWS", vec!["Terraform", "Cloud Formation"])]
#[case("Azure", vec!["Terraform", "Azure Resource Manager"])]
#[case("GCP", vec!["Terraform", "Deployment Manager"])]
#[ignore = "pending"]
fn test_templated_prompt_with_templated_choices(
    #[case] cloud: &str,
    #[case] iac_choices: Vec<&str>,
) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => indoc! {r#"
            cloud:
                type: str
                help: Which cloud provider do you use?
                choices:
                    - Any
                    - AWS
                    - Azure
                    - GCP

            iac:
                type: str
                help: Which IaC tool do you use?
                choices: |
                    Terraform: tf
                    {%- if cloud == 'AWS' %}
                    Cloud Formation: cf
                    {%- endif %}
                    {%- if cloud == 'Azure' %}
                    Azure Resource Manager: arm
                    {%- endif %}
                    {%- if cloud == 'GCP' %}
                    Deployment Manager: dm
                    {%- endif %}
        "#},
    };
    let (src_str, dst_str) = (src.to_str().unwrap(), dst.to_str().unwrap());
    let mut tui = spawn(&["copy", &format!("--data=cloud={cloud}"), src_str, dst_str]);
    expect_prompt(&mut tui, "iac", "str", Some("Which IaC tool do you use?"));
    for iac in iac_choices {
        tui.expect_exact(iac);
    }
    tui.sendline("");
}

#[test]
#[ignore = "pending"]
fn test_templated_prompt_update_previous_answer_disabled() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => indoc! {r#"
            cloud:
                type: str
                help: Which cloud provider do you use?
                choices:
                    - Any
                    - AWS
                    - Azure
                    - GCP
            iac:
                type: str
                help: Which IaC tool do you use?
                choices:
                    Terraform: tf
                    Cloud Formation:
                        value: cf
                        validator: "{% if cloud != 'AWS' %}Requires AWS{% endif %}"
                    Azure Resource Manager:
                        value: arm
                        validator: "{% if cloud != 'Azure' %}Requires Azure{% endif %}"
                    Deployment Manager:
                        value: dm
                        validator: "{% if cloud != 'GCP' %}Requires GCP{% endif %}"
        "#},
        src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|to_nice_yaml }}",
    };

    git_init(&src, "v1");
    git(&src, &["tag", "v1"]);

    let (src_str, dst_str) = (src.to_str().unwrap(), dst.to_str().unwrap());
    let mut tui = spawn(&["copy", src_str, dst_str]);
    expect_prompt(
        &mut tui,
        "cloud",
        "str",
        Some("Which cloud provider do you use?"),
    );
    tui.sendline(keyboard::DOWN); // select "AWS"
    expect_prompt(&mut tui, "iac", "str", Some("Which IaC tool do you use?"));
    tui.sendline(keyboard::DOWN); // select "Cloud Formation"
    tui.expect_eof();

    assert_eq!(
        load_answers(&dst),
        v(json!({
            "_src_path": src_str,
            "_commit": "v1",
            "cloud": "AWS",
            "iac": "cf",
        }))
    );

    git_init(&dst, "v1");

    let mut tui = spawn(&["update", dst_str]);
    expect_prompt(
        &mut tui,
        "cloud",
        "str",
        Some("Which cloud provider do you use?"),
    );
    tui.sendline(keyboard::DOWN); // select "Azure"
    expect_prompt(&mut tui, "iac", "str", Some("Which IaC tool do you use?"));
    tui.sendline(""); // select "Terraform" (first supported)
    tui.expect_eof();

    assert_eq!(
        load_answers(&dst),
        v(json!({
            "_src_path": src_str,
            "_commit": "v1",
            "cloud": "Azure",
            "iac": "tf",
        }))
    );
}

#[test]
#[ignore = "pending"]
fn test_multiselect_choices_with_templated_default_value() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => indoc! {r#"
            python_version:
                type: str
                help: What version of python are you targeting?
                default: "3.11"
                choices:
                    - "3.8"
                    - "3.9"
                    - "3.10"
                    - "3.11"
                    - "3.12"

            github_runner_python_version:
                type: str
                help: Which Python versions do you want to use on your Github Runner?
                default: ["{{ python_version }}"]
                multiselect: true
                choices:
                    - "3.8"
                    - "3.9"
                    - "3.10"
                    - "3.11"
                    - "3.12"
        "#},
        src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|to_nice_yaml }}",
    };

    let (src_str, dst_str) = (src.to_str().unwrap(), dst.to_str().unwrap());
    let mut tui = spawn(&["copy", src_str, dst_str]);
    expect_prompt(
        &mut tui,
        "python_version",
        "str",
        Some("What version of python are you targeting?"),
    );
    tui.sendline(""); // select `3.11" (default value)
    expect_prompt(
        &mut tui,
        "github_runner_python_version",
        "str",
        Some("Which Python versions do you want to use on your Github Runner?"),
    );
    tui.sendline(""); // select "[3.11]" (default value)
    tui.expect_eof();

    assert_eq!(
        load_answers(&dst),
        v(json!({
            "_src_path": src_str,
            "python_version": "3.11",
            "github_runner_python_version": ["3.11"],
        }))
    );
}

#[test]
#[ignore = "pending"]
fn test_copier_phase_variable() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => indoc! {r#"
            phase:
                type: str
                default: "{{ _copier_phase }}"
        "#},
    };
    let (src_str, dst_str) = (src.to_str().unwrap(), dst.to_str().unwrap());
    let mut tui = spawn(&["copy", src_str, dst_str]);
    expect_prompt(&mut tui, "phase", "str", None);
    tui.expect_exact("prompt");
}

#[test]
#[ignore = "pending"]
fn test_copier_conf_variable() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => indoc! {r#"
            project_name:
                type: str
                default: "{{ _copier_conf.dst_path | basename }}"
        "#},
    };
    let src_str = src.to_str().unwrap();
    let project = dst.join("test_project");
    let mut tui = spawn(&["copy", src_str, project.to_str().unwrap()]);
    expect_prompt(&mut tui, "project_name", "str", None);
    tui.expect_exact("test_project");
}
