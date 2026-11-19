//! Port of `tests/test_minimum_version.py`.

mod common;

use std::path::PathBuf;

use common::{assert_err, file_tree, git, worker, Tmp};
use copier::errors::{capture_warnings, WarningKind};
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

#[test]
#[ignore = "needs-design: copier.__version__ monkeypatch; the version is a const (COPIER_VERSION)"]
fn test_version_less_than_required() {
    let root = Tmp::new();
    let template_path = template(&root);
    let tmp = Tmp::new();
    // TODO(port): override the Copier version for this run; no hook in the crate.
    // monkeypatch.setattr("copier.__version__", "0.0.0a0")
    assert_err!(
        worker(&template_path, tmp.path()).run_copy(),
        CopierError::UnsupportedVersion(_)
    );
}

#[test]
#[ignore = "needs-design: copier.__version__ monkeypatch; the version is a const (COPIER_VERSION)"]
fn test_version_equal_required() {
    let root = Tmp::new();
    let template_path = template(&root);
    let tmp = Tmp::new();
    // TODO(port): override the Copier version for this run; no hook in the crate.
    // monkeypatch.setattr("copier.__version__", "10.5.1")
    // assert no error
    worker(&template_path, tmp.path()).run_copy().unwrap();
}

#[test]
#[ignore = "needs-design: copier.__version__ monkeypatch; the version is a const (COPIER_VERSION)"]
fn test_version_greater_than_required() {
    let root = Tmp::new();
    let template_path = template(&root);
    let tmp = Tmp::new();
    // TODO(port): override the Copier version for this run; no hook in the crate.
    // monkeypatch.setattr("copier.__version__", "99.99.99")
    // assert no error
    let (result, warnings) = capture_warnings(|| worker(&template_path, tmp.path()).run_copy());
    result.unwrap();
    assert!(warnings.iter().any(|w| w.kind == WarningKind::OldTemplate));
}

#[test]
#[ignore = "needs-design: copier.__version__ monkeypatch; the version is a const (COPIER_VERSION)"]
fn test_minimum_version_update() {
    let root = Tmp::new();
    let template_path = template(&root);
    let tmp = Tmp::new();
    let dst = tmp.path();
    // TODO(port): override the Copier version for each run; no hook in the crate.
    // monkeypatch.setattr("copier.__version__", "11.0.0")
    let (result, warnings) = capture_warnings(|| worker(&template_path, dst).run_copy());
    result.unwrap();
    assert!(warnings.iter().any(|w| w.kind == WarningKind::OldTemplate));

    git(dst, &["init"]);
    git(dst, &["add", "."]);
    git(dst, &["commit", "-m", "hello world"]);

    // monkeypatch.setattr("copier.__version__", "0.0.0.post0")
    assert_err!(
        worker(&template_path, dst).run_copy(),
        CopierError::UnsupportedVersion(_)
    );

    // monkeypatch.setattr("copier.__version__", "10.5.1")
    // assert no error
    worker(&template_path, dst).run_copy().unwrap();

    // monkeypatch.setattr("copier.__version__", "99.99.99")
    // assert no error
    let (result, warnings) = capture_warnings(|| worker(&template_path, dst).run_copy());
    result.unwrap();
    assert!(warnings.iter().any(|w| w.kind == WarningKind::OldTemplate));
}

#[test]
#[ignore = "needs-design: patches copier._template.copier_version and turns warnings into errors"]
fn test_version_0_0_0_ignored() {
    let root = Tmp::new();
    let template_path = template(&root);
    let tmp = Tmp::new();
    // TODO(port): patch `copier._template.copier_version` to return 0.0.0; no hook in the crate.
    // monkeypatch.setattr("copier._template.copier_version", lambda: Version("0.0.0"))
    // assert no error
    // with warnings.catch_warnings():
    //     warnings.simplefilter("error")
    //     with pytest.raises(UnknownCopierVersionWarning):
    //         copier.run_copy(template_path, tmp_path)
    // TODO(port): warnings-as-errors (`simplefilter("error")`) has no Rust equivalent;
    // `capture_warnings` only collects them.
    let _ = worker(&template_path, tmp.path()).run_copy();
}

#[test]
#[ignore = "needs-design: copier.__version__ monkeypatch and warnings turned into errors"]
fn test_version_bigger_major_warning() {
    let root = Tmp::new();
    let template_path = template(&root);
    let tmp = Tmp::new();
    // TODO(port): override the Copier version for this run; no hook in the crate.
    // monkeypatch.setattr("copier.__version__", "11.0.0a0")
    // with warnings.catch_warnings():
    //     warnings.simplefilter("error")
    //     with pytest.raises(OldTemplateWarning):
    //         copier.run_copy(template_path, tmp_path)
    // TODO(port): warnings-as-errors (`simplefilter("error")`) has no Rust equivalent;
    // `capture_warnings` only collects them.
    let _ = worker(&template_path, tmp.path()).run_copy();
}
