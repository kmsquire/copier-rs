//! Port of `tests/test_demo_update_tasks.py`.

mod common;

use common::{file_tree, git, worker, Tmp};
use copier::Worker;
use indoc::indoc;

#[test]
fn test_update_tasks() {
    // Test that updating a template runs tasks from the expected version.
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    // Prepare repo bundle
    let repo = src.join("repo");
    let bundle = src.join("demo_update_tasks.bundle");
    file_tree! {
        repo.join(".copier-answers.yml.jinja") => indoc! {"
            # Changes here will be overwritten by Copier
            {{ _copier_answers|to_nice_yaml }}
        "},
        repo.join("copier.yaml") => indoc! {"
            _tasks:
                - cat v1.txt
        "},
        repo.join("v1.txt") => "file only in v1",
    };
    git(&repo, &["init"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m1"]);
    git(&repo, &["tag", "v1"]);
    file_tree! {
        repo.join("copier.yaml") => indoc! {"
            _tasks:
                - cat v2.txt
        "},
        repo.join("v2.txt") => "file only in v2",
    };
    std::fs::remove_file(repo.join("v1.txt")).unwrap();
    git(&repo, &["init"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m2"]);
    git(&repo, &["tag", "v2"]);
    git(
        &repo,
        &["bundle", "create", bundle.to_str().unwrap(), "--all"],
    );
    // Copy the 1st version
    worker(&bundle, &dst)
        .defaults(true)
        .overwrite(true)
        .vcs_ref("v1")
        .unsafe_(true)
        .run_copy()
        .unwrap();
    // Init destination as a new independent git repo
    git(&dst, &["init"]);
    // Commit changes
    git(&dst, &["add", "."]);
    git(&dst, &["commit", "-m", "hello world"]);
    // Update target to v2
    Worker::new()
        .dst_path(&dst)
        .defaults(true)
        .overwrite(true)
        .unsafe_(true)
        .run_update()
        .unwrap();
}
