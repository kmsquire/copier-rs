//! Port of `tests/test_check_update.py`.

mod common;

use std::path::PathBuf;

use common::{file_tree, git, run_cli, Tmp, ANSWERS_FILE_TMPL};

/// A template with three tagged versions: `v1.0.0`, `v2.0.0` and `v3.0.0-alpha`
/// (the `template_path` fixture in Python).
fn template_path(tmp: &Tmp) -> PathBuf {
    let src = tmp.mktemp("src");

    // Create template v1.0.0
    file_tree! {
        src.join(ANSWERS_FILE_TMPL) => "# Changes here will be overwritten by Copier\n{{ _copier_answers|to_nice_yaml }}\n",
        src.join("README.md.jinja") => "# Version 1.0.0\n",
    };
    git(&src, &["init"]);
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m", "v1.0.0"]);
    git(&src, &["tag", "v1.0.0"]);

    // Create template v2.0.0
    file_tree! {
        src.join("README.md.jinja") => "# Version 2.0.0\n",
    };
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m", "v2.0.0"]);
    git(&src, &["tag", "v2.0.0"]);

    // Create template v3.0.0-alpha
    file_tree! {
        src.join("README.md.jinja") => "# Version 3.0.0-alpha\n",
    };
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m", "v3.0.0-alpha"]);
    git(&src, &["tag", "v3.0.0-alpha"]);

    src
}

#[test]
fn test_without_updated_template_no_opts() {
    let tmp = Tmp::new();
    let src = template_path(&tmp);
    let dst = tmp.mktemp("dst");
    let (src_s, dst_s) = (src.to_str().unwrap(), dst.to_str().unwrap());

    run_cli(&[
        "copy",
        src_s,
        dst_s,
        "--quiet",
        "--overwrite",
        "--vcs-ref=v2.0.0",
    ]);

    let (retcode, out) = run_cli(&["check-update", dst_s]);
    assert_eq!(retcode, 0);
    assert!(out.stdout.contains("Project is up-to-date!"));
}

#[test]
fn test_without_updated_template_json_output_opt() {
    let tmp = Tmp::new();
    let src = template_path(&tmp);
    let dst = tmp.mktemp("dst");
    let (src_s, dst_s) = (src.to_str().unwrap(), dst.to_str().unwrap());

    run_cli(&[
        "copy",
        src_s,
        dst_s,
        "--quiet",
        "--overwrite",
        "--vcs-ref=v2.0.0",
    ]);

    let (retcode, out) = run_cli(&["check-update", dst_s, "--output-format", "json"]);
    assert_eq!(retcode, 0);
    assert!(out.stdout.contains(
        r#"{"update_available": false, "current_version": "2.0.0", "latest_version": "2.0.0"}"#
    ));
}

#[test]
fn test_without_updated_template_quiet_opt() {
    let tmp = Tmp::new();
    let src = template_path(&tmp);
    let dst = tmp.mktemp("dst");
    let (src_s, dst_s) = (src.to_str().unwrap(), dst.to_str().unwrap());

    run_cli(&[
        "copy",
        src_s,
        dst_s,
        "--quiet",
        "--overwrite",
        "--vcs-ref=v2.0.0",
    ]);

    let (retcode, out) = run_cli(&["check-update", dst_s, "--quiet"]);
    assert_eq!(retcode, 0);
    assert_eq!(out.stdout, "");
}

#[test]
fn test_with_updated_template_no_opts() {
    let tmp = Tmp::new();
    let src = template_path(&tmp);
    let dst = tmp.mktemp("dst");
    let (src_s, dst_s) = (src.to_str().unwrap(), dst.to_str().unwrap());

    run_cli(&[
        "copy",
        src_s,
        dst_s,
        "--quiet",
        "--overwrite",
        "--vcs-ref=v1.0.0",
    ]);

    let (retcode, out) = run_cli(&["check-update", dst_s]);
    assert_eq!(retcode, 0);
    assert!(out.stdout.contains(
        "New template version available.\nCurrent version is 1.0.0, latest version is 2.0.0."
    ));
}

#[test]
fn test_with_updated_template_json_output_opt() {
    let tmp = Tmp::new();
    let src = template_path(&tmp);
    let dst = tmp.mktemp("dst");
    let (src_s, dst_s) = (src.to_str().unwrap(), dst.to_str().unwrap());

    run_cli(&[
        "copy",
        src_s,
        dst_s,
        "--quiet",
        "--overwrite",
        "--vcs-ref=v1.0.0",
    ]);

    let (retcode, out) = run_cli(&["check-update", dst_s, "--output-format", "json"]);
    assert_eq!(retcode, 0);
    assert!(out.stdout.contains(
        r#"{"update_available": true, "current_version": "1.0.0", "latest_version": "2.0.0"}"#
    ));
}

#[test]
fn test_with_updated_template_quiet_opt() {
    let tmp = Tmp::new();
    let src = template_path(&tmp);
    let dst = tmp.mktemp("dst");
    let (src_s, dst_s) = (src.to_str().unwrap(), dst.to_str().unwrap());

    run_cli(&[
        "copy",
        src_s,
        dst_s,
        "--quiet",
        "--overwrite",
        "--vcs-ref=v1.0.0",
    ]);

    let (retcode, out) = run_cli(&["check-update", dst_s, "--quiet"]);
    assert_eq!(retcode, 2);
    assert_eq!(out.stdout, "");
}

