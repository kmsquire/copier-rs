//! Port of `tests/test_settings.py`.

mod common;

use common::{assert_err, file_tree, read, worker, Tmp};
use copier::errors::{capture_warnings, WarningKind};
use copier::settings::is_trusted_repository;
use copier::settings::with_locations;
use copier::value::Mapping;
use copier::{load_settings, CopierError, Settings, Value};
use rstest::rstest;
use serde_json::json;

/// Build a [`Mapping`] from a `json!` object literal.
fn mapping(json: serde_json::Value) -> Mapping {
    match common::v(json) {
        Value::Map(map) => map,
        _ => unreachable!("expected a JSON object"),
    }
}

/// The home directory, as Python's `Path.home()`.
fn home() -> String {
    std::env::var("HOME").expect("HOME is set")
}

#[test]
fn test_default_settings() {
    let settings = Settings::default();

    assert!(settings.defaults.is_empty());
    assert!(settings.trust.is_empty());
}

#[test]
fn test_load_settings_from_default_location() {
    let tmp = Tmp::new();
    let config = tmp.path().join("config");
    std::fs::create_dir(&config).unwrap();
    let settings_path = config.join("settings.yml");
    std::fs::write(&settings_path, "defaults:\n  foo: bar").unwrap();

    let settings = with_locations(&settings_path, None, || load_settings(None)).unwrap();

    assert_eq!(settings.defaults, mapping(json!({"foo": "bar"})));
}

#[cfg(windows)]
#[test]
fn test_default_windows_settings_path() {
    assert_eq!(
        copier::settings::default_settings_path(),
        std::path::PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default())
            .join("AppData")
            .join("Local")
            .join("copier")
            .join("settings.yml"),
    );
}

#[test]
fn test_load_settings_from_default_location_dont_exist() {
    let tmp = Tmp::new();
    let settings_path = tmp.path().join("config").join("settings.yml");

    let settings = with_locations(&settings_path, None, || load_settings(None)).unwrap();

    assert!(settings.defaults.is_empty());
}

#[test]
fn test_settings_from_env_location() {
    let tmp = Tmp::new();
    let config = tmp.path().join("config");
    std::fs::create_dir(&config).unwrap();
    let settings_path = config.join("settings.yml");
    std::fs::write(&settings_path, "defaults:\n  foo: bar").unwrap();

    let settings_from_env_path = tmp.path().join("settings.yml");
    std::fs::write(&settings_from_env_path, "defaults:\n  foo: baz").unwrap();

    let settings = with_locations(&settings_path, Some(&settings_from_env_path), || {
        load_settings(None)
    })
    .unwrap();

    assert_eq!(settings.defaults, mapping(json!({"foo": "baz"})));
}

#[test]
fn test_load_settings_from_param() {
    let tmp = Tmp::new();
    let config = tmp.path().join("config");
    std::fs::create_dir(&config).unwrap();
    let settings_path = config.join("settings.yml");
    std::fs::write(&settings_path, "defaults:\n  foo: bar").unwrap();

    let settings_from_env_path = tmp.path().join("settings.yml");
    std::fs::write(&settings_from_env_path, "defaults:\n  foo: baz").unwrap();

    let file_path = tmp.path().join("file.yml");
    std::fs::write(&file_path, "defaults:\n  from: file").unwrap();

    let settings = with_locations(&settings_path, Some(&settings_from_env_path), || {
        load_settings(Some(&file_path))
    })
    .unwrap();

    assert_eq!(settings.defaults, mapping(json!({"from": "file"})));
}

#[test]
fn test_load_settings_env_defined_but_missing() {
    let tmp = Tmp::new();
    let config = tmp.path().join("config");
    std::fs::create_dir(&config).unwrap();
    let settings_path = config.join("settings.yml");

    let (_, warnings) = with_locations(&settings_path, Some(&settings_path), || {
        capture_warnings(|| load_settings(None))
    });

    assert!(warnings
        .iter()
        .any(|w| w.kind == WarningKind::MissingSettings));
}

