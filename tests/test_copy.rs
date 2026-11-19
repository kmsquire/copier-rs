//! Port of `tests/test_copy.py`.

mod common;

use std::path::{Path, MAIN_SEPARATOR};
use std::time::Duration;

use common::{
    assert_err, assert_file, copier_cmd, demo_worker, file_tree, fixture, git, git_save,
    git_save_with, project_template, read, symlink, v, worker, Tmp, BRACKET_ENVOPS_JSON,
};
use copier::errors::{capture_warnings, WarningKind};
use copier::vcs::without_git;
use copier::{CopierError, Worker};
use indoc::{formatdoc, indoc};
use rstest::rstest;
use serde_json::json;

/// Expected outcome of a run (`does_not_raise()` / `pytest.raises(...)`).
#[derive(Debug, Clone, Copy)]
enum Expect {
    /// No error.
    Pass,
    /// `ValueError` whose message matches the regex (empty matches anything).
    Value(&'static str),
    /// `KeyError` whose message matches the regex.
    Key(&'static str),
    /// `TypeError`.
    Type,
}

fn check(result: copier::Result<Worker>, expected: Expect) {
    match expected {
        Expect::Pass => {
            result.unwrap();
        }
        Expect::Value(pattern) => assert_err!(result, CopierError::Value(_), pattern),
        Expect::Key(pattern) => assert_err!(result, CopierError::Key(_), pattern),
        // `InvalidTypeError` is a `TypeError` in Python.
        Expect::Type => assert_err!(result, CopierError::Type(_) | CopierError::InvalidType(_)),
    }
}

fn expect_err(result: copier::Result<Worker>) -> CopierError {
    match result {
        Ok(_) => panic!("expected an error"),
        Err(err) => err,
    }
}

fn bracket_envops() -> serde_json::Value {
    serde_json::from_str(BRACKET_ENVOPS_JSON).unwrap()
}

/// `textwrap.indent(text, "    ")`.
fn indent4(text: &str) -> String {
    text.split_inclusive('\n')
        .map(|line| {
            if line.trim().is_empty() {
                line.to_owned()
            } else {
                format!("    {line}")
            }
        })
        .collect()
}

/// `yaml.safe_load`.
fn yaml_safe_load(text: &str) -> copier::Value {
    copier::yaml::load(text).unwrap()
}

fn mtime(path: impl AsRef<Path>) -> std::time::SystemTime {
    std::fs::metadata(path).unwrap().modified().unwrap()
}

#[test]
fn test_project_not_found() {
    let tmp = Tmp::new();
    let dst = tmp.mktemp("dst");

    assert_err!(worker("foobar", &dst).run_copy(), CopierError::Value(_));

    let this_file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("test_copy.rs");
    assert_err!(worker(&this_file, &dst).run_copy(), CopierError::Value(_));
}

/// Generating a minimal project must work even if `git` is not installed.
#[test]
fn test_copy_without_git() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("hello").join("world.txt.jinja") => "Hello, {{ name }}!",
        src.join("copier.yml") => "name: world",
    };

    without_git(|| worker(&src, &dst).defaults(true).run_copy()).unwrap();
    assert_eq!(read(dst.join("hello").join("world.txt")), "Hello, world!");
}

#[test]
fn test_copy_with_non_version_tags() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join(".copier-answers.yml.jinja") => indoc! {"
            # Changes here will be overwritten by Copier
            {{ _copier_answers|to_nice_yaml }}
        "},
        src.join("copier.yaml") => indoc! {"
            _tasks:
                - cat v1.txt
        "},
        src.join("v1.txt") => "file only in v1",
    };
    git(&src, &["init"]);
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m1"]);
    git(&src, &["tag", "test_tag.post23+deadbeef"]);

    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .unsafe_(true)
        .vcs_ref("HEAD")
        .run_copy()
        .unwrap();
}

/// Tags with and without `v` prefix should work the same.
#[rstest]
#[case("1.2.3.post1", true, "2")]
#[case("1.2.4.dev1", true, "2")]
#[case("1.2.3.post1", false, "2")]
#[case("1.2.4.dev1", false, "1")]
fn test_tag_autodetect_v_prefix_optional(
    #[case] tag2: &str,
    #[case] use_prereleases: bool,
    #[case] expected_version: &str,
    #[values("", "v")] prefix: &str,
) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("version.txt") => "1",
        src.join("{{_copier_conf.answers_file}}.jinja") => "{{ _copier_answers|to_nice_yaml -}}",
    };
    // One stable tag
    git_save(&src, "Test commit", Some(&format!("{prefix}1.2.3")));
    // One pre or post release tag
    std::fs::write(src.join("version.txt"), "2").unwrap();
    git_save(&src, "Test commit", Some(&format!("{prefix}{tag2}")));
    // One untagged change
    std::fs::write(src.join("version.txt"), "3").unwrap();
    git_save(&src, "Test commit", None);
    // Copy, leaving latest tag auto-detection
    worker(&src, &dst)
        .use_prereleases(use_prereleases)
        .defaults(true)
        .unsafe_(true)
        .run_copy()
        .unwrap();
    // Check it works as expected
    assert_eq!(read(dst.join("version.txt")), expected_version);
}

#[test]
fn test_copy_with_non_version_tags_and_vcs_ref() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join(".copier-answers.yml.jinja") => indoc! {"
            # Changes here will be overwritten by Copier
            {{ _copier_answers|to_nice_yaml }}
        "},
        src.join("copier.yaml") => indoc! {"
            _tasks:
                - cat v1.txt
        "},
        src.join("v1.txt") => "file only in v1",
    };
    git(&src, &["init"]);
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m1"]);
    git(&src, &["tag", "test_tag.post23+deadbeef"]);

    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .unsafe_(true)
        .vcs_ref("test_tag.post23+deadbeef")
        .run_copy()
        .unwrap();
}

#[test]
fn test_copy_with_vcs_ref_branch() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    git(&src, &["init"]);
    let main_branch = git(&src, &["branch", "--show-current"]).trim().to_owned();
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m1", "--allow-empty"]);
    git(&src, &["checkout", "-b", "branch"]);
    git(&src, &["commit", "-m2", "--allow-empty"]);
    git(&src, &["checkout", &main_branch]);
    worker(&src, &dst).vcs_ref("branch").run_copy().unwrap();
}

