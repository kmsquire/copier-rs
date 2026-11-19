//! Port of `tests/test_vcs.py`.

mod common;

use std::path::{Path, PathBuf};

use common::{
    assert_err, file_tree, fixture, git, git_save, load_answers, read, worker, Tmp,
    ANSWERS_FILE_TMPL,
};
use copier::errors::{capture_warnings, WarningKind};
use copier::vcs::{
    clone, get_git_version, get_latest_tag, get_mirror_path, get_repo, is_git_repo_root, is_remote,
    parse_git_version, with_git_env,
};
use copier::{CopierError, Value};
use rstest::rstest;

/// Local `file://` URL for `path`, which `clone()` treats as remote (`as_uri()` in Python).
fn file_uri(path: &Path) -> String {
    format!("file://{}", path.display())
}

/// Write a git config (the `gitconfig` fixture's defaults plus `extra`) and return its path.
fn write_gitconfig(tmp: &Tmp, extra: &str) -> PathBuf {
    let path = tmp.path().join("gitconfig");
    let base = std::fs::read_to_string(common::fixtures().join("gitconfig")).unwrap();
    std::fs::write(&path, format!("{base}\n{extra}")).unwrap();
    path
}

/// A small local git repo with one tagged commit (the `remote_repo` fixture).
fn remote_repo(tmp: &Tmp) -> String {
    let path = tmp.mktemp("remote_repo");
    file_tree! { path.join("README.md") => "hello world" };
    git_save(&path, "init", Some("v1.0.0"));
    file_uri(&path)
}

#[test]
fn test_get_repo() {
    let get = get_repo;

    assert_eq!(
        get("git@git.myproject.org:MyProject"),
        Some("git@git.myproject.org:MyProject".to_owned())
    );
    assert_eq!(
        get("git://git.myproject.org/MyProject"),
        Some("git://git.myproject.org/MyProject".to_owned())
    );
    assert_eq!(
        get("https://github.com/jpscaletti/copier.git"),
        Some("https://github.com/jpscaletti/copier.git".to_owned())
    );

    assert_eq!(
        get("https://github.com/jpscaletti/copier"),
        Some("https://github.com/jpscaletti/copier.git".to_owned())
    );
    assert_eq!(
        get("https://gitlab.com/gitlab-org/gitlab"),
        Some("https://gitlab.com/gitlab-org/gitlab.git".to_owned())
    );

    assert_eq!(
        get("gh:/jpscaletti/copier.git"),
        Some("https://github.com/jpscaletti/copier.git".to_owned())
    );
    assert_eq!(
        get("gh:jpscaletti/copier.git"),
        Some("https://github.com/jpscaletti/copier.git".to_owned())
    );
    assert_eq!(
        get("gl:jpscaletti/copier.git"),
        Some("https://gitlab.com/jpscaletti/copier.git".to_owned())
    );
    assert_eq!(
        get("gh:jpscaletti/copier"),
        Some("https://github.com/jpscaletti/copier.git".to_owned())
    );
    assert_eq!(
        get("gl:jpscaletti/copier"),
        Some("https://gitlab.com/jpscaletti/copier.git".to_owned())
    );

    assert_eq!(
        get("git+https://git.myproject.org/MyProject"),
        Some("https://git.myproject.org/MyProject".to_owned())
    );
    assert_eq!(
        get("git+ssh://git.myproject.org/MyProject"),
        Some("ssh://git.myproject.org/MyProject".to_owned())
    );

    assert!(get("git://git.myproject.org/MyProject.git@master").is_some());
    assert!(get("git://git.myproject.org/MyProject.git@v1.0").is_some());
    assert!(get("git://git.myproject.org/MyProject.git@da39a3ee5e6b4b0d3255bfef956018").is_some());

    assert_eq!(get("http://google.com"), None);
    assert_eq!(get("git.myproject.org/MyProject"), None);
    assert_eq!(get("https://google.com"), None);

    let bundle = fixture("demo_updatediff_repo.bundle");
    let bundle = bundle.to_str().unwrap();
    assert_eq!(get(bundle), Some(bundle.to_owned()));
}

