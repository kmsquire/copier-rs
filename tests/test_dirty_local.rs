//! Port of `tests/test_dirty_local.py`.

mod common;

use std::io::Write;
use std::path::Path;

use common::{
    assert_err, build_file_tree, demo_data, file_tree, fixture, git, project_template, read,
    symlink, v, worker, Tmp,
};
use copier::errors::{capture_warnings, WarningKind};
use copier::{CopierError, Worker};
use indoc::indoc;
use serde_json::json;

/// Recursively copy `from` into `to` (`copytree` in Python).
fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[test]
#[ignore = "pending"]
fn test_copy() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    copy_tree(&project_template(), &src);

    git(&src, &["init"]);

    let (result, warnings) = capture_warnings(|| {
        worker(&src, &dst)
            .data(demo_data())
            .vcs_ref("HEAD")
            .quiet(true)
            .run_copy()
    });
    result.unwrap();
    assert!(warnings.iter().any(|w| w.kind == WarningKind::DirtyLocal));

    let generated = read(dst.join("pyproject.toml"));
    let control = read(fixture("reference_files").join("pyproject.toml"));
    assert_eq!(generated, control);

    // assert template still dirty
    assert!(!git(&src, &["status", "--porcelain"]).trim().is_empty());
}

#[test]
#[ignore = "pending"]
fn test_copy_with_tag() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("tracked") => "",
        src.join("untracked") => "",
    };
    git(&src, &["init"]);
    git(&src, &["add", "tracked"]);
    git(&src, &["commit", "-m1"]);
    git(&src, &["tag", "v1"]);
    worker(&src, &dst).run_copy().unwrap();
    assert!(dst.join("tracked").exists());
    assert!(!dst.join("untracked").exists());
}

#[test]
#[ignore = "pending"]
fn test_copy_with_tag_and_ref_head() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("tracked") => "",
        src.join("untracked") => "",
    };
    git(&src, &["init"]);
    git(&src, &["add", "tracked"]);
    git(&src, &["commit", "-m1"]);
    git(&src, &["tag", "v1"]);
    let (result, warnings) = capture_warnings(|| worker(&src, &dst).vcs_ref("HEAD").run_copy());
    result.unwrap();
    assert!(warnings.iter().any(|w| w.kind == WarningKind::DirtyLocal));
    assert!(dst.join("tracked").exists());
    assert!(dst.join("untracked").exists());
}

#[test]
#[ignore = "pending"]
fn test_copy_dirty_head() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("tracked") => "",
        src.join("untracked") => "",
    };
    git(&src, &["init"]);
    git(&src, &["add", "tracked"]);
    git(&src, &["commit", "-m1"]);
    let (result, warnings) = capture_warnings(|| worker(&src, &dst).vcs_ref("HEAD").run_copy());
    result.unwrap();
    assert!(warnings.iter().any(|w| w.kind == WarningKind::DirtyLocal));
    assert!(dst.join("tracked").exists());
    assert!(dst.join("untracked").exists());
}

#[test]
#[ignore = "needs-design: per-test git config"]
fn test_copy_dirty_head_with_gpg() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("tracked") => "",
        src.join("untracked") => "",
    };
    git(&src, &["init"]);
    git(&src, &["add", "tracked"]);
    git(&src, &["commit", "-m1"]);
    // TODO(port): gitconfig.set({"user.signinkey": "123456", "commit.gpgsign": "true"}) needs per-test git config
    let (result, warnings) = capture_warnings(|| worker(&src, &dst).vcs_ref("HEAD").run_copy());
    result.unwrap();
    assert!(warnings.iter().any(|w| w.kind == WarningKind::DirtyLocal));

    assert!(dst.join("tracked").exists());
    assert!(dst.join("untracked").exists());
}

