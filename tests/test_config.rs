//! Port of `tests/test_config.py`.

mod common;

use std::path::PathBuf;

use common::{
    assert_err, file_tree, fixture, git_init, read, v, worker, Tmp, ANSWERS_FILE_TMPL,
    BRACKET_ENVOPS_JSON, SUFFIX_TMPL,
};
use copier::template::{load_template_config, Command, Task, Template, DEFAULT_EXCLUDE};
use copier::value::Mapping;
use copier::{CopierError, Settings, Value, Worker};
use indoc::{formatdoc, indoc};
use rstest::rstest;
use serde_json::json;

/// Build a [`Mapping`] from a `json!` object literal.
fn mapping(json: serde_json::Value) -> Mapping {
    match v(json) {
        Value::Map(map) => map,
        _ => unreachable!("expected a JSON object"),
    }
}

/// A shell task whose extra variables are `{"stage": "task"}`.
fn stage_task(cmd: &str) -> Task {
    Task {
        cmd: Command::Shell(cmd.to_owned()),
        extra_vars: mapping(json!({"stage": "task"})),
        condition: Value::Bool(true),
        working_directory: PathBuf::from("."),
    }
}

/// `DEFAULT_EXCLUDE` followed by `extra`.
fn with_default_exclude(extra: &[&str]) -> Vec<String> {
    DEFAULT_EXCLUDE
        .iter()
        .copied()
        .chain(extra.iter().copied())
        .map(str::to_owned)
        .collect()
}

#[test]
#[ignore = "pending"]
fn test_config_data_is_loaded_from_file() {
    let tpl = Template::new(fixture("demo_data").to_str().unwrap());
    assert_eq!(tpl.exclude().unwrap(), vec!["exclude1", "exclude2"]);
    assert_eq!(
        tpl.skip_if_exists().unwrap(),
        vec!["skip_if_exists1", "skip_if_exists2"]
    );
    assert_eq!(
        tpl.tasks().unwrap(),
        vec![stage_task("touch 1"), stage_task("touch 2")]
    );
}

#[test]
#[ignore = "pending"]
fn test_config_data_is_merged_from_files() {
    let tpl = Template::new(
        fixture("demo_merge_options_from_answerfiles")
            .to_str()
            .unwrap(),
    );
    assert_eq!(
        tpl.skip_if_exists().unwrap(),
        vec!["skip_if_exists0", "skip_if_exists1", "skip_if_exists2"]
    );
    assert_eq!(
        tpl.exclude().unwrap(),
        vec!["exclude1", "exclude21", "exclude22"]
    );
    assert_eq!(
        tpl.jinja_extensions().unwrap(),
        vec!["jinja2.ext.0", "jinja2.ext.2"]
    );
    assert_eq!(tpl.secret_questions().unwrap(), vec!["question1"]);
}