// impure: needs network
#[test]
#[ignore = "network: clones from GitHub; run with --include-ignored"]
fn test_clone() {
    let tmp = clone("https://github.com/copier-org/copier.git", None, None).unwrap();
    assert!(tmp.join("README.md").exists());
    std::fs::remove_dir_all(&tmp).unwrap();
}

// impure: needs network
#[test]
#[ignore = "network: clones from GitHub; run with --include-ignored"]
fn test_local_clone() {
    let tmp = clone("https://github.com/copier-org/copier.git", None, None).unwrap();
    assert!(tmp.join("README.md").exists());

    let local_tmp = clone(tmp.to_str().unwrap(), None, None).unwrap();
    assert!(local_tmp.join("README.md").exists());
    std::fs::remove_dir_all(&local_tmp).unwrap();
}

#[test]
fn test_is_remote() {
    assert!(is_remote("https://github.com/copier-org/copier.git"));
    assert!(is_remote("git@github.com:copier-org/copier.git"));
    assert!(is_remote("file:///some/where/repo.git"));
    // Local, on-disk paths are not remote.
    let cwd = std::env::current_dir().unwrap();
    assert!(!is_remote(cwd.to_str().unwrap()));
    // `get_repo` marks resolved local repos so they keep the old behavior.
    assert!(!is_remote(
        &get_repo(cwd.to_str().unwrap()).unwrap_or_default()
    ));
}

#[test]
fn test_mirror_path_ignores_credentials() {
    // Embedded credentials must not affect the cache key, so the same repo
    // accessed with or without a token maps to a single mirror.
    let base = "https://github.com/org/repo.git";
    let with_token = "https://x-access-token:github_pat_secret@github.com/org/repo.git";
    assert_eq!(get_mirror_path(with_token), get_mirror_path(base));
}

#[test]
fn test_remote_clone_creates_and_reuses_mirror() {
    let tmp = Tmp::new();
    let remote = remote_repo(&tmp);
    // First use creates the mirror and checks out a worktree. Passing an
    // explicit, test-managed location keeps the worktree from leaking. Distinct
    // subpaths under one base dir avoids `mktemp` reusing a deleted name.
    let clones = tmp.mktemp("clones");
    let dst1 = clone(&remote, None, Some(&clones.join("first"))).unwrap();
    let mirror = get_mirror_path(&remote);
    assert!(mirror.join("objects").is_dir());
    assert_eq!(read(dst1.join("README.md")), "hello world");

    // Drop a sentinel inside the mirror; if the second use re-clones from
    // scratch the mirror directory (and the sentinel) would be recreated.
    let sentinel = mirror.join("copier-cache-sentinel");
    std::fs::write(&sentinel, "kept").unwrap();

    let dst2 = clone(&remote, None, Some(&clones.join("second"))).unwrap();
    assert!(sentinel.exists(), "mirror was re-created instead of reused");
    assert!(dst2.join("README.md").exists());
    assert_ne!(dst2, dst1); // a fresh worktree per use
}

#[test]
fn test_remote_clone_checks_out_ref() {
    let tmp = Tmp::new();
    let remote = remote_repo(&tmp);
    let dst = clone(&remote, Some("v1.0.0"), Some(&tmp.mktemp("clone"))).unwrap();
    assert_eq!(read(dst.join("README.md")), "hello world");
}

