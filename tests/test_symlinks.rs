//! Port of `tests/test_symlinks.py`.

#![cfg(unix)]

mod common;

use std::path::Path;

use common::{assert_err, file_tree, git, read, symlink, v, worker, Tmp, ANSWERS_FILE_TMPL};
use copier::errors::{capture_warnings, WarningKind};
use copier::{CopierError, Worker};
use indoc::indoc;
use rstest::rstest;
use serde_json::json;

#[test]
#[ignore = "pending"]
fn test_copy_symlink() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    // Prepare repo bundle
    let repo = src.join("repo");
    file_tree! {
        repo.join("copier.yaml") => indoc! {"
            _preserve_symlinks: true
        "},
        repo.join("target.txt") => "Symlink target",
        repo.join("symlink.txt") => symlink("target.txt"),
    };

    git(&src, &["init"]);
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m", "hello world"]);

    worker(&repo, &dst)
        .defaults(true)
        .overwrite(true)
        .vcs_ref("HEAD")
        .run_copy()
        .unwrap();

    assert!(dst.join("target.txt").exists());
    assert!(dst.join("symlink.txt").exists());
    assert!(dst.join("symlink.txt").is_symlink());
    assert_eq!(
        std::fs::read_link(dst.join("symlink.txt")).unwrap(),
        Path::new("target.txt")
    );
}

#[test]
#[ignore = "pending"]
fn test_copy_symlink_templated_name() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    // Prepare repo bundle
    let repo = src.join("repo");
    file_tree! {
        repo.join("copier.yaml") => indoc! {"
            _preserve_symlinks: true
            symlink_name: symlink
        "},
        repo.join("target.txt") => "Symlink target",
        repo.join("{{ symlink_name }}.txt") => symlink("target.txt"),
    };

    git(&src, &["init"]);
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m", "hello world"]);

    worker(&repo, &dst)
        .defaults(true)
        .overwrite(true)
        .vcs_ref("HEAD")
        .run_copy()
        .unwrap();

    assert!(dst.join("target.txt").exists());
    assert!(dst.join("symlink.txt").exists());
    assert!(dst.join("symlink.txt").is_symlink());
    assert_eq!(
        std::fs::read_link(dst.join("symlink.txt")).unwrap(),
        Path::new("target.txt")
    );
}

#[test]
#[ignore = "pending"]
fn test_copy_symlink_templated_target() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    // Prepare repo bundle
    let repo = src.join("repo");
    file_tree! {
        repo.join("copier.yaml") => indoc! {"
            _preserve_symlinks: true
            target_name: target
        "},
        repo.join("{{ target_name }}.txt") => "Symlink target",
        repo.join("symlink1.txt.jinja") => symlink("{{ target_name }}.txt"),
        repo.join("symlink2.txt") => symlink("{{ target_name }}.txt"),
    };

    git(&src, &["init"]);
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m", "hello world"]);

    worker(&repo, &dst)
        .defaults(true)
        .overwrite(true)
        .vcs_ref("HEAD")
        .run_copy()
        .unwrap();

    assert!(dst.join("target.txt").exists());
    assert!(dst.join("symlink1.txt").exists());
    assert!(dst.join("symlink1.txt").is_symlink());
    assert_eq!(
        std::fs::read_link(dst.join("symlink1.txt")).unwrap(),
        Path::new("target.txt")
    );

    assert!(!dst.join("symlink2.txt").exists());
    assert!(dst.join("symlink2.txt").is_symlink());
    assert_eq!(
        std::fs::read_link(dst.join("symlink2.txt")).unwrap(),
        Path::new("{{ target_name }}.txt")
    );
}

#[test]
#[ignore = "pending"]
fn test_copy_symlink_missing_target() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    // Prepare repo bundle
    let repo = src.join("repo");
    file_tree! {
        repo.join("copier.yaml") => indoc! {"
            _preserve_symlinks: true
        "},
        repo.join("symlink.txt") => symlink("target.txt"),
    };

    git(&src, &["init"]);
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m", "hello world"]);

    worker(&repo, &dst)
        .defaults(true)
        .overwrite(true)
        .vcs_ref("HEAD")
        .run_copy()
        .unwrap();

    assert!(dst.join("symlink.txt").is_symlink());
    assert_eq!(
        std::fs::read_link(dst.join("symlink.txt")).unwrap(),
        Path::new("target.txt")
    );
    // exists follows symlinks, It returns False as the target doesn't exist
    assert!(!dst.join("symlink.txt").exists());
}

