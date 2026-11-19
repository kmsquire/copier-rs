//! Port of `tests/test_unsafe.py`.

mod common;

use common::{assert_err, file_tree, git, load_answers, run_cli, worker, Tmp, ANSWERS_FILE_TMPL};
use copier::errors::{capture_warnings, WarningKind};
use copier::settings::with_locations;
use copier::{CopierError, Value, Worker};
use rstest::rstest;
use serde_json::json;

// Python's `tests.test_unsafe.JinjaExtension` cannot be loaded by a Rust binary: the
// extension is only a vehicle for "the template declares a Jinja extension", so a
// natively supported one stands in for it.

/// Expected outcome of a case: no error (`does_not_raise`), an `UnsafeTemplate` error
/// matching the pattern (`pytest.raises`), or a deprecation warning (`deprecated_call`).
#[derive(Debug, Clone, Copy)]
enum Expected {
    Ok,
    Unsafe(&'static str),
    Deprecation,
}

#[rstest]
#[case(json!({}), Expected::Ok)]
#[case(json!({"_tasks": []}), Expected::Ok)]
#[case(json!({"_tasks": ["touch task.txt"]}), Expected::Unsafe("Template uses potentially unsafe feature: tasks."))]
#[case(json!({"_migrations": []}), Expected::Ok)]
#[case(
    json!({
        "_migrations": [
            {
                "version": "v1",
                "when": "{{ _stage == 'before' }}",
                "command": "touch v1-before.txt",
            },
            {
                "version": "v1",
                "when": "{{ _stage == 'after' }}",
                "command": "touch v1-after.txt",
            },
        ]
    }),
    Expected::Ok
)]
#[case(json!({"_jinja_extensions": []}), Expected::Ok)]
#[case(json!({"_jinja_extensions": ["jinja2.ext.loopcontrols"]}), Expected::Unsafe("Template uses potentially unsafe feature: jinja_extensions."))]
#[case(
    json!({
        "_tasks": ["touch task.txt"],
        "_jinja_extensions": ["jinja2.ext.loopcontrols"],
    }),
    Expected::Unsafe("Template uses potentially unsafe features: jinja_extensions, tasks.")
)]
fn test_copy(
    #[values(false, true)] unsafe_: bool,
    #[case] spec: serde_json::Value,
    #[case] expected: Expected,
) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yaml") => spec.to_string(),
    };
    let result = worker(&src, &dst).unsafe_(unsafe_).run_copy();
    match expected {
        Expected::Unsafe(pattern) if !unsafe_ => {
            assert_err!(result, CopierError::UnsafeTemplate { .. }, pattern)
        }
        _ => {
            result.unwrap();
        }
    }
}

#[rstest]
fn test_copy_cli(
    #[values(false, true)] unsafe_: bool,
    #[values(false, true)] trusted_from_settings: bool,
) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    let (src_s, dst_s) = (src.to_str().unwrap(), dst.to_str().unwrap());
    file_tree! {
        src.join("copier.yaml") => json!({"_tasks": ["touch task.txt"]}).to_string(),
    };
    let config = tmp.path().join("config");
    std::fs::create_dir(&config).unwrap();
    let settings_path = config.join("settings.yml");
    if trusted_from_settings {
        std::fs::write(&settings_path, format!("trust: ['{}']", src.display())).unwrap();
    }

    let mut args = vec!["copy"];
    if unsafe_ {
        args.push("--UNSAFE");
    }
    args.extend([src_s, dst_s]);
    let (retcode, out) = with_locations(&settings_path, None, || run_cli(&args));
    if unsafe_ || trusted_from_settings {
        assert_eq!(retcode, 0);
    } else {
        assert_eq!(retcode, 4);
        assert!(out
            .stderr
            .contains("Template uses potentially unsafe feature: tasks."));
    }
}