#[rstest]
#[case("v1.0.0")]
#[case("HEAD")]
#[case("branch")]
fn test_local_clone_checks_out_ref(#[case] git_ref: &str) {
    let tmp = Tmp::new();
    let src = tmp.mktemp("src");
    git(&src, &["init"]);
    let main_branch = git(&src, &["branch", "--show-current"]).trim().to_owned();
    file_tree! { src.join("README.md") => "main" };
    git(&src, &["add", "-A"]);
    git(&src, &["commit", "-m", "init"]);
    git(&src, &["tag", "v1.0.0"]);
    git(&src, &["checkout", "-b", "branch"]);
    file_tree! { src.join("README.md") => "branch" };
    git(&src, &["commit", "-am", "branch"]);
    git(&src, &["checkout", &main_branch]);
    let dst = clone(
        src.to_str().unwrap(),
        Some(git_ref),
        Some(&tmp.mktemp("clone")),
    )
    .unwrap();
    assert_eq!(
        read(dst.join("README.md")),
        if git_ref == "branch" {
            "branch"
        } else {
            "main"
        }
    );
}

#[rstest]
#[case("--orphan=pwned")]
#[case("--detach")]
#[case("-b")]
#[case("--")]
#[case("HEAD\n--orphan=pwned")]
#[case("missing")]
fn test_local_clone_rejects_invalid_ref(#[case] git_ref: &str) {
    let tmp = Tmp::new();
    let src = tmp.mktemp("src");
    git(&src, &["init"]);
    file_tree! { src.join("README.md") => "main" };
    git(&src, &["add", "-A"]);
    git(&src, &["commit", "-m", "init"]);
    assert_err!(
        clone(
            src.to_str().unwrap(),
            Some(git_ref),
            Some(&tmp.mktemp("clone")),
        ),
        CopierError::Value(_),
        "Unknown revision"
    );
}

#[test]
fn test_remote_clone_prunes_stale_worktree() {
    let tmp = Tmp::new();
    let remote = remote_repo(&tmp);
    let mirror = get_mirror_path(&remote);
    let clones = tmp.mktemp("clones");

    // Simulate Copier's cleanup: the worktree directory is removed, leaving a
    // stale registration behind in the mirror.
    let dst1 = clone(&remote, None, Some(&clones.join("stale"))).unwrap();
    std::fs::remove_dir_all(&dst1).unwrap();
    assert!(!dst1.exists());

    // The next use must prune the stale registration and succeed.
    let dst2 = clone(&remote, None, Some(&clones.join("fresh"))).unwrap();
    assert!(dst2.join("README.md").exists());
    let worktrees = git(&mirror, &["worktree", "list", "--porcelain"]);
    assert!(!worktrees
        .replace('\\', "/")
        .contains(&dst1.to_string_lossy().replace('\\', "/")));
}

#[test]
fn test_remote_clone_recovers_from_corrupt_mirror() {
    let tmp = Tmp::new();
    let remote = remote_repo(&tmp);
    let clones = tmp.mktemp("clones");
    let dst1 = clone(&remote, None, Some(&clones.join("orig"))).unwrap();
    std::fs::remove_dir_all(&dst1).unwrap();
    let mirror = get_mirror_path(&remote);

    // Simulate a partial/corrupt cache entry: the `objects` directory survives
    // but the rest of the repository is gone (e.g. an interrupted deletion).
    for child in std::fs::read_dir(&mirror).unwrap() {
        let child = child.unwrap().path();
        if child.file_name().unwrap() != "objects" {
            if child.is_dir() {
                std::fs::remove_dir_all(&child).unwrap();
            } else {
                std::fs::remove_file(&child).unwrap();
            }
        }
    }
    assert!(mirror.join("objects").is_dir());

    // The next use must discard the corrupt mirror and rebuild it.
    let dst2 = clone(&remote, None, Some(&clones.join("rebuilt"))).unwrap();
    assert_eq!(read(dst2.join("README.md")), "hello world");
}