#[test]
fn test_copy() {
    let tmp = Tmp::new();
    let dst = tmp.mktemp("dst");
    demo_worker(&dst).run_copy().unwrap();

    let generated = read(dst.join("pyproject.toml"));
    let control = read(fixture("reference_files").join("pyproject.toml"));
    assert_eq!(generated, control);

    assert_file(&dst, "doc/mañana.txt");
    assert_file(&dst, "doc/images/nslogo.gif");

    let p1 = dst.join("awesome").join("hello.txt");
    let p2 = project_template().join("[[ myvar ]]").join("hello.txt");
    assert_eq!(std::fs::read(p1).unwrap(), std::fs::read(p2).unwrap());

    assert_eq!(
        read(dst.join("README.txt")),
        "This is the README for Copier.\n"
    );

    let p1 = dst.join("awesome.txt");
    let p2 = project_template().join("[[ myvar ]].txt");
    assert_eq!(std::fs::read(p1).unwrap(), std::fs::read(p2).unwrap());

    assert!(!dst.join("[% if not py3 %]py2_only.py[% endif %]").exists());
    assert!(!dst.join("[% if py3 %]py3_only.py[% endif %]").exists());
    assert!(!dst.join("py2_only.py").exists());
    assert!(dst.join("py3_only.py").exists());
    assert!(!dst
        .join("[% if not py3 %]py2_folder[% endif %]")
        .join("thing.py")
        .exists());
    assert!(!dst
        .join("[% if py3 %]py3_folder[% endif %]")
        .join("thing.py")
        .exists());
    assert!(!dst.join("py2_folder").join("thing.py").exists());
    assert!(dst.join("py3_folder").join("thing.py").exists());

    assert!(dst.join("aaaa.txt").exists());
}

// impure: needs network
#[test]
#[ignore = "network: clones from GitHub; run with --include-ignored"]
fn test_copy_repo() {
    let tmp = Tmp::new();
    let dst = tmp.mktemp("dst");
    worker("gh:copier-org/copier.git", &dst)
        .vcs_ref("HEAD")
        .quiet(true)
        .exclude(["*", "!README.*"])
        .run_copy()
        .unwrap();
    assert!(dst.join("README.md").exists());
}

#[test]
fn test_default_exclude() {
    let tmp = Tmp::new();
    let dst = tmp.mktemp("dst");
    demo_worker(&dst).run_copy().unwrap();
    assert!(!dst.join(".svn").exists());
}

#[test]
fn test_include_file() {
    let tmp = Tmp::new();
    let dst = tmp.mktemp("dst");
    demo_worker(&dst).exclude(["!.svn"]).run_copy().unwrap();
    assert_file(&dst, ".svn");
}

#[test]
fn test_include_pattern() {
    let tmp = Tmp::new();
    let dst = tmp.mktemp("dst");
    demo_worker(&dst).exclude(["!.*"]).run_copy().unwrap();
    assert!(dst.join(".svn").exists());
}

// @pytest.mark.xfail(
//     condition=platform.system() == "Darwin",
//     reason="Mac claims to use UTF-8 filesystem, but behaves differently.",
//     strict=True,
// )
#[test]
#[cfg_attr(
    target_os = "macos",
    ignore = "xfail on macOS: the filesystem normalizes Unicode file names"
)]
fn test_exclude_file() {
    let tmp = Tmp::new();
    let dst = tmp.mktemp("dst");
    // This file name is b"man\xcc\x83ana.txt".decode()
    demo_worker(&dst)
        .exclude(["man\u{303}ana.txt"])
        .run_copy()
        .unwrap();
    assert!(!dst.join("doc").join("man\u{303}ana.txt").exists());
    // This file name is b"ma\xc3\xb1ana.txt".decode()
    assert!(dst.join("doc").join("ma\u{f1}ana.txt").exists());
    assert!(dst.join("doc").join("manana.txt").exists());
}

/// Exclude argument extends the original exclusions instead of replacing them.
#[test]
fn test_exclude_extends() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("test.txt") => "Test text",
        src.join("test.json") => r#""test json""#,
    };
    // Convert to git repo
    git(&src, &["init"]);
    git(&src, &["add", "."]);
    git(&src, &["commit", "-m", "hello world"]);
    worker(&src, &dst).exclude(["*.txt"]).run_copy().unwrap();
    assert!(dst.join("test.json").is_file());
    assert!(!dst.join("test.txt").exists());
    // .git exists in src, but not in dst because it is excluded by default
    assert!(!dst.join(".git").exists());
}

/// Exclude in copier.yml replaces default values.
#[test]
fn test_exclude_replaces() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("test.txt") => "Test text",
        src.join("test.json") => r#""test json""#,
        src.join("test.yaml") => r#""test yaml""#,
        src.join("copier.yaml.jinja") => "purpose: template inception",
        src.join("copier.yml") => "_exclude: ['*.json']",
    };
    worker(&src, &dst).exclude(["*.txt"]).run_copy().unwrap();
    assert!(dst.join("test.yaml").is_file());
    assert!(!dst.join("test.txt").exists());
    assert!(!dst.join("test.json").exists());
    assert!(dst.join("copier.yml").exists());
    assert!(dst.join("copier.yaml").is_file());
}

#[test]
fn test_skip_if_exists() {
    let tmp = Tmp::new();
    let dst = tmp.mktemp("dst");
    worker(fixture("demo_skip_dst"), &dst).run_copy().unwrap();
    worker(fixture("demo_skip_src"), &dst)
        .skip_if_exists(["b.noeof.txt", "meh/c.noeof.txt"])
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();

    assert_eq!(read(dst.join("a.noeof.txt")), "SKIPPED");
    assert_eq!(read(dst.join("b.noeof.txt")), "SKIPPED");
    assert_eq!(read(dst.join("meh").join("c.noeof.txt")), "SKIPPED");
}

#[test]
fn test_timestamp_identical() {
    let tmp = Tmp::new();
    let dst = tmp.mktemp("dst");
    worker(fixture("demo_skip_dst"), &dst).run_copy().unwrap();
    let modification_time_before = mtime(dst.join("a.noeof.txt"));
    std::thread::sleep(Duration::from_secs(2));
    worker(fixture("demo_skip_dst"), &dst).run_copy().unwrap();
    let modification_time_after = mtime(dst.join("a.noeof.txt"));

    assert_eq!(modification_time_before, modification_time_after);
}

