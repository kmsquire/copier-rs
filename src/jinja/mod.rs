//! Template rendering on top of `minijinja`, configured to behave like the Jinja2
//! environment Python Copier builds (`Worker.jinja_env` and `copier/_jinja_ext.py`).

mod convert;
mod filters;
mod pycompat;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use minijinja::syntax::SyntaxConfig;
use minijinja::value::{Kwargs, Rest, Value as JinjaValue};
use minijinja::{AutoEscape, Environment, ErrorKind, UndefinedBehavior};
use regex::Regex;
use sha2::{Digest, Sha512};

pub use convert::{from_jinja, map_to_jinja, to_jinja};

use crate::errors::{warn, CopierError, Result, TemplateErrorKind, WarningKind};
use crate::value::{Mapping, Value};

/// Extensions Python Copier loads by name that need nothing extra here.
const BUILTIN_EXTENSIONS: &[&str] = &[
    "jinja2_ansible_filters.AnsibleCoreFiltersExtension",
    "jinja2.ext.loopcontrols",
];

/// Jinja `Environment` options that have no effect on rendering here.
const IGNORED_ENVOPS: &[&str] = &[
    "newline_sequence",
    "optimized",
    "cache_size",
    "auto_reload",
    "bytecode_cache",
    "enable_async",
    "finalize",
];

/// A `{% yield name from iterable %}...{% endyield %}` tag found in a path component.
#[derive(Debug, Clone, PartialEq)]
pub struct YieldTag {
    /// Loop variable name.
    pub name: String,
    /// Source of the expression producing the items.
    pub iterable: String,
    /// The component with the tag reduced to its body, to render once per item.
    pub body: String,
}

/// Variables a template is rendered with, in insertion order.
#[derive(Debug, Clone, Default)]
pub struct Context(indexmap::IndexMap<String, JinjaValue>);

impl Context {
    /// An empty context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a variable to an engine value.
    pub fn insert(&mut self, name: impl Into<String>, value: JinjaValue) {
        self.0.insert(name.into(), value);
    }

    /// Set a variable to a Copier value.
    pub fn insert_value(&mut self, name: impl Into<String>, value: &Value) {
        self.insert(name, to_jinja(value));
    }

    /// Set every entry of `map` as a variable, named by the key's text.
    pub fn extend_from(&mut self, map: &Mapping) {
        for (key, value) in map {
            self.insert_value(key.py_str(), value);
        }
    }

    /// A copy with one more variable set.
    pub fn with(&self, name: impl Into<String>, value: JinjaValue) -> Self {
        let mut extended = self.clone();
        extended.insert(name, value);
        extended
    }

    /// Whether a variable is set.
    pub fn contains(&self, name: &str) -> bool {
        self.0.contains_key(name)
    }

    /// The variables as plain data; engine objects become their text.
    pub fn to_mapping(&self) -> Mapping {
        self.0
            .iter()
            .map(|(name, value)| (Value::from(name.as_str()), from_jinja(value)))
            .collect()
    }

    /// The value handed to the engine.
    pub fn to_jinja(&self) -> JinjaValue {
        JinjaValue::from_pairs(
            self.0
                .iter()
                .map(|(name, value)| (name.as_str(), value.clone())),
        )
    }
}

/// A slot for a Copier error raised somewhere the engine can only signal its own
/// error type (the loader, lazily loaded data); rendering reports it afterwards.
#[derive(Debug, Clone, Default)]
pub struct DeferredError(Arc<Mutex<Option<CopierError>>>);

impl DeferredError {
    /// Record `error` unless one is already pending.
    pub fn set(&self, error: CopierError) {
        if let Ok(mut slot) = self.0.lock() {
            slot.get_or_insert(error);
        }
    }

    fn take(&self) -> Option<CopierError> {
        self.0.lock().ok().and_then(|mut slot| slot.take())
    }
}

