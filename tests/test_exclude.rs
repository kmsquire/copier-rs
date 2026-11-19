//! Port of `tests/test_exclude.py`.

mod common;

use std::path::Path;

use common::{build_file_tree, file_tree, fixture, symlink, v, worker, Content, Tmp};
use copier::template::DEFAULT_EXCLUDE;
use indoc::indoc;
use rstest::rstest;
use serde_json::json;

/// Copy is done properly when excluding recursively.
#[test]
#[ignore = "pending"]
fn test_exclude_recursive() {
    let tmp = Tmp::new();
    let dst = tmp.path();
    let src = fixture("demo_exclude");
    worker(&src, dst).run_copy().unwrap();
    assert!(!dst.join("bad").exists());
    assert!(!dst.join("bad").is_dir());
}

/// Copy is done properly when copy_me.txt is the sole file copied.
#[test]
#[ignore = "pending"]
fn test_exclude_recursive_negate() {
    let tmp = Tmp::new();
    let dst = tmp.path();
    let src = fixture("demo_exclude_negate");
    worker(&src, dst).run_copy().unwrap();
    assert!(dst.join("copy_me.txt").exists());
    assert!(dst.join("copy_me.txt").is_file());
    assert!(!dst.join("do_not_copy_me.txt").exists());
}

#[test]
#[ignore = "pending"]
fn test_config_exclude() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => "_exclude: ['*.txt']",
        src.join("aaaa.txt") => "",
    };
    worker(&src, &dst).quiet(true).run_copy().unwrap();
    assert!(!dst.join("aaaa.txt").exists());
    assert!(dst.join("copier.yml").exists());
}

#[test]
#[ignore = "pending"]
fn test_config_exclude_extended() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => "_exclude: ['*.txt']",
        src.join("aaaa.txt") => "",
    };
    worker(&src, &dst)
        .quiet(true)
        .exclude(["*.yml"])
        .run_copy()
        .unwrap();
    assert!(!dst.join("aaaa.txt").exists());
    assert!(!dst.join("copier.yml").exists());
}

#[test]
#[ignore = "pending"]
fn test_config_include() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => "_exclude: ['!.svn']",
        src.join(".svn").join("hello") => "",
    };
    worker(&src, &dst).quiet(true).run_copy().unwrap();
    assert!(dst.join(".svn").join("hello").exists());
    assert!(dst.join("copier.yml").exists());
}

/// A single exclusion entry may render to multiple newline-separated patterns.
///
/// This lets one Jinja block conditionally emit a whole list of patterns instead of
/// repeating the condition on every entry. When the guard is false the block renders
/// empty and excludes nothing.
#[rstest]
#[ignore = "pending"]
fn test_config_exclude_multiline_conditional(#[values(true, false)] skip: bool) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => indoc! {"
            skip:
                type: bool

            _exclude:
                - copier.yml
                - |
                    {% if skip %}
                    /a.txt
                    /b.txt
                    /sub/c.txt
                    {% endif %}
        "},
        src.join("a.txt") => "",
        src.join("b.txt") => "",
        src.join("sub").join("c.txt") => "",
        src.join("keep.txt") => "",
    };
    worker(&src, &dst)
        .quiet(true)
        .data(v(json!({"skip": skip})))
        .run_copy()
        .unwrap();
    // keep.txt is never in the block, so it is always copied.
    assert!(dst.join("keep.txt").exists());
    // Files are excluded when skip=True
    assert_eq!(dst.join("a.txt").exists(), !skip);
    assert_eq!(dst.join("b.txt").exists(), !skip);
    assert_eq!(dst.join("sub").join("c.txt").exists(), !skip);
}

#[test]
#[ignore = "pending"]
// xfail(condition=platform.system() == "Darwin", reason="OS without proper UTF-8 filesystem.", strict=True)
fn test_path_filter() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    // Python dict literal: the duplicate "mañana.txt" key keeps only the last value.
    let file_excluded: [(&str, bool); 7] = [
        ("x.exclude", true),
        ("do_not.exclude!", false),
        // dir patterns and their negations
        ("exclude_dir/x", true),
        ("exclude_dir/please_copy_me", false), // no mercy
        ("not_exclude_dir/x!", false),
        // unicode patterns
        ("mañana.txt", false),
        ("manana.txt", false),
    ];
    let mut tree = vec![(
        src.join("copier.yaml"),
        Content::from(indoc! {r#"
            _exclude:
                # simple file patterns and their negations
                - "*.exclude"
                - "!do_not.exclude"
                # dir patterns and their negations
                - "exclude_dir/"
                - "!exclude_dir/please_copy_me"
                - "!not_exclude_dir/x"
                # unicode patterns
                - "mañana.txt"
        "#}),
    )];
    for (key, value) in file_excluded {
        let content = if value { "True" } else { "False" };
        tree.push((src.join(key), Content::from(content)));
    }
    build_file_tree(tree);
    worker(&src, &dst).run_copy().unwrap();
    for (key, value) in file_excluded {
        assert!(dst.join(key).exists() != value);
    }
}

/// Default excludes are not applied when a (true) subdirectory is specified.
#[test]
#[ignore = "pending"]
fn test_config_exclude_with_subdirectory() {
    // Make sure the file under test is in the list of default excludes
    assert!(DEFAULT_EXCLUDE.contains(&"copier.yml"));

    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => "_subdirectory: 'template'",
        src.join("template").join("copier.yml") => "",
    };
    worker(&src, &dst).quiet(true).run_copy().unwrap();
    assert!(dst.join("copier.yml").exists());
}