#[test]
fn test_skip_if_exists_rendered_patterns() {
    let tmp = Tmp::new();
    let dst = tmp.mktemp("dst");
    worker(fixture("demo_skip_dst"), &dst).run_copy().unwrap();
    worker(fixture("demo_skip_src"), &dst)
        .data(v(json!({"name": "meh"})))
        .skip_if_exists(["{{ name }}/c.noeof.txt"])
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    assert_eq!(read(dst.join("a.noeof.txt")), "SKIPPED");
    assert_eq!(read(dst.join("b.noeof.txt")), "OVERWRITTEN");
    assert_eq!(read(dst.join("meh").join("c.noeof.txt")), "SKIPPED");
}

#[test]
fn test_skip_option() {
    let tmp = Tmp::new();
    let dst = tmp.mktemp("dst");
    demo_worker(&dst).run_copy().unwrap();
    let path = dst.join("pyproject.toml");
    let content = "lorem ipsum";
    std::fs::write(&path, content).unwrap();
    demo_worker(&dst).skip_if_exists(["**"]).run_copy().unwrap();
    assert_eq!(read(&path), content);
}

#[test]
fn test_force_option() {
    let tmp = Tmp::new();
    let dst = tmp.mktemp("dst");
    demo_worker(&dst).run_copy().unwrap();
    let path = dst.join("pyproject.toml");
    let content = "lorem ipsum";
    std::fs::write(&path, content).unwrap();
    demo_worker(&dst)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    assert_ne!(read(&path), content);
}

#[test]
fn test_pretend_option() {
    let tmp = Tmp::new();
    let dst = tmp.mktemp("dst");
    demo_worker(&dst).pretend(true).run_copy().unwrap();
    assert!(!dst.join("doc").exists());
    assert!(!dst.join("config.py").exists());
    assert!(!dst.join("pyproject.toml").exists());
}

#[rstest]
fn test_empty_dir(#[values(true, false)] generate: bool) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yaml") => formatdoc! {"
            _subdirectory: tpl
            _templates_suffix: .jinja
            _envops: {BRACKET_ENVOPS_JSON}
            do_it:
                type: bool
        "},
        src.join("tpl").join("[% if do_it %]one_dir[% endif %]").join("one.txt.jinja") => "[[ do_it ]]",
        src.join("tpl").join("two.txt") => "[[ do_it ]]",
        src.join("tpl").join("[% if do_it %]three.txt[% endif %].jinja") => "[[ do_it ]]",
        src.join("tpl").join("[% if do_it %]four.txt[% endif %].jinja") => symlink("[% if do_it %]three.txt[% endif %].jinja"),
        src.join("tpl").join("five").join("[% if do_it %]six.txt[% endif %].jinja") => "[[ do_it ]]",
    };
    worker(&src, &dst)
        .data(v(json!({"do_it": generate})))
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    assert!(dst.join("five").is_dir());
    assert_eq!(read(dst.join("two.txt")), "[[ do_it ]]");
    assert_eq!(dst.join("one_dir").exists(), generate);
    assert_eq!(dst.join("three.txt").exists(), generate);
    assert_eq!(dst.join("four.txt").exists(), generate);
    assert_eq!(dst.join("one_dir").is_dir(), generate);
    assert_eq!(dst.join("one_dir").join("one.txt").is_file(), generate);
    if generate {
        // `repr(True)`
        assert_eq!(read(dst.join("one_dir").join("one.txt")), "True");
        assert_eq!(read(dst.join("three.txt")), "True");
        assert!(!dst.join("four.txt").is_symlink());
        assert_eq!(read(dst.join("four.txt")), "True");
        assert_eq!(read(dst.join("five").join("six.txt")), "True");
    }
}

// @pytest.mark.skipif(condition=platform.system() == "Windows",
//                     reason="Windows does not have UNIX-like permissions")
#[cfg(unix)]
#[rstest]
#[case(0o400)]
#[case(0o700)]
#[case(0o770)]
#[case(0o707)]
#[case(0o777)]
fn test_preserved_permissions(#[case] permissions: u32) {
    use std::os::unix::fs::PermissionsExt;

    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    let src_file = src.join("the_file");
    std::fs::write(&src_file, "content").unwrap();
    std::fs::set_permissions(&src_file, std::fs::Permissions::from_mode(permissions)).unwrap();
    worker(&src, &dst)
        .defaults(true)
        .overwrite(true)
        .run_copy()
        .unwrap();
    let mode = std::fs::metadata(dst.join("the_file"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & permissions, permissions);
}

#[test]
fn test_commit_hash() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("commit.jinja") => "{{_copier_conf.vcs_ref_hash}}",
        src.join("tag.jinja") => "{{_copier_answers._commit}}",
    };
    git(&src, &["init"]);
    git(&src, &["add", "-A"]);
    git(&src, &["commit", "-m1"]);
    git(&src, &["tag", "1.0"]);
    let commit = git(&src, &["rev-parse", "HEAD"]).trim().to_owned();
    worker(&src, &dst).run_copy().unwrap();
    assert_eq!(read(dst.join("tag")), "1.0");
    assert_eq!(read(dst.join("commit")), commit);
}

#[test]
fn test_value_with_forward_slash() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("{{ filename.replace('.', _copier_conf.sep) }}.txt") => "This is template.",
    };
    worker(&src, &dst)
        .data(v(json!({"filename": "a.b.c"})))
        .run_copy()
        .unwrap();
    assert_eq!(
        read(dst.join("a").join("b").join("c.txt")),
        "This is template."
    );
}