#[ignore = "pending"]
#[rstest]
fn test_read_data(#[values("yaml", "yml")] config_suffix: &str) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join(format!("copier.{config_suffix}")) => formatdoc! {"
            # This is a comment
            _envops: {BRACKET_ENVOPS_JSON}
            a_string: lorem ipsum
            a_number: 12345
            a_boolean: true
            a_list:
                - one
                - two
                - three
        "},
        src.join("user_data.txt.jinja") => indoc! {r#"
            A string: [[ a_string ]]
            A number: [[ a_number ]]
            A boolean: [[ a_boolean ]]
            A list: [[ ", ".join(a_list) ]]
        "#},
    };
    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    assert_eq!(
        read(dst.join("user_data.txt")),
        indoc! {"
            A string: lorem ipsum
            A number: 12345
            A boolean: True
            A list: one, two, three
        "}
    );
}

#[test]
#[ignore = "pending"]
fn test_settings_defaults_precedence() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => formatdoc! {"
            # This is a comment
            _envops: {BRACKET_ENVOPS_JSON}
            a_string: lorem ipsum
            a_number: 12345
            a_boolean: true
            a_list:
                - one
                - two
                - three
            a_string_without_default:
                type: str
        "},
        src.join("user_data.txt.jinja") => indoc! {r#"
            A string: [[ a_string ]]
            A number: [[ a_number ]]
            A boolean: [[ a_boolean ]]
            A list: [[ ", ".join(a_list) ]]
            A string without default: [[ a_string_without_default ]]
        "#},
    };
    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .settings(Settings {
            defaults: mapping(json!({
                "a_string": "whatever",
                "a_number": 42,
                "a_boolean": false,
                "a_list": ["one", "two"],
                "a_string_without_default": "something",
            })),
            trust: vec![],
        })
        .run_copy()
        .unwrap();
    assert_eq!(
        read(dst.join("user_data.txt")),
        indoc! {"
            A string: whatever
            A number: 42
            A boolean: False
            A list: one, two
            A string without default: something
        "}
    );
}

#[test]
#[ignore = "pending"]
fn test_invalid_yaml() {
    let conf_path = fixture("demo_invalid").join("copier.yml");
    let (result, out) = copier::output::capture(|| load_template_config(&conf_path, false));
    assert_err!(result, CopierError::InvalidConfigFile { .. });
    assert!(out.stderr.contains("INVALID CONFIG FILE"));
    assert!(out.stderr.contains(conf_path.to_str().unwrap()));
}

// `copier.yml` is untrusted input. `yaml.FullLoader` resolves these into live
// Python objects; `SafeLoader` must not.
#[rstest]
#[case("!!python/name:os.system")]
#[case("!!python/name:os.popen")]
#[case("!!python/name:builtins.eval")]
#[case("!!python/name:os.path")]
#[case("!!python/object/apply:os.system ['echo pwned']")]
#[case("!!python/object/new:os.system ['echo pwned']")]
#[case("!!python/object:pathlib.PurePosixPath {}")]
#[case("!!python/module:os")]
/// A template's own config cannot name a Python object.
#[ignore = "pending"]
fn test_config_cannot_construct_python_objects(#[case] tag: &str) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => format!("q:\n  type: str\n  default: {tag}\n"),
    };
    assert_err!(
        worker(&src, &dst)
            .defaults(true)
            .overwrite(true)
            .quiet(true)
            .run_copy(),
        CopierError::InvalidConfigFile { .. }
    );
}

// TODO(port): the Python test runs in `cwd` via `local.cwd`; the `finalize` side
// effect is cwd-relative and rule 5 forbids changing the working directory, so the
// marker check is not meaningful yet.
#[ignore = "needs-design: finalize side effect is cwd-relative; cannot change cwd"]
#[rstest]
/// Trust enables declared unsafe features, not arbitrary YAML constructors.
fn test_authorization_does_not_enable_python_yaml_tags(
    #[values("none", "unsafe", "settings")] authorization: &str,
) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    let cwd = tmp.mktemp("cwd");
    let marker = cwd.join("created");
    file_tree! {
        src.join("copier.yml") => "_envops:\n  finalize: !!python/name:os.mkdir\n",
        src.join("f.txt.jinja") => "{{ 'created' }}",
    };
    let mut copy = worker(&src, &dst);
    match authorization {
        "unsafe" => copy = copy.unsafe_(true),
        "settings" => {
            copy = copy.settings(Settings {
                defaults: Mapping::new(),
                trust: vec![src.to_str().unwrap().to_owned()],
            })
        }
        _ => {}
    }
    assert_err!(copy.run_copy(), CopierError::InvalidConfigFile { .. });
    assert!(!marker.exists());
}

#[rstest]
// Python's `check_err` lambda: whether stderr must mention "INVALID".
#[case("demo_invalid", true)]
// test key collision between including and included file
#[case("demo_transclude_invalid", false)]
// test key collision between two included files
#[case("demo_transclude_invalid_multi", false)]
#[ignore = "pending"]
fn test_invalid_config_data(#[case] conf_path: &str, #[case] check_err: bool) {
    let template = Template::new(fixture(conf_path).to_str().unwrap());
    let (result, out) = copier::output::capture(|| template.config_data());
    assert_err!(result, CopierError::InvalidConfigFile { .. });
    if check_err {
        assert!(out.stderr.contains("INVALID"));
    }
}

