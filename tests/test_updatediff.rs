//! Port of `tests/test_updatediff.py`.

mod common;

use std::path::{Path, PathBuf};

use common::{
    assert_err, copier_cmd, dedent, file_tree, git, git_init, git_save, load_answers,
    load_answers_from, read, run_cli, spawn, spawn_with, try_git, v, worker, Tmp,
    BRACKET_ENVOPS_JSON, SPAWN_TIMEOUT, SUFFIX_TMPL,
};
use copier::{ConflictMode, CopierError, Value, VcsRef, Worker};
use indoc::indoc;
use rstest::rstest;
use serde_json::json;

/// Set the given permission bits on a file (`path.chmod(mode | bits)`).
#[cfg(unix)]
fn chmod_add(path: &Path, bits: u32) {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path).unwrap().permissions();
    permissions.set_mode(permissions.mode() | bits);
    std::fs::set_permissions(path, permissions).unwrap();
}

/// Clear the given permission bits on a file (`path.chmod(mode & ~bits)`).
#[cfg(unix)]
fn chmod_remove(path: &Path, bits: u32) {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path).unwrap().permissions();
    permissions.set_mode(permissions.mode() & !bits);
    std::fs::set_permissions(path, permissions).unwrap();
}

/// The `st_mode` of a file.
#[cfg(unix)]
fn st_mode(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).unwrap().permissions().mode()
}

/// The index mode of `launcher.sh` (first field of `git ls-files --stage`).
fn index_mode(dir: &Path) -> String {
    git(dir, &["ls-files", "--stage", "--", "launcher.sh"])
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned()
}