#[test]
#[ignore = "pending"]
fn test_option_preserve_symlinks_false() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    // Prepare repo bundle
    let repo = src.join("repo");
    file_tree! {
        repo.join("copier.yaml") => indoc! {"
            _preserve_symlinks: false
        "},
        repo.join("target.txt") => "Symlink target",
        repo.join("symlink.txt") => symlink("target.txt"),
    };

    git(&src, &["init"]);
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m", "hello world"]);

    worker(&repo, &dst)
        .defaults(true)
        .overwrite(true)
        .vcs_ref("HEAD")
        .run_copy()
        .unwrap();

    assert!(dst.join("target.txt").exists());
    assert!(dst.join("symlink.txt").exists());
    assert!(!dst.join("symlink.txt").is_symlink());
}

#[test]
#[ignore = "pending"]
fn test_option_preserve_symlinks_default() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    // Prepare repo bundle
    let repo = src.join("repo");
    file_tree! {
        repo.join("copier.yaml") => "",
        repo.join("target.txt") => "Symlink target",
        repo.join("symlink.txt") => symlink("target.txt"),
    };

    git(&src, &["init"]);
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m", "hello world"]);

    worker(&repo, &dst)
        .defaults(true)
        .overwrite(true)
        .vcs_ref("HEAD")
        .run_copy()
        .unwrap();

    assert!(dst.join("target.txt").exists());
    assert!(dst.join("symlink.txt").exists());
    assert!(!dst.join("symlink.txt").is_symlink());
}

