//! Port of `tests/test_jinja2_extensions.py`.

mod common;

use common::{assert_err, file_tree, fixture, read, worker, Tmp};
use copier::CopierError;
use indoc::indoc;

// The fixtures `demo_extensions_additional` names these Python extensions in
// `_jinja_extensions`; the Rust binary cannot load them.
//
// class FilterExtension(Extension):
//     def __init__(self, environment: Environment) -> None:
//         super().__init__(environment)
//
//         def super_filter(obj: Any) -> str:
//             return str(obj) + " super filter!"
//
//         environment.filters["super_filter"] = super_filter
//
//
// class GlobalsExtension(Extension):
//     def __init__(self, environment: Environment) -> None:
//         super().__init__(environment)
//
//         def super_func(argument: Any) -> str:
//             return str(argument) + " super func!"
//
//         environment.globals.update(super_func=super_func)
//         environment.globals.update(super_var="super var!")

#[test]
fn test_default_jinja2_extensions() {
    let tmp = Tmp::new();
    let dst = tmp.path();
    worker(fixture("demo_extensions_default"), dst)
        .run_copy()
        .unwrap();
    let super_file = dst.join("super_file.md");
    assert!(super_file.exists());
    assert_eq!(read(&super_file), "path\n");
}

#[test]
#[ignore = "not applicable: Python Jinja extensions cannot be loaded"]
fn test_additional_jinja2_extensions() {
    // TODO(port): the fixture's `_jinja_extensions` names the Python classes above.
    let tmp = Tmp::new();
    let dst = tmp.path();
    worker(fixture("demo_extensions_additional"), dst)
        .unsafe_(true)
        .run_copy()
        .unwrap();
    let super_file = dst.join("super_file.md");
    assert!(super_file.exists());
    assert_eq!(read(&super_file), "super var! super func! super filter!\n");
}

#[test]
fn test_additional_jinja2_extensions_error() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join(".copier-answers.yml.jinja") => indoc! {"
            # Changes here will be overwritten by Copier
            {{ _copier_answers|to_nice_yaml }}
        "},
        src.join("copier.yaml") => indoc! {"
            _jinja_extensions:
                - non.existent.Extension
        "},
    };

    assert_err!(
        worker(&src, &dst).unsafe_(true).run_copy(),
        CopierError::ExtensionNotFound(_),
        "No module named 'non'"
    );
}

#[test]
fn test_to_json_filter_with_conf() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("conf.json.jinja") => "{{ _copier_conf|to_json }}",
    };
    worker(&src, &dst).run_copy().unwrap();
    let conf_file = dst.join("conf.json");
    assert!(conf_file.exists());
    // must not raise an error
    let conf: serde_json::Value = serde_json::from_str(&read(&conf_file)).unwrap();
    assert!(conf.as_object().is_some_and(|object| !object.is_empty()));
}
