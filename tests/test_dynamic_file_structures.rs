//! Port of `tests/test_dynamic_file_structures.py`.

mod common;

use std::path::{Path, PathBuf};

use common::{assert_err, file_tree, read, v, worker, Tmp};
use copier::errors::capture_warnings;
use copier::CopierError;
use serde_json::json;

/// All regular files below `dir` (`rglob("*")` filtered to files).
fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(files_under(&path));
        } else if path.is_file() {
            files.push(path);
        }
    }
    files
}

/// Files under `dst` that are not in `expected`.
fn unexpected_files(dst: &Path, expected: &[PathBuf]) -> Vec<PathBuf> {
    files_under(dst)
        .into_iter()
        .filter(|path| !expected.contains(path))
        .collect()
}

/// The final path component as text (`Path.name` in Python).
fn name_of(path: &Path) -> String {
    path.file_name().unwrap().to_string_lossy().into_owned()
}

#[test]
#[ignore = "pending"]
fn test_folder_loop() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => "",
        src.join("folder_loop")
            .join("{% yield item from strings %}{{ item }}{% endyield %}")
            .join("{{ item }}.txt.jinja") => "Hello {{ item }}",
    };
    let (result, warnings) = capture_warnings(|| {
        worker(&src, &dst)
            .data(v(json!({
                "strings": ["a", "b", "c"],
            })))
            .defaults(true)
            .overwrite(true)
            .run_copy()
    });
    result.unwrap();
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");

    let expected_files: Vec<PathBuf> = ["a", "b", "c"]
        .iter()
        .map(|i| dst.join("folder_loop").join(i).join(format!("{i}.txt")))
        .collect();

    for f in &expected_files {
        assert!(f.exists());
        assert_eq!(read(f), format!("Hello {}", name_of(f.parent().unwrap())));
    }

    let unexpected = unexpected_files(&dst, &expected_files);
    assert!(
        unexpected.is_empty(),
        "Unexpected files found: {unexpected:?}"
    );
}

#[test]
#[ignore = "pending"]
fn test_folder_loop_reports_path_when_yielded_part_rendering_fails() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    let broken_part = "{% yield item from items %}{% if item is defined %}{% include 'missing-template.jinja' %}{% endif %}{% endyield %}";
    file_tree! {
        src.join("copier.yml") => "",
        src.join("folder_loop").join(broken_part).join("child.txt") => "Hello",
    };

    let err = worker(&src, &dst)
        .data(v(json!({
            "items": [{}],
        })))
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .expect_err("expected an error");
    assert!(
        matches!(err, CopierError::UserMessage(_)),
        "unexpected error: {err:?}"
    );

    let message = err.to_string();
    let broken_path = PathBuf::from("folder_loop").join(broken_part);
    assert!(message.contains(&broken_path.to_string_lossy().into_owned()));
    assert!(!message.contains(&broken_path.join("child.txt").to_string_lossy().into_owned()));
}

#[test]
#[ignore = "pending"]
fn test_nested_folder_loop() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => "",
        src.join("nested_folder_loop")
            .join("{% yield string_item from strings %}{{ string_item }}{% endyield %}")
            .join("{% yield integer_item from integers %}{{ integer_item }}{% endyield %}")
            .join("{{ string_item }}_{{ integer_item }}.txt.jinja") => "Hello {{ string_item }} {{ integer_item }}",
    };
    let (result, warnings) = capture_warnings(|| {
        worker(&src, &dst)
            .data(v(json!({
                "strings": ["a", "b"],
                "integers": [1, 2, 3],
            })))
            .defaults(true)
            .overwrite(true)
            .run_copy()
    });
    result.unwrap();
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");

    let mut expected_files = Vec::new();
    for s in ["a", "b"] {
        for i in [1, 2, 3] {
            expected_files.push(
                dst.join("nested_folder_loop")
                    .join(s)
                    .join(i.to_string())
                    .join(format!("{s}_{i}.txt")),
            );
        }
    }

    for f in &expected_files {
        assert!(f.exists());
        assert_eq!(
            read(f),
            format!(
                "Hello {} {}",
                name_of(f.parent().unwrap().parent().unwrap()),
                name_of(f.parent().unwrap())
            )
        );
    }

    let unexpected = unexpected_files(&dst, &expected_files);
    assert!(
        unexpected.is_empty(),
        "Unexpected files found: {unexpected:?}"
    );
}