/// The template engine for one template.
pub struct JinjaEnv {
    env: Environment<'static>,
    yield_open: Regex,
    yield_close: Regex,
    path_division: Regex,
    deferred: DeferredError,
}

fn delimiter(envops: &Mapping, key: &str, default: &str) -> String {
    envops
        .get(&Value::from(key))
        .filter(|value| !value.is_null())
        .map_or_else(|| default.to_owned(), Value::py_str)
}

fn flag(envops: &Mapping, key: &str, default: bool) -> bool {
    envops
        .get(&Value::from(key))
        .map_or(default, Value::is_truthy)
}

impl JinjaEnv {
    /// Build the engine for the template checked out at `root`.
    ///
    /// # Errors
    ///
    /// Fails on unsupported `_envops` values and on Jinja extensions that have no
    /// native implementation.
    pub fn new(root: &Path, envops: &Mapping, extensions: &[String]) -> Result<Self> {
        for extension in extensions {
            if !BUILTIN_EXTENSIONS.contains(&extension.as_str()) {
                let module = extension.split('.').next().unwrap_or(extension);
                return Err(CopierError::ExtensionNotFound(format!(
                    "Copier could not load some Jinja extensions:\nNo module named '{module}'\nMake sure to install these extensions alongside Copier itself.\nSee the docs at https://copier.readthedocs.io/en/latest/configuring/#jinja_extensions"
                )));
            }
        }
        let block_start = delimiter(envops, "block_start_string", "{%");
        let block_end = delimiter(envops, "block_end_string", "%}");
        let mut syntax = SyntaxConfig::builder();
        syntax
            .block_delimiters(block_start.clone(), block_end.clone())
            .variable_delimiters(
                delimiter(envops, "variable_start_string", "{{"),
                delimiter(envops, "variable_end_string", "}}"),
            )
            .comment_delimiters(
                delimiter(envops, "comment_start_string", "{#"),
                delimiter(envops, "comment_end_string", "#}"),
            )
            .trim_blocks(flag(envops, "trim_blocks", false))
            .lstrip_blocks(flag(envops, "lstrip_blocks", false))
            .keep_trailing_newline(flag(envops, "keep_trailing_newline", true));
        let line_statement = delimiter(envops, "line_statement_prefix", "");
        if !line_statement.is_empty() {
            syntax.line_statement_prefix(line_statement);
        }
        let line_comment = delimiter(envops, "line_comment_prefix", "");
        if !line_comment.is_empty() {
            syntax.line_comment_prefix(line_comment);
        }
        let syntax = syntax
            .build()
            .map_err(|err| CopierError::ConfigFile(format!("Invalid envops delimiters: {err}")))?;

        let mut env = Environment::new();
        env.set_syntax(syntax);
        env.set_undefined_behavior(match envops.get(&Value::from("undefined")).filter(|value| value.is_truthy()) {
            None => UndefinedBehavior::Lenient,
            Some(Value::Str(name)) if name == "jinja2.Undefined" => UndefinedBehavior::Lenient,
            Some(Value::Str(name)) if name == "jinja2.StrictUndefined" => UndefinedBehavior::Strict,
            Some(other) => {
                return Err(CopierError::ConfigFile(format!(
                    "Unsupported envops.undefined value specified: {}.\nSupported values are \"jinja2.Undefined\" and \"jinja2.StrictUndefined\".",
                    other.py_str()
                )))
            }
        });
        // Jinja2 escapes nothing unless asked; the engine would otherwise guess from
        // file extensions.
        let autoescape = flag(envops, "autoescape", false);
        env.set_auto_escape_callback(move |_| {
            if autoescape {
                AutoEscape::Html
            } else {
                AutoEscape::None
            }
        });
        const KNOWN: &[&str] = &[
            "block_start_string",
            "block_end_string",
            "variable_start_string",
            "variable_end_string",
            "comment_start_string",
            "comment_end_string",
            "line_statement_prefix",
            "line_comment_prefix",
            "trim_blocks",
            "lstrip_blocks",
            "keep_trailing_newline",
            "autoescape",
            "undefined",
            "extensions",
        ];
        for key in envops.keys().map(Value::py_str) {
            if !KNOWN.contains(&key.as_str()) && !IGNORED_ENVOPS.contains(&key.as_str()) {
                return Err(CopierError::Type(format!(
                    "Environment.__init__() got an unexpected keyword argument '{key}'"
                )));
            }
        }

        let deferred = DeferredError::default();
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let stash = deferred.clone();
        env.set_loader(move |name| load_source(&root, name, &stash));
        env.set_unknown_method_callback(pycompat::unknown_method_callback);
        minijinja_contrib::add_to_environment(&mut env);
        filters::register(&mut env);
        env.add_function("pathjoin", pathjoin);
        env.add_function("now", now);
        env.add_function("make_secret", make_secret);

        let tag = |keyword: &str| {
            Regex::new(&format!(
                r"{}[-+]?\s*{keyword}\s*[-+]?{}",
                regex::escape(&block_start),
                regex::escape(&block_end)
            ))
        };
        let pattern_error = |err: regex::Error| {
            CopierError::ConfigFile(format!("Invalid envops delimiters: {err}"))
        };
        let yield_open =
            tag(r"yield\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)\s+from\s+(?P<iterable>.+?)")
                .map_err(pattern_error)?;
        let yield_close = tag("endyield").map_err(pattern_error)?;
        let path_division = Regex::new(
            r#"(?P<path>_copier_conf\.(?:src_path|dst_path|answers_file))\s*/\s*(?P<operand>'[^']*'|"[^"]*"|[A-Za-z_][\w.]*)"#,
        )
        .map_err(pattern_error)?;
        Ok(Self {
            env,
            yield_open,
            yield_close,
            path_division,
            deferred,
        })
    }