// Cases dropped (no JSON form): `b"bytes"`, `bytearray("abc", "utf-8")`,
// `Decimal("1.1")`, a plain (non-`str`) `Enum` member, `object()`.
#[rstest]
#[case(json!({"type": "int"}), json!("no-int"), Expect::Value(""))]
#[case(json!({"type": "int"}), json!("0"), Expect::Pass)]
#[case(
    json!({"type": "int", "validator": "[% if q <= 0 %]must be positive[% endif %]"}),
    json!("0"),
    Expect::Value("")
)]
#[case(
    json!({"type": "int", "validator": "[% if q <= 0 %]must be positive[% endif %]"}),
    json!("1"),
    Expect::Pass
)]
#[case(json!({"type": "str"}), json!("string"), Expect::Pass)]
#[case(json!({"type": "str"}), json!(1), Expect::Pass)]
#[case(json!({"type": "str"}), json!(1.1), Expect::Pass)]
#[case(json!({"type": "str"}), json!(true), Expect::Pass)]
#[case(json!({"type": "str"}), json!(false), Expect::Pass)]
// `Enum("A", ["a", "b"], type=str).a`: a `str` subclass whose text is "1".
#[case(json!({"type": "str"}), json!("1"), Expect::Pass)]
#[case(json!({"type": "str"}), json!({}), Expect::Value(""))]
#[case(json!({"type": "str"}), json!([]), Expect::Value(""))]
#[case(
    json!({"type": "str", "validator": "[% if q|length < 3 %]too short[% endif %]"}),
    json!("ab"),
    Expect::Value("")
)]
#[case(
    json!({"type": "str", "validator": "[% if q|length < 3 %]too short[% endif %]"}),
    json!("abc"),
    Expect::Pass
)]
#[case(
    json!({
        "type": "str",
        "secret": true,
        "default": "",
        "validator": "[% if q|length < 3 %]too short[% endif %]",
    }),
    json!(""),
    Expect::Value(r"^Validation error for question '\w+': too short$")
)]
#[case(json!({"type": "bool"}), json!("true"), Expect::Pass)]
#[case(json!({"type": "bool"}), json!("false"), Expect::Pass)]
#[case(json!({"type": "int", "choices": [1, 2, 3]}), json!("1"), Expect::Pass)]
#[case(json!({"type": "int", "choices": [1, 2, 3]}), json!("4"), Expect::Value(""))]
#[case(
    json!({"type": "int", "choices": [1, ["2", {"value": null}], 3]}),
    json!("2"),
    Expect::Pass
)]
#[case(
    json!({"type": "int", "choices": [1, ["2", {"value": null, "disabled": ""}], 3]}),
    json!("2"),
    Expect::Pass
)]
#[case(
    json!({"type": "int", "choices": [1, ["2", {"value": null, "validator": "disabled"}], 3]}),
    json!("2"),
    Expect::Value("Invalid choice for 'q': disabled")
)]
#[case(
    json!({"type": "int", "choices": {"one": 1, "two": 2, "three": 3}}),
    json!("1"),
    Expect::Pass
)]
#[case(
    json!({"type": "int", "choices": {"one": 1, "two": 2}}),
    json!("3"),
    Expect::Value(r"Invalid choice for 'q': 3 is not in \[1, 2\]")
)]
#[case(
    json!({"type": "int", "choices": {"one": 1, "two": {"value": 2}, "three": 3}}),
    json!("2"),
    Expect::Pass
)]
#[case(
    json!({
        "type": "int",
        "choices": {"one": 1, "two": {"value": 2, "validator": ""}, "three": 3},
    }),
    json!("2"),
    Expect::Pass
)]
#[case(
    json!({
        "type": "int",
        "choices": {"one": 1, "two": {"value": 2, "validator": "disabled"}, "three": 3},
    }),
    json!("2"),
    Expect::Value("Invalid choice for 'q': disabled")
)]
#[case(
    json!({"type": "str", "choices": {"one": null, "two": null, "three": null}}),
    json!("one"),
    Expect::Pass
)]
#[case(
    json!({"type": "str", "choices": {"one": null, "two": null, "three": null}}),
    json!("four"),
    Expect::Value("")
)]
#[case(
    json!({
        "type": "str",
        "choices": {
            "banana": {"value": "yellow", "validator": "invalid"},
            "tshirt": {"value": "yellow", "validator": ""},
        },
    }),
    json!("yellow"),
    Expect::Pass
)]
#[case(json!({"type": "path"}), json!("/etc/hostname"), Expect::Pass)]
#[case(json!({"type": "path"}), json!("/this/does/not/exist"), Expect::Pass)]
#[case(
    json!({
        "type": "path",
        "choices": {"one": null, "two": "/etc/hostname", "three": "/does/not/exist"},
    }),
    json!("/etc/hostname"),
    Expect::Pass
)]
#[case(
    json!({
        "type": "path",
        "choices": {"one": null, "two": "/etc/hostname", "three": "/does/not/exist"},
    }),
    json!("/does/not/exist"),
    Expect::Pass
)]
#[case(
    json!({
        "type": "path",
        "choices": {"one": null, "two": "/etc/hostname", "three": "/does/not/exist"},
    }),
    json!("/another/path/that/does/not/exist"),
    Expect::Value("")
)]
#[case(
    json!({"type": "path", "validator": ""}),
    json!("/this/does/not/exist"),
    Expect::Pass
)]
#[case(
    json!({
        "type": "path",
        "validator": "[% if q | length < 1 or q[0] != '/' %]must be absolute path[% endif %]",
    }),
    json!("/this/does/not/exist"),
    Expect::Pass
)]
#[case(
    json!({
        "type": "path",
        "validator": "[% if q | length < 1 or q[0] != '/' %]must be absolute path[% endif %]",
    }),
    json!("./local/path"),
    Expect::Value("")
)]
#[case(
    json!({"type": "yaml", "choices": {"one": null, "two": 2, "three": "null"}}),
    json!("one"),
    Expect::Pass
)]
#[case(
    json!({"type": "yaml", "choices": {"one": null, "two": 2, "three": "null"}}),
    json!("2"),
    Expect::Pass
)]
#[case(
    json!({"type": "yaml", "choices": {"one": null, "two": 2, "three": "null"}}),
    json!("null"),
    Expect::Pass
)]
#[case(
    json!({"type": "yaml", "choices": {"one": null, "two": 2, "three": "null"}}),
    json!("two"),
    Expect::Value("")
)]
#[case(
    json!({"type": "yaml", "choices": {"one": null, "two": 2, "three": "null"}}),
    json!("three"),
    Expect::Value("")
)]
#[case(
    json!({"type": "yaml", "choices": {"array": "[a, b]"}}),
    json!("[a, b]"),
    Expect::Pass
)]
#[case(
    json!({"type": "yaml", "choices": {"object": "k: v"}}),
    json!("k: v"),
    Expect::Pass
)]
#[case(
    json!({"type": "yaml", "choices": [{"one": 1, "two": 2}]}),
    json!("one: 1\ntwo: 2"),
    Expect::Pass
)]
#[case(
    json!({"type": "yaml", "choices": {"complex": {"value": {"key": "value"}}}}),
    json!("key: value"),
    Expect::Pass
)]
#[case(
    json!({
        "type": "yaml",
        "choices": {"complex": {"value": {"key": "value"}, "validator": ""}},
    }),
    json!("key: value"),
    Expect::Pass
)]
#[case(
    json!({
        "type": "yaml",
        "choices": {"complex": {"value": {"key": "value"}, "validator": "disabled"}},
    }),
    json!("key: value"),
    Expect::Value("Invalid choice for 'q': disabled")
)]
#[case(
    json!({"type": "yaml", "choices": {"complex": {"key": "value"}}}),
    json!("key: value"),
    Expect::Key("Property 'value' is required")
)]
#[case(
    json!({"type": "yaml", "choices": {"complex": {"value": 1, "validator": {}}}}),
    json!("1"),
    Expect::Value("Property 'validator' must be a string")
)]
#[case(
    json!({"type": "yaml", "choices": [{"key": "value"}]}),
    json!("key: value"),
    Expect::Pass
)]
#[case(
    json!({"type": "yaml", "choices": [{"value": 1, "validator": {}}]}),
    json!("value: 1\nvalidator: {}"),
    Expect::Pass
)]
#[case(
    json!({"type": "yaml", "choices": [["complex", {"value": {"key": "value"}}]]}),
    json!("key: value"),
    Expect::Pass
)]
#[case(
    json!({
        "type": "yaml",
        "choices": [["complex", {"value": {"key": "value"}, "validator": ""}]],
    }),
    json!("key: value"),
    Expect::Pass
)]
#[case(
    json!({
        "type": "yaml",
        "choices": [["complex", {"value": {"key": "value"}, "validator": "disabled"}]],
    }),
    json!("key: value"),
    Expect::Value("Invalid choice for 'q': disabled")
)]
#[case(
    json!({"type": "yaml", "choices": [["complex", {"key": "value"}]]}),
    json!("key: value"),
    Expect::Key("Property 'value' is required")
)]
#[case(
    json!({"type": "yaml", "choices": [["complex", {"value": 1, "validator": {}}]]}),
    json!("1"),
    Expect::Value("Property 'validator' must be a string")
)]
#[case(
    json!({
        "type": "str",
        "choices": ["one", "two", "three"],
        "multiselect": true,
        "validator": "[% if not q %]At least one choice required[% endif %]",
    }),
    json!([]),
    Expect::Value("At least one choice required")
)]
#[case(
    json!({"type": "str", "choices": ["one", "two"]}),
    json!("three"),
    Expect::Value(r"Invalid choice for 'q': 'three' is not in \['one', 'two'\]")
)]
#[case(
    json!({
        "type": "str",
        "choices": [["one", {"value": "one", "validator": "no longer available"}]],
    }),
    json!("one"),
    Expect::Value("Invalid choice for 'q': no longer available")
)]
fn test_validate_init_data(
    #[case] spec: serde_json::Value,
    #[case] value: serde_json::Value,
    #[case] expected: Expect,
) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => json!({
            "_envops": bracket_envops(),
            "_templates_suffix": ".jinja",
            "q": spec,
        })
        .to_string(),
    };
    check(
        worker(&src, &dst).data(v(json!({"q": value}))).run_copy(),
        expected,
    );
}

