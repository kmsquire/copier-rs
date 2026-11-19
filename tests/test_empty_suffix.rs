//! Port of `tests/test_empty_suffix.py`.

mod common;

use common::{file_tree, read, worker, Tmp};
use indoc::indoc;

#[test]
#[ignore = "pending"]
fn test_empty_suffix() {
    let tmp = Tmp::new();
    let (root, dest) = tmp.src_dst();
    file_tree! {
        root.join("copier.yaml") => indoc! {r#"
            _templates_suffix: ""
            name:
                type: str
                default: pingu
        "#},
        root.join("render_me") => "Hello {{name}}!",
        root.join("{{name}}.txt") => "Hello {{name}}!",
        root.join("{{name}}").join("render_me.txt") => "Hello {{name}}!",
    };
    worker(&root, &dest)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();

    assert!(!dest.join("copier.yaml").exists());

    assert!(dest.join("render_me").exists());
    assert!(dest.join("pingu.txt").exists());
    assert!(dest.join("pingu").join("render_me.txt").exists());

    let expected = "Hello pingu!";
    assert_eq!(read(dest.join("render_me")), expected);
    assert_eq!(read(dest.join("pingu.txt")), expected);
    assert_eq!(read(dest.join("pingu").join("render_me.txt")), expected);
}

#[test]
#[ignore = "pending"]
fn test_binary_file_fallback_to_copy() {
    let tmp = Tmp::new();
    let (root, dest) = tmp.src_dst();
    file_tree! {
        root.join("copier.yaml") => indoc! {r#"
            _templates_suffix: ""
            name:
                type: str
                default: pingu
        "#},
        root.join("logo.png") => b"\x89PNG\r\n\x1a\n\x00\rIHDR\x00\xec\n{{name}}\n\x00\xec",
    };
    worker(&root, &dest)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    let logo = dest.join("logo.png");
    assert!(logo.exists());
    let logo_bytes = std::fs::read(&logo).unwrap();
    assert!(contains(&logo_bytes, b"{{name}}"));
    assert!(!contains(&logo_bytes, b"pingu"));
}

/// Whether `haystack` contains `needle` as a byte substring (`in` on `bytes`).
fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}