#[test]
fn test_with_prerelease_template_no_opts() {
    let tmp = Tmp::new();
    let src = template_path(&tmp);
    let dst = tmp.mktemp("dst");
    let (src_s, dst_s) = (src.to_str().unwrap(), dst.to_str().unwrap());

    run_cli(&[
        "copy",
        src_s,
        dst_s,
        "--quiet",
        "--overwrite",
        "--vcs-ref=v2.0.0",
    ]);

    let (retcode, out) = run_cli(&["check-update", dst_s]);
    assert_eq!(retcode, 0);
    assert!(out.stdout.contains("Project is up-to-date!"));
}

#[test]
fn test_with_prerelease_template_json_output_opt() {
    let tmp = Tmp::new();
    let src = template_path(&tmp);
    let dst = tmp.mktemp("dst");
    let (src_s, dst_s) = (src.to_str().unwrap(), dst.to_str().unwrap());

    run_cli(&[
        "copy",
        src_s,
        dst_s,
        "--quiet",
        "--overwrite",
        "--vcs-ref=v2.0.0",
    ]);

    let (retcode, out) = run_cli(&["check-update", dst_s, "--output-format", "json"]);
    assert_eq!(retcode, 0);
    assert!(out.stdout.contains(
        r#"{"update_available": false, "current_version": "2.0.0", "latest_version": "2.0.0"}"#
    ));
}

#[test]
fn test_with_prerelease_template_quiet_opt() {
    let tmp = Tmp::new();
    let src = template_path(&tmp);
    let dst = tmp.mktemp("dst");
    let (src_s, dst_s) = (src.to_str().unwrap(), dst.to_str().unwrap());

    run_cli(&[
        "copy",
        src_s,
        dst_s,
        "--quiet",
        "--overwrite",
        "--vcs-ref=v2.0.0",
    ]);

    let (retcode, out) = run_cli(&["check-update", dst_s, "--quiet"]);
    assert_eq!(retcode, 0);
    assert_eq!(out.stdout, "");
}

#[test]
fn test_with_prerelease_template_prereleases_opt() {
    let tmp = Tmp::new();
    let src = template_path(&tmp);
    let dst = tmp.mktemp("dst");
    let (src_s, dst_s) = (src.to_str().unwrap(), dst.to_str().unwrap());

    run_cli(&[
        "copy",
        src_s,
        dst_s,
        "--quiet",
        "--overwrite",
        "--vcs-ref=v2.0.0",
    ]);

    let (retcode, out) = run_cli(&["check-update", dst_s, "--prereleases"]);
    assert_eq!(retcode, 0);
    assert!(out.stdout.contains(
        "New template version available.\nCurrent version is 2.0.0, latest version is 3.0.0a0."
    ));
}

#[test]
fn test_with_prerelease_template_prereleases_and_json_output_opts() {
    let tmp = Tmp::new();
    let src = template_path(&tmp);
    let dst = tmp.mktemp("dst");
    let (src_s, dst_s) = (src.to_str().unwrap(), dst.to_str().unwrap());

    run_cli(&[
        "copy",
        src_s,
        dst_s,
        "--quiet",
        "--overwrite",
        "--vcs-ref=v2.0.0",
    ]);

    let (retcode, out) = run_cli(&[
        "check-update",
        dst_s,
        "--output-format",
        "json",
        "--prereleases",
    ]);
    assert_eq!(retcode, 0);
    assert!(out.stdout.contains(
        r#"{"update_available": true, "current_version": "2.0.0", "latest_version": "3.0.0a0"}"#
    ));
}

#[test]
fn test_with_prerelease_template_prereleases_and_quiet_opts() {
    let tmp = Tmp::new();
    let src = template_path(&tmp);
    let dst = tmp.mktemp("dst");
    let (src_s, dst_s) = (src.to_str().unwrap(), dst.to_str().unwrap());

    run_cli(&[
        "copy",
        src_s,
        dst_s,
        "--quiet",
        "--overwrite",
        "--vcs-ref=v2.0.0",
    ]);

    let (retcode, out) = run_cli(&["check-update", dst_s, "--quiet", "--prereleases"]);
    assert_eq!(retcode, 2);
    assert_eq!(out.stdout, "");
}

#[test]
fn test_with_custom_answers_file_path_opt() {
    let tmp = Tmp::new();
    let src = template_path(&tmp);
    let dst = tmp.mktemp("dst");
    let (src_s, dst_s) = (src.to_str().unwrap(), dst.to_str().unwrap());
    let answers_file = ".my-answers.yml";

    run_cli(&[
        "copy",
        src_s,
        dst_s,
        "--quiet",
        "--overwrite",
        "--vcs-ref=v1.0.0",
        "--answers-file",
        answers_file,
    ]);
    assert!(dst.join(".my-answers.yml").exists());

    let (retcode, _) = run_cli(&["check-update", dst_s]);
    assert_eq!(retcode, 1);

    let (retcode, out) = run_cli(&["check-update", dst_s, "--answers-file", answers_file]);
    assert_eq!(retcode, 0);
    assert!(out.stdout.contains(
        "New template version available.\nCurrent version is 1.0.0, latest version is 2.0.0."
    ));
}
