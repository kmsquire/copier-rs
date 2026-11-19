//! Port of `tests/test_settings_deprecated.py`, run against the current `Settings`
//! API. The deprecated `SettingsModel` has no equivalent, and no test here asserts
//! its deprecation warning.

mod common;

use common::Tmp;
use copier::errors::{capture_warnings, WarningKind};
use copier::settings::with_locations;
use copier::value::Mapping;
use copier::{load_settings, Settings, Value};
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
fn test_settings_from_default_location() {
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
fn test_settings_from_default_location_dont_exists() {
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
fn test_settings_from_param() {
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
fn test_settings_defined_but_missing() {
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
fn test_settings_from_utf_file(
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
#[case("https://github.com/user/repo.git".to_owned(), vec![], false)]
#[case(
    "https://github.com/user/repo.git".to_owned(),
    vec!["https://github.com/user/repo.git".to_owned()],
    true
)]
#[case(
    "https://github.com/user/repo".to_owned(),
    vec!["https://github.com/user/repo.git".to_owned()],
    false
)]
#[case(
    "https://github.com/user/repo.git".to_owned(),
    vec!["https://github.com/user/".to_owned()],
    true
)]
#[case(
    "https://github.com/user/repo.git".to_owned(),
    vec!["https://github.com/user/repo".to_owned()],
    false
)]
#[case(
    "https://github.com/user/repo.git".to_owned(),
    vec!["https://github.com/user".to_owned()],
    false
)]
#[case("https://github.com/user/repo.git".to_owned(), vec!["https://github.com/".to_owned()], true)]
#[case("https://github.com/user/repo.git".to_owned(), vec!["https://github.com".to_owned()], false)]
#[case(format!("{}/template", home()), vec![], false)]
#[case(format!("{}/template", home()), vec![format!("{}/template", home())], true)]
#[case(format!("{}/template", home()), vec!["~/template".to_owned()], true)]
#[case(format!("{}/path/to/template", home()), vec!["~/path/to/template".to_owned()], true)]
#[case(format!("{}/path/to/template", home()), vec!["~/path/to/".to_owned()], true)]
#[case(format!("{}/path/to/template", home()), vec!["~/path/to".to_owned()], false)]
fn test_is_trusted(
    #[case] repository: String,
    #[case] trust: Vec<String>,
    #[case] is_trusted: bool,
) {
    let settings = Settings {
        trust,
        ..Settings::default()
    };

    assert_eq!(settings.is_trusted(&repository), is_trusted);
}
