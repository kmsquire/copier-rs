//! Port of `tests/test_cleanup.py`.

mod common;

use std::path::{Path, PathBuf};

use common::{assert_err, file_tree, fixture, git_save, worker, Tmp, ANSWERS_FILE_TMPL};
use copier::errors::TemplateErrorKind;
use copier::CopierError;
use indoc::indoc;

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

#[test]
#[ignore = "needs-design: SIGINT from a task and tempfile.tempdir/TMPDIR isolation"]
fn test_cleanup_on_interrupt_during_copy_task() {
    // Test cleaning up when copy is interrupted during task.
    let root = Tmp::new();
    let (src, _dst) = root.src_dst();
    let tmp = root.mktemp("tmp");

    // TODO(port): monkeypatch tempfile.tempdir and TMPDIR to `tmp`; the Rust
    // crate has no hook for the temp root (`Tmp` uses the system temp dir).
    // monkeypatch.setattr(tempfile, "tempdir", str(tmp))
    // monkeypatch.setenv("TMPDIR", str(tmp))

    file_tree! {
        src.join("copier.yml") => sigint_tasks(),
    };
    git_save(&src, "Test commit", Some("v1"));

    // TODO(port): the task sends SIGINT to the test process itself, which would
    // kill the Rust test harness; the copy must raise KeyboardInterrupt instead.
    // with pytest.raises(KeyboardInterrupt):
    //     copier.run_copy(str(src), dst, unsafe=True)
    // worker(&src, &_dst).unsafe_(true).run_copy()

    assert!(tree_entries(&tmp).is_empty());
}

#[test]
#[ignore = "needs-design: SIGINT from a task and tempfile.tempdir/TMPDIR isolation"]
fn test_cleanup_on_interrupt_during_update_task() {
    // Test cleaning up when update is interrupted during task.
    let root = Tmp::new();
    let (src, dst) = root.src_dst();
    let tmp = root.mktemp("tmp");

    // TODO(port): monkeypatch tempfile.tempdir and TMPDIR to `tmp`; the Rust
    // crate has no hook for the temp root (`Tmp` uses the system temp dir).
    // monkeypatch.setattr(tempfile, "tempdir", str(tmp))
    // monkeypatch.setenv("TMPDIR", str(tmp))

    file_tree! {
        src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|to_yaml }}",
    };
    git_save(&src, "Test commit", Some("v1"));

    file_tree! {
        src.join("copier.yml") => sigint_tasks(),
    };
    git_save(&src, "Test commit", Some("v2"));

    worker(&src, &dst).vcs_ref("v1").run_copy().unwrap();
    git_save(&dst, "Test commit", None);

    // TODO(port): the task sends SIGINT to the test process itself, which would
    // kill the Rust test harness; the update must raise KeyboardInterrupt instead.
    // with pytest.raises(KeyboardInterrupt):
    //     copier.run_update(dst, overwrite=True, unsafe=True)
    // Worker::new().dst_path(&dst).overwrite(true).unsafe_(true).run_update()

    assert!(tree_entries(&tmp).is_empty());
}

#[test]
#[ignore = "needs-design: patches plumbing's Popen to raise SIGINT during git clone"]
fn test_cleanup_on_interrupt_during_git_clone() {
    // Test cleaning up when copy is interrupted during git clone.
    let root = Tmp::new();
    let (src, _dst) = root.src_dst();
    let tmp = root.mktemp("tmp");

    // TODO(port): monkeypatch tempfile.tempdir and TMPDIR to `tmp`; the Rust
    // crate has no hook for the temp root (`Tmp` uses the system temp dir).
    // monkeypatch.setattr(tempfile, "tempdir", str(tmp))
    // monkeypatch.setenv("TMPDIR", str(tmp))

    file_tree! {
        src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|to_yaml }}",
    };
    git_save(&src, "Test commit", Some("v1"));

    // TODO(port): no hook to intercept the git clone subprocess and raise SIGINT.
    // original_popen = subprocess.Popen
    //
    // def interrupting_popen(args, *a, **kw):
    //     cmd_list = list(args) if isinstance(args, (list, tuple)) else [args]
    //     is_git_clone = (
    //         len(cmd_list) >= 2
    //         and Path(str(cmd_list[0])).stem == "git"
    //         and "clone" in cmd_list
    //     )
    //     if is_git_clone:
    //         # `signal.raise_signal` works on both POSIX and Windows, whereas
    //         # `os.kill(os.getpid(), SIGINT)` would terminate the process on Windows
    //         # instead of raising `KeyboardInterrupt`.
    //         signal.raise_signal(signal.SIGINT)
    //     return original_popen(args, *a, **kw)
    //
    // `plumbum.machines.local` executes `from subprocess import Popen` at import
    // time, this is what we need to patch, but this module actually returns a
    // `LocalMachine` instance via plumbum's module-replacement magic. The real module
    // is accessible via `sys.modules`.
    // import plumbum.machines.local  # noqa: F401  (ensure module is loaded into sys.modules)
    //
    // monkeypatch.setattr(
    //     sys.modules["plumbum.machines.local"], "Popen", interrupting_popen
    // )

    // with pytest.raises(KeyboardInterrupt):
    //     copier.run_copy(str(src), dst)

    assert!(tree_entries(&tmp).is_empty());
}

#[test]
fn test_cleanup_does_not_mask_render_error() {
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