/// Default excludes are applied when subdirectory is `.`.
#[test]
#[ignore = "pending"]
fn test_config_exclude_with_subdirectory_dot() {
    // Make sure the file under test is in the list of default excludes
    assert!(DEFAULT_EXCLUDE.contains(&"copier.yml"));

    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => "_subdirectory: '.'",
        src.join("template").join("copier.yml") => "",
    };
    worker(&src, &dst).quiet(true).run_copy().unwrap();
    assert!(!dst.join("template").join("copier.yml").exists());
}

/// Default excludes are applied when subdirectory is `""`.
#[test]
#[ignore = "pending"]
fn test_config_exclude_with_subdirectory_empty_string() {
    // Make sure the file under test is in the list of default excludes
    assert!(DEFAULT_EXCLUDE.contains(&"copier.yml"));

    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => "_subdirectory: ''",
        src.join("template").join("copier.yml") => "",
    };
    worker(&src, &dst).quiet(true).run_copy().unwrap();
    assert!(!dst.join("template").join("copier.yml").exists());
}

/// Default excludes are applied when no subdirectory is specified.
#[test]
#[ignore = "pending"]
fn test_config_exclude_without_subdirectory() {
    // Make sure the file under test is in the list of default excludes
    assert!(DEFAULT_EXCLUDE.contains(&"copier.yml"));

    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => "",
        src.join("template").join("copier.yml") => "",
    };
    worker(&src, &dst).quiet(true).run_copy().unwrap();
    assert!(!dst.join("template").join("copier.yml").exists());
}

#[test]
#[ignore = "pending"]
fn test_config_exclude_copieryml_without_templates_suffix() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => indoc! {r#"
            _subdirectory: .
            _templates_suffix: ""
            _envops:
                block_start_string: "[?"
                block_end_string: "?]"
        "#},
    };
    worker(&src, &dst).quiet(true).run_copy().unwrap();
    assert!(!dst.join("copier.yml").exists());
}

#[test]
#[ignore = "pending"]
fn test_config_exclude_file_with_bad_jinja_syntax_without_templates_suffix() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => indoc! {r#"
            _subdirectory: .
            _templates_suffix: ""
            _exclude:
                - copier.yml
                - exclude-me.txt
        "#},
        src.join("exclude-me.txt") => indoc! {r#"
            "{%" is malformed Jinja syntax but it doesn't matter because
            this file is excluded
        "#},
    };
    worker(&src, &dst).quiet(true).run_copy().unwrap();
    assert!(!dst.join("copier.yml").exists());
    assert!(!dst.join("exclude-me.txt").exists());
}

#[test]
#[ignore = "pending"]
fn test_config_exclude_with_templated_path() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => indoc! {r#"
            _exclude:
                - "*"
                - "!keep-me.txt"

            filename_keep: keep-me.txt
            filename_exclude: exclude-me.txt
        "#},
        src.join("{{ filename_keep }}.jinja") => "",
        src.join("{{ filename_exclude }}.jinja") => "",
    };
    worker(&src, &dst)
        .defaults(true)
        .quiet(true)
        .run_copy()
        .unwrap();
    assert!(dst.join("keep-me.txt").exists());
    assert!(!dst.join("exclude-me.txt").exists());
}

/// Test wildcard exclude with negated nested file and symlink excludes.
#[test]
#[ignore = "pending"]
fn test_exclude_wildcard_negate_nested_file() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => indoc! {r#"
            _preserve_symlinks: true
            _exclude:
                - "*"
                - "!/foo/keep-me.txt"
                - "!/foo/bar/keep-me-symlink.txt"
        "#},
        src.join("exclude-me.txt") => "",
        src.join("foo").join("keep-me.txt") => "",
        src.join("foo").join("bar").join("keep-me-symlink.txt") => symlink(Path::new("..").join("keep-me.txt")),
    };
    worker(&src, &dst).run_copy().unwrap();
    assert!(dst.join("foo").join("keep-me.txt").exists());
    assert!(dst.join("foo").join("keep-me.txt").is_file());
    assert!(dst
        .join("foo")
        .join("bar")
        .join("keep-me-symlink.txt")
        .exists());
    assert!(dst
        .join("foo")
        .join("bar")
        .join("keep-me-symlink.txt")
        .is_symlink());
    assert_eq!(
        std::fs::read_link(dst.join("foo").join("bar").join("keep-me-symlink.txt")).unwrap(),
        Path::new("..").join("keep-me.txt")
    );
    assert!(!dst.join("exclude-me.txt").exists());
}
