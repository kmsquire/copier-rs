//! Loading of `copier.yml`: `!include` transclusion and multi-document merging.

use std::path::Path;

use crate::errors::{CopierError, Result};
use crate::output;
use crate::value::{Mapping, Value};
use crate::yaml::{self, LoadError, TaggedNode};

/// Settings whose values are concatenated across documents instead of overridden.
const MERGED_OPTIONS: &[&str] = &[
    "_exclude",
    "_jinja_extensions",
    "_secret_questions",
    "_skip_if_exists",
];

/// Load a `copier.yml`, resolving `!include` tags and merging multiple documents.
///
/// Later documents override earlier ones, except for the list settings in
/// `_exclude`, `_jinja_extensions`, `_secret_questions` and `_skip_if_exists`,
/// which accumulate. Keys keep the order of their first appearance.
///
/// # Errors
///
/// Returns [`CopierError::InvalidConfigFile`] for invalid YAML (after printing a
/// banner unless `quiet`), and the include errors Python raises: a `Type` error
/// for non-scalar `!include` nodes, a `Value` error for absolute patterns and
/// [`CopierError::ForbiddenPath`] for files outside the template.
pub fn load_template_config(conf_path: &Path, quiet: bool) -> Result<Value> {
    let documents = load_documents(conf_path, conf_path).map_err(|err| match err {
        LoadError::Custom(err) => err,
        LoadError::Yaml(err) => {
            if !quiet {
                output::printf_exception(
                    &err.to_string(),
                    "INVALID CONFIG FILE",
                    &conf_path.display().to_string(),
                );
            }
            CopierError::InvalidConfigFile {
                conf_path: conf_path.to_path_buf(),
            }
        }
    })?;
    let mut merged = Mapping::new();
    for option in MERGED_OPTIONS {
        let key = Value::from(*option);
        let mut items = Vec::new();
        for document in &documents {
            match document.get(&key) {
                Some(Value::Seq(values)) => items.extend(values.iter().cloned()),
                // Python extends a list with the characters of a string and fails on scalars.
                Some(Value::Str(text)) => {
                    items.extend(text.chars().map(|ch| Value::Str(ch.to_string())))
                }
                Some(other) => {
                    return Err(CopierError::Type(format!(
                        "'{}' object is not iterable",
                        other.py_type_name()
                    )))
                }
                None => {}
            }
        }
        if documents.iter().any(|document| document.contains_key(&key)) {
            merged.insert(key, Value::Seq(items));
        }
    }
    // Later documents win; iteration order follows first appearance, starting with
    // the merged list options, like Python's `ChainMap`.
    for document in documents.iter().rev() {
        for (key, value) in document {
            if !merged.contains_key(key) {
                merged.insert(key.clone(), value.clone());
            }
        }
    }
    let mut ordered = Mapping::with_capacity(merged.len());
    for document in &documents {
        for key in document.keys() {
            if let Some(value) = merged.get(key) {
                ordered.entry(key.clone()).or_insert_with(|| value.clone());
            }
        }
    }
    Ok(Value::Map(ordered))
}

/// Read every non-empty mapping document of `path`, flattening included lists.
fn load_documents(path: &Path, conf_path: &Path) -> std::result::Result<Vec<Mapping>, LoadError> {
    let bytes = std::fs::read(path).map_err(|err| LoadError::Custom(err.into()))?;
    let text = yaml::decode(&bytes)?;
    let mut handler =
        |tag: &str, node: TaggedNode<'_>| (tag == "!include").then(|| include(node, conf_path));
    let loaded = yaml::load_all_with(&text, &mut handler)?;
    let mut documents = Vec::new();
    flatten(loaded, &mut documents);
    Ok(documents)
}

/// Port of `funcy.lflatten(filter(None, docs))`: drop empty documents and splice
/// nested lists, keeping mappings.
fn flatten(values: Vec<Value>, out: &mut Vec<Mapping>) {
    for value in values {
        match value {
            Value::Seq(items) => flatten(items, out),
            Value::Map(map) if !map.is_empty() => out.push(map),
            _ => {}
        }
    }
}

fn include(node: TaggedNode<'_>, conf_path: &Path) -> Result<Value> {
    let TaggedNode::Scalar(pattern) = node else {
        let kind = if node == TaggedNode::Sequence {
            "SequenceNode"
        } else {
            "MappingNode"
        };
        return Err(CopierError::Type(format!(
            "Unsupported YAML node: {kind}(tag='!include')"
        )));
    };
    if pattern.starts_with('/') {
        return Err(CopierError::Value(
            "YAML include file path must be a relative path".to_owned(),
        ));
    }
    let template_root = conf_path
        .parent()
        .unwrap_or(Path::new("."))
        .canonicalize()?;
    let mut data = Vec::new();
    for include_file in glob(&template_root, pattern)? {
        let include_file = include_file.canonicalize()?;
        if !include_file.starts_with(&template_root) {
            return Err(CopierError::ForbiddenPath {
                path: include_file,
                hint: "YAML include file path must be inside the template directory".to_owned(),
            });
        }
        let documents = load_documents(&include_file, conf_path).map_err(|err| match err {
            LoadError::Custom(err) => err,
            LoadError::Yaml(_) => CopierError::InvalidConfigFile {
                conf_path: include_file.clone(),
            },
        })?;
        data.extend(documents.into_iter().map(Value::Map));
    }
    Ok(Value::Seq(data))
}

/// Minimal `Path.glob`: `*`, `?` and `[...]` within path components, plus `..`
/// and literal components. Matches are returned sorted.
fn glob(root: &Path, pattern: &str) -> Result<Vec<std::path::PathBuf>> {
    let mut candidates = vec![root.to_path_buf()];
    for component in pattern
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
    {
        let mut next = Vec::new();
        for candidate in candidates {
            if component == ".." || !component.contains(['*', '?', '[']) {
                let path = candidate.join(component);
                if path.exists() {
                    next.push(path);
                }
                continue;
            }
            let Ok(entries) = std::fs::read_dir(&candidate) else {
                continue;
            };
            let mut matched: Vec<_> = entries
                .flatten()
                .filter(|entry| {
                    crate::tools::fnmatch(&entry.file_name().to_string_lossy(), component)
                })
                .map(|entry| entry.path())
                .collect();
            matched.sort();
            next.extend(matched);
        }
        candidates = next;
    }
    Ok(candidates)
}

/// Split raw configuration into settings (keys starting with `_`, underscore
/// stripped) and questions (everything else, normalized to mappings).
pub fn filter_config(data: &Mapping) -> (Mapping, Mapping) {
    let mut config = Mapping::new();
    let mut questions = Mapping::new();
    for (key, value) in data {
        match key.as_str().and_then(|name| name.strip_prefix('_')) {
            Some(setting) => {
                config.insert(Value::from(setting), value.clone());
            }
            None => {
                let question = match value {
                    Value::Map(_) => value.clone(),
                    other => Value::map([("default", other.clone())]),
                };
                questions.insert(key.clone(), question);
            }
        }
    }
    (config, questions)
}