#[rstest]
fn test_validate_init_data_with_skipped_question(#[values(false, true)] defaults: bool) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => formatdoc! {r#"
            _envops: {BRACKET_ENVOPS_JSON}

            kind:
                type: str
                help: What kind do you need?
                choices:
                    foo: foo
                    bar: bar

            testbar:
                when: "[[ kind == 'bar' ]]"
                type: str
                help: any string bar?

            testfoo:
                when: "[[ kind == 'foo' ]]"
                type: str
                help: any string foo?
        "#},
        src.join("result.jinja") => indoc! {"
            [[ kind ]]
            [[ testbar ]]
            [[ testfoo ]]
        "},
    };
    worker(&src, &dst)
        .defaults(defaults)
        .data(v(json!({"kind": "foo", "testfoo": "helloworld"})))
        .run_copy()
        .unwrap();
    assert_eq!(read(dst.join("result")), "foo\n\nhelloworld\n");
}

#[rstest]
fn test_required_question_without_data(
    #[values("str", "int", "float", "bool", "json", "yaml")] type_name: &str,
) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => json!({"question": {"type": type_name}}).to_string(),
    };
    assert_err!(
        worker(&src, &dst).defaults(true).run_copy(),
        CopierError::Value(_),
        r#"Question "question" is required"#
    );
}

#[rstest]
#[case("str", json!(["one", "two", "three"]))]
#[case("int", json!([1, 2, 3]))]
#[case("float", json!([1.0, 2.0, 3.0]))]
#[case("json", json!(["[1]", "[2]", "[3]"]))]
#[case("yaml", json!(["- 1", "- 2", "- 3"]))]
fn test_required_choice_question_without_data(
    #[case] type_name: &str,
    #[case] choices: serde_json::Value,
) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => json!({"question": {"type": type_name, "choices": choices}}).to_string(),
    };
    assert_err!(
        worker(&src, &dst).defaults(true).run_copy(),
        CopierError::Value(_),
        r#"Question "question" is required"#
    );
}

