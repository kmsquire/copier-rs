//! Port of `tests/test_tmpdir.py`.

mod common;

use std::path::{Path, PathBuf};

use common::{file_tree, git, read, run_cli, worker, Tmp, ANSWERS_FILE_TMPL};
use copier::tools::with_temp_root;
use copier::Worker;

/// A template with two tagged versions; `v2` is only added in the second one.
fn template(tmp: &Tmp) -> PathBuf {
    // V1 of the template
    let root = tmp.mktemp("template");
    file_tree! {
        root.join("copier.yml") => "favorite_app: Copier",
        root.join("fav.txt.jinja") => "{{ favorite_app }}",
        root.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|to_nice_yaml }}",
    };
    git(&root, &["init"]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-m", "Initial commit"]);
    git(&root, &["tag", "v1"]);
    // V2 of the template
    file_tree! {
        root.join("v2") => "true",
    };
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-m", "Second commit"]);
    git(&root, &["tag", "v2"]);
    root
}

/// Assert that `dir` exists and has no entries.
fn empty_dir(dir: &Path) {
    assert!(dir.is_dir());
    assert!(dir.exists());
    assert_eq!(std::fs::read_dir(dir).unwrap().count(), 0);
}

#[test]
fn test_api() {
    let root = Tmp::new();
    let template_path = template(&root);
    let dst = root.mktemp("dst");
    let tmp = root.mktemp("tmp");
    // Mock tmp dir to assert it ends up clean
    with_temp_root(&tmp, || {
        // Copy
        worker(&template_path, &dst)
            .vcs_ref("v1")
            .quiet(true)
            .defaults(true)
            .run_copy()
            .unwrap();
        assert_eq!(read(dst.join("fav.txt")), "Copier");
        assert!(!dst.join("v2").exists());
        git(&dst, &["init"]);
        git(&dst, &["add", "-A"]);
        git(&dst, &["commit", "-m", "Initial commit"]);
        empty_dir(&tmp);
        // Recopy
        Worker::new()
            .dst_path(&dst)
            .vcs_ref("v1")
            .quiet(true)
            .defaults(true)
            .run_recopy()
            .unwrap();
        assert_eq!(read(dst.join("fav.txt")), "Copier");
        assert!(!dst.join("v2").exists());
        empty_dir(&tmp);
        // Update
        Worker::new()
            .dst_path(&dst)
            .quiet(true)
            .defaults(true)
            .overwrite(true)
            .run_update()
            .unwrap();
        assert_eq!(read(dst.join("fav.txt")), "Copier");
        assert_eq!(read(dst.join("v2")), "true");
        empty_dir(&tmp);
    });
}

#[test]
fn test_cli() {
    let root = Tmp::new();
    let template_path = template(&root);
    let dst = root.mktemp("dst");
    let tmp = root.mktemp("tmp");
    let template_str = template_path.to_str().unwrap();
    let dst_str = dst.to_str().unwrap();
    // Mock tmp dir to assert it ends up clean
    with_temp_root(&tmp, || {
        // Copy
        let (retcode, _) = run_cli(&["copy", "-fqrv1", template_str, dst_str]);
        assert_eq!(retcode, 0);
        assert_eq!(read(dst.join("fav.txt")), "Copier");
        assert!(!dst.join("v2").exists());
        empty_dir(&tmp);
        git(&dst, &["init"]);
        git(&dst, &["add", "-A"]);
        git(&dst, &["commit", "-m", "Initial commit"]);
        // Recopy
        let (retcode, _) = run_cli(&["recopy", "-fqrv1", dst_str]);
        assert_eq!(retcode, 0);
        assert_eq!(read(dst.join("fav.txt")), "Copier");
        assert!(!dst.join("v2").exists());
        empty_dir(&tmp);
        // Update
        let (retcode, _) = run_cli(&["update", "-fq", dst_str]);
        assert_eq!(retcode, 0);
        assert_eq!(read(dst.join("fav.txt")), "Copier");
        assert_eq!(read(dst.join("v2")), "true");
        empty_dir(&tmp);
    });
}