    fn convert_error(&self, err: &minijinja::Error) -> CopierError {
        if let Some(deferred) = self.deferred.take() {
            return deferred;
        }
        let kind = match err.kind() {
            ErrorKind::UndefinedError => TemplateErrorKind::Undefined,
            ErrorKind::SyntaxError | ErrorKind::BadEscape | ErrorKind::InvalidDelimiter => {
                TemplateErrorKind::Syntax
            }
            _ => TemplateErrorKind::Other,
        };
        let detail = err.detail().unwrap_or_default();
        let message = match (kind, detail.strip_suffix(" is undefined")) {
            // Jinja2 quotes the name with apostrophes.
            (TemplateErrorKind::Undefined, Some(name)) => {
                format!("'{}' is undefined", name.trim_matches('`'))
            }
            _ if detail.is_empty() => err.kind().to_string(),
            _ => detail.to_owned(),
        };
        CopierError::Template { kind, message }
    }

    /// Rewrite `_copier_conf.<path> / <operand>` into a `pathjoin` call.
    ///
    /// In Python those values are `pathlib` paths and `/` joins them; here they are
    /// strings, for which the engine has no division.
    fn rewrite_path_division<'s>(&self, source: &'s str) -> std::borrow::Cow<'s, str> {
        if !source.contains("_copier_conf.") {
            return std::borrow::Cow::Borrowed(source);
        }
        self.path_division
            .replace_all(source, "pathjoin(${path}, ${operand}, mode='native')")
    }

    /// Render the template `source` with `context`.
    ///
    /// # Errors
    ///
    /// Returns [`CopierError::Template`] on syntax or rendering failures.
    pub fn render_str(&self, source: &str, context: &Context) -> Result<String> {
        let rendered = self
            .env
            .render_str(&self.rewrite_path_division(source), context.to_jinja());
        self.finish(rendered)
    }

    /// Render the template file `name`, a POSIX path relative to the template root.
    ///
    /// # Errors
    ///
    /// Returns [`CopierError::Template`] on syntax or rendering failures and
    /// [`CopierError::ForbiddenPath`] when the file resolves outside the root.
    pub fn render_file(&self, name: &str, context: &Context) -> Result<String> {
        let rendered = self
            .env
            .get_template(name)
            .and_then(|template| template.render(context.to_jinja()));
        self.finish(rendered)
    }

    /// Evaluate the expression `source` with `context`.
    ///
    /// # Errors
    ///
    /// Returns [`CopierError::Template`] on syntax or evaluation failures.
    pub fn eval(&self, source: &str, context: &Context) -> Result<JinjaValue> {
        let evaluated = self
            .env
            .compile_expression(source)
            .and_then(|expression| expression.eval(context.to_jinja()));
        self.finish(evaluated)
    }

    /// A handle for reporting errors from code the engine calls back into.
    pub fn deferred_error(&self) -> DeferredError {
        self.deferred.clone()
    }

    fn finish<T>(&self, outcome: std::result::Result<T, minijinja::Error>) -> Result<T> {
        match outcome {
            Err(err) => Err(self.convert_error(&err)),
            Ok(value) => match self.deferred.take() {
                Some(deferred) => Err(deferred),
                None => Ok(value),
            },
        }
    }

    /// Whether `source` contains a `yield` tag.
    pub fn has_yield_tag(&self, source: &str) -> bool {
        self.yield_open.is_match(source)
    }

    /// Extract the `yield` tag of a path component, if any.
    ///
    /// # Errors
    ///
    /// Returns [`CopierError::MultipleYieldTags`] when there is more than one.
    pub fn yield_tag(&self, source: &str) -> Result<Option<YieldTag>> {
        let mut tags = self.yield_open.captures_iter(source);
        let Some(first) = tags.next() else {
            return Ok(None);
        };
        if tags.next().is_some() {
            return Err(CopierError::MultipleYieldTags(format!(
                "Attempted to parse the yield tag twice. Only one yield tag is allowed per path name.\nA yield tag with the name: \"{}\" and iterable: \"{}\" already exists.",
                &first["name"], &first["iterable"]
            )));
        }
        // The body is emitted once per item, so the tags themselves render nothing.
        let without_open = self.yield_open.replace(source, "");
        let body = self.yield_close.replace(&without_open, "").into_owned();
        Ok(Some(YieldTag {
            name: first["name"].to_owned(),
            iterable: first["iterable"].trim().to_owned(),
            body,
        }))
    }
}

