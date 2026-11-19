//! Port of `tests/test_minimum_version.py`.

mod common;

use std::path::PathBuf;

use common::{assert_err, file_tree, git, worker, Tmp};
use copier::errors::{capture_warnings, WarningKind};
use copier::template::with_copier_version;
use copier::CopierError;
use indoc::indoc;

/// A template requiring Copier 10.5.1 or newer.
fn template(tmp: &Tmp) -> PathBuf {
    let root = tmp.mktemp("template");
    file_tree! {
        root.join("copier.yaml") => indoc! {r#"
            _min_copier_version: "10.5.1"
        "#},
        root.join("README.md") => "",
    };
    root
}

fn run(template_path: &std::path::Path, dst: &std::path::Path) -> Result<(), CopierError> {
    worker(template_path, dst).run_copy().map(|_| ())
}

fn has_warning(warnings: &[copier::errors::Warning], kind: WarningKind) -> bool {
    warnings.iter().any(|w| w.kind == kind)
}

#[test]
fn test_version_less_than_required() {
    let root = Tmp::new();
    let template_path = template(&root);
    let tmp = Tmp::new();
    assert_err!(
        with_copier_version("0.0.0a0", || run(&template_path, tmp.path())),
        CopierError::UnsupportedVersion(_)
    );
}

#[test]
fn test_version_equal_required() {
    let root = Tmp::new();
    let template_path = template(&root);
    let tmp = Tmp::new();
    // assert no error
    with_copier_version("10.5.1", || run(&template_path, tmp.path())).unwrap();
}

#[test]
fn test_version_greater_than_required() {
    let root = Tmp::new();
    let template_path = template(&root);
    let tmp = Tmp::new();
    // assert no error
    let (result, warnings) = with_copier_version("99.99.99", || {
        capture_warnings(|| run(&template_path, tmp.path()))
    });
    result.unwrap();
    assert!(has_warning(&warnings, WarningKind::OldTemplate));
}

#[test]
fn test_minimum_version_update() {
    let root = Tmp::new();
    let template_path = template(&root);
    let tmp = Tmp::new();
    let dst = tmp.path();
    let (result, warnings) =
        with_copier_version("11.0.0", || capture_warnings(|| run(&template_path, dst)));
    result.unwrap();
    assert!(has_warning(&warnings, WarningKind::OldTemplate));

    git(dst, &["init"]);
    git(dst, &["add", "."]);
    git(dst, &["commit", "-m", "hello world"]);

    assert_err!(
        with_copier_version("0.0.0.post0", || run(&template_path, dst)),
        CopierError::UnsupportedVersion(_)
    );

    // assert no error
    with_copier_version("10.5.1", || run(&template_path, dst)).unwrap();

    // assert no error
    let (result, warnings) =
        with_copier_version("99.99.99", || capture_warnings(|| run(&template_path, dst)));
    result.unwrap();
    assert!(has_warning(&warnings, WarningKind::OldTemplate));
}

#[test]
fn test_version_0_0_0_ignored() {
    let root = Tmp::new();
    let template_path = template(&root);
    let tmp = Tmp::new();
    let (_, warnings) = with_copier_version("0.0.0", || {
        capture_warnings(|| run(&template_path, tmp.path()))
    });
    assert!(has_warning(&warnings, WarningKind::UnknownCopierVersion));
}

#[test]
fn test_version_bigger_major_warning() {
    let root = Tmp::new();
    let template_path = template(&root);
    let tmp = Tmp::new();
    let (_, warnings) = with_copier_version("11.0.0a0", || {
        capture_warnings(|| run(&template_path, tmp.path()))
    });
    assert!(has_warning(&warnings, WarningKind::OldTemplate));
}
