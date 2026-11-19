//! Port of `tests/test_cleanup.py`.

mod common;

use std::path::{Path, PathBuf};

use common::{assert_err, file_tree, fixture, git_save, worker, Tmp, ANSWERS_FILE_TMPL};
use copier::errors::TemplateErrorKind;
use copier::{CopierError, Worker};
use indoc::indoc;

/// Interrupts are recorded process-wide, so the tests of this file take turns:
/// a test that triggers one must not interrupt a test running beside it.
fn serial() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Every path below `dir`, recursively (`dir.glob("**/*")`).
fn tree_entries(dir: &Path) -> Vec<PathBuf> {
    let mut entries = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(current) = pending.pop() {
        for entry in std::fs::read_dir(&current).expect("read dir") {
            let path = entry.expect("read entry").path();
            if path.is_dir() {
                pending.push(path.clone());
            }
            entries.push(path);
        }
    }
    entries
}

/// A `copier.yml` whose only task sends SIGINT to this test process.
fn sigint_tasks() -> String {
    indoc! {r#"
        _tasks:
            - "{{ _copier_python }} -c 'import os, signal; os.kill(PID, signal.SIGINT)'"
    "#}
    .replace("PID", &std::process::id().to_string())
}

#[test]
fn test_cleanup() {
    let _serial = serial();
    // Copier creates dst_path, fails to copy and removes it.
    let tmp = Tmp::new();
    let dst = tmp.path().join("new_folder");
    assert_err!(
        worker(fixture("demo_cleanup"), &dst)
            .quiet(true)
            .unsafe_(true)
            .run_copy(),
        CopierError::Task { .. }
    );
    assert!(!dst.exists());
}

#[test]
fn test_do_not_cleanup() {
    let _serial = serial();
    // Copier creates dst_path, fails to copy and keeps it.
    let tmp = Tmp::new();
    let dst = tmp.path().join("new_folder");
    assert_err!(
        worker(fixture("demo_cleanup"), &dst)
            .quiet(true)
            .unsafe_(true)
            .cleanup_on_error(false)
            .run_copy(),
        CopierError::Task { .. }
    );
    assert!(dst.exists());
}

#[test]
fn test_no_cleanup_when_folder_existed() {
    let _serial = serial();
    // Copier will not delete a folder if it didn't create it.
    let tmp = Tmp::new();
    let preexisting_file = tmp.path().join("something");
    std::fs::write(&preexisting_file, "").unwrap();
    assert_err!(
        worker(fixture("demo_cleanup"), tmp.path())
            .quiet(true)
            .unsafe_(true)
            .cleanup_on_error(true)
            .run_copy(),
        CopierError::Task { .. }
    );
    assert!(tmp.path().exists());
    assert!(preexisting_file.exists());
}

#[test]
fn test_no_cleanup_when_template_in_parent_folder() {
    let _serial = serial();
    // Copier will not delete a local template in a parent folder.
    let tmp = Tmp::new();
    let src = tmp.mktemp("src");
    let dst = tmp.mktemp("dst");
    let cwd = tmp.mktemp("cwd");
    // Python ran from `cwd` with the relative source `../src`; the path is passed explicitly.
    worker(cwd.join("..").join("src"), &dst)
        .quiet(true)
        .run_copy()
        .unwrap();
    assert!(src.exists());
}

// Python: skipif win32 (`os.kill(pid, SIGINT)` is not supported on Windows).
#[cfg(unix)]
#[test]
fn test_cleanup_on_interrupt_during_copy_task() {
    // Test cleaning up when copy is interrupted during task.
    let _serial = serial();
    let root = Tmp::new();
    let (src, dst) = root.src_dst();
    let tmp = root.mktemp("tmp");

    file_tree! {
        src.join("copier.yml") => sigint_tasks(),
    };
    git_save(&src, "Test commit", Some("v1"));

    // Isolate the temp root so we only see clone dirs created by this test.
    let result =
        copier::tools::with_temp_root(&tmp, || worker(&src, &dst).unsafe_(true).run_copy());
    assert_err!(result, CopierError::Interrupted);

    assert!(tree_entries(&tmp).is_empty());
}

// Python: skipif win32 (`os.kill(pid, SIGINT)` is not supported on Windows).
#[cfg(unix)]
#[test]
fn test_cleanup_on_interrupt_during_update_task() {
    // Test cleaning up when update is interrupted during task.
    let _serial = serial();
    let root = Tmp::new();
    let (src, dst) = root.src_dst();
    let tmp = root.mktemp("tmp");

    file_tree! {
        src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|to_yaml }}",
    };
    git_save(&src, "Test commit", Some("v1"));

    file_tree! {
        src.join("copier.yml") => sigint_tasks(),
    };
    git_save(&src, "Test commit", Some("v2"));

    // Isolate the temp root so we only see clone dirs created by this test.
    copier::tools::with_temp_root(&tmp, || {
        worker(&src, &dst).vcs_ref("v1").run_copy().unwrap();
        git_save(&dst, "Test commit", None);

        let result = Worker::new()
            .dst_path(&dst)
            .overwrite(true)
            .unsafe_(true)
            .run_update();
        assert_err!(result, CopierError::Interrupted);
    });

    assert!(tree_entries(&tmp).is_empty());
}

#[test]
fn test_cleanup_on_interrupt_during_git_clone() {
    // Test cleaning up when copy is interrupted during git clone.
    let _serial = serial();
    let root = Tmp::new();
    let (src, dst) = root.src_dst();
    let tmp = root.mktemp("tmp");

    file_tree! {
        src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|to_yaml }}",
    };
    git_save(&src, "Test commit", Some("v1"));

    // Python patches `Popen` to raise SIGINT when `git clone` starts. Here the
    // interrupt is recorded up front, so it is noticed at the first git command
    // of the clone, after the temporary checkout directory has been created.
    let result = copier::tools::with_temp_root(&tmp, || {
        copier::interrupt::raise();
        worker(&src, &dst).run_copy()
    });
    assert_err!(result, CopierError::Interrupted);

    assert!(tree_entries(&tmp).is_empty());
}

#[test]
fn test_cleanup_does_not_mask_render_error() {
    let _serial = serial();
    // Copier reports the render error, not a failure from its own cleanup.
    let tmp = Tmp::new();
    let src = tmp.path().join("src");
    let dst = tmp.path().join("new_folder");
    file_tree! {
        src.join("template.txt.jinja") => "{{ not_a_question.value }}",
    };

    // The template's only file fails, so `dst` is never created and cleanup has
    // nothing to remove.
    assert_err!(
        worker(&src, &dst).quiet(true).run_copy(),
        CopierError::Template {
            kind: TemplateErrorKind::Undefined,
            ..
        },
        "'not_a_question' is undefined"
    );

    assert!(!dst.exists());
}