#[rstest]
#[case(json!({"type": "str", "default": "string"}), Expect::Pass)]
#[case(json!({"type": "str", "default": "1.0"}), Expect::Pass)]
#[case(json!({"type": "str", "default": 1.0}), Expect::Pass)]
#[case(json!({"type": "str", "default": null}), Expect::Type)]
#[case(
    json!({
        "type": "str",
        "default": "",
        "validator": "[% if q|length < 3 %]too short[% endif %]",
    }),
    Expect::Value("")
)]
// Python marks this case xfail ("Default values of secrets are currently not
// validated"), expecting a `ValueError` that is not raised; the port asserts the
// actual behavior.
#[case(
    json!({
        "type": "str",
        "default": "",
        "secret": true,
        "validator": "[% if q|length < 3 %]too short[% endif %]",
    }),
    Expect::Pass
)]
#[case(json!({"type": "int", "default": 1}), Expect::Pass)]
#[case(json!({"type": "int", "default": 1.0}), Expect::Pass)]
#[case(json!({"type": "int", "default": "1"}), Expect::Pass)]
#[case(json!({"type": "int", "default": "1.0"}), Expect::Value(""))]
#[case(json!({"type": "int", "default": "no-int"}), Expect::Value(""))]
#[case(json!({"type": "int", "default": null}), Expect::Type)]
#[case(json!({"type": "int", "default": {}}), Expect::Type)]
#[case(json!({"type": "int", "default": []}), Expect::Type)]
#[case(json!({"type": "float", "default": 1.1}), Expect::Pass)]
#[case(json!({"type": "float", "default": 1}), Expect::Pass)]
#[case(json!({"type": "float", "default": "1.1"}), Expect::Pass)]
#[case(json!({"type": "float", "default": "no-float"}), Expect::Value(""))]
#[case(json!({"type": "float", "default": null}), Expect::Type)]
#[case(json!({"type": "float", "default": {}}), Expect::Type)]
#[case(json!({"type": "float", "default": []}), Expect::Type)]
#[case(json!({"type": "bool", "default": true}), Expect::Pass)]
#[case(json!({"type": "bool", "default": false}), Expect::Pass)]
#[case(json!({"type": "bool", "default": "y"}), Expect::Pass)]
#[case(json!({"type": "bool", "default": "n"}), Expect::Pass)]
#[case(json!({"type": "bool", "default": null}), Expect::Type)]
#[case(json!({"type": "json", "default": "\"string\""}), Expect::Pass)]
#[case(json!({"type": "json", "default": "1"}), Expect::Pass)]
#[case(json!({"type": "json", "default": 1}), Expect::Pass)]
#[case(json!({"type": "json", "default": "1.1"}), Expect::Pass)]
#[case(json!({"type": "json", "default": 1.1}), Expect::Pass)]
#[case(json!({"type": "json", "default": "true"}), Expect::Pass)]
#[case(json!({"type": "json", "default": true}), Expect::Pass)]
#[case(json!({"type": "json", "default": "false"}), Expect::Pass)]
#[case(json!({"type": "json", "default": false}), Expect::Pass)]
#[case(json!({"type": "json", "default": "{}"}), Expect::Pass)]
#[case(json!({"type": "json", "default": {}}), Expect::Pass)]
#[case(json!({"type": "json", "default": "[]"}), Expect::Pass)]
#[case(json!({"type": "json", "default": []}), Expect::Pass)]
#[case(json!({"type": "json", "default": "null"}), Expect::Pass)]
#[case(json!({"type": "json", "default": null}), Expect::Pass)]
#[case(json!({"type": "path", "default": null}), Expect::Type)]
#[case(json!({"type": "path", "default": ""}), Expect::Pass)]
#[case(json!({"type": "path", "default": "/etc/hostname"}), Expect::Pass)]
#[case(json!({"type": "path", "default": "/does/not/exist"}), Expect::Pass)]
#[case(json!({"type": "yaml", "default": "\"string\""}), Expect::Pass)]
#[case(json!({"type": "yaml", "default": "string"}), Expect::Pass)]
#[case(json!({"type": "yaml", "default": "1"}), Expect::Pass)]
#[case(json!({"type": "yaml", "default": 1}), Expect::Pass)]
#[case(json!({"type": "yaml", "default": "1.1"}), Expect::Pass)]
#[case(json!({"type": "yaml", "default": 1.1}), Expect::Pass)]
#[case(json!({"type": "yaml", "default": "true"}), Expect::Pass)]
#[case(json!({"type": "yaml", "default": true}), Expect::Pass)]
#[case(json!({"type": "yaml", "default": "false"}), Expect::Pass)]
#[case(json!({"type": "yaml", "default": false}), Expect::Pass)]
#[case(json!({"type": "yaml", "default": "{}"}), Expect::Pass)]
#[case(json!({"type": "yaml", "default": {}}), Expect::Pass)]
#[case(json!({"type": "yaml", "default": "[]"}), Expect::Pass)]
#[case(json!({"type": "yaml", "default": []}), Expect::Pass)]
#[case(json!({"type": "yaml", "default": "null"}), Expect::Pass)]
#[case(json!({"type": "yaml", "default": null}), Expect::Pass)]
fn test_validate_default_value(#[case] spec: serde_json::Value, #[case] expected: Expect) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => json!({"q": spec}).to_string(),
    };
    check(worker(&src, &dst).defaults(true).run_copy(), expected);
}

#[test]
fn test_multiselect_choices_preserve_order() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => json!({
            "q": {
                "type": "str",
                "choices": ["one", "two", "three"],
                "multiselect": true,
            }
        })
        .to_string(),
        src.join("q.yml.jinja") => "{{ q | to_nice_yaml }}",
    };
    worker(&src, &dst)
        .data(v(json!({"q": ["three", "one", "two"]})))
        .run_copy()
        .unwrap();
    assert_eq!(
        yaml_safe_load(&read(dst.join("q.yml"))),
        v(json!(["one", "two", "three"]))
    );
}

#[rstest]
#[case(indoc! {r#"
    choices: '["one", "two", "three"]'
"#})]
#[case(indoc! {r#"
    choices: '[{% for c in ["one", "two", "three"] %}{{ c }}{% if not loop.last %},{% endif %}{% endfor %}]'
"#})]
#[case(indoc! {r#"
    choices: |
        {%- for c in ['one', 'two', 'three'] %}
        - {{ c }}
        {%- endfor %}
"#})]
#[case(indoc! {r#"
    choices: |
        {%- for c in ['one', 'two', 'three'] %}
        {{ c }}: null
        {%- endfor %}
"#})]
#[case(indoc! {r#"
    choices: |
        {%- for c in ['one', 'two', 'three'] %}
        {{ c|upper }}: {{ c }}
        {%- endfor %}
"#})]
#[case(indoc! {r#"
    choices: |
        {%- for c in ['one', 'two', 'three'] %}
        - [{{ c|upper }}, {{ c }}]
        {%- endfor %}
"#})]
#[case(indoc! {r#"
    choices: |
        {%- for c in ['one', 'two', 'three'] %}
        {{ c|upper }}:
            value: {{ c }}
        {%- endfor %}
"#})]
#[case(indoc! {r#"
    choices: |
        {%- for c in ['one', 'two', 'three'] %}
        {{ c|upper }}:
            value: {{ c }}
            validator: "{% raw %}{% if false %}Validation error{% endif %}{% endraw %}"
        {%- endfor %}
"#})]
fn test_templated_choices(#[case] spec: &str) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => format!("q:\n    type: str\n{}", indent4(spec)),
        src.join("q.txt.jinja") => "{{ q }}",
    };
    worker(&src, &dst)
        .data(v(json!({"q": "two"})))
        .run_copy()
        .unwrap();
    // `yaml.safe_load` of the plain scalar `two` is the string itself.
    assert_eq!(read(dst.join("q.txt")), "two");
}