#[test]
fn test_remote_clone_submodule_with_moved_url() {
    // Submodules are resolved from each checkout's own `.gitmodules`.
    //
    // A previous worktree's `git submodule update --init` registers the
    // submodule URL in the mirror's shared config, so a submodule that moved
    // between template revisions would be fetched from the stale URL, failing
    // when the pinned commit no longer exists there.
    let tmp = Tmp::new();
    let gitconfig = write_gitconfig(&tmp, "[protocol \"file\"]\n\tallow = always\n");
    // The submodule, initially hosted in one place...
    let sub1 = tmp.mktemp("sub1");
    git(&sub1, &["init"]);
    file_tree! { sub1.join("sub.txt") => "v1" };
    git(&sub1, &["add", "-A"]);
    git(&sub1, &["commit", "-m", "submodule v1"]);

    // ...and later moved to a new location, with a new commit.
    let sub2 = tmp.mktemp("sub2");
    git(&sub2, &["init"]);
    file_tree! { sub2.join("sub.txt") => "v2" };
    git(&sub2, &["add", "-A"]);
    git(&sub2, &["commit", "-m", "submodule v2"]);

    // The template's submodule points at the first location...
    let template = tmp.mktemp("template");
    git(&template, &["init"]);
    file_tree! { template.join("README.md") => "template" };
    git(&template, &["add", "-A"]);
    git(&template, &["commit", "-m", "init"]);
    let sub1_str = sub1.to_str().unwrap();
    let sub2_str = sub2.to_str().unwrap();
    git(
        &template,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            sub1_str,
            "sub",
        ],
    );
    git(
        &template,
        &["commit", "-m", "add submodule at its original location"],
    );
    git(&template, &["tag", "v1"]);

    // ...and moves to the second location, pinning a commit that only
    // exists there.
    let gitmodules = template.join(".gitmodules");
    std::fs::write(&gitmodules, read(&gitmodules).replace(sub1_str, sub2_str)).unwrap();
    git(template.join("sub"), &["fetch", sub2_str]);
    git(template.join("sub"), &["checkout", "FETCH_HEAD"]);
    git(&template, &["add", "-A"]);
    git(
        &template,
        &["commit", "-m", "move submodule to its new location"],
    );
    git(&template, &["tag", "v2"]);

    let clones = tmp.mktemp("clones");
    let template_uri = file_uri(&template);
    // Checking out the first revision registers the original submodule URL
    // in the mirror's shared config.
    let env = [("GIT_CONFIG_GLOBAL", gitconfig.as_os_str())];
    let dst1 = with_git_env(&env, || {
        clone(&template_uri, Some("v1"), Some(&clones.join("first")))
    })
    .unwrap();
    assert_eq!(read(dst1.join("sub").join("sub.txt")), "v1");

    // The second checkout must follow v2's `.gitmodules` instead of using
    // the stale URL registered by the first checkout.
    let dst2 = with_git_env(&env, || {
        clone(&template_uri, Some("v2"), Some(&clones.join("second")))
    })
    .unwrap();
    assert_eq!(read(dst2.join("sub").join("sub.txt")), "v2");
}

#[test]
fn test_local_dirty_clone() {
    // When core.fsmonitor is enabled, normal `git checkout` command won't works.
    let tmp = Tmp::new();
    let gitconfig = write_gitconfig(&tmp, "[core]\n\tfsmonitor = true\n");
    let src = tmp.mktemp("src");

    file_tree! { src.join("version.txt") => "0.1.0" };
    git_save(&src, "Test commit", None);

    file_tree! {
        src.join("version.txt") => "0.2.0",
        src.join("README.md") => "hello world",
    };

    let (result, warnings) = with_git_env(&[("GIT_CONFIG_GLOBAL", gitconfig.as_os_str())], || {
        capture_warnings(|| clone(src.to_str().unwrap(), None, None))
    });
    let local_tmp = result.unwrap();
    assert!(warnings.iter().any(|w| w.kind == WarningKind::DirtyLocal));
    assert!(local_tmp.join("version.txt").exists());
    assert_eq!(read(local_tmp.join("version.txt")), "0.2.0");
    assert!(local_tmp.join("README.md").exists());
    assert_eq!(read(local_tmp.join("README.md")), "hello world");
    std::fs::remove_dir_all(&local_tmp).unwrap();
}

