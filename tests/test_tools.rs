//! Port of `tests/test_tools.py`. `test_types` (mypy) is N/A.

mod common;

use std::fs;

use common::git;
use copier::tools::{cast_to_bool, split_nul};
use copier::Value;
use rstest::rstest;

#[rstest]
fn test_cast_to_bool_treats_blank_strings_as_false(
    #[values("", " ", "\t", "\n", "  \n\t  ")] value: &str,
) {
    assert!(!cast_to_bool(&Value::from(value)));
}

#[rstest]
#[case(" true ", true)]
#[case("\nfalse\n", false)]
fn test_cast_to_bool_ignores_surrounding_whitespace(#[case] value: &str, #[case] expected: bool) {
    assert_eq!(cast_to_bool(&Value::from(value)), expected);
}

/// Ensure temporary directories containing read-only files are properly deleted,
/// whatever the OS.
#[test]
fn test_temporary_directory_with_readonly_files_deletion() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let tmp_path = tmp_dir.path().to_path_buf();
    let ro_file = tmp_path.join("readonly.txt");
    fs::write(&ro_file, "don't touch me!").unwrap();
    let mut permissions = fs::metadata(&ro_file).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&ro_file, permissions).unwrap();
    drop(tmp_dir);
    assert!(!tmp_path.exists());
}

/// Ensure temporary directories containing git repositories are properly deleted,
/// whatever the OS.
#[test]
fn test_temporary_directory_with_git_repo_deletion() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let tmp_path = tmp_dir.path().to_path_buf();
    git(&tmp_path, &["init"]);
    drop(tmp_dir);
    assert!(!tmp_path.exists());
}

#[rstest]
#[case("", &[])]
#[case("readme.md\0", &["readme.md"])]
#[case("\0readme.md", &["", "readme.md"])]
#[case("a\0b\0", &["a", "b"])]
#[case("a\0b", &["a", "b"])]
#[case("\0a\0\0b\0", &["", "a", "", "b"])]
#[case("\0", &[""])]
#[case("quo\"tes\0", &["quo\"tes"])]
#[case("\"surrounded\"\0", &["\"surrounded\""])]
#[case("m4â4ñ4a\0", &["m4â4ñ4a"])]
#[case("tab\t\0", &["tab\t"])]
#[case("lf\n\0", &["lf\n"])]
#[case("crlf\r\n\0", &["crlf\r\n"])]
#[case("back\\slash\0", &["back\\slash"])]
#[case("qu\"o\\tes\0", &["qu\"o\\tes"])]
#[case(
    "\x07\x08\x0c\n\t\x0bcontrol\x07\x08\x0c\n\t\x0bcharacters\x07\x08\x0c\n\t\x0b\0",
    &["\x07\x08\x0c\n\t\x0bcontrol\x07\x08\x0c\n\t\x0bcharacters\x07\x08\x0c\n\t\x0b"]
)]
#[case("lf\n\0qu\"o\\tes\0", &["lf\n", "qu\"o\\tes"])]
fn test_split_nul(#[case] output: &str, #[case] entries: &[&str]) {
    assert_eq!(split_nul(output), entries);
}