#[rstest]
#[case(
    indoc! {r#"
        type: str
        choices: ["one", "two", "three"]
        multiselect: true
        default: "{{ '[one, three]' }}"
    "#},
    json!(["one", "three"])
)]
#[case(
    indoc! {r#"
        type: str
        choices:
            one: '[1]'
            two: '[2]'
            three: '[3]'
        multiselect: true
        # NOTE: Balanced (nested) brackets don't need to be quoted because the
        # shallow list parser retains raw list items
        default: "{{ '[[1], [3]]' }}"
    "#},
    json!(["[1]", "[3]"])
)]
#[case(
    indoc! {r#"
        type: str
        choices:
            left: '['
            right: ']'
        multiselect: true
        # NOTE: Unbalanced brackets need to be explicitly quoted to avoid ambiguity
        # with the surrounding list brackets.
        default: >
            {{ '["[", "]"]' }}
    "#},
    json!(["[", "]"])
)]
#[case(
    indoc! {r#"
        type: int
        choices: [1, 2, 3]
        multiselect: true
        default: "{{ '[1, 3]' }}"
    "#},
    json!([1, 3])
)]
#[case(
    indoc! {r#"
        type: int
        choices: ['1', '2', '3']
        multiselect: true
        default: >
            {{ '["1", "3"]' }}
    "#},
    json!([1, 3])
)]
#[case(
    indoc! {r#"
        type: float
        choices: [1.0, 2.0, 3.0]
        multiselect: true
        default: "{{ '[1.0, 3.0]' }}"
    "#},
    json!([1.0, 3.0])
)]
#[case(
    indoc! {r#"
        type: float
        choices: ['1.0', '2.0', '3.0']
        multiselect: true
        default: >
            {{ '["1.0", "3.0"]' }}
    "#},
    json!([1.0, 3.0])
)]
#[case(
    indoc! {r#"
        type: json
        choices:
            one: '{"value": 1}'
            two: '{"value": 2}'
            three: '{"value": 3}'
        multiselect: true
        default: |
            {%- if true %}
            - '{"value": 1}'
            - '{"value": 3}'
            {%- endif %}
    "#},
    json!([{"value": 1}, {"value": 3}])
)]
#[case(
    indoc! {r#"
        type: yaml
        choices:
            one: '{value: [1, null]}'
            two: '{value: [2, null]}'
            three: '{value: [3, null]}'
        multiselect: true
        default: |
            {%- if true %}
            - {value: [1, null]}
            - {value: [3, null]}
            {%- endif %}
    "#},
    json!([{"value": [1, null]}, {"value": [3, null]}])
)]
#[case(
    indoc! {r#"
        type: yaml
        choices:
            one: '{value: [1, null]}'
            two: '{value: [2, null]}'
            three: '{value: [3, null]}'
        multiselect: true
        default: |
            {%- if true %}
            - '{value: [1, null]}'
            - '{value: [3, null]}'
            {%- endif %}
    "#},
    json!([{"value": [1, null]}, {"value": [3, null]}])
)]
#[case(
    indoc! {r#"
        type: yaml
        choices:
            one: '{value: "[1]"}'
            two: '{value: "[2]"}'
            three: '{value: "[3]"}'
        multiselect: true
        default: |
            {%- if true %}
            - {value: '[1]'}
            - {value: '[3]'}
            {%- endif %}
    "#},
    json!([{"value": "[1]"}, {"value": "[3]"}])
)]
fn test_multiselect_choices_with_templated_default_value(
    #[case] spec: &str,
    #[case] expected: serde_json::Value,
) {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => format!("q:\n{}", indent4(spec)),
        src.join("q.yml.jinja") => "{{ q | to_nice_yaml }}",
    };
    worker(&src, &dst).defaults(true).run_copy().unwrap();
    assert_eq!(yaml_safe_load(&read(dst.join("q.yml"))), v(expected));
}

#[test]
fn test_copier_phase_variable() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! { src.join("{{ _copier_phase }}.jinja") => "{{ _copier_phase }}" };
    worker(&src, &dst).run_copy().unwrap();
    assert!(dst.join("render").exists());
    assert_eq!(read(dst.join("render")), "render");
}

#[test]
fn test_invalid_jinja_in_rendered_path_reports_template_path() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    let broken_path = "some-{{{{ jinja_error }}}}";
    file_tree! { src.join(broken_path) => "content" };

    assert_err!(
        worker(&src, &dst).overwrite(true).run_copy(),
        CopierError::UserMessage(_),
        &regex::escape(broken_path)
    );
}

#[test]
fn test_invalid_jinja_in_nested_rendered_path_reports_failing_path_part() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    let broken_part = "some-{{{{ jinja_error }}}}";
    file_tree! { src.join("parent").join(broken_part).join("child.txt") => "content" };

    let error = expect_err(worker(&src, &dst).overwrite(true).run_copy());
    assert!(matches!(error, CopierError::UserMessage(_)));

    let message = error.to_string();
    assert!(message.contains(&format!("parent{MAIN_SEPARATOR}{broken_part}")));
    assert!(!message.contains(&format!(
        "parent{MAIN_SEPARATOR}{broken_part}{MAIN_SEPARATOR}child.txt"
    )));
}

#[test]
fn test_relative_render_path_outside_destination_raises_error() {
    let tmp = Tmp::new();
    let root = tmp.mktemp("root");
    let src = root.join("src");
    std::fs::create_dir(&src).unwrap();
    let dst = root.join("dst");
    std::fs::create_dir(&dst).unwrap();
    file_tree! {
        root.join("forbidden.txt") => "foo",
        src.join("{{ pathjoin('..', 'forbidden.txt') }}") => "bar",
    };
    assert_err!(
        worker(&src, &dst).overwrite(true).run_copy(),
        CopierError::ForbiddenPath { .. },
        &regex::escape(&format!(
            "\"{}\" is forbidden",
            Path::new("..").join("forbidden.txt").display()
        ))
    );
    assert!(!dst.join("forbidden.txt").exists());
    assert!(root.join("forbidden.txt").exists());
    assert_eq!(read(root.join("forbidden.txt")), "foo");
}

