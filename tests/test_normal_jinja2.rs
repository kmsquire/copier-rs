//! Port of `tests/test_normal_jinja2.py`.

mod common;

use common::{file_tree, read, worker, Tmp};
use copier::errors::capture_warnings;
use indoc::indoc;

#[test]
fn test_normal_jinja2() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => indoc! {r##"
            _templates_suffix: .jinja
            _envops:
                autoescape: false
                variable_start_string: "{{"
                variable_end_string: "}}"
                block_start_string: "{%"
                block_end_string: "%}"
                comment_start_string: "{#"
                comment_end_string: "#}"
                lstrip_blocks: true
                trim_blocks: true
            name: Guybrush
            todo: Become a pirate
        "##},
        src.join("TODO.txt.jinja") => indoc! {"
            [[ {{ name }} TODO LIST]]
            {# Let's put an ugly not-comment below #}
            [# GROG #]
                {% if name == 'Guybrush' %}
                - {{ todo }}
                {% endif %}
        "},
    };
    // No warnings, because template is explicit
    let (result, warnings) =
        capture_warnings(|| worker(&src, &dst).defaults(true).overwrite(true).run_copy());
    result.unwrap();
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    let todo = read(dst.join("TODO.txt"));
    let expected = "[[ Guybrush TODO LIST]]\n[# GROG #]\n    - Become a pirate\n";
    assert_eq!(todo, expected);
}

#[test]
fn test_to_not_keep_trailing_newlines_in_jinja2() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => indoc! {r#"
            _templates_suffix: .jinja
            _envops:
                keep_trailing_newline: false
            data: foo
        "#},
        src.join("data.txt.jinja") => "This is {{ data }}.\n",
    };
    // No warnings, because template is explicit
    let (result, warnings) =
        capture_warnings(|| worker(&src, &dst).defaults(true).overwrite(true).run_copy());
    result.unwrap();
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    assert_eq!(read(dst.join("data.txt")), "This is foo.");
}