#[rstest]
#[case(json!({}), json!({}), Expected::Ok)]
#[case(json!({"_tasks": []}), json!({"_tasks": []}), Expected::Ok)]
#[case(json!({"_tasks": ["touch task-old.txt"]}), json!({}), Expected::Unsafe("Template uses potentially unsafe feature: tasks."))]
#[case(json!({}), json!({"_tasks": ["touch task-new.txt"]}), Expected::Unsafe("Template uses potentially unsafe feature: tasks."))]
#[case(json!({"_tasks": ["touch task-old.txt"]}), json!({"_tasks": ["touch task-new.txt"]}), Expected::Unsafe("Template uses potentially unsafe feature: tasks."))]
#[case(json!({"_migrations": []}), json!({"_migrations": []}), Expected::Ok)]
#[case(
    json!({}),
    json!({
        "_migrations": [
            {
                "version": "v0",
                "command": "touch v0-before.txt",
                "when": "{{ _stage == 'before' }}",
            },
            {
                "version": "v0",
                "command": "touch v0-after.txt",
                "when": "{{ _stage == 'after' }}",
            },
        ]
    }),
    Expected::Ok
)]
// This case only exists on legacy migrations and raises a DeprecationWarning
#[case(
    json!({}),
    json!({"_migrations": [{"version": "v2", "before": [], "after": []}]}),
    Expected::Deprecation
)]
#[case(
    json!({}),
    json!({
        "_migrations": [
            {
                "version": "v2",
                "when": "{{ _stage == 'before' }}",
                "command": "touch v2-before.txt",
            }
        ]
    }),
    Expected::Unsafe("Template uses potentially unsafe feature: migrations.")
)]
#[case(
    json!({}),
    json!({
        "_migrations": [
            {
                "version": "v2",
                "when": "{{ _stage == 'after' }}",
                "command": "touch v2-after.txt",
            }
        ]
    }),
    Expected::Unsafe("Template uses potentially unsafe feature: migrations.")
)]
#[case(
    json!({}),
    json!({
        "_migrations": [
            {
                "version": "v2",
                "when": "{{ _stage == 'before' }}",
                "command": "touch v2-before.txt",
            },
            {
                "version": "v2",
                "when": "{{ _stage == 'after' }}",
                "command": "touch v2-after.txt",
            },
        ]
    }),
    Expected::Unsafe("Template uses potentially unsafe feature: migrations.")
)]
#[case(json!({"_jinja_extensions": []}), json!({"_jinja_extensions": []}), Expected::Ok)]
#[case(json!({"_jinja_extensions": ["jinja2.ext.loopcontrols"]}), json!({}), Expected::Unsafe("Template uses potentially unsafe feature: jinja_extensions."))]
#[case(json!({}), json!({"_jinja_extensions": ["jinja2.ext.loopcontrols"]}), Expected::Unsafe("Template uses potentially unsafe feature: jinja_extensions."))]
#[case(json!({"_jinja_extensions": ["jinja2.ext.loopcontrols"]}), json!({"_jinja_extensions": ["jinja2.ext.loopcontrols"]}), Expected::Unsafe("Template uses potentially unsafe feature: jinja_extensions"))]
#[case(
    json!({}),
    json!({
        "_tasks": ["touch task-new.txt"],
        "_migrations": [
            {
                "version": "v2",
                "when": "{{ _stage == 'before' }}",
                "command": "touch v2-before.txt",
            },
            {
                "version": "v2",
                "when": "{{ _stage == 'after' }}",
                "command": "touch v2-after.txt",
            },
        ],
        "_jinja_extensions": ["jinja2.ext.loopcontrols"],
    }),
    Expected::Unsafe("Template uses potentially unsafe features: jinja_extensions, migrations, tasks.")
)]
fn test_update(
    #[values(false, true)] unsafe_: bool,
    #[case] spec_old: serde_json::Value,
    #[case] spec_new: serde_json::Value,
    #[case] expected: Expected,
) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    file_tree! {
        src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers | to_nice_yaml }}",
        src.join("copier.yaml") => spec_old.to_string(),
    };
    git(&src, &["init"]);
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m1"]);
    git(&src, &["tag", "v1"]);

    worker(&src, &dst).unsafe_(true).run_copy().unwrap();
    assert_eq!(load_answers(&dst)["_commit"], Value::from("v1"));

    git(&dst, &["init"]);
    git(&dst, &["add", "."]);
    git(&dst, &["commit", "-m1"]);

    file_tree! {
        src.join("copier.yaml") => spec_new.to_string(),
    };
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m2", "--allow-empty"]);
    git(&src, &["tag", "v2"]);

    let (result, warnings) = capture_warnings(|| {
        Worker::new()
            .dst_path(&dst)
            .overwrite(true)
            .unsafe_(unsafe_)
            .run_update()
    });
    match expected {
        Expected::Unsafe(pattern) if !unsafe_ => {
            assert_err!(result, CopierError::UnsafeTemplate { .. }, pattern)
        }
        Expected::Deprecation if !unsafe_ => {
            result.unwrap();
            assert!(warnings.iter().any(|w| w.kind == WarningKind::Deprecation));
        }
        _ => {
            result.unwrap();
        }
    }
}

#[rstest]
#[case(None)]
#[case(Some("--trust"))]
#[case(Some("--UNSAFE"))]
fn test_update_cli(
    #[case] unsafe_arg: Option<&str>,
    #[values(false, true)] trusted_from_settings: bool,
) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    let (src_s, dst_s) = (src.to_str().unwrap(), dst.to_str().unwrap());
    let unsafe_ = unsafe_arg.is_some();

    file_tree! {
        src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers | to_nice_yaml }}",
        src.join("copier.yaml") => "",
    };
    git(&src, &["init"]);
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m1"]);
    git(&src, &["tag", "v1"]);

    let config = tmp.path().join("config");
    std::fs::create_dir(&config).unwrap();
    let settings_path = config.join("settings.yml");
    if trusted_from_settings {
        std::fs::write(&settings_path, format!("trust: ['{}']", src.display())).unwrap();
    }

    let copy_args: Vec<&str> = ["copy", src_s, dst_s]
        .into_iter()
        .chain(unsafe_arg)
        .collect();
    let (retcode, _) = with_locations(&settings_path, None, || run_cli(&copy_args));
    assert_eq!(retcode, 0);
    assert_eq!(load_answers(&dst)["_commit"], Value::from("v1"));

    git(&dst, &["init"]);
    git(&dst, &["add", "."]);
    git(&dst, &["commit", "-m1"]);

    file_tree! {
        src.join("copier.yaml") => json!({"_tasks": ["touch task-new.txt"]}).to_string(),
    };
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m2"]);
    git(&src, &["tag", "v2"]);

    let update_args: Vec<&str> = ["update", dst_s].into_iter().chain(unsafe_arg).collect();
    let (retcode, out) = with_locations(&settings_path, None, || run_cli(&update_args));
    if unsafe_ || trusted_from_settings {
        assert_eq!(retcode, 0);
    } else {
        assert_eq!(retcode, 4);
        assert!(out
            .stderr
            .contains("Template uses potentially unsafe feature: tasks."));
    }
}