fn load_source(
    root: &Path,
    name: &str,
    stash: &DeferredError,
) -> std::result::Result<Option<String>, minijinja::Error> {
    let path: PathBuf = root.join(name);
    let Ok(resolved) = path.canonicalize() else {
        return Ok(None);
    };
    if !resolved.starts_with(root) {
        stash.set(CopierError::ForbiddenPath {
            path: resolved,
            hint: "File path is outside the template root".to_owned(),
        });
        return Err(minijinja::Error::new(
            ErrorKind::InvalidOperation,
            "file path is outside the template root",
        ));
    }
    match std::fs::read(&resolved) {
        Ok(bytes) => String::from_utf8(bytes).map(Some).map_err(|_| {
            minijinja::Error::new(
                ErrorKind::BadSerialization,
                format!("{name} is not valid UTF-8"),
            )
        }),
        Err(_) => Ok(None),
    }
}

/// `pathjoin(*paths, mode="posix")`: join path fragments like Python's `PurePath`.
fn pathjoin(parts: Rest<String>, kwargs: Kwargs) -> std::result::Result<String, minijinja::Error> {
    let mode = kwargs
        .get::<Option<String>>("mode")?
        .unwrap_or_else(|| "posix".to_owned());
    kwargs.assert_all_used()?;
    let windows = match mode.as_str() {
        "posix" => false,
        "windows" => true,
        "native" => cfg!(windows),
        other => {
            return Err(minijinja::Error::new(
                ErrorKind::InvalidOperation,
                format!("'{other}'"),
            ));
        }
    };
    Ok(pure_path_join(&parts, windows))
}

