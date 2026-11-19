//! Port of `tests/test_migrations.py`.

mod common;

use common::{
    assert_err, file_tree, git, git_save, git_save_with, load_answers, read, worker, Tmp,
    ANSWERS_FILE_TMPL, BRACKET_ENVOPS_JSON,
};
use copier::{CopierError, Value, Worker};
use indoc::{formatdoc, indoc};
use rstest::rstest;
use serde_json::json;

/// Test a basic migration running on every version
#[test]
fn test_basic_migration() {
    let tmp = Tmp::new();
    let src = tmp.mktemp("src");
    let dst = tmp.mktemp("dst");

    file_tree! {
        src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|tojson }}",
        src.join("copier.yml") => indoc! {"
            _migrations:
                - touch foo
        "},
    };
    git_save(&src, "Test commit", Some("v1"));
    worker(&src, &dst).run_copy().unwrap();
    git_save(&dst, "Test commit", None);

    assert!(!dst.join("foo").exists()); // Migrations don't run on initial copy

    git(&src, &["tag", "v2"]);
    Worker::new()
        .dst_path(&dst)
        .defaults(true)
        .overwrite(true)
        .unsafe_(true)
        .run_update()
        .unwrap();

    assert!(dst.join("foo").is_file());
}

/// Test a basic migration running on every version
#[rstest]
#[case(json!("touch foo"))]
#[case(json!(["touch", "foo"]))]
#[case(json!({"command": "touch foo"}))]
#[case(json!({"command": ["touch", "foo"]}))]
fn test_post_migration_by_default(
    #[case] command: serde_json::Value,
    #[values(false, true)] bracket_envops: bool,
) {
    let tmp = Tmp::new();
    let src = tmp.mktemp("src");
    let dst = tmp.mktemp("dst");

    let envops_json = if bracket_envops {
        BRACKET_ENVOPS_JSON
    } else {
        "{}"
    };
    let (v_start, v_end) = if bracket_envops {
        ("[[", "]]")
    } else {
        ("{{", "}}")
    };

    file_tree! {
        src.join(format!("{v_start} _copier_conf.answers_file {v_end}.jinja")) => format!("{v_start} _copier_answers|tojson {v_end}"),
        src.join("copier.yml") => formatdoc! {"
            _envops: {envops_json}
            _migrations:
                - {command}
        "},
    };
    git_save(&src, "Test commit", Some("v1"));
    worker(&src, &dst).run_copy().unwrap();
    git_save(&dst, "Test commit", None);

    assert!(!dst.join("foo").exists()); // Migrations don't run on initial copy

    git(&src, &["tag", "v2"]);
    Worker::new()
        .dst_path(&dst)
        .defaults(true)
        .overwrite(true)
        .unsafe_(true)
        .run_update()
        .unwrap();

    assert!(dst.join("foo").is_file());
}

/// Tests that migrations require the unsafe flag to be passed
#[test]
fn test_requires_unsafe() {
    let tmp = Tmp::new();
    let src = tmp.mktemp("src");
    let dst = tmp.mktemp("dst");

    file_tree! {
        src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|tojson }}",
        src.join("copier.yml") => indoc! {"
            _migrations:
                - touch foo
        "},
    };
    git_save(&src, "Test commit", Some("v1"));
    worker(&src, &dst).run_copy().unwrap();
    git_save(&dst, "Test commit", None);

    assert!(!dst.join("foo").exists()); // Migrations don't run on initial copy

    git(&src, &["tag", "v2"]);
    assert_err!(
        Worker::new()
            .dst_path(&dst)
            .defaults(true)
            .overwrite(true)
            .unsafe_(false)
            .run_update(),
        CopierError::UnsafeTemplate { .. }
    );

    assert!(!dst.join("foo").exists());
}

/// Test a migration running on a specific version
#[test]
fn test_version_migration() {
    let tmp = Tmp::new();
    let src = tmp.mktemp("src");
    let dst = tmp.mktemp("dst");

    file_tree! {
        src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|tojson }}",
        src.join("copier.yml") => indoc! {"
            _migrations:
            -   version: v3
                command: touch foo
        "},
    };
    git_save(&src, "Test commit", Some("v1"));
    worker(&src, &dst).run_copy().unwrap();

    assert!(!dst.join("foo").exists()); // Migrations don't run on initial copy

    for i in 2..5 {
        let tag = format!("v{i}");
        git_save_with(&src, "Test commit", Some(tag.as_str()), true);
        git_save(&dst, "Test commit", None);
        Worker::new()
            .dst_path(&dst)
            .defaults(true)
            .overwrite(true)
            .unsafe_(true)
            .run_update()
            .unwrap();

        if i == 3 {
            assert!(dst.join("foo").is_file());
            std::fs::remove_file(dst.join("foo")).unwrap();
        } else {
            assert!(!dst.join("foo").exists());
        }
    }
}

