//! Port of `tests/test_legacy_migration.py`.

mod common;

use std::path::Path;

use common::{
    assert_err, file_tree, fixture, git, load_answers, read, v, worker, Tmp, BRACKET_ENVOPS_JSON,
};
use copier::errors::{capture_warnings, WarningKind};
use copier::{CopierError, Value, Worker};
use indoc::formatdoc;
use rstest::rstest;
use serde_json::json;

/// Recursive copy of a directory tree (`shutil.copytree`).
fn copytree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copytree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// Whether any entry of `dir` has a name ending with `suffix` (`dir.glob(f"*{suffix}")`).
fn has_entry_ending_with(dir: &Path, suffix: &str) -> bool {
    std::fs::read_dir(dir).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(suffix)
    })
}

/// Check migrations and tasks are run properly.
// This fails on windows CI because, when the test tries to execute
// `migrations.py`, it doesn't understand that it should be interpreted
// by python.exe. Or maybe it fails because CI is using Git bash instead
// of WSL bash, which happened to work fine in real world tests.
// FIXME Some generous Windows power user please fix this test!
// @pytest.mark.xfail(condition=platform.system() == "Windows", reason="Windows ignores shebang?", strict=True)
#[rstest]
fn test_migrations_and_tasks(#[values(true, false)] skip_tasks: bool) {
    // Convert demo_migrations in a git repository with 2 versions
    let tmp = Tmp::new();
    let src = tmp.path().join("src");
    let dst = tmp.path().join("dst");
    copytree(&fixture("demo_legacy_migrations"), &src);
    git(&src, &["init"]);
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m1"]);
    git(&src, &["tag", "v1.0.0"]);
    git(&src, &["commit", "--allow-empty", "-m2"]);
    git(&src, &["tag", "v2.0"]);
    // Copy it in v1
    worker(&src, &dst)
        .vcs_ref("v1.0.0")
        .unsafe_(true)
        .skip_tasks(skip_tasks)
        .run_copy()
        .unwrap();
    // Check copy was OK
    if skip_tasks {
        assert!(!dst.join("created-with-tasks.txt").exists());
        assert!(dst.join("delete-in-tasks.txt").exists());
    } else {
        assert_eq!(read(dst.join("created-with-tasks.txt")), "task 1\ntask 2\n");
        assert!(!dst.join("delete-in-tasks.txt").exists());
    }
    assert!(dst.join("delete-in-migration-v2.txt").is_file());
    assert!(!dst.join("migrations.py").exists());
    assert!(!dst.join("tasks.py").exists());
    assert!(!has_entry_ending_with(&dst, "-before.txt"));
    assert!(!has_entry_ending_with(&dst, "-after.txt"));
    let answers = load_answers(&dst);
    assert_eq!(
        answers,
        v(json!({"_commit": "v1.0.0", "_src_path": src.to_str().unwrap()}))
    );
    // Save changes in downstream repo
    git(&dst, &["init"]);
    git(&dst, &["add", "."]);
    git(&dst, &["commit", "-m1"]);
    // Update it to v2
    let (result, warnings) = capture_warnings(|| {
        Worker::new()
            .dst_path(&dst)
            .defaults(true)
            .overwrite(true)
            .unsafe_(true)
            .skip_tasks(skip_tasks)
            .run_update()
    });
    result.unwrap();
    assert!(warnings.iter().any(|w| w.kind == WarningKind::Deprecation));
    // Check update was OK
    if skip_tasks {
        assert!(!dst.join("created-with-tasks.txt").exists());
        assert!(dst.join("delete-in-tasks.txt").exists());
    } else {
        assert_eq!(
            read(dst.join("created-with-tasks.txt")),
            "task 1\ntask 2\n".repeat(2)
        );
        assert!(!dst.join("delete-in-tasks.txt").exists());
    }
    assert!(!dst.join("delete-in-migration-v2.txt").exists());
    assert!(!dst.join("migrations.py").exists());
    assert!(!dst.join("tasks.py").exists());
    assert!(dst.join("v1.0.0-v2-v2.0-before.json").is_file());
    assert!(dst.join("v1.0.0-v2-v2.0-after.json").is_file());
    assert!(dst.join("PEP440-1.0.0-2-2.0-before.json").is_file());
    assert!(dst.join("PEP440-1.0.0-2-2.0-after.json").is_file());
    let answers = load_answers(&dst);
    assert_eq!(
        answers,
        v(json!({"_commit": "v2.0", "_src_path": src.to_str().unwrap()}))
    );
}