// impure: needs network
#[test]
#[ignore = "pending"]
fn test_updatediff() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    // Prepare repo bundle
    let repo = src.join("repo");
    let bundle = src.join("demo_updatediff_repo.bundle");
    let bundle_str = bundle.to_str().unwrap();
    file_tree! {
        repo.join(".copier-answers.yml.jinja") => indoc! {"
            # Changes here will be overwritten by Copier
            {{ _copier_answers|to_nice_yaml }}
        "},
        repo.join("copier.yml") => indoc! {r#"
            _envops:
                "keep_trailing_newline": True
            project_name: to become a pirate
            author_name: Guybrush
        "#},
        repo.join("README.txt.jinja") => "
            Let me introduce myself.

            My name is {{author_name}}, and my project is {{project_name}}.

            Thanks for your attention.
            ",
    };
    git(&repo, &["init"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "Guybrush wants to be a pirate"]);
    git(&repo, &["tag", "v0.0.1"]);
    file_tree! {
        repo.join("copier.yml") => indoc! {r#"
            _envops:
                "keep_trailing_newline": True
            project_name: to become a pirate
            author_name: Guybrush
            _migrations:
                -   version: v0.0.1
                    when: "{{ _stage == 'before' }}"
                    command: touch before-v0.0.1
                -   version: v0.0.1
                    when: "{{ _stage == 'after' }}"
                    command: touch after-v0.0.1
                -   version: v0.0.2
                    when: "{{ _stage == 'before' }}"
                    command: touch before-v0.0.2
                -   version: v0.0.2
                    when: "{{ _stage == 'after' }}"
                    command: touch after-v0.0.2
                -   version: v1.0.0
                    when: "{{ _stage == 'before' }}"
                    command: touch before-v1.0.0
                -   version: v1.0.0
                    when: "{{ _stage == 'after' }}"
                    command: touch after-v1.0.0
        "#},
    };
    git(&repo, &["init"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "Add migrations"]);
    git(&repo, &["tag", "v0.0.2"]);
    file_tree! {
        repo.join("copier.yml") => indoc! {r#"
            _envops:
                "keep_trailing_newline": True
            project_name: to rule
            author_name: Elaine
            _migrations:
                -   version: v0.0.1
                    when: "{{ _stage == 'before' }}"
                    command: touch before-v0.0.1
                -   version: v0.0.1
                    when: "{{ _stage == 'after' }}"
                    command: touch after-v0.0.1
                -   version: v0.0.2
                    when: "{{ _stage == 'before' }}"
                    command: touch before-v0.0.2
                -   version: v0.0.2
                    when: "{{ _stage == 'after' }}"
                    command: touch after-v0.0.2
                -   version: v1.0.0
                    when: "{{ _stage == 'before' }}"
                    command: touch before-v1.0.0
                -   version: v1.0.0
                    when: "{{ _stage == 'after' }}"
                    command: touch after-v1.0.0
        "#},
        repo.join("README.txt.jinja") => "
            Let me introduce myself.

            My name is {{author_name}}.

            My project is {{project_name}}.

            Thanks for your attention.
            ",
    };
    git(&repo, &["init"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "Elaine wants to rule"]);
    git(&repo, &["bundle", "create", bundle_str, "--all"]);
    let last_commit = git(&repo, &["describe", "--tags"]).trim().to_owned();
    // Generate repo bundle
    let target = dst.join("target");
    let target_str = target.to_str().unwrap();
    let readme = target.join("README.txt");
    let commit = |message: &str| git(&target, &["commit", "--all", "-m", message]);
    // Run copier 1st time, with specific tag
    run_cli(&[
        "copy",
        bundle_str,
        target_str,
        "--defaults",
        "--overwrite",
        "--vcs-ref=v0.0.1",
    ]);
    // Check it's copied OK
    assert_eq!(
        load_answers(&target),
        v(json!({
            "_commit": "v0.0.1",
            "_src_path": bundle_str,
            "author_name": "Guybrush",
            "project_name": "to become a pirate",
        }))
    );
    assert_eq!(
        read(&readme),
        dedent(
            "
            Let me introduce myself.

            My name is Guybrush, and my project is to become a pirate.

            Thanks for your attention.
            "
        )
    );
    // Init destination as a new independent git repo
    git(&target, &["init"]);
    // Commit changes
    git(&target, &["add", "."]);
    commit("hello world");
    // Emulate the user modifying the README by hand
    std::fs::write(
        &readme,
        dedent(
            "
            Let me introduce myself.

            My name is Guybrush, and my project is to become a pirate.

            Thanks for your grog.
            ",
        ),
    )
    .unwrap();
    commit("I prefer grog");
    // Update target to latest tag and check it's updated in answers file
    run_cli(&["update", "--defaults", "--UNSAFE", target_str]);
    assert_eq!(
        load_answers(&target),
        v(json!({
            "_commit": "v0.0.2",
            "_src_path": bundle_str,
            "author_name": "Guybrush",
            "project_name": "to become a pirate",
        }))
    );
    // Check migrations were executed properly
    assert!(!target.join("before-v0.0.1").is_file());
    assert!(!target.join("after-v0.0.1").is_file());
    assert!(target.join("before-v0.0.2").is_file());
    assert!(target.join("after-v0.0.2").is_file());
    std::fs::remove_file(target.join("before-v0.0.2")).unwrap();
    std::fs::remove_file(target.join("after-v0.0.2")).unwrap();
    assert!(!target.join("before-v1.0.0").is_file());
    assert!(!target.join("after-v1.0.0").is_file());
    commit("Update template to v0.0.2");
    // Update target to latest commit, which is still untagged
    run_cli(&["update", "--defaults", "--vcs-ref=HEAD", target_str]);
    // Check no new migrations were executed
    assert!(!target.join("before-v0.0.1").is_file());
    assert!(!target.join("after-v0.0.1").is_file());
    assert!(!target.join("before-v0.0.2").is_file());
    assert!(!target.join("after-v0.0.2").is_file());
    assert!(!target.join("before-v1.0.0").is_file());
    assert!(!target.join("after-v1.0.0").is_file());
    // Check it's updated OK
    assert_eq!(
        load_answers(&target),
        v(json!({
            "_commit": last_commit,
            "_src_path": bundle_str,
            "author_name": "Guybrush",
            "project_name": "to become a pirate",
        }))
    );
    assert_eq!(
        read(&readme),
        dedent(
            "
            Let me introduce myself.

            My name is Guybrush.

            My project is to become a pirate.

            Thanks for your grog.
            "
        )
    );
    commit(&format!("Update template to {last_commit}"));
    assert!(git(&target, &["status", "--porcelain"]).is_empty());
    // No more updates exist, so updating again should change nothing
    run_cli(&["update", "--defaults", "--vcs-ref=HEAD", target_str]);
    assert!(git(&target, &["status", "--porcelain"]).is_empty());
    // If I change an option, it updates properly
    Worker::new()
        .dst_path(&target)
        .data(v(
            json!({"author_name": "Largo LaGrande", "project_name": "to steal a lot"}),
        ))
        .defaults(true)
        .overwrite(true)
        .vcs_ref("HEAD")
        .run_update()
        .unwrap();
    assert_eq!(
        read(&readme),
        dedent(
            "
            Let me introduce myself.

            My name is Largo LaGrande.

            My project is to steal a lot.

            Thanks for your grog.
            "
        )
    );
    commit("Subproject evolved");
    // Reapply template ignoring subproject evolution
    Worker::new()
        .dst_path(&target)
        .data(v(
            json!({"author_name": "Largo LaGrande", "project_name": "to steal a lot"}),
        ))
        .defaults(true)
        .overwrite(true)
        .vcs_ref("HEAD")
        .run_copy()
        .unwrap();
    assert_eq!(
        read(&readme),
        dedent(
            "
            Let me introduce myself.

            My name is Largo LaGrande.

            My project is to steal a lot.

            Thanks for your attention.
            "
        )
    );
}

// xfail(get_git_version() >= Version("2.55.0"), reason="Git v2.55.0+ no longer supports
// `--inter-hunk-context=-1` (https://github.com/copier-org/copier/issues/2759)")
//
// This fails on Windows because there's some problem while detecting
// the diff. It seems like an older Git version were being used, while
// that's not the case...
// FIXME Some generous Windows power user please fix this test!
// xfail(condition=platform.system() == "Windows", reason="Git broken on Windows?")
//
// impure: needs network
/// Commit hooks are taken into account when producing the update diff.
#[test]
#[ignore = "needs-design: requires the pre-commit tool and network"]
fn test_commit_hooks_respected() {
    // Prepare source template v1
    let tmp = Tmp::new();
    let src = tmp.mktemp("src");
    let dst1 = tmp.mktemp("dst1");
    let dst2 = tmp.mktemp("dst2");
    file_tree! {
        src.join("copier.yml") => format!(r#"
            _envops: {BRACKET_ENVOPS_JSON}
            _templates_suffix: {SUFFIX_TMPL}
            _tasks:
                - git init
                - pre-commit install -t pre-commit -t commit-msg
                - pre-commit run -a || true
            what: grog
            "#),
        src.join("[[ _copier_conf.answers_file ]].tmpl") => "
            [[ _copier_answers|to_nice_yaml ]]
            ",
        src.join(".pre-commit-config.yaml") => r#"
            repos:
            -   repo: https://github.com/pre-commit/mirrors-prettier
                rev: v2.0.4
                hooks:
                -   id: prettier
            -   repo: https://github.com/commitizen-tools/commitizen
                rev: v3.12.0
                hooks:
                -   id: commitizen
            -   repo: local
                hooks:
                -   id: forbidden-files
                    name: forbidden files
                    entry: found forbidden files; remove them
                    language: fail
                    files: "\\.rej$"
            "#,
        src.join("life.yml.tmpl") => "
            # Following code should be reformatted by pre-commit after copying
            Line 1:      hello
            Line 2:      [[ what ]]
            Line 3:      bye
            ",
    };
    git(&src, &["init"]);
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m", "feat: commit 1"]);
    git(&src, &["tag", "v1"]);
    // Copy source template
    worker(&src, &dst1)
        .defaults(true)
        .overwrite(true)
        .unsafe_(true)
        .run_copy()
        .unwrap();
    let life = dst1.join("life.yml");
    git(&dst1, &["add", "."]);
    // 1st commit fails because pre-commit reformats life.yml
    assert!(try_git(&dst1, &["commit", "-am", "feat: failed commit"]).is_err());
    // 2nd commit works because it's already formatted
    git(&dst1, &["commit", "-am", "feat: copied v1"]);
    assert_eq!(
        read(&life),
        indoc! {"
            # Following code should be reformatted by pre-commit after copying
            Line 1: hello
            Line 2: grog
            Line 3: bye
        "}
    );
    // Evolve source template to v2
    file_tree! {
        src.join("life.yml.tmpl") => indoc! {"
            # Following code should be reformatted by pre-commit after copying
            Line 1:     hello world
            Line 2:     grow up
            Line 3:     [[ what ]]
            Line 4:     grow old
            Line 5:     bye bye world
        "},
    };
    git(&src, &["init"]);
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m", "feat: commit 2"]);
    git(&src, &["tag", "v2"]);
    // Update subproject to v2
    Worker::new()
        .dst_path(&dst1)
        .defaults(true)
        .overwrite(true)
        .conflict(ConflictMode::Rej)
        .context_lines(1)
        .unsafe_(true)
        .run_update()
        .unwrap();
    git(&dst1, &["commit", "-am", "feat: copied v2"]);
    assert_eq!(
        read(&life),
        indoc! {"
            # Following code should be reformatted by pre-commit after copying
            Line 1: hello world
            Line 2: grow up
            Line 3: grog
            Line 4: grow old
            Line 5: bye bye world
        "}
    );
    // No .rej files created (update diff was smart)
    assert!(git(&dst1, &["status", "--porcelain"]).is_empty());
    // Subproject evolves
    std::fs::write(
        &life,
        indoc! {"
            Line 1: hello world
            Line 2: grow up
            Line 2.5: make friends
            Line 3: grog
            Line 4: grow old
            Line 4.5: no more work
            Line 5: bye bye world
        "},
    )
    .unwrap();
    git(&dst1, &["commit", "-am", "chore: subproject is evolved"]);
    // A new subproject appears, which is a shallow clone of the 1st one.
    // Using file:// prefix to allow local shallow clones.
    git(
        tmp.path(),
        &[
            "clone",
            "--depth=1",
            &format!("file://{}", dst1.display()),
            dst2.to_str().unwrap(),
        ],
    );
    // Subproject re-updates just to change some values
    Worker::new()
        .dst_path(&dst2)
        .data(v(json!({"what": "study"})))
        .defaults(true)
        .overwrite(true)
        .conflict(ConflictMode::Rej)
        .context_lines(1)
        .unsafe_(true)
        .run_update()
        .unwrap();
    git(
        &dst2,
        &[
            "commit",
            "-am",
            "chore: re-updated to change values after evolving",
        ],
    );
    // Subproject evolution was respected up to sane possibilities.
    // In an ideal world, this file would be exactly the same as what's written
    // a few lines above, just changing "grog" for "study". However, that's nearly
    // impossible to achieve, because each change hunk needs at least 1 line of
    // context to let git apply that patch smartly, and that context couldn't be
    // found because we changed data when updating, so the sanest thing we can
    // do is to provide a .rej file to notify those
    // unresolvable diffs. OTOH, some other changes are be applied.
    // If some day you are able to produce that ideal result, you should be
    // happy to modify these asserts.
    let life2 = dst2.join("life.yml");
    assert_eq!(
        read(&life2),
        indoc! {"
            Line 1: hello world
            Line 2: grow up
            Line 3: study
            Line 4: grow old
            Line 4.5: no more work
            Line 5: bye bye world
        "}
    );
    // This time a .rej file is unavoidable
    assert!(dst2.join("life.yml.rej").is_file());
}

/// Ignore post-checkout hook when conflicts are encountered.
#[test]
#[ignore = "pending"]
fn test_post_checkout_hook_ignored() {
    // Prepare source template v1
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => format!(r#"
            _envops: {BRACKET_ENVOPS_JSON}
            _templates_suffix: {SUFFIX_TMPL}
            "#),
        src.join("[[ _copier_conf.answers_file ]].tmpl") => "
            [[ _copier_answers|to_nice_yaml ]]
            ",
        src.join("test.txt") => "This is a file",
    };
    git(&src, &["init"]);
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m", "feat: commit 1"]);
    git(&src, &["tag", "v1"]);
    // Copy source template
    worker(&src, &dst).run_copy().unwrap();
    git(&dst, &["init"]);
    git(&dst, &["add", "."]);
    // Commit initial copy
    git(&dst, &["commit", "-am", "feat: copied v1"]);
    // Introduce conflict
    std::fs::write(dst.join("test.txt"), "This is a conflicting change").unwrap();
    git(&dst, &["add", "."]);
    git(&dst, &["commit", "-am", "feat: edit test.txt"]);
    // Add post-checkout hook that fails
    let hook_file = dst.join(".git").join("hooks").join("post-checkout");
    std::fs::write(&hook_file, "exit 1").unwrap();
    #[cfg(unix)]
    chmod_add(&hook_file, 0o100);
    // Evolve source template to v2
    file_tree! {
        src.join("test.txt") => "This is an edited file in v2",
    };
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m", "feat: Update post-checkout"]);
    git(&src, &["tag", "v2"]);
    // Update subproject to v2
    // No errors should be raised due to post-checkout hook
    Worker::new()
        .dst_path(&dst)
        .overwrite(true)
        .run_update()
        .unwrap();
}

#[test]
#[ignore = "pending"]
fn test_update_from_tagged_to_head() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    // Build a template
    file_tree! {
        src.join("{{ _copier_conf.answers_file }}.jinja") => "{{ _copier_answers|to_nice_yaml }}",
        src.join("example") => "1",
    };
    git(&src, &["init"]);
    git(&src, &["add", "-A"]);
    git(&src, &["commit", "-m1"]);
    // Publish v1 release
    git(&src, &["tag", "v1"]);
    // New commit, no release
    file_tree! { src.join("example") => "2" };
    git(&src, &["commit", "-am2"]);
    let sha = git(&src, &["rev-parse", "--short", "HEAD"])
        .trim()
        .to_owned();
    // Copy it without specifying version
    worker(&src, &dst).run_copy().unwrap();
    let example = dst.join("example");
    assert_eq!(read(&example), "1");
    assert_eq!(load_answers(&dst)["_commit"], Value::from("v1"));
    // Build repo on copy
    git(&dst, &["init"]);
    git(&dst, &["add", "-A"]);
    git(&dst, &["commit", "-m3"]);
    // Update project, it must let us do it
    Worker::new()
        .dst_path(&dst)
        .vcs_ref("HEAD")
        .defaults(true)
        .overwrite(true)
        .run_update()
        .unwrap();
    assert_eq!(read(&example), "2");
    assert_eq!(
        load_answers(&dst)["_commit"],
        Value::from(format!("v1-1-g{sha}"))
    );
}

#[rstest]
#[ignore = "pending"]
fn test_skip_update(
    #[values("skip_me", "/skip_me")] skip_pattern: &str,
    #[values("", "subproject")] subproject_path: &str,
) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    let dst_subproject = if subproject_path.is_empty() {
        dst.clone()
    } else {
        dst.join(subproject_path)
    };
    file_tree! {
        src.join("copier.yaml") => format!("_skip_if_exists: ['{skip_pattern}']"),
        src.join("{{ _copier_conf.answers_file }}.jinja") => "{{ _copier_answers|to_yaml }}",
        src.join("skip_me") => "1",
    };
    git(&src, &["init"]);
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m1"]);
    git(&src, &["tag", "1.0.0"]);
    worker(&src, &dst_subproject)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    let skip_me = dst_subproject.join("skip_me");
    let answers = load_answers(&dst_subproject);
    assert_eq!(read(&skip_me), "1");
    assert_eq!(answers["_commit"], Value::from("1.0.0"));
    std::fs::write(&skip_me, "2").unwrap();
    git(&dst, &["init"]);
    git(&dst, &["add", "."]);
    git(&dst, &["commit", "-m1"]);
    file_tree! { src.join("skip_me") => "3" };
    git(&src, &["commit", "-am2"]);
    git(&src, &["tag", "2.0.0"]);
    Worker::new()
        .dst_path(&dst_subproject)
        .defaults(true)
        .overwrite(true)
        .run_update()
        .unwrap();
    let answers = load_answers(&dst_subproject);
    assert_eq!(read(&skip_me), "2");
    assert_eq!(answers["_commit"], Value::from("2.0.0"));
    assert!(!dst_subproject.join("skip_me.rej").exists());
}

/// Ensure that paths in `skip_if_exists` are always recreated
/// if they are absent before updating.
#[rstest]
#[case("skip_normal_file")]
// xfail(platform.system() in {"Darwin", "Windows"}, reason="OS without proper UTF-8 filesystem.")
#[case("skip_unicode_âñ")]
#[case("skip file with whitespace")]
#[case(" skip_leading_whitespace")]
#[case("skip_trailing_whitespace ")]
#[case("   skip_multi_whitespace   ")]
// skipif(platform.system() == "Windows", reason="Disallowed characters in file name")
#[case("\tskip_other_whitespace\t\\t")]
// skipif(platform.system() == "Windows", reason="Disallowed characters in file name")
#[case("\x07\x0c\n\t\x0bskip_control\x07\x0c\n\t\x0bcharacters\x0b\t\n\x0c\x07")]
// skipif(platform.system() == "Windows", reason="Disallowed characters in file name")
#[case("skip_back\\slash")]
// skipif(platform.system() == "Windows", reason="Disallowed characters in file name")
#[case("!skip_special")]
// skipif(platform.system() == "Windows", reason="Disallowed characters in file name")
#[case("sk\\ip_with_backslash_in_skip_pattern")]
#[ignore = "pending"]
fn test_skip_update_deleted(#[case] file_name: &str) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    file_tree! {
        src.join("copier.yaml") => "_skip_if_exists: ['*skip*', '*sk\\ip*']",
        src.join("{{ _copier_conf.answers_file }}.jinja") => "{{ _copier_answers|to_yaml }}",
        src.join(file_name) => "1",
        src.join("another_file") => "foobar",
    };
    git(&src, &["init"]);
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m1"]);
    git(&src, &["tag", "1.0.0"]);
    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    let skip_me = dst.join(file_name);
    let answers = load_answers(&dst);
    assert_eq!(read(&skip_me), "1");
    assert_eq!(answers["_commit"], Value::from("1.0.0"));
    std::fs::remove_file(&skip_me).unwrap();
    git(&dst, &["init"]);
    git(&dst, &["add", "."]);
    git(&dst, &["commit", "-m1"]);
    Worker::new()
        .dst_path(&dst)
        .overwrite(true)
        .run_update()
        .unwrap();
    assert!(skip_me.exists());
    assert_eq!(read(&skip_me), "1");
}

/// Ensure that deleted paths are not regenerated during updates,
/// even if the template has changes in that path.
#[rstest]
#[case("normal_file")]
// xfail(platform.system() in {"Darwin", "Windows"}, reason="OS without proper UTF-8 filesystem.")
#[case("unicode_âñ")]
#[case("file with whitespace")]
#[case(" leading_whitespace")]
#[case("trailing_whitespace ")]
#[case("   multi_whitespace   ")]
// skipif(platform.system() == "Windows", reason="Disallowed characters in file name")
#[case("\tother_whitespace\t\\t")]
// This param accounts for some limitations that would
// otherwise make the test fail:
//   * \r in path segment names is converted to \n by Jinja rendering,
//     hence the rendered file would be named differently altogether.
//   * The pathspec lib does not account for different kinds of escaped
//     whitespace at the end of the pattern, only a space.
//     If there are control characters at the end of the string
//     that would be stripped by .strip(), the pattern would end
//     in the backslash that should have escaped it.
// skipif(platform.system() == "Windows", reason="Disallowed characters in file name")
#[case("\x07\x0c\n\t\x0bcontrol\x07\x0c\n\t\x0bcharacters\x0b\t\n\x0c\x07")]
// skipif(platform.system() == "Windows", reason="Disallowed characters in file name")
#[case("back\\slash")]
// skipif(platform.system() == "Windows", reason="Disallowed characters in file name")
#[case("!special")]
// skipif(platform.system() == "Windows", reason="Disallowed characters in file name")
#[case("dont_wildmatch*")]
#[ignore = "pending"]
fn test_update_deleted_path(#[case] file_name: &str) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("{{ _copier_conf.answers_file }}.jinja") => "{{ _copier_answers|to_yaml }}",
        src.join(file_name) => "foo",
        src.join("another_file") => "foobar",
        src.join("dont_wildmatch") => "bar",
    };
    git(&src, &["init"]);
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m1"]);
    git(&src, &["tag", "1.0.0"]);
    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    let updated_file = dst.join(file_name);
    let dont_wildmatch = dst.join("dont_wildmatch");
    let answers = load_answers(&dst);
    assert_eq!(read(&dont_wildmatch), "bar");
    assert_eq!(read(&updated_file), "foo");
    assert_eq!(answers["_commit"], Value::from("1.0.0"));
    std::fs::remove_file(&updated_file).unwrap();
    git(&dst, &["init"]);
    git(&dst, &["add", "."]);
    git(&dst, &["commit", "-m1"]);
    file_tree! { src.join(file_name) => "bar", src.join("dont_wildmatch") => "baz" };
    git(&src, &["commit", "-am2"]);
    git(&src, &["tag", "2.0.0"]);
    Worker::new()
        .dst_path(&dst)
        .overwrite(true)
        .run_update()
        .unwrap();
    assert!(dont_wildmatch.exists());
    assert_eq!(read(&dont_wildmatch), "baz");
    assert!(!updated_file.exists());
}

/// Test that user-deleted paths don't cause deletion of other paths matching
/// the same pattern when updating a project.
#[test]
#[ignore = "pending"]
fn test_update_deleted_path_not_used_as_pattern() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    file_tree! {
        src.join("{{ _copier_conf.answers_file }}.jinja") => "{{ _copier_answers|to_yaml }}",
        src.join("test.txt") => "remove",
        src.join("foo").join("test.txt") => "keep",
        src.join("foo").join("bar").join("test.txt") => "keep",
    };
    git_save(&src, "initial commit", None);

    worker(&src, &dst).run_copy().unwrap();
    assert_eq!(read(dst.join("test.txt")), "remove");
    assert_eq!(read(dst.join("foo").join("test.txt")), "keep");
    assert_eq!(read(dst.join("foo").join("bar").join("test.txt")), "keep");

    std::fs::remove_file(dst.join("test.txt")).unwrap();
    git_save(&dst, "remove test.txt", None);

    Worker::new()
        .dst_path(&dst)
        .overwrite(true)
        .run_update()
        .unwrap();
    assert!(!dst.join("test.txt").exists());
    assert_eq!(read(dst.join("foo").join("test.txt")), "keep");
    assert_eq!(read(dst.join("foo").join("bar").join("test.txt")), "keep");
}

#[rstest]
#[case(None)]
#[case(Some(".copier-answers.yml"))]
#[case(Some(".custom.copier-answers.yaml"))]
#[ignore = "pending"]
fn test_overwrite_answers_file_always(#[case] answers_file: Option<&str>) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yaml") => "question_1: true",
        src.join("{{ _copier_conf.answers_file }}.jinja") => "{{ _copier_answers|to_yaml }}",
        src.join("answer_1.jinja") => "{{ question_1 }}",
    };
    git(&src, &["init"]);
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m1"]);
    git(&src, &["tag", "1"]);
    file_tree! { src.join("copier.yaml") => "question_1: false" };
    git(&src, &["commit", "-am2"]);
    git(&src, &["tag", "2"]);
    // When copying, there's nothing to overwrite, overwrite=False shouldn't hang
    let mut copy_worker = worker(&src, &dst).vcs_ref("1").defaults(true);
    if let Some(answers_file) = answers_file {
        copy_worker = copy_worker.answers_file(answers_file);
    }
    copy_worker.run_copy().unwrap();
    git(&dst, &["init"]);
    git(&dst, &["add", "."]);
    git(&dst, &["commit", "-m1"]);
    // When updating, the only thing to overwrite is the copier answers file,
    // which shouldn't ask, so also this shouldn't hang with overwrite=False
    let mut update_worker = Worker::new().dst_path(&dst).defaults(true).overwrite(true);
    if let Some(answers_file) = answers_file {
        update_worker = update_worker.answers_file(answers_file);
    }
    update_worker.run_update().unwrap();
    let answers = load_answers_from(&dst, answers_file.unwrap_or(".copier-answers.yml"));
    assert_eq!(answers["question_1"], Value::Bool(true));
    assert_eq!(answers["_commit"], Value::from("2"));
    assert_eq!(read(dst.join("answer_1")), "True");
}

#[test]
#[ignore = "pending"]
fn test_file_removed() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    // Add a file in the template repo
    file_tree! {
        src.join("{{ _copier_conf.answers_file }}.jinja") => "{{ _copier_answers|to_yaml }}",
        src.join("1.txt") => "content 1",
        src.join("dir 2").join("2.txt") => "content 2",
        src.join("dir 3").join("subdir 3").join("3.txt") => "content 3",
        src.join("dir 4").join("subdir 4").join("4.txt") => "content 4",
        src.join("dir 5").join("subdir 5").join("5.txt") => "content 5",
        src.join("toignore.txt") => "should survive update",
    };
    git(&src, &["init"]);
    git(&src, &["add", "-A"]);
    git(&src, &["commit", "-m1"]);
    git(&src, &["tag", "1"]);
    // Copy in subproject
    git(&dst, &["init"]);
    worker(&src, &dst).run_copy().unwrap();
    // Subproject has an extra file
    file_tree! {
        dst.join("I.txt") => "content I",
        dst.join("dir II").join("II.txt") => "content II",
        dst.join("dir 3").join("subdir III").join("III.txt") => "content III",
        dst.join("dir 4").join("subdir 4").join("IV.txt") => "content IV",
    };
    git(&dst, &["add", "-A"]);
    git(&dst, &["commit", "-m2"]);
    // All files exist
    assert!(dst.join(".copier-answers.yml").is_file());
    assert!(dst.join("1.txt").is_file());
    assert!(dst.join("dir 2").join("2.txt").is_file());
    assert!(dst.join("dir 3").join("subdir 3").join("3.txt").is_file());
    assert!(dst.join("dir 4").join("subdir 4").join("4.txt").is_file());
    assert!(dst.join("dir 5").join("subdir 5").join("5.txt").is_file());
    assert!(dst.join("toignore.txt").is_file());
    assert!(dst.join("I.txt").is_file());
    assert!(dst.join("dir II").join("II.txt").is_file());
    assert!(dst
        .join("dir 3")
        .join("subdir III")
        .join("III.txt")
        .is_file());
    assert!(dst.join("dir 4").join("subdir 4").join("IV.txt").is_file());
    // Template removes file 1
    std::fs::remove_file(src.join("1.txt")).unwrap();
    std::fs::remove_dir_all(src.join("dir 2")).unwrap();
    std::fs::remove_dir_all(src.join("dir 3")).unwrap();
    std::fs::remove_dir_all(src.join("dir 4")).unwrap();
    std::fs::remove_dir_all(src.join("dir 5")).unwrap();
    file_tree! { src.join("6.txt") => "content 6" };
    git(&src, &["add", "-A"]);
    git(&src, &["commit", "-m3"]);
    git(&src, &["tag", "2"]);
    // Subproject updates
    std::fs::write(dst.join(".gitignore"), "toignore.txt").unwrap();
    git(&dst, &["add", ".gitignore"]);
    git(&dst, &["commit", "-m", "ignore file"]);
    assert_err!(
        Worker::new()
            .dst_path(&dst)
            .conflict(ConflictMode::Rej)
            .run_update(),
        CopierError::UserMessage(_),
        "Enable overwrite to update a subproject."
    );
    Worker::new()
        .dst_path(&dst)
        .conflict(ConflictMode::Rej)
        .overwrite(true)
        .run_update()
        .unwrap();
    // Check what must still exist
    assert!(dst.join(".copier-answers.yml").is_file());
    assert!(dst.join("I.txt").is_file());
    assert!(dst.join("dir II").join("II.txt").is_file());
    assert!(dst
        .join("dir 3")
        .join("subdir III")
        .join("III.txt")
        .is_file());
    assert!(dst.join("dir 4").join("subdir 4").join("IV.txt").is_file());
    assert!(dst.join("6.txt").is_file());
    assert!(dst.join("toignore.txt").is_file());
    // Check what must not exist
    assert!(!dst.join("1.txt").exists());
    assert!(!dst.join("dir 2").exists());
    assert!(!dst.join("dir 3").join("subdir 3").exists());
    assert!(!dst.join("dir 4").join("subdir 4").join("4.txt").exists());
    assert!(!dst.join("dir 5").exists());
}

#[rstest]
#[ignore = "pending"]
fn test_update_inline_changed_answers_and_questions(#[values(true, false)] interactive: bool) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    let (src_str, dst_str) = (src.to_str().unwrap(), dst.to_str().unwrap());
    file_tree! {
        src.join("{{ _copier_conf.answers_file }}.jinja") => "{{ _copier_answers|to_yaml }}",
        src.join("copier.yml") => "b: false",
        src.join("content.jinja") => indoc! {"
            aaa
            {%- if b %}
            bbb
            {%- endif %}
            zzz
        "},
    };
    git(&src, &["init"]);
    git(&src, &["add", "-A"]);
    git(&src, &["commit", "-m1"]);
    git(&src, &["tag", "1"]);
    file_tree! {
        src.join("copier.yml") => indoc! {"
            b: false
            c: false
        "},
        src.join("content.jinja") => indoc! {"
            aaa
            {%- if b %}
            bbb
            {%- endif %}
            {%- if c %}
            ccc
            {%- endif %}
            zzz
        "},
    };
    git(&src, &["commit", "-am2"]);
    git(&src, &["tag", "2"]);
    // Init project
    if interactive {
        let mut tui = spawn(&["copy", "-r1", src_str, dst_str]);
        tui.expect_exact("b (bool)");
        tui.expect_exact("(y/N)");
        tui.send("y");
        tui.expect_eof();
    } else {
        worker(&src, &dst)
            .data(v(json!({"b": true})))
            .vcs_ref("1")
            .run_copy()
            .unwrap();
    }
    assert!(!read(dst.join("content")).contains("ccc"));
    git(&dst, &["init"]);
    git(&dst, &["add", "-A"]);
    git(&dst, &["commit", "-m1"]);
    // Project evolution
    std::fs::write(
        dst.join("content"),
        indoc! {"
            aaa
            bbb
            jjj
            zzz
        "},
    )
    .unwrap();
    git(&dst, &["commit", "-am2"]);
    // Update from template, inline, with answer changes
    if interactive {
        let mut tui = spawn_with(&["update", "--conflict=inline"], SPAWN_TIMEOUT, Some(&dst));
        tui.expect_exact("b (bool)");
        tui.expect_exact("(Y/n)");
        tui.sendline("");
        tui.expect_exact("c (bool)");
        tui.expect_exact("(y/N)");
        tui.send("y");
        tui.expect_eof();
    } else {
        Worker::new()
            .dst_path(&dst)
            .data(v(json!({"c": true})))
            .defaults(true)
            .overwrite(true)
            .conflict(ConflictMode::Inline)
            .run_update()
            .unwrap();
    }
    assert_eq!(
        read(dst.join("content")),
        indoc! {"
            aaa
            bbb
            <<<<<<< before updating
            jjj
            =======
            ccc
            >>>>>>> after updating
            zzz
        "}
    );
}

#[rstest]
#[case(ConflictMode::Rej)]
#[case(ConflictMode::Inline)]
#[ignore = "pending"]
fn test_update_in_repo_subdirectory(#[case] conflict: ConflictMode) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    let subdir = PathBuf::from("subdir");

    file_tree! {
        src.join("{{ _copier_conf.answers_file }}.jinja") => "{{ _copier_answers|to_yaml }}",
        src.join("version.txt") => "v1",
    };
    git(&src, &["init"]);
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m1"]);
    git(&src, &["tag", "v1"]);

    worker(&src, dst.join(&subdir)).run_copy().unwrap();

    git(&dst, &["init"]);
    git(&dst, &["add", "."]);
    git(&dst, &["commit", "-m1"]);

    assert!(dst.join(&subdir).join(".copier-answers.yml").is_file());
    assert!(dst.join(&subdir).join("version.txt").is_file());
    assert_eq!(read(dst.join(&subdir).join("version.txt")), "v1");

    file_tree! { dst.join(&subdir).join("version.txt") => "v1 edited" };
    git(&dst, &["add", "."]);
    git(&dst, &["commit", "-m1e"]);

    file_tree! { src.join("version.txt") => "v2" };
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m2"]);
    git(&src, &["tag", "v2"]);

    Worker::new()
        .dst_path(dst.join(&subdir))
        .overwrite(true)
        .conflict(conflict)
        .run_update()
        .unwrap();

    assert!(dst.join(&subdir).join(".copier-answers.yml").is_file());
    assert!(dst.join(&subdir).join("version.txt").is_file());
    if conflict == ConflictMode::Rej {
        assert_eq!(read(dst.join(&subdir).join("version.txt")), "v2");
        assert!(dst.join(&subdir).join("version.txt.rej").is_file());
    } else {
        assert_eq!(
            read(dst.join(&subdir).join("version.txt")),
            indoc! {"
                <<<<<<< before updating
                v1 edited
                =======
                v2
                >>>>>>> after updating
            "}
        );
    }
}

// context_lines=1 and context_lines=2 carry
// xfail(raises=AssertionError, reason="Not enough context lines to resolve the conflict.",
// strict=True); context_lines=3 is expected to pass.
#[rstest]
#[ignore = "pending"]
fn test_update_needs_more_context(
    #[values(1, 2, 3)] context_lines: u32,
    #[values(true, false)] api: bool,
) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    let (src_str, dst_str) = (src.to_str().unwrap(), dst.to_str().unwrap());
    // Create a template where some code blocks are similar
    file_tree! {
        src.join("{{ _copier_conf.answers_file }}.jinja") => "{{ _copier_answers|to_yaml }}",
        src.join("sample.py") => indoc! {r#"
            def function_one():
                print("This line is equal to the next one.")
                print("This line is equal to the next one.")
                print("This line is equal to the next one.")
                print("This line is equal to the next one.")
                print("Previous line lied.")

            def function_two():
                print("This line is equal to the next one.")
                print("This line is equal to the next one.")
                print("This line is equal to the next one.")
                print("This line is equal to the next one.")
                print("Previous line lied.")
        "#},
    };
    git(&src, &["init"]);
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m1"]);
    git(&src, &["tag", "v1"]);
    // Render and evolve that project
    if api {
        worker(&src, &dst).run_copy().unwrap();
    } else {
        run_cli(&["copy", src_str, dst_str]);
    }
    git(&dst, &["init"]);
    git(&dst, &["add", "."]);
    git(&dst, &["commit", "-m1"]);
    file_tree! {
        dst.join("sample.py") => indoc! {r#"
            def function_one():
                print("This line is equal to the next one.")
                print("This line is equal to the next one.")
                print("This line is equal to the next one.")
                print("This line is equal to the next one.")
                print("Previous line lied.")

            def function_two():
                print("This line is equal to the next one.")
                print("This line is equal to the next one.")
                print("This line is new.")
                print("This line is equal to the next one.")
                print("This line is equal to the next one.")
                print("Previous line lied.")
        "#},
    };
    git(&dst, &["commit", "-am2"]);
    // Evolve the template
    file_tree! {
        src.join("sample.py") => indoc! {r#"
            def function_zero():
                print("This line is equal to the next one.")
                print("This line is equal to the next one.")
                print("This line is equal to the next one.")
                print("This line is equal to the next one.")
                print("Previous line lied.")

            def function_one():
                print("This line is equal to the next one.")
                print("This line is equal to the next one.")
                print("This line is equal to the next one.")
                print("This line is equal to the next one.")
                print("Previous line lied.")

            def function_two():
                print("This line is equal to the next one.")
                print("This line is equal to the next one.")
                print("This line is equal to the next one.")
                print("This line is equal to the next one.")
                print("Previous line lied.")
        "#},
    };
    git(&src, &["commit", "-am3"]);
    git(&src, &["tag", "v2"]);
    // Update the project
    if api {
        Worker::new()
            .dst_path(&dst)
            .overwrite(true)
            .conflict(ConflictMode::Inline)
            .context_lines(context_lines)
            .run_update()
            .unwrap();
    } else {
        let output = copier_cmd()
            .args([
                "update",
                dst_str,
                "--conflict=inline",
                &format!("--context-lines={context_lines}"),
            ])
            .output()
            .unwrap();
        assert!(output.status.success());
    }
    // Check the update result
    assert_eq!(
        read(dst.join("sample.py")),
        indoc! {r#"
            def function_zero():
                print("This line is equal to the next one.")
                print("This line is equal to the next one.")
                print("This line is equal to the next one.")
                print("This line is equal to the next one.")
                print("Previous line lied.")

            def function_one():
                print("This line is equal to the next one.")
                print("This line is equal to the next one.")
                print("This line is equal to the next one.")
                print("This line is equal to the next one.")
                print("Previous line lied.")

            def function_two():
                print("This line is equal to the next one.")
                print("This line is equal to the next one.")
                print("This line is new.")
                print("This line is equal to the next one.")
                print("This line is equal to the next one.")
                print("Previous line lied.")
        "#}
    );
}

// Python picks the file name "qu`o'tes" on Windows (double quotes are not supported in
// file names there) and 'qu`o"tes' elsewhere.
#[rstest]
#[ignore = "pending"]
fn test_conflicted_files_are_marked_unmerged(
    #[values("README.md", "spa ces", "qu`o\"tes", "m4â4ñ4a")] filename: &str,
    #[values(false, true)] gitignore_rej: bool,
) {
    // Template in v1 has a file with a single line;
    // in v2 it changes that line.
    // Meanwhile, downstream project appended contents to the first line.
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    // First, create the template with an initial file
    file_tree! {
        src.join(filename) => "upstream version 1",
        src.join("{{_copier_conf.answers_file}}.jinja") => "{{_copier_answers|to_nice_yaml}}",
    };
    git_init(&src, "hello template");
    git(&src, &["tag", "v1"]);

    // Generate the project a first time, assert the file exists
    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    assert!(dst.join(filename).exists());
    assert_eq!(load_answers(&dst).get("_commit"), Some(&Value::from("v1")));

    // Start versioning the generated project
    git_init(&dst, "hello project");

    // Optionally gitignore .rej files. Inline-conflict resolution
    // must still produce inline markers — the .rej is just an
    // internal artifact of the resolution, never something the
    // downstream user wants to see, so it's reasonable for them to
    // ignore it. Regression for the case where copier's `git status
    // --porcelain` invocation silently dropped ignored .rej files.
    if gitignore_rej {
        std::fs::write(dst.join(".gitignore"), "*.rej\n").unwrap();
        git(&dst, &["add", ".gitignore"]);
        git(&dst, &["commit", "-m", "ignore .rej files"]);
    }

    // After first commit, change the file, commit again
    std::fs::write(dst.join(filename), "upstream version 1 + downstream").unwrap();
    git(&dst, &["commit", "-am", "updated file"]);

    // Now change the template
    // Update the file
    std::fs::write(src.join(filename), "upstream version 2").unwrap();

    // Commit the changes
    git(&src, &["add", ".", "-A"]);
    git(&src, &["commit", "-m", "change line in file"]);
    git(&src, &["tag", "v2"]);

    // Finally, update the generated project
    Worker::new()
        .dst_path(&dst)
        .defaults(true)
        .overwrite(true)
        .conflict(ConflictMode::Inline)
        .run_update()
        .unwrap();
    assert_eq!(load_answers(&dst).get("_commit"), Some(&Value::from("v2")));

    // Assert that the file still exists, has inline conflict markers,
    // and is reported as "unmerged" by Git.
    assert!(dst.join(filename).exists());

    let expected_contents = indoc! {"
        <<<<<<< before updating
        upstream version 1 + downstream
        =======
        upstream version 2
        >>>>>>> after updating
    "};
    assert_eq!(
        read(dst.join(filename)).lines().collect::<Vec<_>>(),
        expected_contents.lines().collect::<Vec<_>>()
    );
    assert!(!dst.join(format!("{filename}.rej")).exists());

    let lines = copier::tools::split_nul(&git(&dst, &["status", "-z", "--porcelain=v1"]));
    assert!(lines
        .iter()
        .any(|line| line.starts_with("UU") && line[3..] == *filename));
}

#[test]
#[ignore = "pending"]
fn test_3way_merged_files_without_conflicts_are_not_marked_unmerged() {
    let filename = "readme.md";

    // Template in v1 has a file with a single line;
    // in v2 it changes that line.
    // Meanwhile, downstream project made the same change.
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    // First, create the template with an initial file
    file_tree! {
        src.join(filename) => "upstream version 1",
        src.join("{{_copier_conf.answers_file}}.jinja") => "{{_copier_answers|to_nice_yaml}}",
    };
    git_init(&src, "hello template");
    git(&src, &["tag", "v1"]);

    // Generate the project a first time, assert the file exists
    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    assert!(dst.join(filename).exists());
    assert_eq!(load_answers(&dst).get("_commit"), Some(&Value::from("v1")));

    // Start versioning the generated project
    git_init(&dst, "hello project");

    // After first commit, change the file, commit again
    std::fs::write(dst.join(filename), "upstream version 2").unwrap();
    git(&dst, &["commit", "-am", "updated file"]);

    // Now change the template
    // Update the file
    std::fs::write(src.join(filename), "upstream version 2").unwrap();

    // Commit the changes
    git(&src, &["add", ".", "-A"]);
    git(&src, &["commit", "-m", "change line in file"]);
    git(&src, &["tag", "v2"]);

    // Finally, update the generated project
    Worker::new()
        .dst_path(&dst)
        .defaults(true)
        .overwrite(true)
        .conflict(ConflictMode::Inline)
        .run_update()
        .unwrap();
    assert_eq!(load_answers(&dst).get("_commit"), Some(&Value::from("v2")));

    // Assert that the file still exists, does not have inline conflict markers,
    // and is not reported as "unmerged" by Git.
    assert!(dst.join(filename).exists());

    let expected_contents = "upstream version 2";
    assert_eq!(read(dst.join(filename)), expected_contents);
    assert!(!dst.join(format!("{filename}.rej")).exists());

    let lines = copier::tools::split_nul(&git(&dst, &["status", "-z", "--porcelain=v1"]));
    assert!(!lines
        .iter()
        .any(|line| line.starts_with("UU") && line[3..] == *filename));
}

#[test]
#[ignore = "pending"]
fn test_update_with_new_file_in_template_and_project() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    file_tree! {
        src.join("{{ _copier_conf.answers_file }}.jinja") => "{{ _copier_answers|to_yaml }}",
    };
    git_init(&src, "v1");
    git(&src, &["tag", "v1"]);

    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    assert_eq!(load_answers(&dst).get("_commit"), Some(&Value::from("v1")));

    git_init(&dst, "v1");
    std::fs::write(
        dst.join(".gitlab-ci.yml"),
        indoc! {"
            tests:
                stage: test
                script:
                    - ./test.sh

            pages:
                stage: deploy
                script:
                    - ./deploy.sh
        "},
    )
    .unwrap();
    git(&dst, &["add", "."]);
    git(&dst, &["commit", "-m", "v2"]);

    std::fs::write(
        src.join(".gitlab-ci.yml.jinja"),
        indoc! {"
            tests:
                stage: test
                script:
                    - ./test.sh --slow
        "},
    )
    .unwrap();
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m", "v2"]);
    git(&src, &["tag", "v2"]);

    Worker::new()
        .dst_path(&dst)
        .defaults(true)
        .overwrite(true)
        .conflict(ConflictMode::Inline)
        .run_update()
        .unwrap();
    assert_eq!(load_answers(&dst).get("_commit"), Some(&Value::from("v2")));
    assert_eq!(
        read(dst.join(".gitlab-ci.yml")),
        indoc! {"
            tests:
                stage: test
                script:
            <<<<<<< before updating
                    - ./test.sh

            pages:
                stage: deploy
                script:
                    - ./deploy.sh
            =======
                    - ./test.sh --slow
            >>>>>>> after updating
        "}
    );
}

#[rstest]
#[case("m4â4ñ4a")]
// skipif(platform.system() == "Windows", reason="Double quotes are not supported in file names on Windows.")
#[case("qu\"otes")]
// skipif(platform.system() == "Windows", reason="Control characters are not supported in file names on Windows.")
#[case("tab\there")]
// skipif(platform.system() == "Windows", reason="Control characters are not supported in file names on Windows.")
#[case("line\nbreak")]
#[ignore = "pending"]
fn test_update_with_new_file_in_template_and_project_with_special_characters(
    #[case] filename: &str,
) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    file_tree! {
        src.join("{{ _copier_conf.answers_file }}.jinja") => "{{ _copier_answers|to_yaml }}",
    };
    git_init(&src, "v1");
    git(&src, &["tag", "v1"]);

    worker(&src, &dst).run_copy().unwrap();

    git_init(&dst, "v1");
    std::fs::write(dst.join(filename), "project\n").unwrap();
    git(&dst, &["add", "."]);
    git(&dst, &["commit", "-m", "v2"]);

    std::fs::write(src.join(filename), "template\n").unwrap();
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m", "v2"]);
    git(&src, &["tag", "v2"]);

    Worker::new()
        .dst_path(&dst)
        .overwrite(true)
        .conflict(ConflictMode::Inline)
        .run_update()
        .unwrap();
    assert_eq!(load_answers(&dst).get("_commit"), Some(&Value::from("v2")));
    assert_eq!(
        read(dst.join(filename)),
        indoc! {"
            <<<<<<< before updating
            project
            =======
            template
            >>>>>>> after updating
        "}
    );
}

/// Merge conflicts are yielded when both template and project add same file.
///
/// The project adds content to `.gitlab-ci.yml` on top of what template v1 provides.
/// In a template v2, `.gitlab-ci.yml.jinja` is moved to `.gitlab/ci/main.yml.jinja`
/// and `.gitlab-ci.yml.jinja` now includes the generated `.gitlab/ci/main.yml`. To
/// retain the project's changes/additions to `.gitlab-ci.yml`, a pre-update migration
/// task copies `.gitlab-ci.yml` (containing those changes/additions) to
/// `.gitlab/ci/main.yml` and stages it, then Copier applies template v2's version of
/// that file (which was also moved there, but Git doesn't recognize it as status `R`
/// but as `A`).
#[test]
#[ignore = "pending"]
fn test_update_with_new_file_in_template_and_project_via_migration() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    file_tree! {
        src.join("{{ _copier_conf.answers_file }}.jinja") => "{{ _copier_answers|to_yaml }}",
        src.join(".gitlab-ci.yml.jinja") => indoc! {"
            tests:
                stage: test
                script:
                    - ./test.sh
        "},
    };
    git_init(&src, "v1");
    git(&src, &["tag", "v1"]);

    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    assert_eq!(load_answers(&dst).get("_commit"), Some(&Value::from("v1")));
    assert!(dst.join(".gitlab-ci.yml").exists());

    {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(dst.join(".gitlab-ci.yml"))
            .unwrap();
        f.write_all(
            indoc! {"

                pages:
                    stage: deploy
                    script:
                        - ./deploy.sh
            "}
            .as_bytes(),
        )
        .unwrap();
    }
    git_init(&dst, "v1");

    let old_file = src.join(".gitlab-ci.yml.jinja");
    let new_file = src.join(".gitlab").join("ci").join("main.yml.jinja");
    std::fs::create_dir_all(new_file.parent().unwrap()).unwrap();
    // Move `.gitlab-ci.yml.jinja` to `.gitlab/ci/main.yml.jinja`
    git(
        &src,
        &["mv", ".gitlab-ci.yml.jinja", ".gitlab/ci/main.yml.jinja"],
    );
    // Make a small modification in `.gitlab/ci/main.yml.jinja`
    std::fs::write(
        &new_file,
        read(&new_file).replace("test.sh", "test.sh --slow"),
    )
    .unwrap();
    // Include `.gitlab/ci/main.yml.jinja` in `.gitlab-ci.yml.jinja`
    std::fs::write(
        &old_file,
        indoc! {"
            include:
                - local: .gitlab/ci/main.yml
        "},
    )
    .unwrap();
    // Add a pre-migration that copies `.gitlab-ci.yml` to
    // `.gitlab/ci/main.yml` and stages it, so that the user changes made in
    // the project are retained after moving the file.
    file_tree! {
        src.join("copier.yml") => indoc! {r#"
            _migrations:
            -   version: v2
                when: "{{ _stage == 'before' }}"
                command: "{{ _copier_python }} {{ _copier_conf.src_path / 'migrate.py' }}"
        "#},
        src.join("migrate.py") => indoc! {r#"
            from pathlib import Path
            from plumbum.cmd import git

            f = Path(".gitlab", "ci", "main.yml")
            f.parent.mkdir(parents=True)
            f.write_text(Path(".gitlab-ci.yml").read_text())
            git("add", f)
        "#},
    };
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m", "v2"]);
    git(&src, &["tag", "v2"]);

    Worker::new()
        .dst_path(&dst)
        .defaults(true)
        .overwrite(true)
        .conflict(ConflictMode::Inline)
        .unsafe_(true)
        .run_update()
        .unwrap();
    assert_eq!(load_answers(&dst).get("_commit"), Some(&Value::from("v2")));
    assert_eq!(
        read(dst.join(".gitlab-ci.yml")),
        indoc! {"
            <<<<<<< before updating
            tests:
                stage: test
                script:
                    - ./test.sh

            pages:
                stage: deploy
                script:
                    - ./deploy.sh
            =======
            include:
                - local: .gitlab/ci/main.yml
            >>>>>>> after updating
        "}
    );
    assert_eq!(
        read(dst.join(".gitlab").join("ci").join("main.yml")),
        indoc! {"
            tests:
                stage: test
                script:
            <<<<<<< before updating
                    - ./test.sh

            pages:
                stage: deploy
                script:
                    - ./deploy.sh
            =======
                    - ./test.sh --slow
            >>>>>>> after updating
        "}
    );
}

#[test]
#[ignore = "pending"]
fn test_update_with_separate_git_directory() {
    let tmp = Tmp::new();
    let src = tmp.mktemp("src");
    let dst = tmp.mktemp("dst");
    let dst_git_dir = tmp.mktemp("dst_git_dir");

    file_tree! {
        src.join("version.txt") => "v1",
        src.join("{{ _copier_conf.answers_file }}.jinja") => "{{ _copier_answers|to_nice_yaml }}",
    };
    git(&src, &["init"]);
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m1"]);
    git(&src, &["tag", "v1"]);

    worker(&src, &dst).overwrite(true).run_copy().unwrap();
    assert_eq!(load_answers(&dst).get("_commit"), Some(&Value::from("v1")));

    git(
        &dst,
        &["init", "--separate-git-dir", dst_git_dir.to_str().unwrap()],
    );
    // Add a file to make sure the subproject's tree object is different from
    // that of the fresh copy from the old template version; otherwise, we
    // cannot test the linking of local (temporary) repositories for
    // borrowing Git objects.
    file_tree! { dst.join("foo.txt") => "bar" };
    git(&dst, &["add", "."]);
    git(&dst, &["commit", "-m1"]);

    file_tree! { src.join("version.txt") => "v2" };
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m2"]);
    git(&src, &["tag", "v2"]);

    Worker::new()
        .dst_path(&dst)
        .overwrite(true)
        .run_update()
        .unwrap();
    assert_eq!(load_answers(&dst).get("_commit"), Some(&Value::from("v2")));
}

#[test]
#[ignore = "pending"]
fn test_update_with_skip_answered_and_new_answer() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    file_tree! {
        src.join("copier.yml") => "boolean: false",
        src.join("{{ _copier_conf.answers_file }}.jinja") => "{{ _copier_answers|to_nice_yaml }}",
    };
    git_init(&src, "v1");
    git(&src, &["tag", "v1"]);

    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    let answers = load_answers(&dst);
    assert_eq!(answers["boolean"], Value::Bool(false));

    git_init(&dst, "v1");

    Worker::new()
        .dst_path(&dst)
        .data(v(json!({"boolean": "true"})))
        .skip_answered(true)
        .overwrite(true)
        .run_update()
        .unwrap();
    let answers = load_answers(&dst);
    assert_eq!(answers["boolean"], Value::Bool(true));
}

#[rstest]
#[ignore = "pending"]
fn test_update_vcs_ref_current(#[values(true, false)] cli: bool) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    let (src_str, dst_str) = (src.to_str().unwrap(), dst.to_str().unwrap());

    file_tree! {
        src.join("copier.yml") => "boolean: false",
        src.join("{{ _copier_conf.answers_file }}.jinja") => "{{ _copier_answers|to_nice_yaml }}",
    };
    git_init(&src, "v1");
    git(&src, &["tag", "v1"]);

    if cli {
        run_cli(&["copy", src_str, dst_str, "--defaults", "--overwrite"]);
    } else {
        worker(&src, &dst)
            .defaults(true)
            .overwrite(true)
            .run_copy()
            .unwrap();
    }
    let answers = load_answers(&dst);
    assert_eq!(answers["_commit"], Value::from("v1"));
    assert_eq!(answers["boolean"], Value::Bool(false));

    git_init(&dst, "v1");

    file_tree! { src.join("README.md") => "# Template Update" };
    git_save(&src, "update template", Some("v2"));

    if cli {
        run_cli(&[
            "update",
            "--data",
            "boolean=true",
            "--vcs-ref=:current:",
            dst_str,
        ]);
    } else {
        Worker::new()
            .dst_path(&dst)
            .data(v(json!({"boolean": "true"})))
            .vcs_ref(VcsRef::Current)
            .overwrite(true)
            .run_update()
            .unwrap();
    }
    let answers = load_answers(&dst);
    assert_eq!(answers["_commit"], Value::from("v1"));
    assert_eq!(answers["boolean"], Value::Bool(true));

    // assert that the README.md file was not created
    assert!(!dst.join("README.md").exists());
}

#[test]
#[ignore = "pending"]
fn test_update_dont_validate_computed_value() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    file_tree! {
        src.join("copier.yml") => indoc! {r#"
            computed:
                type: str
                default: foo
                when: false
                validator: "This validator should never be rendered"
        "#},
        src.join("{{ _copier_conf.answers_file }}.jinja") => "{{ _copier_answers|to_nice_yaml }}",
    };
    git(&src, &["init"]);
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m1"]);
    git(&src, &["tag", "v1"]);

    worker(&src, &dst).overwrite(true).run_copy().unwrap();
    let answers = load_answers(&dst);
    assert!(!answers.contains_key("computed"));

    git(&dst, &["init"]);
    git(&dst, &["add", "."]);
    git(&dst, &["commit", "-m1"]);

    Worker::new()
        .dst_path(&dst)
        .overwrite(true)
        .run_update()
        .unwrap();
    let answers = load_answers(&dst);
    assert!(!answers.contains_key("computed"));
}

#[test]
#[ignore = "pending"]
fn test_update_git_submodule() {
    let tmp = Tmp::new();
    let src = tmp.mktemp("src");
    let dst = tmp.mktemp("dst");
    let submodule = tmp.mktemp("submodule");

    file_tree! {
        src.join("{{ _copier_conf.answers_file }}.jinja") => "{{ _copier_answers|to_yaml }}",
        src.join("version.txt") => "v1",
    };
    git(&src, &["init"]);
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m1"]);
    git(&src, &["tag", "v1"]);

    worker(&src, &submodule).run_copy().unwrap();

    git(&submodule, &["init"]);
    git(&submodule, &["add", "."]);
    git(&submodule, &["commit", "-m1"]);

    assert!(submodule.join(".copier-answers.yml").is_file());
    assert!(submodule.join("version.txt").is_file());
    assert_eq!(read(submodule.join("version.txt")), "v1");

    git(&dst, &["init"]);
    // See https://github.com/git/git/security/advisories/GHSA-3wp6-j8xr-qw85
    // for more details on why we need to set `protocol.file.allow=always` to
    // be able to clone a local submodule.
    git(
        &dst,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            submodule.to_str().unwrap(),
            "sub",
        ],
    );
    git(&dst, &["add", "."]);
    git(&dst, &["commit", "-m", "add submodule"]);

    assert!(dst.join("sub").join(".copier-answers.yml").is_file());
    assert!(dst.join("sub").join("version.txt").is_file());
    assert_eq!(read(dst.join("sub").join("version.txt")), "v1");

    file_tree! { src.join("version.txt") => "v2" };
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m2"]);
    git(&src, &["tag", "v2"]);

    Worker::new()
        .dst_path(dst.join("sub"))
        .overwrite(true)
        .run_update()
        .unwrap();

    assert!(dst.join("sub").join(".copier-answers.yml").is_file());
    assert!(dst.join("sub").join("version.txt").is_file());
    assert_eq!(read(dst.join("sub").join("version.txt")), "v2");
}

#[test]
#[ignore = "pending"]
fn test_gitignore_file_unignored() {
    let env_file = ".env";
    let gitignore_file = ".gitignore";

    // Template in v1 has a file with a single line;
    // in v2 it changes that line.
    // Meanwhile, downstream project made the same change.
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    // First, create the template with an initial file
    file_tree! {
        src.join(gitignore_file) => "",
        src.join(env_file) => "",
        src.join("{{_copier_conf.answers_file}}.jinja") => "{{_copier_answers|to_nice_yaml}}",
    };

    git_init(&src, "hello template");
    git(&src, &["tag", "v1"]);

    // Generate the project a first time, assert the file exists
    worker(&src, &dst).run_copy().unwrap();
    for f in [env_file, gitignore_file] {
        assert!(dst.join(f).exists());
    }
    assert_eq!(load_answers(&dst).get("_commit"), Some(&Value::from("v1")));

    // Start versioning the generated project
    git_init(&dst, "hello project");

    // Add a file to the `.gitignore` file
    std::fs::write(src.join(gitignore_file), env_file).unwrap();
    git(&src, &["commit", "-am", "ignore file"]);
    git(&src, &["tag", "v2"]);

    // Update the generated project
    Worker::new()
        .dst_path(&dst)
        .overwrite(true)
        .run_update()
        .unwrap();
    assert_eq!(load_answers(&dst).get("_commit"), Some(&Value::from("v2")));

    // Commit project changes.
    git(&dst, &["commit", "-am", "update to template v2"]);

    // Remove the file previously added to `.gitignore`
    std::fs::write(src.join(gitignore_file), "").unwrap();
    git(&src, &["commit", "-am", "un-ignore file"]);
    git(&src, &["tag", "v3"]);

    // Update the generated project.
    // This would fail if `git add` was called without the `--force` flag;
    // Otherwise, it should succeed.
    Worker::new()
        .dst_path(&dst)
        .overwrite(true)
        .run_update()
        .unwrap();
    assert_eq!(load_answers(&dst).get("_commit"), Some(&Value::from("v3")));
}

#[test]
#[ignore = "pending"]
fn test_update_keeps_template_generated_gitignored_file() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    file_tree! {
        src.join("{{ _copier_conf.answers_file }}.jinja") => "{{ _copier_answers|to_yaml }}",
        src.join(".env.jinja") => "VERSION=v1",
    };
    git_init(&src, "v1");
    git(&src, &["tag", "v1"]);

    file_tree! { src.join(".env.jinja") => "VERSION=v2" };
    git(&src, &["commit", "-am2"]);
    git(&src, &["tag", "v2"]);

    worker(&src, &dst).vcs_ref("v1").run_copy().unwrap();
    assert_eq!(read(dst.join(".env")), "VERSION=v1");
    std::fs::write(dst.join(".gitignore"), ".env\n").unwrap();
    git(&dst, &["init"]);
    git(&dst, &["add", "."]);
    assert!(!git(&dst, &["ls-files", "--", ".env"])
        .lines()
        .any(|line| line == ".env"));
    git(&dst, &["commit", "-m1"]);

    Worker::new()
        .dst_path(&dst)
        .overwrite(true)
        .run_update()
        .unwrap();
    assert_eq!(read(dst.join(".env")), "VERSION=v2");
}

#[test]
#[ignore = "pending"]
fn test_update_with_answers_with_umlaut() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    file_tree! {
        src.join("copier.yml") => indoc! {"
            umlaut:
                type: str
        "},
        src.join("{{ _copier_conf.answers_file }}.jinja") => "{{ _copier_answers|to_nice_yaml }}",
    };
    git_init(&src, "v1");
    git(&src, &["tag", "v1"]);

    worker(&src, &dst)
        .data(v(json!({"umlaut": "äöü"})))
        .overwrite(true)
        .run_copy()
        .unwrap();
    let answers = load_answers(&dst);
    assert_eq!(answers["umlaut"], Value::from("äöü"));

    git_init(&dst, "v1");

    Worker::new()
        .dst_path(&dst)
        .skip_answered(true)
        .overwrite(true)
        .run_update()
        .unwrap();
    let answers = load_answers(&dst);
    assert_eq!(answers["umlaut"], Value::from("äöü"));
}

#[test]
#[ignore = "pending"]
fn test_conflict_on_update_with_unicode_in_content() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("{{ _copier_conf.answers_file }}.jinja") => "{{ _copier_answers|to_yaml }}",
        src.join("copier.yml") => "b: false",
        src.join("content.jinja") => indoc! {"
            aaa🐍
            {%- if b %}
            bbb🐍
            {%- endif %}
            zzz🐍
        "},
    };
    git(&src, &["init"]);
    git(&src, &["add", "-A"]);
    git(&src, &["commit", "-m1"]);
    git(&src, &["tag", "1"]);
    file_tree! {
        src.join("copier.yml") => indoc! {"
            b: false
            c: false
        "},
        src.join("content.jinja") => indoc! {"
            aaa🐍
            {%- if b %}
            bbb🐍
            {%- endif %}
            {%- if c %}
            ccc🐍
            {%- endif %}
            zzz🐍
        "},
    };
    git(&src, &["commit", "-am2"]);
    git(&src, &["tag", "2"]);
    // Init project
    worker(&src, &dst)
        .data(v(json!({"b": true})))
        .vcs_ref("1")
        .run_copy()
        .unwrap();
    assert!(!read(dst.join("content")).contains("ccc"));
    git(&dst, &["init"]);
    git(&dst, &["add", "-A"]);
    git(&dst, &["commit", "-m1"]);
    // Project evolution
    std::fs::write(
        dst.join("content"),
        indoc! {"
            aaa🐍
            bbb🐍
            jjj🐍
            zzz🐍
        "},
    )
    .unwrap();
    git(&dst, &["commit", "-am2"]);
    // Update from template, inline, with answer changes
    Worker::new()
        .dst_path(&dst)
        .data(v(json!({"c": true})))
        .defaults(true)
        .overwrite(true)
        .conflict(ConflictMode::Inline)
        .run_update()
        .unwrap();
    assert_eq!(
        read(dst.join("content")),
        indoc! {"
            aaa🐍
            bbb🐍
            <<<<<<< before updating
            jjj🐍
            =======
            ccc🐍
            >>>>>>> after updating
            zzz🐍
        "}
    );
}

#[test]
#[ignore = "pending"]
fn test_conditional_computed_value() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    file_tree! {
        src.join("copier.yml") => indoc! {r#"
            first:
                type: bool

            second:
                type: bool
                default: "{{ first }}"
                when: "{{ first }}"
        "#},
        src.join("{{ _copier_conf.answers_file }}.jinja") => "{{ _copier_answers|to_nice_yaml }}",
        src.join("log.txt.jinja") => "{{ first }} {{ second }}",
    };
    git_init(&src, "v1");
    git(&src, &["tag", "v1"]);

    worker(&src, &dst)
        .data(v(json!({"first": true})))
        .defaults(true)
        .run_copy()
        .unwrap();
    let answers = load_answers(&dst);
    assert_eq!(answers["first"], Value::Bool(true));
    assert_eq!(answers["second"], Value::Bool(true));
    assert_eq!(read(dst.join("log.txt")), "True True");

    git_init(&dst, "v1");

    Worker::new()
        .dst_path(&dst)
        .data(v(json!({"first": false})))
        .overwrite(true)
        .run_update()
        .unwrap();
    let answers = load_answers(&dst);
    assert_eq!(answers["first"], Value::Bool(false));
    assert!(!answers.contains_key("second"));
    assert_eq!(read(dst.join("log.txt")), "False False");

    git(&dst, &["add", "."]);
    git(&dst, &["commit", "-m", "v2"]);

    Worker::new()
        .dst_path(&dst)
        .data(v(json!({"first": true})))
        .defaults(true)
        .overwrite(true)
        .run_update()
        .unwrap();
    let answers = load_answers(&dst);
    assert_eq!(answers["first"], Value::Bool(true));
    assert_eq!(answers["second"], Value::Bool(true));
    assert_eq!(read(dst.join("log.txt")), "True True");
}

#[test]
#[ignore = "pending"]
fn test_disable_secret_validator_on_replay() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    file_tree! {
        src.join("copier.yml") => indoc! {r#"
            token:
                type: str
                secret: true
                default: ""
                validator: "{% if token == '' %}Must not be empty{% endif %}"
        "#},
        src.join("{{ _copier_conf.answers_file }}.jinja") => "{{ _copier_answers|to_nice_yaml }}",
        src.join(".gitignore") => ".env",
        src.join(".env.jinja") => "TOKEN={{ token }}",
    };
    git_init(&src, "v1");
    git(&src, &["tag", "v1"]);

    worker(&src, &dst).defaults(true).run_copy().unwrap();
    let answers = load_answers(&dst);
    assert_eq!(
        answers,
        v(json!({"_src_path": src.to_str().unwrap(), "_commit": "v1"}))
    );
    assert_eq!(read(dst.join(".env")), "TOKEN=");

    git_init(&dst, "v1");

    Worker::new()
        .dst_path(&dst)
        .data(v(json!({"token": "$up3r-$3cr3t"})))
        .overwrite(true)
        .run_update()
        .unwrap();
    let answers = load_answers(&dst);
    assert_eq!(
        answers,
        v(json!({"_src_path": src.to_str().unwrap(), "_commit": "v1"}))
    );
    assert_eq!(read(dst.join(".env")), "TOKEN=$up3r-$3cr3t");
}

#[test]
#[ignore = "pending"]
fn test_exclude_added_in_new_version() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    file_tree! {
        src.join("{{ _copier_conf.answers_file }}.jinja") => "{{ _copier_answers|to_yaml }}",
        src.join("copy-only") => "foo",
        src.join("copy-and-update") => "foo",
    };
    git_save(&src, "Test commit", Some("v1"));
    file_tree! {
        src.join("copier.yml") => "_exclude:\n - copy-only",
        src.join("copy-only") => "bar",
        src.join("copy-and-update") => "bar",
    };
    git_save(&src, "Test commit", Some("v2"));
    let copy_only = dst.join("copy-only");
    let copy_and_update = dst.join("copy-and-update");

    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .vcs_ref("v1")
        .run_copy()
        .unwrap();
    for file in [&copy_only, &copy_and_update] {
        assert!(file.exists());
        assert_eq!(read(file), "foo");
    }

    git_save(&dst, "Test commit", None);

    Worker::new()
        .dst_path(&dst)
        .overwrite(true)
        .run_update()
        .unwrap();
    assert_eq!(read(&copy_only), "foo");
    assert_eq!(read(&copy_and_update), "bar");
}

#[test]
#[ignore = "pending"]
fn test_skip_if_exists_templated() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    file_tree! {
        src.join("copier.yml") => indoc! {r#"
            _skip_if_exists:
                - "{{ 'skip-if-exists.txt' }}"
        "#},
        src.join("{{ _copier_conf.answers_file }}.jinja") => "{{ _copier_answers|to_yaml }}",
        src.join("skip-if-exists.txt") => "foo",
    };
    git_save(&src, "Test commit", Some("v1"));
    file_tree! { src.join("skip-if-exists.txt") => "bar" };
    git_save(&src, "Test commit", Some("v2"));

    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .vcs_ref("v1")
        .run_copy()
        .unwrap();
    assert_eq!(read(dst.join("skip-if-exists.txt")), "foo");

    git_save(&dst, "Test commit", None);
    file_tree! { dst.join("skip-if-exists.txt") => "baz" };
    git_save(&dst, "Test commit", None);

    Worker::new()
        .dst_path(&dst)
        .overwrite(true)
        .run_update()
        .unwrap();
    assert_eq!(read(dst.join("skip-if-exists.txt")), "baz");
}

/// See https://github.com/copier-org/copier/issues/2448.
#[test]
#[ignore = "pending"]
fn test_render_copier_conf_as_json_without_circular_reference() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    file_tree! {
        src.join("{{ _copier_conf.answers_file }}.jinja") => "{{ _copier_answers|to_yaml }}",
        src.join("copier_conf.json.jinja") => "{{ _copier_conf|to_json }}",
    };
    git_save(&src, "Test commit", Some("v1"));

    worker(&src, &dst)
        .overwrite(true)
        .unsafe_(true)
        .run_copy()
        .unwrap();
    assert!(dst.join("copier_conf.json").exists());

    git_save(&dst, "Test commit", None);

    Worker::new()
        .dst_path(&dst)
        .overwrite(true)
        .unsafe_(true)
        .run_update()
        .unwrap();
    assert!(dst.join("copier_conf.json").exists());
}

/// A template that gains `+x` between versions (without changing the
/// file's content) must propagate that bit to existing destinations on
/// `copier update`.
// skipif(condition=platform.system() == "Windows", reason="Windows does not have UNIX-like permissions")
#[cfg(unix)]
#[rstest]
#[ignore = "pending"]
fn test_update_propagates_executable_bit_addition(#[values(true, false)] file_mode: bool) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    // Template v1: launcher.sh is NOT executable.
    file_tree! {
        src.join("{{ _copier_conf.answers_file }}.jinja") => "{{ _copier_answers|to_nice_yaml }}",
        src.join("launcher.sh") => "#!/bin/sh\necho hi\n",
    };
    git_save(&src, "Test commit", Some("v1"));

    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    assert_eq!(st_mode(&dst.join("launcher.sh")) & 0o111, 0);
    git(&dst, &["init"]);
    git(&dst, &["config", "core.fileMode", &file_mode.to_string()]);
    git(&dst, &["add", "-A"]);
    git(&dst, &["commit", "-m", "init"]);
    // TODO: simplify with `--format %(objectmode)` once the minimum
    // git version is raised to 2.38+ (--format cannot be combined with
    // --stage; see git-ls-files(1)).
    let mode = index_mode(&dst);
    assert_eq!(mode, "100644");

    // Template v2: launcher.sh gains +x with no content change.
    chmod_add(&src.join("launcher.sh"), 0o100 | 0o010 | 0o001);
    git(&src, &["commit", "-am", "make executable"]);
    git(&src, &["tag", "v2"]);

    Worker::new()
        .dst_path(&dst)
        .defaults(true)
        .overwrite(true)
        .run_update()
        .unwrap();

    // On disk, the bit must be set after the update.
    assert_ne!(st_mode(&dst.join("launcher.sh")) & 0o111, 0);
    // TODO: simplify with `--format %(objectmode)` once the minimum
    // git version is raised to 2.38+ (--format cannot be combined with
    // --stage; see git-ls-files(1)).
    let mode = index_mode(&dst);
    // Preserve the leading space — porcelain columns are index (XY) and
    // worktree, and a leading space distinguishes " M" (unstaged) from
    // "M " (staged). `.strip()` would swallow the space.
    let status = git(&dst, &["status", "--porcelain", "--", "launcher.sh"])
        .trim_end_matches('\n')
        .to_owned();
    if file_mode {
        // With `core.fileMode=true`, copier must NOT stage the index mode
        // change — git picks up the on-disk `chmod` as an unstaged
        // modification, matching copier's normal behavior of leaving
        // rendered changes unstaged for user review. Guards against
        // auto-staging regression.
        assert_eq!(mode, "100644");
        assert!(
            status.starts_with(" M"),
            "expected unstaged change, got {status:?}"
        );
        // A subsequent `git add` records 100755 via git's normal flow.
        git(&dst, &["add", "--", "launcher.sh"]);
        let mode_after_add = index_mode(&dst);
        assert_eq!(mode_after_add, "100755");
    } else {
        // With `core.fileMode=false`, git would ignore the on-disk chmod,
        // so copier must explicitly call `git update-index --chmod=+x`
        // to record the new mode in the index.
        assert_eq!(mode, "100755");
    }
}

/// When a template drops the executable bit between versions, the
/// destination's on-disk mode and git index must lose `+x` too.
// skipif(condition=platform.system() == "Windows", reason="Windows does not have UNIX-like permissions")
#[cfg(unix)]
#[rstest]
#[ignore = "pending"]
fn test_update_propagates_executable_bit_removal(#[values(true, false)] file_mode: bool) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    file_tree! {
        src.join("{{ _copier_conf.answers_file }}.jinja") => "{{ _copier_answers|to_nice_yaml }}",
        src.join("launcher.sh") => "#!/bin/sh\necho hi\n",
    };
    chmod_add(&src.join("launcher.sh"), 0o100 | 0o010 | 0o001);
    git_save(&src, "Test commit", Some("v1"));

    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    git(&dst, &["init"]);
    git(&dst, &["config", "core.fileMode", &file_mode.to_string()]);
    git(&dst, &["add", "-A"]);
    if !file_mode {
        // With core.fileMode=false, git add ignores on-disk exec bits,
        // so we must set the index mode explicitly to start at 100755.
        git(&dst, &["update-index", "--chmod=+x", "--", "launcher.sh"]);
    }
    git(&dst, &["commit", "-m", "init"]);
    // TODO: simplify with `--format %(objectmode)` once the minimum
    // git version is raised to 2.38+ (--format cannot be combined with
    // --stage; see git-ls-files(1)).
    let mode = index_mode(&dst);
    assert_eq!(mode, "100755");

    // Template v2: drop +x (no content change).
    chmod_remove(&src.join("launcher.sh"), 0o100 | 0o010 | 0o001);
    git(&src, &["commit", "-am", "drop executable"]);
    git(&src, &["tag", "v2"]);

    Worker::new()
        .dst_path(&dst)
        .defaults(true)
        .overwrite(true)
        .run_update()
        .unwrap();

    assert_eq!(st_mode(&dst.join("launcher.sh")) & 0o111, 0);
    // TODO: simplify with `--format %(objectmode)` once the minimum
    // git version is raised to 2.38+ (--format cannot be combined with
    // --stage; see git-ls-files(1)).
    let mode = index_mode(&dst);
    // Preserve the leading space — porcelain columns are index (XY) and
    // worktree, and a leading space distinguishes " M" (unstaged) from
    // "M " (staged). `.strip()` would swallow the space.
    let status = git(&dst, &["status", "--porcelain", "--", "launcher.sh"])
        .trim_end_matches('\n')
        .to_owned();
    if file_mode {
        // Same rationale as the addition test: with `core.fileMode=true`
        // the chmod should be visible to git as unstaged, and copier must
        // not pre-stage it via `git update-index --chmod`.
        assert_eq!(mode, "100755");
        assert!(
            status.starts_with(" M"),
            "expected unstaged change, got {status:?}"
        );
        git(&dst, &["add", "--", "launcher.sh"]);
        let mode_after_add = index_mode(&dst);
        assert_eq!(mode_after_add, "100644");
    } else {
        assert_eq!(mode, "100644");
    }
}

/// A Windows user running `copier update` on a template that was
/// authored on Linux/macOS must see executable-bit changes propagate
/// to the destination's git index.
///
/// On Windows, git's default `core.fileMode=false` means the
/// executable bit is never represented on disk, only in the index.
/// Copier cannot rely on `os.stat().st_mode` to read the template's
/// intended mode — it must consult the template's git index instead.
/// This test sets up the template using only `git update-index
/// --chmod` (no on-disk `chmod`, which would be a no-op on Windows
/// anyway) and verifies the bit survives end-to-end into the
/// destination's index.
// skipif(condition=platform.system() != "Windows", reason="Exercises the Windows code path where the
// filesystem does not represent UNIX-like mode bits.")
#[cfg(windows)]
#[test]
#[ignore = "pending"]
fn test_update_preserves_exec_bit_authored_on_unix() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    // Template v1: launcher.sh is NOT executable. Set up the git index
    // entry without touching on-disk mode bits — mimics how a
    // Linux-authored commit appears to a Windows clone.
    file_tree! {
        src.join("{{ _copier_conf.answers_file }}.jinja") => "{{ _copier_answers|to_nice_yaml }}",
        src.join("launcher.sh") => "#!/bin/sh\necho hi\n",
    };
    git_save(&src, "Test commit", Some("v1"));

    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    git(&dst, &["init"]);
    // Windows default — explicit here to avoid relying on it.
    git(&dst, &["config", "core.fileMode", "false"]);
    git(&dst, &["add", "-A"]);
    git(&dst, &["commit", "-m", "init"]);
    // TODO: simplify with `--format %(objectmode)` once the minimum
    // git version is raised to 2.38+ (--format cannot be combined with
    // --stage; see git-ls-files(1)).
    let mode = index_mode(&dst);
    assert_eq!(mode, "100644");

    // Template v2: launcher.sh gains +x via `git update-index --chmod`
    // only. No on-disk `chmod` — this is the scenario where the
    // template author committed `100755` from Linux, and the
    // destination is a filesystem that does not carry exec bits.
    git(&src, &["update-index", "--chmod=+x", "launcher.sh"]);
    git(&src, &["commit", "-am", "make executable"]);
    git(&src, &["tag", "v2"]);

    Worker::new()
        .dst_path(&dst)
        .defaults(true)
        .overwrite(true)
        .run_update()
        .unwrap();

    // The destination's git index must record 100755, even though
    // `src_abspath.stat().st_mode` on Windows would not show the exec
    // bit. This only works because copier reads the template's intended
    // mode from its git index via `Template.git_index_modes`.
    let mode = index_mode(&dst);
    assert_eq!(mode, "100755");
}

/// `copier update` on a file that gains `+x` AND has a
/// conflicting local edit must register a proper merge conflict in the
/// index — the executable-bit plumbing must not silently collapse the
/// conflict stages.
///
/// Regression guard: `git update-index --chmod` on a file with
/// stages 1/2/3 collapses them into stage 0, destroying the conflict.
/// Copier only avoids this because `_sync_git_index_executable_bit`
/// runs in `_render_file` *before* the conflict registration in
/// `_apply_update`. This test locks in that ordering guarantee
/// against future refactors.
// skipif(condition=platform.system() == "Windows", reason="Windows does not have UNIX-like permissions")
#[cfg(unix)]
#[rstest]
#[ignore = "pending"]
fn test_update_with_exec_bit_change_and_merge_conflict(#[values(true, false)] file_mode: bool) {
    let filename = "launcher.sh";
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    // Template v1: non-exec launcher.sh with one line of content.
    file_tree! {
        src.join(filename) => "upstream version 1",
        src.join("{{_copier_conf.answers_file}}.jinja") => "{{_copier_answers|to_nice_yaml}}",
    };
    git_init(&src, "hello template");
    git(&src, &["tag", "v1"]);

    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    git_init(&dst, "hello project");
    if !file_mode {
        git(&dst, &["config", "core.fileMode", "false"]);
    }
    // Downstream edit that conflicts with template v2.
    std::fs::write(dst.join(filename), "upstream version 1 + downstream").unwrap();
    git(&dst, &["commit", "-am", "downstream edit"]);

    // Template v2: gain +x AND change the same line (conflicts with the
    // downstream edit above).
    std::fs::write(src.join(filename), "upstream version 2").unwrap();
    chmod_add(&src.join(filename), 0o100 | 0o010 | 0o001);
    git(&src, &["add", ".", "-A"]);
    git(
        &src,
        &["commit", "-m", "make executable and update content"],
    );
    git(&src, &["tag", "v2"]);

    Worker::new()
        .dst_path(&dst)
        .defaults(true)
        .overwrite(true)
        .conflict(ConflictMode::Inline)
        .run_update()
        .unwrap();

    // The conflict must still be registered. If our `update-index`
    // index manipulation were called after stages 1/2/3 were registered,
    // it would collapse them into stage 0 and the conflict would
    // silently disappear. Quick human-readable sanity check that
    // conflict markers were written to the working tree:
    assert!(read(dst.join(filename)).contains("<<<<<<< before updating"));
    // Snapshot the full human-readable `git status` — a natural user
    // view of the post-update state. If `_sync_git_index_executable_bit`
    // ever regresses into stomping conflict stages, this snapshot will
    // diverge immediately. `git diff` is scoped to `filename` to
    // avoid unstable tmp-path strings that otherwise appear in the
    // `.copier-answers.yml` diff (`_src_path` points at the test's
    // tmp dir).
    // `editorconfig-checker-disable` / `-enable` wrap the snapshots
    // below because git's output format (2-space indents on hints, tabs
    // on unmerged-path entries, 1-space diff context prefix) violates
    // copier's `.editorconfig` rules, which the CI pre-commit hook
    // enforces. The snapshots are literal copies of git's output, so
    // editing them would defeat the regression guard.
    // editorconfig-checker-disable
    assert_eq!(
        git(&dst, &["status"]),
        [
            "On branch main",
            "Unmerged paths:",
            "  (use \"git restore --staged <file>...\" to unstage)",
            "  (use \"git add <file>...\" to mark resolution)",
            "\tboth modified:   launcher.sh",
            "",
            "Changes not staged for commit:",
            "  (use \"git add <file>...\" to update what will be committed)",
            "  (use \"git restore <file>...\" to discard changes in working directory)",
            "\tmodified:   .copier-answers.yml",
            "",
            "no changes added to commit (use \"git add\" and/or \"git commit -a\")",
            "",
        ]
        .join("\n")
    );
    assert_eq!(
        git(&dst, &["diff", "--", filename]),
        [
            "diff --cc launcher.sh",
            "index f163f4b,d20125d..0000000",
            "--- a/launcher.sh",
            "+++ b/launcher.sh",
            "@@@ -1,1 -1,1 +1,5 @@@",
            "- upstream version 1 + downstream",
            " -upstream version 2",
            "++<<<<<<< before updating",
            "++upstream version 1 + downstream",
            "++=======",
            "++upstream version 2",
            "++>>>>>>> after updating",
            "",
        ]
        .join("\n")
    );
    // editorconfig-checker-enable
}