/// Test if prerelease version migrations work
#[test]
fn test_prerelease_version_migration() {
    let tmp = Tmp::new();
    let src = tmp.mktemp("src");
    let dst = tmp.mktemp("dst");

    let versions = ["v2.dev0", "v2.dev2", "v2.a1", "v2.a2"];

    file_tree! {
        src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|tojson }}",
        src.join("copier.yml") => indoc! {"
            _migrations:
            -   version: v1.9
                command: touch v1.9
            -   version: v2.dev0
                command: touch v2.dev0
            -   version: v2.dev2
                command: touch v2.dev2
            -   version: v2.a1
                command: touch v2.a1
            -   version: v2.a2
                command: touch v2.a2
        "},
    };
    git_save(&src, "Test commit", Some("v1"));
    worker(&src, &dst).run_copy().unwrap();
    for version in std::iter::once("v1.9").chain(versions) {
        git_save_with(&src, "Test commit", Some(version), true);
    }

    assert!(!dst.join("v1.9").exists());
    assert!(versions.iter().all(|version| !dst.join(version).exists()));

    git_save(&dst, "Test commit", None);
    // No pre-releases. Should update to v1.9
    Worker::new()
        .dst_path(&dst)
        .defaults(true)
        .overwrite(true)
        .unsafe_(true)
        .run_update()
        .unwrap();

    let answers = load_answers(&dst);
    assert_eq!(answers["_commit"], Value::from("v1.9"));
    assert!(dst.join("v1.9").exists());
    assert!(versions.iter().all(|version| !dst.join(version).exists()));

    git_save(&dst, "Test commit", None);
    // With pre-releases. Should update to v2.a2
    Worker::new()
        .dst_path(&dst)
        .defaults(true)
        .overwrite(true)
        .unsafe_(true)
        .use_prereleases(true)
        .run_update()
        .unwrap();

    let answers = load_answers(&dst);
    assert_eq!(answers["_commit"], Value::from("v2.a2"));
    assert!(dst.join("v1.9").exists());
    assert!(versions.iter().all(|version| dst.join(version).exists()));

    git_save(&dst, "Test commit", None);
    // Can't downgrade
    assert_err!(
        Worker::new()
            .dst_path(&dst)
            .defaults(true)
            .overwrite(true)
            .unsafe_(true)
            .run_update(),
        CopierError::UserMessage(_)
    );
}

/// Test the working directory attribute of migrations
#[test]
fn test_migration_working_directory() {
    let tmp = Tmp::new();
    let src = tmp.mktemp("src");
    let dst = tmp.mktemp("dst");
    let workdir = tmp.mktemp("workdir");
    let workdir_str = workdir.to_str().unwrap();

    file_tree! {
        src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|tojson }}",
        src.join("copier.yml") => formatdoc! {"
            _migrations:
            -   command: touch foo
                working_directory: {workdir_str}
        "},
    };
    git_save(&src, "Test commit", Some("v1"));
    worker(&src, &dst).run_copy().unwrap();
    git_save(&dst, "Test commit", None);

    assert!(!workdir.join("foo").exists()); // Migrations don't run on initial copy

    git(&src, &["tag", "v2"]);
    Worker::new()
        .dst_path(&dst)
        .defaults(true)
        .overwrite(true)
        .unsafe_(true)
        .run_update()
        .unwrap();

    assert!(!dst.join("foo").exists());
    assert!(workdir.join("foo").is_file());
}

/// Test the `when` argument of migrations
#[rstest]
fn test_migration_condition(#[values(true, false)] condition: bool) {
    let tmp = Tmp::new();
    let src = tmp.mktemp("src");
    let dst = tmp.mktemp("dst");

    let when = condition.to_string();
    file_tree! {
        src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|tojson }}",
        src.join("copier.yml") => formatdoc! {"
            _migrations:
            -   command: touch foo
                when: {when}
        "},
    };
    git_save(&src, "Test commit", Some("v1"));
    worker(&src, &dst).run_copy().unwrap();
    git_save(&dst, "Test commit", None);

    assert!(!dst.join("foo").exists()); // Migrations don't run on initial copy

    git(&src, &["tag", "v2"]);
    Worker::new()
        .dst_path(&dst)
        .defaults(true)
        .overwrite(true)
        .unsafe_(true)
        .run_update()
        .unwrap();

    assert_eq!(dst.join("foo").is_file(), condition);
}