/// Join fragments like `PurePosixPath(*parts)` / `PureWindowsPath(*parts)`: an
/// absolute fragment restarts the path, and `.` and empty segments disappear.
pub(crate) fn pure_path_join(parts: &[String], windows: bool) -> String {
    let separator = if windows { '\\' } else { '/' };
    let is_separator = |ch: char| ch == '/' || (windows && ch == '\\');
    let mut absolute = false;
    let mut segments: Vec<&str> = Vec::new();
    for part in parts {
        if part.starts_with(is_separator) {
            absolute = true;
            segments.clear();
        }
        segments.extend(
            part.split(is_separator)
                .filter(|segment| !segment.is_empty() && *segment != "."),
        );
    }
    let joined = segments.join(&separator.to_string());
    match (absolute, joined.is_empty()) {
        (true, _) => format!("{separator}{joined}"),
        (false, true) => ".".to_owned(),
        (false, false) => joined,
    }
}

fn now() -> String {
    warn(
        WarningKind::Future,
        "'now' will be removed in a future release of Copier.\nPlease use this instead: {{ '%Y-%m-%d %H:%M:%S' | strftime }}\nstrftime format reference https://strftime.org/",
    );
    chrono::Utc::now()
        .format("%Y-%m-%d %H:%M:%S%.6f+00:00")
        .to_string()
}

