//! Port of `tests/test_jinja2_undefined.py`.

mod common;

use common::{assert_err, file_tree, read, worker, Tmp};
use copier::errors::TemplateErrorKind;
use copier::CopierError;
use indoc::indoc;

#[test]
#[ignore = "pending"]
fn test_default() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    file_tree! {
        src.join("copier.yaml") => "",
        src.join("test.jinja") => "{{ undefined_variable }}",
    };

    worker(&src, &dst).run_copy().unwrap();
    assert_eq!(read(dst.join("test")), "");
}

#[test]
#[ignore = "pending"]
fn test_invalid_value() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    file_tree! {
        src.join("copier.yaml") => indoc! {"
            _envops:
                undefined: jinja2.ChainableUndefined
            "},
    };

    assert_err!(
        worker(&src, &dst).run_copy(),
        CopierError::ConfigFile(_),
        "Unsupported envops.undefined value specified: jinja2.ChainableUndefined."
    );
}

#[test]
#[ignore = "pending"]
fn test_undefined() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    file_tree! {
        src.join("copier.yaml") => indoc! {"
            _envops:
                undefined: jinja2.Undefined
            "},
        src.join("test.jinja") => "{{ undefined_variable }}",
    };

    worker(&src, &dst).run_copy().unwrap();
    assert_eq!(read(dst.join("test")), "");
}

#[test]
#[ignore = "pending"]
fn test_strictundefined_undefined_variable() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    file_tree! {
        src.join("copier.yaml") => indoc! {"
            _envops:
                undefined: jinja2.StrictUndefined
            "},
        src.join("test.jinja") => "{{ undefined_variable }}",
    };

    assert_err!(
        worker(&src, &dst).run_copy(),
        CopierError::Template {
            kind: TemplateErrorKind::Undefined,
            ..
        },
        "'undefined_variable' is undefined"
    );
}