/// Test that migrations aren't run in pretend mode
#[test]
fn test_pretend_migration() {
    let tmp = Tmp::new();
    let src = tmp.mktemp("src");
    let dst = tmp.mktemp("dst");

    file_tree! {
        src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|tojson }}",
        src.join("copier.yml") => indoc! {"
            _migrations:
            -   touch foo
        "},
    };
    git_save(&src, "Test commit", Some("v1"));
    worker(&src, &dst).run_copy().unwrap();
    git_save(&dst, "Test commit", None);

    assert!(!dst.join("foo").exists()); // Migrations don't run on initial copy

    git(&src, &["tag", "v2"]);
    Worker::new()
        .dst_path(&dst)
        .defaults(true)
        .overwrite(true)
        .unsafe_(true)
        .pretend(true)
        .run_update()
        .unwrap();

    assert!(!dst.join("foo").exists()); // In pretend mode the command shouldn't run
}

/// Test that migrations aren't run in pretend mode
#[test]
fn test_skip_migration() {
    let tmp = Tmp::new();
    let src = tmp.mktemp("src");
    let dst = tmp.mktemp("dst");

    file_tree! {
        src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|tojson }}",
        src.join("copier.yml") => indoc! {"
            _migrations:
            -   touch foo
        "},
    };
    git_save(&src, "Test commit", Some("v1"));
    worker(&src, &dst).run_copy().unwrap();
    git_save(&dst, "Test commit", None);

    assert!(!dst.join("foo").exists()); // Migrations don't run on initial copy

    git(&src, &["tag", "v2"]);
    Worker::new()
        .dst_path(&dst)
        .defaults(true)
        .overwrite(true)
        .unsafe_(true)
        .skip_tasks(true)
        .run_update()
        .unwrap();

    assert!(dst.join("foo").exists()); // Migrations are not skipped by skip_tasks
}

/// Test running migrations in the before upgrade step
#[test]
fn test_migration_run_before() {
    let tmp = Tmp::new();
    let src = tmp.mktemp("src");
    let dst = tmp.mktemp("dst");

    file_tree! {
        src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|tojson }}",
        // We replace an answer in the before step, so it will be used during
        // update
        src.join("copier.yml") => indoc! {r#"
            hello:
                default: World
            _migrations:
            -   command:
                -   "{{ _copier_python }}"
                -   -c
                -   |
                    import yaml
                    with open(".copier-answers.yml", "r") as f:
                        v = yaml.safe_load(f)
                    v["hello"] = "Copier"
                    with open(".copier-answers.yml", "w") as f:
                        yaml.safe_dump(v, f)
                when: "{{ _stage == 'before' }}"
        "#},
        src.join("foo.jinja") => "Hello {{ hello }}",
    };
    git_save(&src, "Test commit", Some("v1"));
    worker(&src, &dst).defaults(true).run_copy().unwrap();
    git_save(&dst, "Test commit", None);

    assert!(dst.join("foo").is_file());
    assert_eq!(read(dst.join("foo")), "Hello World");

    file_tree! { src.join("foo") => "" };
    git_save(&src, "Test commit", Some("v2"));
    Worker::new()
        .dst_path(&dst)
        .defaults(true)
        .overwrite(true)
        .unsafe_(true)
        .run_update()
        .unwrap();

    assert!(dst.join("foo").is_file());
    assert_eq!(read(dst.join("foo")), "Hello Copier");
}

/// Test running migrations in the before upgrade step
/// Also checks that this is the default behaviour if no `when` is given.
#[rstest]
fn test_migration_run_after(#[values(true, false)] explicit: bool) {
    let tmp = Tmp::new();
    let src = tmp.mktemp("src");
    let dst = tmp.mktemp("dst");

    let when_line = if explicit {
        r#"when: "{{ _stage == 'after' }}""#
    } else {
        ""
    };
    file_tree! {
        src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|tojson }}",
        src.join("copier.yml") => formatdoc! {"
            _migrations:
            -   command: mv foo bar
                {when_line}
        "},
    };
    git_save(&src, "Test commit", Some("v1"));
    worker(&src, &dst).run_copy().unwrap();
    git_save(&dst, "Test commit", None);

    file_tree! { src.join("foo") => "" };
    git_save(&src, "Test commit", Some("v2"));
    Worker::new()
        .dst_path(&dst)
        .defaults(true)
        .overwrite(true)
        .unsafe_(true)
        .run_update()
        .unwrap();

    assert!(!dst.join("foo").exists());
    assert!(dst.join("bar").is_file());
}