// impure: needs network
#[test]
#[ignore = "network: clones from GitHub; run with --include-ignored"]
fn test_shallow_clone() {
    // This test should always work but should be much slower if `is_git_shallow_repo()`
    // is not checked in `clone()`.
    let tmp = Tmp::new();
    let src_path = tmp.path().join("autopretty");
    git(
        tmp.path(),
        &[
            "clone",
            "--depth=2",
            "https://github.com/copier-org/autopretty.git",
            src_path.to_str().unwrap(),
        ],
    );
    assert!(src_path.join("README.md").exists());

    let local_tmp = if get_git_version().unwrap() >= (2, 27, 0) {
        let (result, warnings) = capture_warnings(|| clone(src_path.to_str().unwrap(), None, None));
        let local_tmp = result.unwrap();
        assert!(warnings.iter().any(|w| w.kind == WarningKind::ShallowClone));
        local_tmp
    } else {
        let (result, warnings) = capture_warnings(|| clone(src_path.to_str().unwrap(), None, None));
        let local_tmp = result.unwrap();
        assert!(warnings.is_empty());
        local_tmp
    };
    assert!(local_tmp.join("README.md").exists());
    std::fs::remove_dir_all(&local_tmp).unwrap();
}

// impure: needs network
#[test]
#[ignore = "network: clones from GitHub; run with --include-ignored"]
fn test_removes_temporary_clone() {
    let tmp = Tmp::new();
    let src_path = "https://github.com/copier-org/autopretty.git";
    let worker = worker(src_path, tmp.path())
        .defaults(true)
        .unsafe_(true)
        .run_copy()
        .unwrap();
    let temp_clone = worker.template().unwrap().local_abspath().unwrap();
    // The clone lives as long as the worker; Python's `with Worker(...)` ends here.
    drop(worker);
    assert!(!temp_clone.exists());
}

// impure: needs network
#[test]
#[ignore = "network: clones from GitHub; run with --include-ignored"]
fn test_dont_remove_local_clone() {
    let tmp = Tmp::new();
    let src_path = tmp.path().join("autopretty");
    git(
        tmp.path(),
        &[
            "clone",
            "https://github.com/copier-org/autopretty.git",
            src_path.to_str().unwrap(),
        ],
    );
    worker(&src_path, tmp.path())
        .defaults(true)
        .unsafe_(true)
        .run_copy()
        .unwrap();
    assert!(src_path.exists());
}

// impure: needs network
#[test]
#[ignore = "network: clones from GitHub; run with --include-ignored"]
fn test_update_using_local_source_path_with_tilde() {
    let tmp = Tmp::new();
    // first, get a local repository clone
    let src_path = tmp.path().join("autopretty");
    git(
        tmp.path(),
        &[
            "clone",
            "https://github.com/copier-org/autopretty.git",
            src_path.to_str().unwrap(),
        ],
    );

    // then prepare the user path to this clone (starting with ~)
    // temporary path is in /tmp, so climb back up from ~ using ../
    let home = std::env::var("HOME").unwrap();
    let home_depth = Path::new(&home).components().count();
    let user_src_path = format!(
        "~/{}{}",
        vec![".."; home_depth].join("/"),
        src_path.to_str().unwrap()
    );

    // generate project and assert correct path in answers
    let worker = worker(&user_src_path, tmp.path())
        .defaults(true)
        .unsafe_(true)
        .run_copy()
        .unwrap();
    assert_eq!(
        worker.answers.combined()[&Value::from("_src_path")],
        Value::from(user_src_path.as_str())
    );

    // assert project update works and correct path again
    git(tmp.path(), &["init"]);
    git(tmp.path(), &["add", "-A"]);
    git(tmp.path(), &["commit", "-m", "init"]);
    let worker = copier::Worker::new()
        .dst_path(tmp.path())
        .defaults(true)
        .overwrite(true)
        .answers_file(".copier-answers.autopretty.yml")
        .unsafe_(true)
        .run_update()
        .unwrap();
    assert_eq!(
        worker.answers.combined()[&Value::from("_src_path")],
        Value::from(user_src_path.as_str())
    );
}