/// Including multiple files works fine merged with multiple sections.
#[test]
#[ignore = "pending"]
fn test_valid_multi_section() {
    let tmp = Tmp::new();
    file_tree! {
        tmp.path().join("exclusions.yml") => "_exclude: ['*.yml']",
        tmp.path().join("common_jinja.yml") => formatdoc! {r##"
            _templates_suffix: {SUFFIX_TMPL}
            _envops:
                block_start_string: "[%"
                block_end_string: "%]"
                comment_start_string: "[#"
                comment_end_string: "#]"
                variable_start_string: "[["
                variable_end_string: "]]"
                keep_trailing_newline: true
        "##},
        tmp.path().join("common_questions.yml") => indoc! {"
            your_age:
                type: int
            your_name:
                type: yaml
                help: your name from common questions
        "},
        tmp.path().join("copier.yml") => indoc! {"
            ---
            !include 'common_*.yml'
            ---
            !include exclusions.yml
            ---
            your_name:
                type: str
                help: your name from latest section
        "},
    };
    let template = Template::new(tmp.path().to_str().unwrap());
    assert_eq!(template.exclude().unwrap(), vec!["*.yml"]);
    assert_eq!(
        template.envops().unwrap(),
        mapping(json!({
            "block_end_string": "%]",
            "block_start_string": "[%",
            "comment_end_string": "#]",
            "comment_start_string": "[#",
            "keep_trailing_newline": true,
            "variable_end_string": "]]",
            "variable_start_string": "[[",
        }))
    );
    assert_eq!(
        template.questions_data().unwrap(),
        mapping(json!({
            "your_age": {"type": "int"},
            "your_name": {"type": "str", "help": "your name from latest section"},
        }))
    );
}

/// Empty sections are ignored.
#[test]
#[ignore = "pending"]
fn test_empty_section() {
    let tmp = Tmp::new();
    file_tree! {
        tmp.path().join("copier.yml") => indoc! {"
            ---
            ---

            ---
            your_age:
                type: int
            your_name:
                type: str
            ---
        "},
    };
    let template = Template::new(tmp.path().to_str().unwrap());
    assert_eq!(
        template.questions_data().unwrap(),
        mapping(json!({
            "your_age": {"type": "int"},
            "your_name": {"type": "str"},
        }))
    );
}

/// Include path must not be a relative path.
#[test]
#[ignore = "pending"]
fn test_include_path_must_be_relative() {
    let tmp = Tmp::new();
    file_tree! {
        tmp.path().join("copier.yml") => indoc! {"
            !include /absolute/path/to/common.yml
        "},
    };
    let template = Template::new(tmp.path().to_str().unwrap());
    assert_err!(
        template.config_data(),
        CopierError::Value(_),
        "YAML include file path must be a relative path"
    );
}

/// Include value must be a YAML scalar node.
#[ignore = "pending"]
#[rstest]
fn test_include_value_must_be_scalar_node(#[values("[]", "{}")] include_value: &str) {
    let tmp = Tmp::new();
    file_tree! {
        tmp.path().join("copier.yml") => format!("!include {include_value}\n"),
    };
    let template = Template::new(tmp.path().to_str().unwrap());
    assert_err!(
        template.config_data(),
        CopierError::Type(_),
        "^Unsupported YAML node:"
    );
}

#[test]
#[ignore = "pending"]
fn test_include_path_outside_template_root_raises_error() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("template").join("copier.yml") => indoc! {"
            !include ../outside/include.yml
        "},
        src.join("outside").join("include.yml") => "foo: bar",
    };
    let outside = src.join("outside").join("include.yml");
    assert_err!(
        worker(src.join("template"), &dst).defaults(true).run_copy(),
        CopierError::ForbiddenPath { .. },
        &regex::escape(&format!("\"{}\" is forbidden", outside.display()))
    );
}

#[test]
#[ignore = "pending"]
fn test_config_data_empty() {
    let template = Template::new(fixture("demo_config_empty").to_str().unwrap());
    assert_eq!(template.config_data().unwrap(), Mapping::new());
    assert_eq!(template.questions_data().unwrap(), Mapping::new());
}

#[test]
#[ignore = "pending"]
fn test_multiple_config_file_error() {
    let template = Template::new(fixture("demo_multi_config").to_str().unwrap());
    assert_err!(
        template.config_data(),
        CopierError::MultipleConfigFiles { .. }
    );
}

#[test]
#[ignore = "pending"]
fn test_missing_template() {
    let tmp = Tmp::new();
    assert_err!(
        worker("./i_do_not_exist", tmp.path()).run_copy(),
        CopierError::Value(_)
    );
}

#[test]
#[ignore = "pending"]
fn test_worker_good_data() {
    // This test is probably useless, as it tests the what and not the how
    let tmp = Tmp::new();
    let conf = worker(fixture("demo_data"), tmp.path());
    assert_eq!(
        conf.render_context().unwrap()[&Value::from("_folder_name")],
        Value::from(tmp.path().file_name().unwrap().to_str().unwrap())
    );
    assert_eq!(conf.all_exclusions().unwrap(), vec!["exclude1", "exclude2"]);
    assert_eq!(
        conf.template().unwrap().skip_if_exists().unwrap(),
        vec!["skip_if_exists1", "skip_if_exists2"]
    );
    assert_eq!(
        conf.template().unwrap().tasks().unwrap(),
        vec![stage_task("touch 1"), stage_task("touch 2")]
    );
}

#[rstest]
// func_args > defaults
#[case(PathBuf::from("."), &["aaa"], with_default_exclude(&["aaa"]))]
// func_args > user_data
#[case(
    fixture("demo_data"),
    &["aaa"],
    vec!["exclude1".to_owned(), "exclude2".to_owned(), "aaa".to_owned()]
)]
#[ignore = "pending"]
fn test_worker_config_precedence(
    #[case] src_path: PathBuf,
    #[case] exclude: &[&str],
    #[case] expected_exclusions: Vec<String>,
) {
    let tmp = Tmp::new();
    let conf = Worker::new()
        .src_path(src_path)
        .dst_path(tmp.path())
        .vcs_ref("HEAD")
        .exclude(exclude.to_vec());
    assert_eq!(expected_exclusions, conf.all_exclusions().unwrap());
}

#[test]
#[ignore = "pending"]
fn test_config_data_transclusion() {
    let config = Worker::new().src_path(fixture("demo_transclude"));
    assert_eq!(
        config.all_exclusions().unwrap(),
        vec!["exclude1", "exclude2"]
    );
}

#[test]
#[ignore = "pending"]
fn test_config_data_nested_transclusions() {
    let config = Worker::new().src_path(fixture("demo_transclude_nested_include"));
    assert_eq!(
        config.all_exclusions().unwrap(),
        vec!["exclude1", "exclude2"]
    );
}

#[rstest]
// Nothing overridden.
#[case(
    json!({}),
    json!({}),
    indoc! {"
        A string: lorem ipsum
        A number: 12345
        A boolean: True
        A list: one, two, three
    "}
)]
// User defaults provided.
#[case(
    json!({
        "a_string": "foo",
        "a_number": 42,
        "a_boolean": false,
        "a_list": ["four", "five", "six"],
    }),
    json!({}),
    indoc! {"
        A string: foo
        A number: 42
        A boolean: False
        A list: four, five, six
    "}
)]
// User defaults + data provided.
#[case(
    json!({
        "a_string": "foo",
        "a_number": 42,
        "a_boolean": false,
        "a_list": ["four", "five", "six"],
    }),
    json!({
        "a_string": "yosemite",
    }),
    indoc! {"
        A string: yosemite
        A number: 42
        A boolean: False
        A list: four, five, six
    "}
)]
#[ignore = "pending"]
fn test_user_defaults(
    #[case] user_defaults: serde_json::Value,
    #[case] data: serde_json::Value,
    #[case] expected: &str,
) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yaml") => indoc! {"
            a_string:
                default: lorem ipsum
                type: str
            a_number:
                default: 12345
                type: int
            a_boolean:
                default: true
                type: bool
            a_list:
                default:
                    - one
                    - two
                    - three
                type: json
        "},
        src.join("user_data.txt.jinja") => indoc! {r#"
            A string: {{ a_string }}
            A number: {{ a_number }}
            A boolean: {{ a_boolean }}
            A list: {{ ", ".join(a_list) }}
        "#},
        src.join(ANSWERS_FILE_TMPL) => indoc! {"
            # Changes here will be overwritten by Copier
            {{ _copier_answers|to_nice_yaml }}
        "},
    };
    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .user_defaults(user_defaults)
        .data(data)
        .run_copy()
        .unwrap();
    assert_eq!(read(dst.join("user_data.txt")), expected);
}

