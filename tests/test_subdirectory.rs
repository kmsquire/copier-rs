//! Port of `tests/test_subdirectory.py`.

mod common;

use std::path::PathBuf;

use common::{
    assert_err, file_tree, git, git_init, git_save, load_answers, load_answers_from, read, v,
    worker, Tmp, BRACKET_ENVOPS_JSON, SUFFIX_TMPL,
};
use copier::{ConflictMode, CopierError, Value, Worker};
use indoc::{formatdoc, indoc};
use rstest::rstest;
use serde_json::json;

/// Template with a subdirectory chosen by the `choose_subdir` answer (the `template_path` fixture).
fn template_path(tmp: &Tmp) -> PathBuf {
    let root = tmp.mktemp("template");
    file_tree! {
        root.join("api_project").join("api_readme.md") => "",
        root.join("api_project").join("[[ _copier_conf.answers_file ]].tmpl") => "[[ _copier_answers|to_nice_yaml ]]",
        root.join("conf_project").join("conf_readme.md") => indoc! {"
            # Template subdirectory

            This is the template README.
        "},
        root.join("conf_project").join("conf_readme.md.tmpl") => indoc! {"
            # Demo subdirectory

            Generated using previous answers `_subdirectory` value.
        "},
        root.join("conf_project").join("[[ _copier_conf.answers_file ]].tmpl") => "[[ _copier_answers|to_nice_yaml ]]",
        root.join("conf_project").join("[[ filename ]].tmpl") => "[[ filename ]] contents",
        root.join("copier.yml") => formatdoc! {r#"
            _templates_suffix: {SUFFIX_TMPL}
            _envops: {BRACKET_ENVOPS_JSON}
            choose_subdir:
                type: str
                default: conf_project
                choices:
                    - api_project
                    - conf_project
            _subdirectory: "[[ choose_subdir ]]"
            filename:
                type: str
                default: mock_filename
        "#},
    };
    git_init(&root, "hello world");
    root
}

#[test]
fn test_copy_subdirectory_api_option() {
    let tmp = Tmp::new();
    let template = template_path(&tmp);
    let dst = tmp.mktemp("dst");
    worker(&template, &dst)
        .defaults(true)
        .overwrite(true)
        .data(v(json!({"choose_subdir": "api_project"})))
        .run_copy()
        .unwrap();
    assert!(dst.join("api_readme.md").exists());
    assert!(!dst.join("conf_readme.md").exists());
}

#[test]
fn test_copy_subdirectory_config() {
    let tmp = Tmp::new();
    let template = template_path(&tmp);
    let dst = tmp.mktemp("dst");
    worker(&template, &dst)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    assert!(dst.join("conf_readme.md").exists());
    assert!(!dst.join("api_readme.md").exists());
}

#[test]
fn test_copy_subdirectory_config_no_overwrite() {
    let tmp = Tmp::new();
    let template = template_path(&tmp);
    let dst = tmp.mktemp("dst");
    worker(&template, &dst)
        .defaults(true)
        .overwrite(false)
        .run_copy()
        .unwrap();
    assert!(dst.join("conf_readme.md").exists());
    assert!(dst.join("mock_filename").exists());
    assert!(read(dst.join("mock_filename")).contains("mock_filename contents"));
    assert!(read(dst.join("conf_readme.md")).contains("# Demo subdirectory"));
    assert!(!read(dst.join("conf_readme.md")).contains("# Template subdirectory"));
    assert!(!dst.join("api_readme.md").exists());
}

#[test]
fn test_update_subdirectory() {
    let tmp = Tmp::new();
    let template = template_path(&tmp);
    let dst = tmp.mktemp("dst");
    worker(&template, &dst)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();

    git_init(&dst, "hello world");

    Worker::new()
        .dst_path(&dst)
        .defaults(true)
        .overwrite(true)
        .run_update()
        .unwrap();
    assert!(!dst.join("conf_project").exists());
    assert!(!dst.join("api_project").exists());
    assert!(!dst.join("api_readme.md").exists());
    assert!(dst.join("conf_readme.md").exists());
}

#[test]
fn test_update_subdirectory_from_root_path() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yaml") => indoc! {"
            q1:
                type: str
                default: a1
        "},
        src.join("file1.jinja") => indoc! {"
            version 1
            hello
            {{ q1 }}
            bye
        "},
        src.join("{{ _copier_conf.answers_file }}.jinja") => "{{ _copier_answers|to_nice_yaml }}",
    };
    git_save(&src, "1", Some("1"));
    file_tree! {
        src.join("file1.jinja") => indoc! {"
            version 2
            hello
            {{ q1 }}
            bye
        "},
    };
    git(&src, &["commit", "-am2"]);
    git(&src, &["tag", "2"]);

    file_tree! {
        dst.join("dst_top_file") => "one",
    };
    git_init(&dst, "0");

    worker(&src, dst.join("subfolder"))
        .vcs_ref("1")
        .defaults(true)
        .overwrite(true)
        .answers_file(".custom.copier-answers.yaml")
        .run_copy()
        .unwrap();
    assert_eq!(
        read(dst.join("subfolder").join("file1")),
        "version 1\nhello\na1\nbye\n"
    );
    git(&dst, &["add", "."]);
    git(&dst, &["commit", "-m1"]);
    Worker::new()
        .dst_path(dst.join("subfolder"))
        .defaults(true)
        .overwrite(true)
        .answers_file(".custom.copier-answers.yaml")
        .run_update()
        .unwrap();
    let answers = load_answers_from(dst.join("subfolder"), ".custom.copier-answers.yaml");
    assert_eq!(answers["_commit"], Value::from("2"));
    assert_eq!(
        read(dst.join("subfolder").join("file1")),
        "version 2\nhello\na1\nbye\n"
    );
}

#[rstest]
#[case(ConflictMode::Rej, "upstream version 2\n", true)]
#[case(
    ConflictMode::Inline,
    indoc! {"
        <<<<<<< before updating
        downstream version 1
        =======
        upstream version 2
        >>>>>>> after updating
    "},
    false
)]
fn test_new_version_uses_subdirectory(
    #[case] conflict: ConflictMode,
    #[case] readme: &str,
    #[case] expect_reject: bool,
) {
    // Template in v1 doesn't have a _subdirectory;
    // in v2 it moves all things into a subdir and adds that key to copier.yml.
    // Some files change. Downstream project has evolved too. Does that work as expected?
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    // First, create the template with an initial README
    file_tree! {
        src.join("README.md") => "upstream version 1",
        src.join("{{_copier_conf.answers_file}}.jinja") => "{{_copier_answers|to_nice_yaml}}",
    };
    git_init(&src, "hello template");
    git(&src, &["tag", "v1"]);

    // Generate the project a first time, assert the README exists
    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    assert!(dst.join("README.md").exists());
    assert_eq!(load_answers(&dst)["_commit"], Value::from("v1"));

    // Start versioning the generated project
    git_init(&dst, "hello project");

    // After first commit, change the README, commit again
    std::fs::write(dst.join("README.md"), "downstream version 1").unwrap();
    git(&dst, &["commit", "-am", "updated readme"]);

    // Now change the template
    // Update the README
    std::fs::write(src.join("README.md"), "upstream version 2").unwrap();

    // Create a subdirectory, move files into it
    let subdir = src.join("subdir");
    std::fs::create_dir(&subdir).unwrap();
    std::fs::rename(src.join("README.md"), subdir.join("README.md")).unwrap();
    std::fs::rename(
        src.join("{{_copier_conf.answers_file}}.jinja"),
        subdir.join("{{_copier_conf.answers_file}}.jinja"),
    )
    .unwrap();

    // Add the subdirectory option to copier.yml
    std::fs::write(src.join("copier.yml"), "_subdirectory: subdir").unwrap();

    // Commit the changes
    git(&src, &["add", ".", "-A"]);
    git(&src, &["commit", "-m", "use a subdirectory now"]);
    git(&src, &["tag", "v2"]);

    // Finally, update the generated project
    Worker::new()
        .dst_path(&dst)
        .defaults(true)
        .overwrite(true)
        .conflict(conflict)
        .run_update()
        .unwrap();
    assert_eq!(load_answers(&dst)["_commit"], Value::from("v2"));

    // Assert that the README still exists, and the conflicts were handled
    // correctly.
    assert!(dst.join("README.md").exists());

    assert_eq!(
        read(dst.join("README.md")).lines().collect::<Vec<_>>(),
        readme.lines().collect::<Vec<_>>()
    );
    assert_eq!(dst.join("README.md.rej").exists(), expect_reject);

    // Also assert the subdirectory itself was not rendered
    assert!(!dst.join("subdir").exists());
}

#[test]
fn test_new_version_changes_subdirectory() {
    // Template in v3 changes from one subdirectory to another.
    // Some file evolves also. Sub-project evolves separately.
    // Sub-project is updated. Does that work as expected?
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();

    // First, create the template with an initial subdirectory and README inside it
    file_tree! {
        src.join("copier.yml") => "_subdirectory: subdir1\n",
        src.join("subdir1").join("[[_copier_conf.answers_file]].tmpl") => "[[_copier_answers|to_nice_yaml]]\n",
        src.join("subdir1").join("README.md") => "upstream version 1\n",
    };
    git_init(&src, "hello template");

    // Generate the project a first time, assert the README exists
    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    assert!(dst.join("README.md").exists());

    // Start versioning the generated project
    git_init(&dst, "hello project");

    // After first commit, change the README, commit again
    std::fs::write(dst.join("README.md"), "downstream version 1\n").unwrap();
    git(&dst, &["commit", "-am", "updated readme"]);

    // Now change the template
    // Update the README
    std::fs::write(
        src.join("subdir1").join("README.md"),
        "upstream version 2\n",
    )
    .unwrap();

    // Rename the subdirectory
    std::fs::rename(src.join("subdir1"), src.join("subdir2")).unwrap();

    // Update copier.yml to reflect this change
    std::fs::write(src.join("copier.yml"), "_subdirectory: subdir2\n").unwrap();

    // Commit the changes
    git(&src, &["add", ".", "-A"]);
    git(&src, &["commit", "-m", "changed from subdir1 to subdir2"]);

    // Finally, update the generated project
    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .skip_if_exists(["README.md"])
        .run_copy()
        .unwrap();

    // Assert that the README still exists, and was NOT force updated
    assert!(dst.join("README.md").exists());
    assert_eq!(read(dst.join("README.md")), "downstream version 1\n");

    // Also assert the subdirectories themselves were not rendered
    assert!(!dst.join("subdir1").exists());
    assert!(!dst.join("subdir2").exists());
}

#[rstest]
fn test_subdirectory_path_outside_template_root_on_copy_raises_error(
    #[values(false, true)] is_absolute: bool,
) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    let subdirectory = if is_absolute {
        src.join("forbidden").display().to_string()
    } else {
        "../forbidden".to_owned()
    };
    file_tree! {
        src.join("template").join("copier.yml") => format!("_subdirectory: {subdirectory}"),
        src.join("forbidden").join("forbidden.txt") => "forbidden",
    };
    assert_err!(
        worker(src.join("template"), &dst).run_copy(),
        CopierError::ForbiddenPath { .. },
        "forbidden"
    );
}

#[rstest]
fn test_subdirectory_path_outside_template_root_on_update_raises_error(
    #[values(false, true)] is_absolute: bool,
) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    let subdirectory = if is_absolute {
        src.join("forbidden").display().to_string()
    } else {
        "../forbidden".to_owned()
    };
    file_tree! {
        src.join("template").join("copier.yml") => "_subdirectory: subdir",
        src.join("template").join("subdir").join("{{ _copier_conf.answers_file }}.jinja") => "{{ _copier_answers | to_nice_yaml }}",
        src.join("template").join("subdir").join("file.txt") => "v1 contents",
    };
    git(src.join("template"), &["init"]);
    git(src.join("template"), &["add", "."]);
    git(src.join("template"), &["commit", "-m", "v1"]);
    git(src.join("template"), &["tag", "v1"]);

    worker(src.join("template"), &dst).run_copy().unwrap();
    assert_eq!(load_answers(&dst)["_commit"], Value::from("v1"));
    assert!(dst.join("file.txt").exists());
    assert_eq!(read(dst.join("file.txt")), "v1 contents");

    git(&dst, &["init"]);
    git(&dst, &["add", "."]);
    git(&dst, &["commit", "-m", "v1"]);

    file_tree! {
        src.join("template").join("copier.yml") => format!("_subdirectory: {subdirectory}"),
        src.join("forbidden").join("forbidden.txt") => "forbidden",
    };
    git(src.join("template"), &["add", "."]);
    git(src.join("template"), &["commit", "-m", "v2"]);
    git(src.join("template"), &["tag", "v2"]);

    assert_err!(
        Worker::new().dst_path(&dst).overwrite(true).run_update(),
        CopierError::ForbiddenPath { .. },
        "forbidden"
    );
}