#[test]
#[ignore = "pending"]
fn test_update_symlink() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    file_tree! {
        src.join(".copier-answers.yml.jinja") => indoc! {"
            # Changes here will be overwritten by Copier
            {{ _copier_answers|to_nice_yaml }}
        "},
        src.join("copier.yml") => indoc! {"
            _preserve_symlinks: true
        "},
        src.join("aaaa.txt") => "\nLorem ipsum\n",
        src.join("bbbb.txt") => "\ndolor sit amet\n",
        src.join("symlink.txt") => symlink("./aaaa.txt"),
    };

    git(&src, &["init"]);
    git(&src, &["add", "-A"]);
    git(&src, &["commit", "-m", "first commit on src"]);

    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();

    // test updating a symlink
    std::fs::remove_file(src.join("symlink.txt")).unwrap();
    std::os::unix::fs::symlink("bbbb.txt", src.join("symlink.txt")).unwrap();

    // dst must be vcs-tracked to use run_update
    git(&dst, &["init"]);
    git(&dst, &["add", "-A"]);
    git(&dst, &["commit", "-m", "first commit on dst"]);

    // make sure changes have not yet propagated
    let p1 = src.join("symlink.txt");
    let p2 = dst.join("symlink.txt");
    assert_ne!(read(&p1), read(&p2));

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
    let p1 = src.join("symlink.txt");
    let p2 = dst.join("symlink.txt");
    assert_eq!(read(&p1), read(&p2));

    assert_eq!(
        std::fs::read_link(dst.join("symlink.txt")).unwrap(),
        Path::new("bbbb.txt")
    );
}

#[test]
#[ignore = "pending"]
fn test_update_file_to_symlink() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    file_tree! {
        src.join(".copier-answers.yml.jinja") => indoc! {"
            # Changes here will be overwritten by Copier
            {{ _copier_answers|to_nice_yaml }}
        "},
        src.join("copier.yml") => indoc! {"
            _preserve_symlinks: true
        "},
        src.join("aaaa.txt") => "\nLorem ipsum\n",
        src.join("bbbb.txt") => "\ndolor sit amet\n",
        src.join("cccc.txt") => symlink("./aaaa.txt"),
    };

    git(&src, &["init"]);
    git(&src, &["add", "-A"]);
    git(&src, &["commit", "-m", "first commit on src"]);

    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();

    // test updating a symlink
    std::fs::remove_file(src.join("bbbb.txt")).unwrap();
    std::os::unix::fs::symlink("aaaa.txt", src.join("bbbb.txt")).unwrap();
    std::fs::remove_file(src.join("cccc.txt")).unwrap();
    std::fs::write(src.join("cccc.txt"), "Lorem ipsum").unwrap();

    // dst must be vcs-tracked to use run_update
    git(&dst, &["init"]);
    git(&dst, &["add", "-A"]);
    git(&dst, &["commit", "-m", "first commit on dst"]);

    // make sure changes have not yet propagated
    assert!(!dst.join("bbbb.txt").is_symlink());
    assert!(dst.join("cccc.txt").is_symlink());

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
    assert!(dst.join("bbbb.txt").is_symlink());
    assert_eq!(
        std::fs::read_link(dst.join("bbbb.txt")).unwrap(),
        Path::new("aaaa.txt")
    );
    assert!(!dst.join("cccc.txt").is_symlink());
    assert_eq!(read(dst.join("cccc.txt")), "Lorem ipsum");
}

#[test]
#[ignore = "pending"]
fn test_exclude_symlink() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    // Prepare repo bundle
    let repo = src.join("repo");
    file_tree! {
        repo.join("copier.yaml") => indoc! {"
            _preserve_symlinks: true
        "},
        repo.join("target.txt") => "Symlink target",
        repo.join("symlink.txt") => symlink("target.txt"),
    };

    git(&src, &["init"]);
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m", "hello world"]);

    worker(&repo, &dst)
        .defaults(true)
        .overwrite(true)
        .exclude(["symlink.txt"])
        .vcs_ref("HEAD")
        .run_copy()
        .unwrap();
    assert!(!dst.join("symlink.txt").exists());
    assert!(!dst.join("symlink.txt").is_symlink());
}

#[test]
#[ignore = "pending"]
fn test_pretend_symlink() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    // Prepare repo bundle
    let repo = src.join("repo");
    file_tree! {
        repo.join("copier.yaml") => indoc! {"
            _preserve_symlinks: true
        "},
        repo.join("target.txt") => "Symlink target",
        repo.join("symlink.txt") => symlink("target.txt"),
    };

    git(&src, &["init"]);
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m", "hello world"]);

    worker(&repo, &dst)
        .defaults(true)
        .overwrite(true)
        .pretend(true)
        .vcs_ref("HEAD")
        .run_copy()
        .unwrap();
    assert!(!dst.join("symlink.txt").exists());
    assert!(!dst.join("symlink.txt").is_symlink());
}

#[test]
#[ignore = "pending"]
fn test_copy_symlink_none_path() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    // Prepare repo bundle
    let repo = src.join("repo");
    file_tree! {
        repo.join("copier.yaml") => indoc! {"
            _preserve_symlinks: true
            render: false
        "},
        repo.join("target.txt") => "Symlink target",
        repo.join("{% if render %}symlink.txt{% endif %}") => symlink("target.txt"),
    };

    git(&src, &["init"]);
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m", "hello world"]);

    worker(&repo, &dst)
        .defaults(true)
        .overwrite(true)
        .vcs_ref("HEAD")
        .run_copy()
        .unwrap();

    assert!(dst.join("target.txt").exists());
    assert!(!dst.join("symlink.txt").exists());
    assert!(!dst.join("symlink.txt").is_symlink());
}

#[test]
#[ignore = "pending"]
fn test_recursive_symlink() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yaml") => "_preserve_symlinks: true",
        src.join("one/two/three/root") => symlink("../../../"),
    };
    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    assert!(dst.join("one/two/three/root").is_symlink());
    assert_eq!(
        std::fs::read_link(dst.join("one/two/three/root")).unwrap(),
        Path::new("../../../")
    );
}

#[test]
#[ignore = "pending"]
fn test_symlinked_dir_expanded() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("a_dir/a_file.txt") => "some content",
        src.join("a_symlinked_dir") => symlink("a_dir"),
        src.join("a_nested/symlink") => symlink("../a_dir"),
    };
    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    assert_eq!(read(dst.join("a_dir/a_file.txt")), "some content");
    assert_eq!(read(dst.join("a_symlinked_dir/a_file.txt")), "some content");
    assert_eq!(
        read(dst.join("a_nested/symlink/a_file.txt")),
        "some content"
    );
}