/// Test support for answers modifications in pre-migrations.
#[test]
fn test_pre_migration_modifies_answers() {
    let tmp = Tmp::new();
    let src = tmp.mktemp("src");
    let dst = tmp.mktemp("dst");

    // v1 of template asks for a favourite song and writes it to songs.json
    file_tree! {
        src.join("[[ _copier_conf.answers_file ]].jinja") => "[[ _copier_answers|tojson ]]",
        src.join("copier.yml") => formatdoc! {"
            _envops: {BRACKET_ENVOPS_JSON}
            best_song: la vie en rose
        "},
        src.join("songs.json.jinja") => "[ [[ best_song|tojson ]] ]",
    };
    git(&src, &["init"]);
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m1"]);
    git(&src, &["tag", "v1"]);
    // User copies v1 template into subproject
    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    let answers = load_answers(&dst);
    assert_eq!(answers["_commit"], Value::from("v1"));
    assert_eq!(answers["best_song"], Value::from("la vie en rose"));
    let songs: serde_json::Value = serde_json::from_str(&read(dst.join("songs.json"))).unwrap();
    assert_eq!(songs, json!(["la vie en rose"]));
    git(&dst, &["init"]);
    git(&dst, &["add", "."]);
    git(&dst, &["commit", "-m1"]);
    // v2 of template supports multiple songs, has a different default
    // and includes a data format migration script
    file_tree! {
        src.join("copier.yml") => formatdoc! {r#"
            _envops: {BRACKET_ENVOPS_JSON}
            best_song_list:
                default: [paranoid android]
            _migrations:
            -   version: v2
                before:
                -   - python
                    - -c
                    - |
                        import sys, json, pathlib
                        answers_path = pathlib.Path(*sys.argv[1:])
                        answers = json.loads(answers_path.read_text())
                        answers["best_song_list"] = [answers.pop("best_song")]
                        answers_path.write_text(json.dumps(answers))
                    - "[[ _copier_conf.dst_path ]]"
                    - "[[ _copier_conf.answers_file ]]"
        "#},
        src.join("songs.json.jinja") => "[[ best_song_list|tojson ]]",
    };
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m2"]);
    git(&src, &["tag", "v2"]);
    // User updates subproject to v2 template
    let (result, warnings) = capture_warnings(|| {
        Worker::new()
            .dst_path(&dst)
            .defaults(true)
            .overwrite(true)
            .unsafe_(true)
            .run_update()
    });
    result.unwrap();
    assert!(warnings.iter().any(|w| w.kind == WarningKind::Deprecation));
    let answers = load_answers(&dst);
    assert_eq!(answers["_commit"], Value::from("v2"));
    assert!(!answers.contains_key("best_song"));
    assert_eq!(answers["best_song_list"], v(json!(["la vie en rose"])));
    let songs: serde_json::Value = serde_json::from_str(&read(dst.join("songs.json"))).unwrap();
    assert_eq!(songs, json!(["la vie en rose"]));
}

/// Test prereleases support for copying and updating.
#[test]
fn test_prereleases() {
    let tmp = Tmp::new();
    let src = tmp.mktemp("src");
    let dst = tmp.mktemp("dst");
    // Build template in v1.0.0
    file_tree! {
        src.join("version.txt") => "v1.0.0",
        src.join("[[ _copier_conf.answers_file ]].jinja") => "[[_copier_answers|to_nice_yaml]]",
        src.join("copier.yaml") => formatdoc! {r#"
            _envops: {BRACKET_ENVOPS_JSON}
            _migrations:
            -   version: v1.9
                before:
                - [python, -c, "import pathlib; pathlib.Path('v1.9').touch()"]
            -   version: v2.dev0
                before:
                - [python, -c, "import pathlib; pathlib.Path('v2.dev0').touch()"]
            -   version: v2.dev2
                before:
                - [python, -c, "import pathlib; pathlib.Path('v2.dev2').touch()"]
            -   version: v2.a1
                before:
                - [python, -c, "import pathlib; pathlib.Path('v2.a1').touch()"]
            -   version: v2.a2
                before:
                - [python, -c, "import pathlib; pathlib.Path('v2.a2').touch()"]
        "#},
    };
    git(&src, &["init"]);
    git(&src, &["add", "."]);
    git(&src, &["commit", "-mv1"]);
    git(&src, &["tag", "v1.0.0"]);
    // Evolve template to v2.0.0.dev1
    file_tree! { src.join("version.txt") => "v2.0.0.dev1" };
    git(&src, &["commit", "-amv2dev1"]);
    git(&src, &["tag", "v2.0.0.dev1"]);
    // Evolve template to v2.0.0.alpha1
    file_tree! { src.join("version.txt") => "v2.0.0.alpha1" };
    git(&src, &["commit", "-amv2a1"]);
    git(&src, &["tag", "v2.0.0.alpha1"]);
    // Copying with use_prereleases=False copies v1
    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    let answers = load_answers(&dst);
    assert_eq!(answers["_commit"], Value::from("v1.0.0"));
    assert_eq!(read(dst.join("version.txt")), "v1.0.0");
    assert!(!dst.join("v1.9").exists());
    assert!(!dst.join("v2.dev0").exists());
    assert!(!dst.join("v2.dev2").exists());
    assert!(!dst.join("v2.a1").exists());
    assert!(!dst.join("v2.a2").exists());
    // Commit subproject
    git(&dst, &["init"]);
    git(&dst, &["add", "."]);
    git(&dst, &["commit", "-mv1"]);
    // Update it without prereleases; nothing changes
    let (result, warnings) = capture_warnings(|| {
        Worker::new()
            .dst_path(&dst)
            .defaults(true)
            .overwrite(true)
            .run_update()
    });
    result.unwrap();
    assert!(warnings.iter().any(|w| w.kind == WarningKind::Deprecation));
    assert_eq!(git(&dst, &["status", "--porcelain"]), "");
    assert!(!dst.join("v1.9").exists());
    assert!(!dst.join("v2.dev0").exists());
    assert!(!dst.join("v2.dev2").exists());
    assert!(!dst.join("v2.a1").exists());
    assert!(!dst.join("v2.a2").exists());
    // Update it with prereleases
    let (result, warnings) = capture_warnings(|| {
        Worker::new()
            .dst_path(&dst)
            .defaults(true)
            .overwrite(true)
            .use_prereleases(true)
            .unsafe_(true)
            .run_update()
    });
    result.unwrap();
    assert!(warnings.iter().any(|w| w.kind == WarningKind::Deprecation));
    let answers = load_answers(&dst);
    assert_eq!(answers["_commit"], Value::from("v2.0.0.alpha1"));
    assert_eq!(read(dst.join("version.txt")), "v2.0.0.alpha1");
    assert!(dst.join("v1.9").exists());
    assert!(dst.join("v2.dev0").exists());
    assert!(dst.join("v2.dev2").exists());
    assert!(dst.join("v2.a1").exists());
    assert!(!dst.join("v2.a2").exists());
    // It should fail if downgrading
    let (result, warnings) = capture_warnings(|| {
        Worker::new()
            .dst_path(&dst)
            .defaults(true)
            .overwrite(true)
            .run_update()
    });
    assert_err!(result, CopierError::UserMessage(_));
    assert!(warnings.iter().any(|w| w.kind == WarningKind::Deprecation));
}

/// Test pretend mode for legacy migrations.
#[test]
fn test_pretend_mode() {
    let tmp = Tmp::new();
    let src = tmp.mktemp("src");
    let dst = tmp.mktemp("dst");

    // Build template in v1
    git(&src, &["init"]);
    file_tree! {
        src.join("[[ _copier_conf.answers_file ]].jinja") => "[[_copier_answers|to_nice_yaml]]",
        src.join("copier.yml") => formatdoc! {"
            _envops: {BRACKET_ENVOPS_JSON}
        "},
    };
    git(&src, &["add", "."]);
    git(&src, &["commit", "-mv1"]);
    git(&src, &["tag", "v1"]);

    worker(&src, &dst).run_copy().unwrap();
    let answers = load_answers(&dst);
    assert_eq!(answers["_commit"], Value::from("v1"));

    git(&dst, &["init"]);
    git(&dst, &["add", "."]);
    git(&dst, &["commit", "-mv1"]);

    // Evolve template to v2
    file_tree! {
        src.join("[[ _copier_conf.answers_file ]].jinja") => "[[_copier_answers|to_nice_yaml]]",
        src.join("copier.yml") => formatdoc! {"
            _envops: {BRACKET_ENVOPS_JSON}
            _migrations:
            -   version: v2
                before:
                -   touch v2-before.txt
                after:
                -   touch v2-after.txt
        "},
    };
    git(&src, &["add", "."]);
    git(&src, &["commit", "-mv2"]);
    git(&src, &["tag", "v2"]);

    let (result, warnings) = capture_warnings(|| {
        Worker::new()
            .dst_path(&dst)
            .overwrite(true)
            .pretend(true)
            .unsafe_(true)
            .run_update()
    });
    result.unwrap();
    assert!(warnings.iter().any(|w| w.kind == WarningKind::Deprecation));
    let answers = load_answers(&dst);
    assert_eq!(answers["_commit"], Value::from("v1"));
    assert!(!dst.join("v2-before.txt").exists());
    assert!(!dst.join("v2-after.txt").exists());
}
