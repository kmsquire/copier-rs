//! Tests for behavior specific to this port, with no counterpart in the Python suite.

mod common;

use common::{assert_err, file_tree, git_save, read, worker, Tmp};
use copier::template::Template;
use copier::CopierError;

#[test]
fn test_self_including_config_is_an_error_not_a_crash() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => "a: !include copier.yml\n",
    };
    assert_err!(
        worker(&src, &dst).defaults(true).run_copy(),
        CopierError::Value(_),
        "nested too deeply"
    );
}

#[test]
fn test_directory_is_not_a_git_bundle() {
    let tmp = Tmp::new();
    let dir = tmp.mktemp("plain");
    assert!(!copier::vcs::is_git_bundle(&dir));
    assert!(!copier::vcs::is_git_bundle(&dir.join("missing")));
}

#[test]
fn test_alias_expands_without_git() {
    let expanded =
        copier::vcs::without_git(|| Template::new("gh:copier-org/copier").url_expanded());
    assert_eq!(expanded, "https://github.com/copier-org/copier.git");
    assert!(!copier::vcs::without_git(|| Template::new(
        "gh:copier-org/copier"
    )
    .is_git()));
}

#[test]
fn test_executable_bit_comes_from_the_git_index_for_every_file() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("one.sh") => "#!/bin/sh\n",
        src.join("two.sh") => "#!/bin/sh\n",
        src.join("plain.txt") => "text\n",
    };
    git_save(&src, "v1", Some("v1"));
    common::git(&src, &["update-index", "--chmod=+x", "one.sh", "two.sh"]);
    common::git(&src, &["commit", "-m", "make scripts executable"]);
    common::git(&src, &["tag", "v2"]);
    worker(&src, &dst).defaults(true).run_copy().unwrap();
    assert_eq!(read(dst.join("plain.txt")), "text\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let executable = |name: &str| {
            std::fs::metadata(dst.join(name))
                .unwrap()
                .permissions()
                .mode()
                & 0o111
                != 0
        };
        assert!(executable("one.sh") && executable("two.sh"));
        assert!(!executable("plain.txt"));
    }
}