#[test]
#[ignore = "pending"]
fn test_update() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    file_tree! {
        src.join(".copier-answers.yml.jinja") => indoc! {"
            # Changes here will be overwritten by Copier
            {{ _copier_answers|to_nice_yaml }}
            "},
        src.join("copier.yml") => indoc! {r#"
            _envops:
                "keep_trailing_newline": True
            "#},
        src.join("aaaa.txt") => "
                Lorem ipsum
                ",
        src.join("to_delete.txt") => "
                delete me.
                ",
        src.join("symlink.txt") => symlink("./to_delete.txt"),
    };

    git(&src, &["init"]);
    git(&src, &["add", "-A"]);
    git(&src, &["commit", "-m", "first commit on src"]);

    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();

    // test adding a file
    std::fs::write(src.join("test_file.txt"), "Test content").unwrap();

    // test updating a file
    let mut aaaa = std::fs::OpenOptions::new()
        .append(true)
        .open(src.join("aaaa.txt"))
        .unwrap();
    aaaa.write_all(b"dolor sit amet").unwrap();
    drop(aaaa);

    // test updating a symlink
    std::fs::remove_file(src.join("symlink.txt")).unwrap();
    build_file_tree(vec![(src.join("symlink.txt"), symlink("test_file.txt"))]);

    // test removing a file
    std::fs::remove_file(src.join("to_delete.txt")).unwrap();

    // dst must be vcs-tracked to use run_update
    git(&dst, &["init"]);
    git(&dst, &["add", "-A"]);
    git(&dst, &["commit", "-m", "first commit on dst"]);

    // make sure changes have not yet propagated
    assert!(!dst.join("test_file.txt").exists());

    assert_ne!(read(src.join("aaaa.txt")), read(dst.join("aaaa.txt")));

    let p1 = src.join("symlink.txt");
    let p2 = dst.join("symlink.txt");
    assert_ne!(read(&p1), read(&p2));

    assert!(dst.join("to_delete.txt").exists());

    let (result, warnings) = capture_warnings(|| {
        Worker::new()
            .dst_path(&dst)
            .defaults(true)
            .overwrite(true)
            .run_update()
    });
    result.unwrap();
    assert!(warnings.iter().any(|w| w.kind == WarningKind::DirtyLocal));

    // make sure changes propagate after update
    assert!(dst.join("test_file.txt").exists());

    assert_eq!(read(src.join("aaaa.txt")), read(dst.join("aaaa.txt")));

    let p1 = src.join("symlink.txt");
    let p2 = dst.join("symlink.txt");
    assert_eq!(read(&p1), read(&p2));
    assert!(!dst.join("symlink.txt").is_symlink());

    // HACK https://github.com/copier-org/copier/issues/461
    // TODO test file deletion on update
    // assert not (dst / "to_delete.txt").exists()
}

#[test]
#[ignore = "needs-design: per-test git config"]
fn test_update_with_gpg_sign() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    file_tree! {
        src.join(".copier-answers.yml.jinja") => indoc! {"
            # Changes here will be overwritten by Copier
            {{ _copier_answers|to_nice_yaml }}
            "},
        src.join("copier.yml") => indoc! {r#"
            _envops:
                "keep_trailing_newline": True
            "#},
        src.join("aaaa.txt") => "
                Lorem ipsum
                ",
    };

    git(&src, &["init"]);
    git(&src, &["add", "-A"]);
    git(&src, &["commit", "-m", "first commit on src"]);

    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();

    std::fs::write(src.join("test_file.txt"), "Test content").unwrap();
    std::fs::write(src.join("aaaa.txt"), "dolor sit amet").unwrap();

    // dst must be vcs-tracked to use run_update
    git(&dst, &["init"]);
    git(&dst, &["add", "-A"]);
    git(&dst, &["commit", "-m", "first commit on dst"]);

    // TODO(port): gitconfig.set({"user.signinkey": "123456", "commit.gpgsign": "true"}) needs per-test git config
    let (result, warnings) = capture_warnings(|| {
        Worker::new()
            .dst_path(&dst)
            .defaults(true)
            .overwrite(true)
            .run_update()
    });
    result.unwrap();
    assert!(warnings.iter().any(|w| w.kind == WarningKind::DirtyLocal));
}

#[test]
#[ignore = "pending"]
fn test_parallel_projects_in_subdirs() {
    let tmp = Tmp::new();
    let src = tmp.mktemp("src");
    let parent = tmp.mktemp("parent");
    let dst1 = parent.join("project1");
    let dst2 = parent.join("project2");

    // Create initial template
    file_tree! {
        src.join("template").join(".copier-answers.yml.jinja") => indoc! {"
            # Changes here will be overwritten by Copier
            {{ _copier_answers|to_nice_yaml }}
            "},
        src.join("copier.yml") => indoc! {"
            question:
                type: str
            _subdirectory: template
            "},
        src.join("template").join("file.txt.jinja") => "{{ question }}",
    };

    git(&src, &["init"]);
    git(&src, &["add", "-A"]);
    git(&src, &["commit", "-m", "initial template commit"]);

    git(&parent, &["init"]);

    // Create projects
    for dst in [&dst1, &dst2] {
        let name = dst.file_name().unwrap().to_str().unwrap();
        worker(&src, dst)
            .overwrite(true)
            .data(v(json!({"question": name})))
            .run_copy()
            .unwrap();
    }

    git(&parent, &["add", "-A"]);
    git(&parent, &["commit", "-m", "initial project commit"]);

    // Update first project, then the second without committing/stashing changes from
    // first
    for dst in [&dst1, &dst2] {
        let name = dst.file_name().unwrap().to_str().unwrap();
        assert_eq!(read(dst.join("file.txt")), name);
        Worker::new()
            .dst_path(dst)
            .overwrite(true)
            .data(v(json!({"question": format!("Updated {name}")})))
            .run_update()
            .unwrap();
        assert_eq!(read(dst.join("file.txt")), format!("Updated {name}"));
    }

    // Verify subdirectories are both now dirty
    let expected = "M project1/.copier-answers.yml\n M project1/file.txt\n M project2/.copier-answers.yml\n M project2/file.txt";
    assert_eq!(git(&parent, &["status", "--porcelain"]).trim(), expected);

    // Test failing on attempting to update our now dirty project subdirectories.
    for dst in [&dst1, &dst2] {
        let name = dst.file_name().unwrap().to_str().unwrap();
        assert_err!(
            Worker::new()
                .dst_path(dst)
                .overwrite(true)
                .data(v(json!({"question": format!("Updated {name} Again")})))
                .run_update(),
            CopierError::UserMessage(_),
            "Destination repository is dirty"
        );
        assert_eq!(read(dst.join("file.txt")), format!("Updated {name}"));
    }
}