// @pytest.mark.xfail(
//     os.name == "nt", reason="Absolute paths not created as symlinks on Windows"
// )
#[test]
#[ignore = "pending"]
fn test_symlinked_to_outside_destination_absolute() {
    let tmp = Tmp::new();
    let (src, dst, other) = (tmp.mktemp("src"), tmp.mktemp("dst"), tmp.mktemp("other"));
    file_tree! {
        src.join(".copier-answers.yml.jinja") => indoc! {"
            # Changes here will be overwritten by Copier
            {{ _copier_answers|to_nice_yaml }}
        "},
        src.join("copier.yaml") => indoc! {"
            _preserve_symlinks: true
        "},
        src.join("a_file.txt") => "some content",
        src.join("a_symlink.txt") => symlink(other.join("outside.txt")),
        src.join("symlink_dir") => symlink(&other),
        other.join("outside.txt") => "outside",
    };

    git(&src, &["init"]);
    git(&src, &["add", "-A"]);
    git(&src, &["commit", "-m", "first commit on src"]);

    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();

    assert!(dst.join("symlink_dir").is_symlink());
    assert_eq!(read(dst.join("symlink_dir/outside.txt")), "outside");
    assert!(dst.join("a_symlink.txt").is_symlink());
    assert_eq!(read(dst.join("a_symlink.txt")), "outside");

    // dst must be vcs-tracked to use run_update
    git(&dst, &["init"]);
    git(&dst, &["add", "-A"]);
    git(&dst, &["commit", "-m", "first commit on dst"]);

    // While update appears like a no-op, this part of the test captures
    // possible changes in behaviour when the symlinks actually exist
    // (having been created by the initial copy) as was the case for
    // https://github.com/copier-org/copier/issues/2426
    Worker::new()
        .dst_path(&dst)
        .defaults(true)
        .overwrite(true)
        .run_update()
        .unwrap();

    assert!(dst.join("symlink_dir").is_symlink());
    assert_eq!(read(dst.join("symlink_dir/outside.txt")), "outside");
    assert!(dst.join("a_symlink.txt").is_symlink());
    assert_eq!(read(dst.join("a_symlink.txt")), "outside");
}

#[test]
#[ignore = "pending"]
fn test_symlinked_to_outside_destination_relative() {
    let tmp = Tmp::new();
    let (src, dst_parent) = (tmp.mktemp("src"), tmp.mktemp("dst_parent"));
    let dst = dst_parent.join("dst");
    std::fs::create_dir(&dst).unwrap();
    let other = dst_parent.join("other");
    std::fs::create_dir(&other).unwrap();
    let other_rel = Path::new("..").join("other");
    file_tree! {
        src.join(".copier-answers.yml.jinja") => indoc! {"
            # Changes here will be overwritten by Copier
            {{ _copier_answers|to_nice_yaml }}
        "},
        src.join("copier.yaml") => indoc! {"
            _preserve_symlinks: true
        "},
        src.join("a_file.txt") => "some content",
        src.join("a_symlink.txt") => symlink(other_rel.join("outside.txt")),
        src.join("symlink_dir") => symlink(&other_rel),
        other.join("outside.txt") => "outside",
    };

    git(&src, &["init"]);
    git(&src, &["add", "-A"]);
    git(&src, &["commit", "-m", "first commit on src"]);

    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();

    assert!(dst.join("symlink_dir").is_symlink());
    assert_eq!(read(dst.join("symlink_dir/outside.txt")), "outside");
    assert!(dst.join("a_symlink.txt").is_symlink());
    assert_eq!(read(dst.join("a_symlink.txt")), "outside");

    // dst must be vcs-tracked to use run_update
    git(&dst, &["init"]);
    git(&dst, &["add", "-A"]);
    git(&dst, &["commit", "-m", "first commit on dst"]);

    // While update appears like a no-op, this part of the test captures
    // possible changes in behaviour when the symlinks actually exist
    // (having been created by the initial copy) as was the case for
    // https://github.com/copier-org/copier/issues/2426
    Worker::new()
        .dst_path(&dst)
        .defaults(true)
        .overwrite(true)
        .run_update()
        .unwrap();

    assert!(dst.join("symlink_dir").is_symlink());
    assert_eq!(read(dst.join("symlink_dir/outside.txt")), "outside");
    assert!(dst.join("a_symlink.txt").is_symlink());
    assert_eq!(read(dst.join("a_symlink.txt")), "outside");
}

#[test]
#[ignore = "needs-design: depends on copier's internal template clone location"]
fn test_resolve_relative_symlink_outside_template_root_raises_error() {
    let tmp = Tmp::new();
    let (src, dst, other) = (tmp.mktemp("src"), tmp.mktemp("dst"), tmp.mktemp("other"));

    file_tree! {
        src.join(".copier-answers.yml.jinja") => indoc! {"
            # Changes here will be overwritten by Copier
            {{ _copier_answers|to_nice_yaml }}
        "},
        src.join("copier.yaml") => indoc! {"
            _preserve_symlinks: false
        "},
        // TODO(port): `symlink.txt` must point at `outside.txt` relative to where
        // the template is cloned (`os.path.relpath(other, start=gettempdir()/"template-clone")`),
        // which depends on the clone location internal to copier.
        // src.join("symlink.txt") => symlink(relative_path_from_clone_dir(&other).join("outside.txt")),
        other.join("outside.txt") => "outside",
    };

    git(&src, &["init"]);
    git(&src, &["add", "-A"]);
    git(&src, &["commit", "-m", "init"]);

    assert_err!(
        worker(&src, &dst).defaults(true).overwrite(true).run_copy(),
        CopierError::ForbiddenPath { .. },
        r#""symlink\.txt" is forbidden"#
    );

    assert!(!dst.join("symlink.txt").exists());
}

// @pytest.mark.skipif(
//     os.name == "nt", reason="Absolute paths not created as symlinks on Windows"
// )
// Covered by the module-level `#![cfg(unix)]`.
#[test]
#[ignore = "pending"]
fn test_resolve_absolute_symlink_outside_template_root_raises_error() {
    let tmp = Tmp::new();
    let (src, dst, other) = (tmp.mktemp("src"), tmp.mktemp("dst"), tmp.mktemp("other"));
    file_tree! {
        src.join(".copier-answers.yml.jinja") => indoc! {"
            # Changes here will be overwritten by Copier
            {{ _copier_answers|to_nice_yaml }}
        "},
        src.join("copier.yaml") => indoc! {"
            _preserve_symlinks: false
        "},
        src.join("symlink.txt") => symlink(other.join("outside.txt")),
        other.join("outside.txt") => "outside",
    };

    git(&src, &["init"]);
    git(&src, &["add", "-A"]);
    git(&src, &["commit", "-m", "init"]);

    assert_err!(
        worker(&src, &dst).defaults(true).overwrite(true).run_copy(),
        CopierError::ForbiddenPath { .. },
        r#""symlink\.txt" is forbidden"#
    );

    assert!(!dst.join("symlink.txt").exists());
}

#[rstest]
#[case(true)]
#[case(false)]
#[ignore = "pending"]
fn test_destination_symlink_outside_destination_root(#[case] preserve_symlinks: bool) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yaml") => format!("_preserve_symlinks: {preserve_symlinks}"),
        src.join("file.txt") => "from template",
        dst.join("other/target.txt") => "external",
        dst.join("project/file.txt") => symlink(Path::new("..").join("other").join("target.txt")),
    };

    worker(&src, dst.join("project"))
        .overwrite(true)
        .run_copy()
        .unwrap();

    assert!(!dst.join("project/file.txt").is_symlink());
    assert_eq!(read(dst.join("project/file.txt")), "from template");
}

#[test]
#[ignore = "pending"]
fn test_update_renamed_dir_does_not_delete_symlink_target() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => indoc! {"
            _preserve_symlinks: true

            name:
                type: str
        "},
        src.join(ANSWERS_FILE_TMPL) => indoc! {"
            # Changes here will be overwritten by Copier
            {{ _copier_answers|to_nice_yaml }}
        "},
        src.join("shared/templates/file.txt") => "test",
        src.join("{{ name }}/templates") => symlink(Path::new("..").join("shared").join("templates")),
    };

    git(&src, &["init"]);
    git(&src, &["add", "-A"]);
    git(&src, &["commit", "-m", "init"]);

    worker(&src, &dst)
        .data(v(json!({"name": "foo"})))
        .run_copy()
        .unwrap();

    assert!(dst.join("foo/templates").is_symlink());
    assert_eq!(
        std::fs::read_link(dst.join("foo/templates")).unwrap(),
        Path::new("..").join("shared").join("templates")
    );
    assert_eq!(read(dst.join("foo/templates/file.txt")), "test");

    git(&dst, &["init"]);
    git(&dst, &["add", "-A"]);
    git(&dst, &["commit", "-m", "init"]);

    Worker::new()
        .dst_path(&dst)
        .data(v(json!({"name": "bar"})))
        .overwrite(true)
        .run_update()
        .unwrap();

    // assert not (dst / "foo").exists()
    assert!(dst.join("bar/templates").is_symlink());
    assert_eq!(
        std::fs::read_link(dst.join("bar/templates")).unwrap(),
        Path::new("..").join("shared").join("templates")
    );
    assert_eq!(read(dst.join("bar/templates/file.txt")), "test");
}