#[test]
#[ignore = "pending"]
fn test_file_loop() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => "",
        src.join("file_loop")
            .join("{% yield string_item from strings %}{{ string_item }}{% endyield %}.jinja") => "Hello {{ string_item }}",
    };
    let (result, warnings) = capture_warnings(|| {
        worker(&src, &dst)
            .data(v(json!({
                // if rendred as '.jinja', it will not be created
                "strings": ["a.txt", "b.txt", "c.txt", ""],
            })))
            .defaults(true)
            .overwrite(true)
            .run_copy()
    });
    result.unwrap();
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");

    let expected_files: Vec<PathBuf> = ["a", "b", "c"]
        .iter()
        .map(|i| dst.join("file_loop").join(format!("{i}.txt")))
        .collect();
    for f in &expected_files {
        assert!(f.exists());
        assert_eq!(
            read(f),
            format!("Hello {}.txt", f.file_stem().unwrap().to_string_lossy())
        );
    }

    let unexpected = unexpected_files(&dst, &expected_files);
    assert!(
        unexpected.is_empty(),
        "Unexpected files found: {unexpected:?}"
    );
}

#[test]
#[ignore = "pending"]
fn test_folder_loop_dict_items() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => "",
        src.join("folder_loop_dict_items")
            .join("{% yield dict_item from dicts %}{{ dict_item.folder_name }}{% endyield %}")
            .join("{{ dict_item.file_name }}.txt.jinja") => "Hello {{ '-'.join(dict_item.content) }}",
    };

    let (result, warnings) = capture_warnings(|| {
        worker(&src, &dst)
            .data(v(json!({
                "dicts": [
                    {
                        "folder_name": "folder_a",
                        "file_name": "file_a",
                        "content": ["folder_a", "file_a"],
                    },
                    {
                        "folder_name": "folder_b",
                        "file_name": "file_b",
                        "content": ["folder_b", "file_b"],
                    },
                    {
                        "folder_name": "folder_c",
                        "file_name": "file_c",
                        "content": ["folder_c", "file_c"],
                    },
                ],
            })))
            .defaults(true)
            .overwrite(true)
            .run_copy()
    });
    result.unwrap();
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");

    let expected_files: Vec<PathBuf> = [
        ("folder_a", "file_a"),
        ("folder_b", "file_b"),
        ("folder_c", "file_c"),
    ]
    .iter()
    .map(|(folder, file)| {
        dst.join("folder_loop_dict_items")
            .join(folder)
            .join(format!("{file}.txt"))
    })
    .collect();

    for f in &expected_files {
        assert!(f.exists());
        assert_eq!(
            read(f),
            format!(
                "Hello {}-{}",
                name_of(f.parent().unwrap()),
                f.file_stem().unwrap().to_string_lossy()
            )
        );
    }

    let unexpected = unexpected_files(&dst, &expected_files);
    assert!(
        unexpected.is_empty(),
        "Unexpected files found: {unexpected:?}"
    );
}

#[test]
#[ignore = "pending"]
fn test_raise_yield_tag_in_file() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => "",
        src.join("file.txt.jinja") => "{% yield item from strings %}{{ item }}{% endyield %}",
    };

    assert_err!(
        worker(&src, &dst)
            .data(v(json!({
                "strings": ["a", "b", "c"],
            })))
            .defaults(true)
            .overwrite(true)
            .run_copy(),
        CopierError::YieldTagInFile(_),
        "file.txt.jinja"
    );
}

#[test]
#[ignore = "pending"]
fn test_raise_multiple_yield_tags() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    // multiple yield tags, not nested
    let file_name = "{% yield item1 from strings %}{{ item1 }}{% endyield %}{% yield item2 from strings %}{{ item2 }}{% endyield %}";

    file_tree! {
        src.join("copier.yml") => "",
        src.join(file_name) => "",
    };

    assert_err!(
        worker(&src, &dst)
            .data(v(json!({
                "strings": ["a", "b", "c"],
            })))
            .defaults(true)
            .overwrite(true)
            .run_copy(),
        CopierError::MultipleYieldTags(_),
        "item"
    );

    let (src, dst) = tmp.src_dst();
    // multiple yield tags, nested
    let file_name = "{% yield item1 from strings %}{% yield item2 from strings %}{{ item1 }}{{ item2 }}{% endyield %}{% endyield %}";

    file_tree! {
        src.join("copier.yml") => "",
        src.join(file_name) => "",
    };

    assert_err!(
        worker(&src, &dst)
            .data(v(json!({
                "strings": ["a", "b", "c"],
            })))
            .defaults(true)
            .overwrite(true)
            .run_copy(),
        CopierError::MultipleYieldTags(_),
        "item"
    );
}