fn make_secret() -> String {
    warn(
        WarningKind::Future,
        "'make_secret' will be removed in a future release of Copier.\nPlease use this instead: {{ 999999999999999999999999999999999|ans_random|hash('sha512') }}\nrandom and hash filters documentation: https://docs.ansible.com/ansible/2.3/playbooks_filters.html",
    );
    let mut entropy = [0u8; 48];
    fastrand::fill(&mut entropy);
    Sha512::digest(entropy)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn env(envops: serde_json::Value) -> JinjaEnv {
        let Value::Map(envops) = Value::from(envops) else {
            panic!("envops must be a mapping")
        };
        JinjaEnv::new(Path::new("."), &envops, &[]).unwrap()
    }

    fn context_of(variables: serde_json::Value) -> Context {
        let mut context = Context::new();
        if let Value::Map(map) = Value::from(variables) {
            context.extend_from(&map);
        }
        context
    }

    fn render(source: &str, context: serde_json::Value) -> String {
        env(json!({}))
            .render_str(source, &context_of(context))
            .unwrap()
    }

    #[test]
    fn python_style_output() {
        let context =
            json!({"flag": true, "none": null, "items": ["a", 1, 2.0], "map": {"k": "v"}});
        assert_eq!(
            render("{{ flag }} {{ none }} {{ items }} {{ map }}", context),
            "True None ['a', 1, 2.0] {'k': 'v'}"
        );
        assert_eq!(
            render(
                "{{ missing }}|{{ 'x'.upper() }}|{{ ', '.join(['a', 'b']) }}\n",
                json!({})
            ),
            "|X|a, b\n"
        );
    }

    #[test]
    fn yaml_and_json_filters() {
        let context = json!({"data": {"b": [1, "yes"], "a": "é"}});
        assert_eq!(
            render("{{ data|to_nice_yaml }}", context.clone()),
            "a: é\nb:\n- 1\n- 'yes'\n"
        );
        assert_eq!(
            render("{{ data|to_yaml }}", context.clone()),
            "a: é\nb: [1, 'yes']\n"
        );
        assert_eq!(
            render("{{ data|to_json }}", context.clone()),
            r#"{"b": [1, "yes"], "a": "\u00e9"}"#
        );
        assert_eq!(
            render("{{ data|tojson }}", context.clone()),
            r#"{"a": "\u00e9", "b": [1, "yes"]}"#
        );
        assert_eq!(
            render("{{ data|to_nice_json }}", context),
            "{\n    \"a\": \"\\u00e9\",\n    \"b\": [\n        1,\n        \"yes\"\n    ]\n}"
        );
    }

    #[test]
    fn ansible_filters() {
        assert_eq!(render("{{ '/absolute/path'|basename }}", json!({})), "path");
        assert_eq!(
            render(
                "{{ 'a-b'|regex_replace('(a)-(b)', '\\\\2-\\\\1') }}",
                json!({})
            ),
            "b-a"
        );
        assert_eq!(
            render("{{ 'hello world'|regex_search('w\\\\w+') }}", json!({})),
            "world"
        );
        assert_eq!(
            render(
                "{{ true|ternary('y', 'n') }}{{ 'x'|hash('md5')|length }}",
                json!({})
            ),
            "y32"
        );
        assert_eq!(
            render("{{ 'aGk='|b64decode }}{{ 'hi'|b64encode }}", json!({})),
            "hiaGk="
        );
        assert_eq!(render("{{ 'it is'|quote }}", json!({})), "'it is'");
    }

    #[test]
    fn custom_delimiters_and_strict_undefined() {
        let brackets = env(
            json!({"variable_start_string": "[[", "variable_end_string": "]]", "block_start_string": "[%", "block_end_string": "%]", "comment_start_string": "[#", "comment_end_string": "#]"}),
        );
        let context = context_of(json!({"name": "x"}));
        assert_eq!(
            brackets
                .render_str(
                    "[% if name %][[ name ]][% endif %][# c #]{{ name }}",
                    &context
                )
                .unwrap(),
            "x{{ name }}"
        );
        let strict = env(json!({"undefined": "jinja2.StrictUndefined"}));
        let err = strict
            .render_str("{{ undefined_variable }}", &context)
            .unwrap_err();
        assert!(matches!(
            err,
            CopierError::Template {
                kind: TemplateErrorKind::Undefined,
                ..
            }
        ));
        assert_eq!(err.to_string(), "'undefined_variable' is undefined");
    }

    #[test]
    fn unsupported_configuration() {
        let Value::Map(envops) = Value::from(json!({"undefined": "jinja2.ChainableUndefined"}))
        else {
            unreachable!()
        };
        let err = JinjaEnv::new(Path::new("."), &envops, &[]).err().unwrap();
        assert!(err.to_string().starts_with(
            "Unsupported envops.undefined value specified: jinja2.ChainableUndefined."
        ));
        let err = JinjaEnv::new(
            Path::new("."),
            &Mapping::new(),
            &["non.existent.Extension".to_owned()],
        )
        .err()
        .unwrap();
        assert!(err.to_string().contains("No module named 'non'"));
    }

    #[test]
    fn yield_tags() {
        let env = env(json!({}));
        let tag = env
            .yield_tag("{% yield item from items %}{{ item }}.txt{% endyield %}")
            .unwrap()
            .unwrap();
        assert_eq!(
            tag,
            YieldTag {
                name: "item".into(),
                iterable: "items".into(),
                body: "{{ item }}.txt".into()
            }
        );
        assert_eq!(env.yield_tag("plain.txt").unwrap(), None);
        let twice = "{% yield a from x %}{% endyield %}{% yield b from y %}{% endyield %}";
        assert!(matches!(
            env.yield_tag(twice),
            Err(CopierError::MultipleYieldTags(_))
        ));
        let items = env
            .eval("items + ['c']", &context_of(json!({"items": ["a", "b"]})))
            .unwrap();
        assert_eq!(from_jinja(&items), json!(["a", "b", "c"]).into());
    }

    #[test]
    fn path_joining() {
        let join = |parts: &[&str]| {
            pure_path_join(
                &parts
                    .iter()
                    .map(|part| (*part).to_owned())
                    .collect::<Vec<_>>(),
                false,
            )
        };
        assert_eq!(join(&["a", "b/", "./c"]), "a/b/c");
        assert_eq!(join(&["a", "/b", "c"]), "/b/c");
        assert_eq!(join(&[]), ".");
        assert_eq!(
            pure_path_join(&["a".to_owned(), "b/c".to_owned()], true),
            "a\\b\\c"
        );
    }
}