#[rstest]
fn test_load_settings_from_utf_file(
    #[values("utf-8", "utf-8-sig", "utf-16-le", "utf-16-be")] encoding: &str,
) {
    let encode = |data: &str| -> Vec<u8> {
        match encoding {
            "utf-8" => data.as_bytes().to_vec(),
            "utf-8-sig" => [b"\xEF\xBB\xBF".as_slice(), data.as_bytes()].concat(),
            "utf-16-le" => format!("\u{feff}{data}")
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect(),
            "utf-16-be" => format!("\u{feff}{data}")
                .encode_utf16()
                .flat_map(u16::to_be_bytes)
                .collect(),
            _ => unreachable!("unknown encoding"),
        }
    };

    let defaults = json!({
        "foo": "\u{3053}\u{3093}\u{306b}\u{3061}\u{306f}", // japanese hiragana
        "bar": "\u{1f60e}", // smiling face with sunglasses
    });

    let tmp = Tmp::new();
    let settings_path = tmp.path().join("settings.yml");
    std::fs::write(
        &settings_path,
        encode(&json!({"defaults": defaults}).to_string()),
    )
    .unwrap();

    // Python overrides `io.text_encoding` to cp932 here; Rust has no default text encoding, so that override is dropped.
    let settings = load_settings(Some(&settings_path)).unwrap();

    assert_eq!(settings.defaults, mapping(defaults));
}

#[rstest]
#[case(b"\x00", "^Invalid YAML data: unacceptable character")]
#[case(
    b"defaults: should be an object",
    "^Invalid format:\n  defaults:\n    Input should be a valid dictionary"
)]
#[case(
    b"trust: should be an array",
    "^Invalid format:\n  trust:\n    Input should be a valid set"
)]
fn test_load_settings_with_invalid_data(#[case] data: &[u8], #[case] error: &str) {
    let tmp = Tmp::new();
    let settings_path = tmp.path().join("settings.yml");
    std::fs::write(&settings_path, data).unwrap();

    assert_err!(
        load_settings(Some(&settings_path)),
        CopierError::Settings(_),
        error
    );
}