// @pytest.mark.skipif(os.name != "posix", reason="Applies only to POSIX")
#[cfg(unix)]
#[test]
fn test_absolute_render_path_outside_destination_raises_error_on_posix() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    let other = tmp.mktemp("other");
    // Skip the root directory, like `other.parts[1:]`.
    let path_parts = other
        .components()
        .skip(1)
        .map(|part| format!("'{}'", part.as_os_str().to_string_lossy()))
        .collect::<Vec<_>>()
        .join(", ");
    file_tree! {
        other.join("forbidden.txt") => "foo",
        src.join("copier.yml") => format!("_envops: {BRACKET_ENVOPS_JSON}"),
        src.join(format!("[[ pathjoin(_copier_conf.sep, {path_parts}, 'forbidden.txt') ]]")) => "bar",
    };
    assert_err!(
        worker(&src, &dst).overwrite(true).run_copy(),
        CopierError::ForbiddenPath { .. },
        &regex::escape(&format!(
            "\"{}\" is forbidden",
            other.join("forbidden.txt").display()
        ))
    );
    assert!(!dst.join("forbidden.txt").exists());
    assert!(other.join("forbidden.txt").exists());
    assert_eq!(read(other.join("forbidden.txt")), "foo");
}

// @pytest.mark.skipif(os.name != "nt", reason="Applies only to Windows")
#[cfg(windows)]
#[test]
fn test_absolute_render_path_outside_destination_raises_error_on_windows() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    let other = tmp.mktemp("other");
    // Skip the drive and the root directory, like `other.parts[1:]`.
    let path_parts = other
        .components()
        .skip(2)
        .map(|part| format!("'{}'", part.as_os_str().to_string_lossy()))
        .collect::<Vec<_>>()
        .join(", ");
    let drive = other
        .components()
        .next()
        .unwrap()
        .as_os_str()
        .to_string_lossy()
        .into_owned();
    file_tree! {
        other.join("forbidden.txt") => "foo",
        src.join("copier.yml") => formatdoc! {"
            _envops: {BRACKET_ENVOPS_JSON}

            drive:
                type: str
                default: \"{drive}\"
                when: false
        "},
        src.join(format!("[[ pathjoin(drive, {path_parts}, 'forbidden.txt') ]]")) => "bar",
    };
    assert_err!(
        worker(&src, &dst).overwrite(true).run_copy(),
        CopierError::ForbiddenPath { .. },
        &regex::escape(&format!(
            "\"{}\" is forbidden",
            other.join("forbidden.txt").display()
        ))
    );
    assert!(!dst.join("forbidden.txt").exists());
    assert!(other.join("forbidden.txt").exists());
    assert_eq!(read(other.join("forbidden.txt")), "foo");
}

#[test]
fn test_skipped_question_with_unset_default_value() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => indoc! {r#"
            q:
                type: str
                default: "{{ UNSET }}"
                when: false
                validator: "{% if not q %}Invalid{% endif %}"
        "#},
        src.join("output.txt.jinja") => "{{ question | default('<undefined>') }}",
    };
    worker(&src, &dst).defaults(true).run_copy().unwrap();
    assert_eq!(read(dst.join("output.txt")), "<undefined>");
}

/// Test that a warning is raised when `chmod` raises a `PermissionError`.
///
/// Changing path permissions is not allowed in some filesystems like gcsfuse. In this
/// case, Copier should not fail but suppress the error and raise a warning.
#[cfg(unix)]
#[test]
#[ignore = "not applicable: cannot inject a chmod failure"]
fn test_warn_if_chmod_raises_permission_error() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("test.txt") => "foo",
        dst.join("test.txt") => "bar",
    };

    // Ensure that the source and destination files have different permissions, so
    // Copier will want to change the permissions of the destination file to those of
    // the source file.
    std::fs::set_permissions(src.join("test.txt"), std::fs::Permissions::from_mode(0o400)).unwrap();
    std::fs::set_permissions(dst.join("test.txt"), std::fs::Permissions::from_mode(0o600)).unwrap();

    let dst_file_mode = std::fs::metadata(dst.join("test.txt"))
        .unwrap()
        .permissions()
        .mode();

    // TODO(port): make `chmod` of `dst/test.txt` fail with a permission error:
    //
    // def mocked_chmod(self: Path, mode: int) -> None:
    //     if self == dst / "test.txt":
    //         raise PermissionError("Operation not permitted")
    //     return chmod(self, mode)
    //
    // chmod = Path.chmod
    // with mock.patch.object(Path, "chmod", new=mocked_chmod), ...
    let (result, warnings) = capture_warnings(|| worker(&src, &dst).overwrite(true).run_copy());
    result.unwrap();
    assert!(warnings.iter().any(|w| w.kind == WarningKind::User
        && regex::Regex::new(r"^Path permissions for .+? cannot be changed")
            .unwrap()
            .is_match(&w.message)));

    assert_eq!(
        std::fs::metadata(dst.join("test.txt"))
            .unwrap()
            .permissions()
            .mode(),
        dst_file_mode
    );
    assert_eq!(read(dst.join("test.txt")), "foo");
}

#[rstest]
fn test_copy_with_relative_path_to_local_git_template_through_multiple_parents(
    #[values(1, 2, 3)] depth: usize,
) {
    let tmp = Tmp::new();
    let root = tmp.mktemp("root");
    let src = root.join("src");
    std::fs::create_dir(&src).unwrap();
    let mut dst = root.clone();
    for _ in 0..depth {
        dst.push("dst");
    }
    std::fs::create_dir_all(&dst).unwrap();

    file_tree! { src.join("test.txt.jinja") => "test" };
    git_save(&src, "Test commit", Some("v1"));

    git_save_with(root.join("dst"), "Test commit", None, true);
    // The point of the test is resolving the relative path against the current
    // directory, so run the binary from `dst`.
    let relative_src = format!("{}src", "../".repeat(depth));
    let output = copier_cmd()
        .current_dir(&dst)
        .args(["copy", &relative_src, dst.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "copier failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(dst.join("test.txt").is_file());
    assert_eq!(read(dst.join("test.txt")), "test");
}

/// Test that a malicious Git URL does not execute shell commands.
#[test]
fn test_git_argument_injection_via_url() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    git(&src, &["init"]);
    git(&dst, &["init"]);
    git(&dst, &["remote", "add", "origin", src.to_str().unwrap()]);
    // `suppress(ProcessExecutionError)`
    let result = worker(
        format!(
            "git+--upload-pack=touch {}",
            dst.join("malicious.txt").display()
        ),
        &dst,
    )
    .run_copy();
    if let Err(err) = result {
        assert!(
            matches!(err, CopierError::Process(_)),
            "unexpected error: {err:?}"
        );
    }
    assert!(!dst.join("malicious.txt").exists());
}
