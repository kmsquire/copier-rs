//! Port of `tests/test_imports.py`.

mod common;

use std::path::Path;

use common::{assert_err, file_tree, read, symlink, worker, Tmp};
use copier::CopierError;
use indoc::indoc;
use rstest::rstest;
use serde_json::json;

#[rstest]
#[case("", json!({"_exclude": ["includes"]}))]
#[case("template", json!({"_subdirectory": "template"}))]
fn test_include(#[case] subdir: &str, #[case] settings: serde_json::Value) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    let mut config = settings;
    config["name"] = json!({"type": "str", "default": "The Name"});
    config["slug"] = json!({
        "type": "str",
        "default": "{% include 'includes/name-slug.jinja' %}",
    });
    file_tree! {
        src.join("copier.yml") => config.to_string(),
        src.join("includes").join("name-slug.jinja") => "{{ name|lower|replace(' ', '-') }}",
        // File for testing the Jinja include statement in `copier.yml`.
        src.join(subdir).join("slug-answer.txt.jinja") => "{{ slug }}",
        // File for testing the Jinja include statement as content.
        src.join(subdir).join("slug-from-include.txt.jinja") => "{% include 'includes/name-slug.jinja' %}",
        // File for testing the Jinja include statement in the file name.
        src.join(subdir).join("{% include pathjoin('includes', 'name-slug.jinja') %}.txt") => "",
        // File for testing the Jinja include statement in the folder name.
        src.join(subdir).join("{% include pathjoin('includes', 'name-slug.jinja') %}").join("test.txt") => "",
    };
    worker(&src, &dst).defaults(true).run_copy().unwrap();
    assert_eq!(read(dst.join("slug-answer.txt")), "the-name");
    assert_eq!(read(dst.join("slug-from-include.txt")), "the-name");
    assert!(dst.join("the-name.txt").exists());
    assert!(dst.join("the-name").join("test.txt").exists());
    assert!(!dst.join("includes").exists());
}

#[rstest]
fn test_include_resolves_symlink_outside_template_root_raises_error(
    #[values(false, true)] absolute: bool,
) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    let target = if absolute {
        src.join("outside").join("secrets.txt")
    } else {
        Path::new("..").join("outside").join("secrets.txt")
    };
    file_tree! {
        src.join("template").join("copier.yml") => "_preserve_symlinks: true",
        src.join("template").join("symlink.txt") => symlink(target),
        src.join("template").join("stolen.txt.jinja") => "{% include 'symlink.txt' %}",
        src.join("outside").join("secrets.txt") => "s3cr3t",
    };
    assert_err!(
        worker(src.join("template"), &dst).run_copy(),
        CopierError::ForbiddenPath { .. },
        &regex::escape(&format!(
            "{}\" is forbidden",
            src.join("outside").join("secrets.txt").display()
        ))
    );
}

#[test]
fn test_include_resolves_symlink_inside_template() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => "_preserve_symlinks: true",
        src.join("inside.txt") => "inside",
        src.join("symlink.txt") => symlink("inside.txt"),
        src.join("result.txt.jinja") => "{% include 'symlink.txt' %}",
    };
    worker(&src, &dst).run_copy().unwrap();
    assert_eq!(read(dst.join("result.txt")), "inside");
}

#[rstest]
#[case("", json!({"_exclude": ["includes"]}))]
#[case("template", json!({"_subdirectory": "template"}))]
fn test_import_macro(#[case] subdir: &str, #[case] settings: serde_json::Value) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    let mut config = settings;
    config["name"] = json!({"type": "str", "default": "The Name"});
    config["slug"] = json!({
        "type": "str",
        "default": "{% from 'includes/slugify.jinja' import slugify %}{{ slugify(name) }}",
    });
    file_tree! {
        src.join("copier.yml") => config.to_string(),
        src.join("includes").join("slugify.jinja") => indoc! {"
            {% macro slugify(value) -%}
            {{ value|lower|replace(' ', '-') }}
            {%- endmacro %}
        "},
        // File for testing the Jinja import statement in `copier.yml`.
        src.join(subdir).join("slug-answer.txt.jinja") => "{{ slug }}",
        // File for testing the Jinja import statement as content.
        src.join(subdir).join("slug-from-macro.txt.jinja") => "{% from 'includes/slugify.jinja' import slugify %}{{ slugify(name) }}",
        // File for testing the Jinja import statement in the file name.
        src.join(subdir).join("{% from pathjoin('includes', 'slugify.jinja') import slugify %}{{ slugify(name) }}.txt") => "",
        // File for testing the Jinja import statement in the folder name.
        src.join(subdir).join("{% from pathjoin('includes', 'slugify.jinja') import slugify %}{{ slugify(name) }}").join("test.txt") => "",
    };
    worker(&src, &dst).defaults(true).run_copy().unwrap();
    assert_eq!(read(dst.join("slug-answer.txt")), "the-name");
    assert_eq!(read(dst.join("slug-from-macro.txt")), "the-name");
    assert!(dst.join("the-name.txt").exists());
    assert!(dst.join("the-name").join("test.txt").exists());
    assert!(!dst.join("includes").exists());
}