/// Test that environment variables are passed to the migration commands
#[rstest]
fn test_migration_env_variables(#[values(true, false)] with_version: bool) {
    let tmp = Tmp::new();
    let src = tmp.mktemp("src");
    let dst = tmp.mktemp("dst");

    let variables = [
        ("STAGE", "after"),
        ("VERSION_FROM", "v1"),
        ("VERSION_TO", "v3"),
        ("VERSION_PEP440_FROM", "1"),
        ("VERSION_PEP440_TO", "3"),
    ];
    let current_only_variables = [("VERSION_CURRENT", "v2"), ("VERSION_PEP440_CURRENT", "2")];

    let version_line = if with_version { "version: v2" } else { "" };
    file_tree! {
        src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|tojson }}",
        src.join("copier.yml") => formatdoc! {"
            _migrations:
            -   command: env > env.txt
                {version_line}
        "},
    };
    git_save(&src, "Test commit", Some("v1"));
    worker(&src, &dst).run_copy().unwrap();
    git_save(&dst, "Test commit", None);

    assert!(!dst.join("foo").exists()); // Migrations don't run on initial copy

    file_tree! { src.join("version") => "v2" };
    git_save(&src, "Test commit", Some("v2"));
    file_tree! { src.join("version") => "v3" };
    git_save(&src, "Test commit", Some("v3"));
    Worker::new()
        .dst_path(&dst)
        .defaults(true)
        .overwrite(true)
        .unsafe_(true)
        .run_update()
        .unwrap();

    assert!(dst.join("env.txt").is_file());
    let content = read(dst.join("env.txt"));
    let env: Vec<&str> = content.split('\n').collect();
    for (variable, value) in variables {
        assert!(env.contains(&format!("{variable}={value}").as_str()));
    }

    for (variable, value) in current_only_variables {
        assert_eq!(
            env.contains(&format!("{variable}={value}").as_str()),
            with_version
        );
    }
}

/// Test that environment variables are passed to the migration commands
#[rstest]
fn test_migration_jinja_variables(#[values(true, false)] with_version: bool) {
    let tmp = Tmp::new();
    let src = tmp.mktemp("src");
    let dst = tmp.mktemp("dst");

    let variables = [
        ("_stage", "after"),
        ("_version_from", "v1"),
        ("_version_to", "v3"),
        ("_version_pep440_from", "1"),
        ("_version_pep440_to", "3"),
    ];
    let current_only_variables = [("_version_current", "v2"), ("_version_pep440_current", "2")];
    let all_variables: Vec<&str> = variables
        .iter()
        .chain(&current_only_variables)
        .map(|(var, _)| *var)
        .collect();

    let command = all_variables
        .iter()
        .map(|var| format!("echo {var}={{{{ {var} }}}} >> vars.txt"))
        .collect::<Vec<_>>()
        .join("&&");

    let version_line = if with_version { "version: v2" } else { "" };
    file_tree! {
        src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|tojson }}",
        src.join("copier.yml") => formatdoc! {"
            _migrations:
            -   command: {command}
                {version_line}
        "},
    };
    git_save(&src, "Test commit", Some("v1"));
    worker(&src, &dst).run_copy().unwrap();
    git_save(&dst, "Test commit", None);

    assert!(!dst.join("foo").exists()); // Migrations don't run on initial copy

    file_tree! { src.join("version") => "v2" };
    git_save(&src, "Test commit", Some("v2"));
    file_tree! { src.join("version") => "v3" };
    git_save(&src, "Test commit", Some("v3"));
    Worker::new()
        .dst_path(&dst)
        .defaults(true)
        .overwrite(true)
        .unsafe_(true)
        .run_update()
        .unwrap();

    assert!(dst.join("vars.txt").is_file());
    let content = read(dst.join("vars.txt"));
    let vars: Vec<&str> = content.split('\n').map(str::trim).collect();
    for (variable, value) in variables {
        assert!(vars.contains(&format!("{variable}={value}").as_str()));
    }

    for (variable, value) in current_only_variables {
        if with_version {
            assert!(vars.contains(&format!("{variable}={value}").as_str()));
        } else {
            assert!(vars.contains(&format!("{variable}=").as_str()));
        }
    }
}

/// Test that the Phase variable is properly set.
#[test]
fn test_copier_phase_variable() {
    let tmp = Tmp::new();
    let src = tmp.mktemp("src");
    let dst = tmp.mktemp("dst");

    file_tree! {
        src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|tojson }}",
        src.join("copier.yml") => indoc! {"
            _migrations:
                - touch {{ _copier_phase }}
        "},
    };
    git_save(&src, "Test commit", Some("v1"));
    worker(&src, &dst).run_copy().unwrap();
    git_save(&dst, "Test commit", None);

    git(&src, &["tag", "v2"]);
    Worker::new()
        .dst_path(&dst)
        .defaults(true)
        .overwrite(true)
        .unsafe_(true)
        .run_update()
        .unwrap();

    assert!(dst.join("migrate").is_file());
}