#[rstest]
// Initial user defaults and updated used defaults. The output
// should remain unchanged following the update operation.
#[case(
    json!({"a_string": "foo"}),
    json!({"a_string": "foobar"}),
    json!({}),
    json!({}),
    json!({}),
    "A string: foo\n",
    "A string: foo\n"
)]
// Settings defaults takes precedence over initial defaults.
// The output should remain unchanged following the update operation.
#[case(
    json!({}),
    json!({}),
    json!({}),
    json!({}),
    json!({"a_string": "bar"}),
    "A string: bar\n",
    "A string: bar\n"
)]
// User provided defaults takes precedence over initial defaults and settings
// defaults.
// The output should remain unchanged following the update operation.
#[case(
    json!({"a_string": "foo"}),
    json!({"a_string": "foobar"}),
    json!({}),
    json!({}),
    json!({"a_string": "bar"}),
    "A string: foo\n",
    "A string: foo\n"
)]
// User defaults + data provided. Provided data should take precedence
// and the resulting content unchanged post-update.
#[case(
    json!({"a_string": "foo"}),
    json!({"a_string": "foobar"}),
    json!({"a_string": "yosemite"}),
    json!({}),
    json!({"a_string": "bar"}),
    "A string: yosemite\n",
    "A string: yosemite\n"
)]
// User defaults + secondary defaults + data overrides. `data_updated` should
// override user and template defaults.
#[case(
    json!({"a_string": "foo"}),
    json!({"a_string": "foobar"}),
    json!({"a_string": "yosemite"}),
    json!({"a_string": "red rocks"}),
    json!({"a_string": "bar"}),
    "A string: yosemite\n",
    "A string: red rocks\n"
)]
#[ignore = "pending"]
fn test_user_defaults_updated(
    #[case] user_defaults_initial: serde_json::Value,
    #[case] user_defaults_updated: serde_json::Value,
    #[case] data_initial: serde_json::Value,
    #[case] data_updated: serde_json::Value,
    #[case] settings_defaults: serde_json::Value,
    #[case] expected_initial: &str,
    #[case] expected_updated: &str,
) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yaml") => indoc! {"
            a_string:
                default: lorem ipsum
                type: str
        "},
        src.join("user_data.txt.jinja") => indoc! {"
            A string: {{ a_string }}
        "},
        src.join(ANSWERS_FILE_TMPL) => indoc! {"
            # Changes here will be overwritten by Copier
            {{ _copier_answers|to_nice_yaml }}
        "},
    };
    git_init(&src, "hello world");
    let settings = || Settings {
        defaults: mapping(settings_defaults.clone()),
        trust: vec![],
    };

    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .user_defaults(user_defaults_initial)
        .data(data_initial)
        .settings(settings())
        .run_copy()
        .unwrap();
    assert_eq!(read(dst.join("user_data.txt")), expected_initial);

    git_init(&dst, "hello world");

    Worker::new()
        .dst_path(&dst)
        .defaults(true)
        .overwrite(true)
        .user_defaults(user_defaults_updated)
        .data(data_updated)
        .settings(settings())
        .run_update()
        .unwrap();
    assert_eq!(read(dst.join("user_data.txt")), expected_updated);
}

#[rstest]
#[case(indoc! {"
    question:
        type: str
        secret: true
"})]
#[case(indoc! {"
    question:
        type: str

    _secret_questions:
        - question
"})]
#[ignore = "pending"]
fn test_secret_question_requires_default_value(#[case] config: &str) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => config,
    };
    assert_err!(
        worker(&src, &dst).run_copy(),
        CopierError::Value(_),
        "Secret question requires a default value"
    );
}