#[test]
fn test_is_trusted() {
    let home = home();
    for base in [
        "https://github.com",
        "ssh://git@github.com",
        "git@github.com:",
        "gh:",
        "gl:",
    ] {
        let cases: [(String, Vec<String>, bool); 34] = [
            // Plain URLs with no dot segments: normal prefix/equality matching.
            (format!("{base}/user/repo.git"), vec![], false),
            (
                format!("{base}/user/repo.git"),
                vec![format!("{base}/user")],
                false,
            ),
            (
                format!("{base}/user/repo.git"),
                vec![format!("{base}/user/")],
                true,
            ),
            (
                format!("{base}/user/repo.git"),
                vec![format!("{base}/user/repo")],
                false,
            ),
            (
                format!("{base}/user/repo.git"),
                vec![format!("{base}/user/repo.git")],
                true,
            ),
            (
                format!("{base}/user/repo.git"),
                vec![format!("{base}/")],
                true,
            ),
            (
                format!("{base}/user/repo.git"),
                vec![base.to_string()],
                false,
            ),
            (
                format!("{base}/user/repo"),
                vec![format!("{base}/user/repo.git")],
                false,
            ),
            // Literal `..` traversal is collapsed, so a trusted prefix still
            // matches (or not) as expected.
            (
                format!("{base}/user/../evil/repo.git"),
                vec![format!("{base}/user/")],
                false,
            ),
            (
                format!("{base}/user/../evil/repo.git"),
                vec![format!("{base}/user/../evil/repo.git")],
                true,
            ),
            (
                format!("{base}/x/../user/repo.git"),
                vec![format!("{base}/user/")],
                true,
            ),
            // A repository URL is "ambiguous" if it contains any character with
            // no agreed-upon meaning to every URL parser or HTTP server (e.g.
            // percent-encoding, backslashes, doubled slashes): we can't know
            // whether a given remote Git server or transport resolves it the
            // same way we do, so it never satisfies a trust *prefix*, no matter
            // what it would normalize to.
            (
                format!("{base}/user/%2e%2e/evil/repo.git"),
                vec![format!("{base}/user/")],
                false,
            ),
            (
                format!("{base}/user/%2E%2E/evil/repo.git"),
                vec![format!("{base}/user/")],
                false,
            ),
            (
                format!("{base}/user/%2e%2E/evil/repo.git"),
                vec![format!("{base}/user/")],
                false,
            ),
            (
                format!("{base}/user/%2e./evil/repo.git"),
                vec![format!("{base}/user/")],
                false,
            ),
            (
                format!("{base}/user/.%2e/evil/repo.git"),
                vec![format!("{base}/user/")],
                false,
            ),
            (
                format!("{base}/user%2f%2e%2e%2fevil/repo.git"),
                vec![format!("{base}/user/")],
                false,
            ),
            (
                format!("{base}/user/%2e%2e%5cevil/repo.git"),
                vec![format!("{base}/user/")],
                false,
            ),
            (
                format!("{base}/user/%2e%2e%5Cevil/repo.git"),
                vec![format!("{base}/user/")],
                false,
            ),
            (
                format!("{base}/user/..%5cevil/repo.git"),
                vec![format!("{base}/user/")],
                false,
            ),
            (
                format!("{base}/user/..\\evil/repo.git"),
                vec![format!("{base}/user/")],
                false,
            ),
            (
                format!("{base}/user%2fsub/../evil/repo.git"),
                vec![format!("{base}/user/")],
                false,
            ),
            (
                format!("{base}/user%5csub/../evil/repo.git"),
                vec![format!("{base}/user/")],
                false,
            ),
            (
                format!("{base}/user\\sub/../evil/repo.git"),
                vec![format!("{base}/user/")],
                false,
            ),
            (
                format!("{base}/user%2fsub/../repo.git"),
                vec![format!("{base}/user/repo.git")],
                false,
            ),
            (
                format!("{base}/x/%2e%2e/user/repo.git"),
                vec![format!("{base}/user/")],
                false,
            ),
            (
                format!("{base}/x/%2E%2E/user/repo.git"),
                vec![format!("{base}/user/")],
                false,
            ),
            (
                format!("{base}/attacker/repo/%2e%2e/%2e%2e/user/repo.git"),
                vec![format!("{base}/user/")],
                false,
            ),
            (
                format!("{base}/attacker/evil/..//user/user"),
                vec![format!("{base}/user/")],
                false,
            ),
            // An ambiguous repository can still be trusted via an *exact* trust
            // entry, but only through an exact raw string match: whatever a
            // server does with these characters, it does identically to
            // identical input.
            (
                format!("{base}/user/%2e%2e/evil/repo.git"),
                vec![format!("{base}/user/%2e%2e/evil/repo.git")],
                true,
            ),
            (
                format!("{base}/user/%2e%2e/evil/repo.git"),
                vec![format!("{base}/user/../evil/repo.git")],
                false,
            ),
            (
                format!("{base}/user/%2e%2e%5cevil/repo.git"),
                vec![format!("{base}/user/%2e%2e%5cevil/repo.git")],
                true,
            ),
            (
                format!("{base}/user/%2e%2e%5cevil/repo.git"),
                vec![format!("{base}/user/..\\evil/repo.git")],
                false,
            ),
            (
                format!("{base}/user/%2e%2e%5cevil/repo.git"),
                vec![format!("{base}/user/../evil/repo.git")],
                false,
            ),
        ];
        for (repository, trust, expected) in cases {
            assert_eq!(
                is_trusted_repository(&trust, &repository),
                expected,
                "repository {repository:?} with trust {trust:?}"
            );
        }
    }

    // Local filesystem paths: normal prefix/equality matching, with `~`
    // expansion and literal `..` traversal collapse.
    let local_cases: [(String, Vec<String>, bool); 11] = [
        (format!("{home}/template"), vec![], false),
        (
            format!("{home}/template"),
            vec![format!("{home}/template")],
            true,
        ),
        (
            format!("{home}/template"),
            vec!["~/template".to_owned()],
            true,
        ),
        (
            format!("{home}/path/to/template"),
            vec!["~/path/to/template".to_owned()],
            true,
        ),
        (
            format!("{home}/path/to/template"),
            vec!["~/path/to/".to_owned()],
            true,
        ),
        (
            format!("{home}/path/to/template"),
            vec!["~/path/to".to_owned()],
            false,
        ),
        (
            format!("{home}/trusted/../attacker/template"),
            vec![format!("{home}/trusted/")],
            false,
        ),
        (
            format!("{home}/trusted/../attacker/template"),
            vec!["~/trusted/".to_owned()],
            false,
        ),
        // Percent-encoded segments are never decoded for local paths, so they're
        // just literal, harmless directory names: they don't traverse anywhere,
        // and they still match a trusted prefix normally.
        (
            format!("{home}/user/%2e%2e/repo"),
            vec![format!("{home}/user/")],
            true,
        ),
        (
            format!("{home}/user/%2e%2e/repo"),
            vec![format!("{home}/user/%2e%2e/repo")],
            true,
        ),
        (
            format!("{home}/user/%2e%2e/repo"),
            vec![format!("{home}/user/repo")],
            false,
        ),
    ];
    for (repository, trust, expected) in local_cases {
        assert_eq!(
            is_trusted_repository(&trust, &repository),
            expected,
            "repository {repository:?} with trust {trust:?}"
        );
    }
}

#[test]
fn test_copy_with_defaults() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => "project_name: foo",
        src.join("result.txt.jinja") => "{{ project_name }}",
    };

    worker(&src, &dst)
        .defaults(true)
        .settings(Settings {
            defaults: mapping(json!({"project_name": "bar"})),
            trust: vec![],
        })
        .run_copy()
        .unwrap();
    assert_eq!(read(dst.join("result.txt")), "bar");
}