#[test]
fn test_invalid_version() {
    let tmp = Tmp::new();
    let dir = tmp.path();
    let sample: PathBuf = dir.join("sample.txt");
    git(dir, &["init"]);
    std::fs::write(&sample, "1").unwrap();
    git(dir, &["add", sample.to_str().unwrap()]);
    git(dir, &["commit", "-m1"]);
    git(dir, &["tag", "not-a-version"]);
    std::fs::write(&sample, "2").unwrap();
    git(dir, &["commit", "-am2"]);
    git(dir, &["tag", "v2"]);
    std::fs::write(&sample, "3").unwrap();
    git(dir, &["commit", "-am3"]);
    assert_ne!(git(dir, &["describe", "--tags"]).trim(), "v2");
    assert_eq!(get_latest_tag(dir.to_str().unwrap(), false).unwrap(), "v2");
}

#[rstest]
#[case(false)]
#[case(true)]
fn test_select_latest_version_tag(#[case] reversed: bool) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    let filename = "version.txt";

    git(&src, &["init"]);
    file_tree! { src.join(ANSWERS_FILE_TMPL) => "{{ _copier_answers|to_nice_yaml }}" };
    let mut versions = vec!["v1", "v1.0", "v1.0.0", "v1.0.1"];
    if reversed {
        versions.reverse();
    }
    for version in versions {
        std::fs::write(src.join(filename), version).unwrap();
        git(&src, &["add", "."]);
        git(&src, &["commit", "-m", version]);
        git(&src, &["tag", version]);
    }

    worker(&src, &dst).run_copy().unwrap();

    assert!(dst.join(filename).is_file());
    assert_eq!(read(dst.join(filename)), "v1.0.1");
    let answers = load_answers(&dst);
    assert_eq!(answers["_commit"], Value::from("v1.0.1"));
}

#[test]
fn test_is_git_repo_root_worktree() {
    // is_git_repo_root should return True for git worktrees where .git is a file.
    let tmp = Tmp::new();
    let main_repo = tmp.mktemp("main");
    git(&main_repo, &["init"]);
    git(&main_repo, &["commit", "--allow-empty", "-m", "init"]);
    git(&main_repo, &["branch", "wt-branch"]);
    let worktree = tmp.path().join("worktree");
    git(
        &main_repo,
        &["worktree", "add", worktree.to_str().unwrap(), "wt-branch"],
    );

    // Sanity: .git in worktree is a file, not a directory
    assert!(worktree.join(".git").is_file());
    assert!(!worktree.join(".git").is_dir());

    assert!(is_git_repo_root(&worktree));
}

#[test]
fn test_get_repo_worktree() {
    // get_repo should recognise a git worktree as a valid local repo.
    let tmp = Tmp::new();
    let main_repo = tmp.mktemp("main");
    git(&main_repo, &["init"]);
    git(&main_repo, &["commit", "--allow-empty", "-m", "init"]);
    git(&main_repo, &["branch", "wt-branch"]);
    let worktree = tmp.path().join("worktree");
    git(
        &main_repo,
        &["worktree", "add", worktree.to_str().unwrap(), "wt-branch"],
    );

    let worktree_str = worktree.to_str().unwrap();
    assert_eq!(get_repo(worktree_str), Some(worktree_str.to_owned()));
}

#[rstest]
#[case("git version 2.53.0\n", (2, 53, 0))]
#[case("git version 2.53.GIT\n", (2, 53, 0))]
fn test_get_git_version(
    #[case] git_version_output: &str,
    #[case] expected_version: (u64, u64, u64),
) {
    // Previously mocked `get_git`; the parsing is exercised directly instead.
    assert_eq!(
        parse_git_version(git_version_output).unwrap(),
        expected_version
    );
}
