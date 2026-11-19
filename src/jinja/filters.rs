//! Filters available to templates: the core set of `jinja2-ansible-filters`, which
//! Python Copier always loads, plus Jinja's `tojson` with Python's output format.

use std::path::Path;

use base64::Engine as _;
use md5::Md5;
use minijinja::value::{Kwargs, Rest, Value as JinjaValue};
use minijinja::{Environment, Error, ErrorKind};
use regex::RegexBuilder;
use sha1::Sha1;
use sha2::{Digest, Sha224, Sha256, Sha384, Sha512};

use super::convert::{from_jinja, to_jinja};
use crate::json;
use crate::value::Value;
use crate::yaml;

/// Namespace `jinja2-ansible-filters` hashes names in for `to_uuid`.
const UUID_NAMESPACE: uuid::Uuid = uuid::uuid!("361e6d51-faec-444a-9079-341386da8e2e");

fn invalid(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::InvalidOperation, message.into())
}

/// Text of a value the way Python's `str()` renders it.
fn text(value: &JinjaValue) -> String {
    match value.as_str() {
        Some(text) => text.to_owned(),
        None => value.to_string(),
    }
}

fn yaml_options(kwargs: &Kwargs, indent: usize, flow: yaml::FlowStyle) -> Result<yaml::DumpOptions, Error> {
    let flow = match kwargs.get::<Option<JinjaValue>>("default_flow_style")? {
        None => flow,
        Some(value) if value.is_none() => yaml::FlowStyle::Auto,
        Some(value) if value.is_true() => yaml::FlowStyle::Flow,
        Some(_) => yaml::FlowStyle::Block,
    };
    let options = yaml::DumpOptions {
        indent: kwargs.get::<Option<usize>>("indent")?.unwrap_or(indent),
        flow,
        sort_keys: kwargs.get::<Option<bool>>("sort_keys")?.unwrap_or(true),
        allow_unicode: kwargs.get::<Option<bool>>("allow_unicode")?.unwrap_or(true),
    };
    // Layout knobs without an equivalent here; accepted so templates keep working.
    for ignored in ["width", "explicit_start", "explicit_end", "default_style", "canonical", "line_break", "encoding"] {
        let _ = kwargs.get::<Option<JinjaValue>>(ignored)?;
    }
    kwargs.assert_all_used()?;
    Ok(options)
}

fn to_yaml(value: JinjaValue, kwargs: Kwargs) -> Result<String, Error> {
    Ok(yaml::dump(&from_jinja(&value), &yaml_options(&kwargs, 2, yaml::FlowStyle::Auto)?))
}

fn to_nice_yaml(value: JinjaValue, indent: Option<usize>, kwargs: Kwargs) -> Result<String, Error> {
    Ok(yaml::dump(&from_jinja(&value), &yaml_options(&kwargs, indent.unwrap_or(4), yaml::FlowStyle::Block)?))
}

fn from_yaml(value: JinjaValue) -> Result<JinjaValue, Error> {
    match value.as_str() {
        Some(text) => yaml::load(text).map(|loaded| to_jinja(&loaded)).map_err(|err| invalid(err.to_string())),
        None => Ok(value),
    }
}

fn from_yaml_all(value: JinjaValue) -> Result<JinjaValue, Error> {
    match value.as_str() {
        Some(text) => yaml::load_all(text)
            .map(|documents| JinjaValue::from(documents.iter().map(to_jinja).collect::<Vec<_>>()))
            .map_err(|err| invalid(err.to_string())),
        None => Ok(value),
    }
}

/// Owned separators so they can outlive the keyword arguments they came from.
struct JsonStyle {
    indent: Option<usize>,
    sort_keys: bool,
    ensure_ascii: bool,
    item_separator: String,
    key_separator: String,
}

impl JsonStyle {
    fn from_kwargs(kwargs: &Kwargs, indent: Option<usize>, sort_keys: bool, nice: bool) -> Result<Self, Error> {
        let indent = match kwargs.get::<Option<JinjaValue>>("indent")? {
            Some(value) if value.is_none() => None,
            Some(value) => Some(value.as_usize().ok_or_else(|| invalid("indent must be a non-negative integer"))?),
            None => indent,
        };
        let (mut item_separator, mut key_separator) = match (indent, nice) {
            (_, true) | (Some(_), false) => (",".to_owned(), ": ".to_owned()),
            (None, false) => (", ".to_owned(), ": ".to_owned()),
        };
        if let Some(separators) = kwargs.get::<Option<Vec<String>>>("separators")? {
            if let [item, key] = separators.as_slice() {
                item_separator.clone_from(item);
                key_separator.clone_from(key);
            }
        }
        let style = Self {
            indent,
            sort_keys: kwargs.get::<Option<bool>>("sort_keys")?.unwrap_or(sort_keys),
            ensure_ascii: kwargs.get::<Option<bool>>("ensure_ascii")?.unwrap_or(true),
            item_separator,
            key_separator,
        };
        kwargs.assert_all_used()?;
        Ok(style)
    }

    fn dumps(&self, value: &Value) -> String {
        json::dumps(
            value,
            &json::DumpOptions {
                indent: self.indent,
                sort_keys: self.sort_keys,
                ensure_ascii: self.ensure_ascii,
                item_separator: &self.item_separator,
                key_separator: &self.key_separator,
            },
        )
    }
}

fn to_json(value: JinjaValue, kwargs: Kwargs) -> Result<String, Error> {
    Ok(JsonStyle::from_kwargs(&kwargs, None, false, false)?.dumps(&from_jinja(&value)))
}

fn to_nice_json(value: JinjaValue, indent: Option<usize>, kwargs: Kwargs) -> Result<String, Error> {
    Ok(JsonStyle::from_kwargs(&kwargs, Some(indent.unwrap_or(4)), true, true)?.dumps(&from_jinja(&value)))
}

fn from_json(value: JinjaValue) -> Result<JinjaValue, Error> {
    json::loads(&text(&value)).map(|loaded| to_jinja(&loaded)).map_err(invalid)
}

/// Jinja's own `tojson`: sorted keys and HTML-safe escapes, marked safe.
fn tojson(value: JinjaValue, indent: Option<usize>) -> JinjaValue {
    let style = JsonStyle {
        indent,
        sort_keys: true,
        ensure_ascii: true,
        item_separator: if indent.is_some() { "," } else { ", " }.to_owned(),
        key_separator: ": ".to_owned(),
    };
    let dumped = style
        .dumps(&from_jinja(&value))
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
        .replace('\'', "\\u0027");
    JinjaValue::from_safe_string(dumped)
}

fn to_bool(value: JinjaValue) -> JinjaValue {
    if value.is_none() || value.is_undefined() || value.kind() == minijinja::value::ValueKind::Bool {
        return value;
    }
    let truthy = match value.as_str() {
        Some(text) => matches!(text.to_lowercase().as_str(), "yes" | "on" | "1" | "true"),
        None => value.as_i64() == Some(1),
    };
    JinjaValue::from(truthy)
}

fn posix_basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn posix_dirname(path: &str) -> String {
    match path.rfind('/') {
        None => String::new(),
        Some(index) => {
            let head = &path[..=index];
            let trimmed = head.trim_end_matches('/');
            if trimmed.is_empty() { head } else { trimmed }.to_owned()
        }
    }
}

fn windows_split(path: &str) -> (String, &str) {
    let (drive, rest) = windows_splitdrive(path);
    match rest.rfind(['/', '\\']) {
        None => (drive.to_owned(), rest),
        Some(index) => {
            let head = &rest[..=index];
            let trimmed = head.trim_end_matches(['/', '\\']);
            (format!("{drive}{}", if trimmed.is_empty() { head } else { trimmed }), &rest[index + 1..])
        }
    }
}

fn windows_splitdrive(path: &str) -> (&str, &str) {
    let bytes = path.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        path.split_at(2)
    } else {
        ("", path)
    }
}

fn splitext(path: &str) -> (String, String) {
    let name_start = path.rfind('/').map_or(0, |index| index + 1);
    let name = &path[name_start..];
    match name.rfind('.') {
        // Leading dots belong to the name: `.bashrc` has no extension.
        Some(dot) if !name[..dot].chars().all(|ch| ch == '.') => {
            (path[..name_start + dot].to_owned(), name[dot..].to_owned())
        }
        _ => (path.to_owned(), String::new()),
    }
}

fn expandvars(path: &str) -> String {
    static VARIABLE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        // A literal pattern, so it always compiles.
        #[allow(clippy::expect_used)]
        regex::Regex::new(r"\$(\w+|\{[^}]*\})").expect("valid variable pattern")
    });
    VARIABLE
        .replace_all(path, |captures: &regex::Captures<'_>| {
            let name = captures[1].trim_start_matches('{').trim_end_matches('}');
            std::env::var(name).unwrap_or_else(|_| captures[0].to_owned())
        })
        .into_owned()
}

fn realpath(path: &str) -> String {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.into());
    absolute.canonicalize().unwrap_or(absolute).to_string_lossy().into_owned()
}

fn relpath(path: &str, start: Option<&str>) -> String {
    let normalize = |path: &str| -> Vec<String> {
        let absolute = std::path::absolute(path).unwrap_or_else(|_| path.into());
        crate::settings::normpath(&absolute.to_string_lossy())
            .split('/')
            .filter(|part| !part.is_empty())
            .map(str::to_owned)
            .collect()
    };
    let (target, base) = (normalize(path), normalize(start.unwrap_or(".")));
    let common = target.iter().zip(&base).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<&str> = vec![".."; base.len() - common];
    parts.extend(target[common..].iter().map(String::as_str));
    if parts.is_empty() {
        ".".to_owned()
    } else {
        parts.join("/")
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn hash(data: &str, hashtype: &str) -> Option<String> {
    let bytes = data.as_bytes();
    Some(match hashtype {
        "md5" => hex(&Md5::digest(bytes)),
        "sha1" => hex(&Sha1::digest(bytes)),
        "sha224" => hex(&Sha224::digest(bytes)),
        "sha256" => hex(&Sha256::digest(bytes)),
        "sha384" => hex(&Sha384::digest(bytes)),
        "sha512" => hex(&Sha512::digest(bytes)),
        _ => return None,
    })
}

fn build_regex(pattern: &str, ignorecase: bool, multiline: bool) -> Result<regex::Regex, Error> {
    RegexBuilder::new(pattern)
        .case_insensitive(ignorecase)
        .multi_line(multiline)
        .build()
        .map_err(|err| invalid(format!("invalid regular expression: {err}")))
}

/// Translate a Python `re.sub` replacement (`\1`, `\g<name>`) to this engine's syntax.
fn python_replacement(replacement: &str) -> String {
    let mut out = String::with_capacity(replacement.len());
    let mut chars = replacement.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '$' => out.push_str("$$"),
            '\\' => match chars.peek().copied() {
                Some(digit) if digit.is_ascii_digit() => {
                    let mut group = String::new();
                    while let Some(digit) = chars.peek().copied().filter(char::is_ascii_digit) {
                        group.push(digit);
                        chars.next();
                    }
                    out.push_str(&format!("${{{group}}}"));
                }
                Some('g') => {
                    chars.next();
                    let name: String = chars.by_ref().skip_while(|ch| *ch == '<').take_while(|ch| *ch != '>').collect();
                    out.push_str(&format!("${{{name}}}"));
                }
                Some('n') => {
                    chars.next();
                    out.push('\n');
                }
                Some('t') => {
                    chars.next();
                    out.push('\t');
                }
                Some('\\') => {
                    chars.next();
                    out.push('\\');
                }
                _ => out.push('\\'),
            },
            other => out.push(other),
        }
    }
    out
}

fn regex_replace(
    value: JinjaValue,
    pattern: String,
    replacement: Option<String>,
    kwargs: Kwargs,
) -> Result<String, Error> {
    let ignorecase = kwargs.get::<Option<bool>>("ignorecase")?.unwrap_or(false);
    let multiline = kwargs.get::<Option<bool>>("multiline")?.unwrap_or(false);
    kwargs.assert_all_used()?;
    let regex = build_regex(&pattern, ignorecase, multiline)?;
    Ok(regex.replace_all(&text(&value), python_replacement(&replacement.unwrap_or_default()).as_str()).into_owned())
}

fn regex_search(value: JinjaValue, pattern: String, groups: Rest<String>, kwargs: Kwargs) -> Result<JinjaValue, Error> {
    let ignorecase = kwargs.get::<Option<bool>>("ignorecase")?.unwrap_or(false);
    let multiline = kwargs.get::<Option<bool>>("multiline")?.unwrap_or(false);
    kwargs.assert_all_used()?;
    let regex = build_regex(&pattern, ignorecase, multiline)?;
    let haystack = text(&value);
    let Some(captures) = regex.captures(&haystack) else {
        return Ok(JinjaValue::from(()));
    };
    if groups.is_empty() {
        return Ok(JinjaValue::from(&captures[0]));
    }
    let mut items = Vec::new();
    for group in groups.iter() {
        let reference = group.strip_prefix('\\').ok_or_else(|| invalid("Unknown argument"))?;
        let found = match reference.strip_prefix("g<").and_then(|name| name.strip_suffix('>')) {
            Some(name) => captures.name(name),
            None => reference.parse::<usize>().ok().and_then(|index| captures.get(index)),
        };
        items.push(found.map_or(JinjaValue::from(()), |found| JinjaValue::from(found.as_str())));
    }
    Ok(JinjaValue::from(items))
}

fn regex_findall(value: JinjaValue, pattern: String, kwargs: Kwargs) -> Result<JinjaValue, Error> {
    let ignorecase = kwargs.get::<Option<bool>>("ignorecase")?.unwrap_or(false);
    let multiline = kwargs.get::<Option<bool>>("multiline")?.unwrap_or(false);
    kwargs.assert_all_used()?;
    let regex = build_regex(&pattern, ignorecase, multiline)?;
    let haystack = text(&value);
    let found: Vec<JinjaValue> = regex
        .captures_iter(&haystack)
        .map(|captures| match captures.len() {
            // Python returns the whole match, the single group, or a tuple of groups.
            1 => JinjaValue::from(&captures[0]),
            2 => JinjaValue::from(captures.get(1).map_or("", |group| group.as_str())),
            _ => JinjaValue::from(
                captures
                    .iter()
                    .skip(1)
                    .map(|group| group.map_or("", |group| group.as_str()).to_owned())
                    .collect::<Vec<_>>(),
            ),
        })
        .collect();
    Ok(JinjaValue::from(found))
}

fn ternary(
    value: JinjaValue,
    true_value: JinjaValue,
    false_value: JinjaValue,
    none_value: Option<JinjaValue>,
) -> JinjaValue {
    match none_value {
        Some(none_value) if value.is_none() => none_value,
        _ if value.is_true() => true_value,
        _ => false_value,
    }
}

fn seeded(seed: Option<JinjaValue>) -> fastrand::Rng {
    match seed.filter(|seed| !seed.is_none()) {
        None => fastrand::Rng::new(),
        Some(seed) => {
            let digest = Sha256::digest(seed.to_string().as_bytes());
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&digest[..8]);
            fastrand::Rng::with_seed(u64::from_le_bytes(bytes))
        }
    }
}

fn ans_random(end: JinjaValue, kwargs: Kwargs) -> Result<JinjaValue, Error> {
    let start = kwargs.get::<Option<i64>>("start")?;
    let step = kwargs.get::<Option<i64>>("step")?;
    let mut rng = seeded(kwargs.get::<Option<JinjaValue>>("seed")?);
    kwargs.assert_all_used()?;
    if let (true, Some(end)) = (end.is_integer(), end.as_i64()) {
        let start = start.unwrap_or(0);
        let step = step.filter(|step| *step != 0).unwrap_or(1);
        let count = (end - start + step - step.signum()) / step;
        if count <= 0 {
            return Err(invalid("empty range for randrange()"));
        }
        return Ok(JinjaValue::from(start + step * rng.i64(0..count)));
    }
    // A number too large for the engine's integers still yields a random number.
    if end.is_number() {
        return Ok(JinjaValue::from(rng.u64(..)));
    }
    if start.is_some() || step.is_some() {
        return Err(invalid("start and step can only be used with integer values"));
    }
    let items: Vec<JinjaValue> =
        end.try_iter().map_err(|_| invalid("random can only be used on sequences and integers"))?.collect();
    if items.is_empty() {
        return Err(invalid("Cannot choose from an empty sequence"));
    }
    Ok(items[rng.usize(..items.len())].clone())
}

fn shuffle(value: JinjaValue, seed: Option<JinjaValue>) -> JinjaValue {
    match value.try_iter() {
        Ok(items) => {
            let mut items: Vec<JinjaValue> = items.collect();
            seeded(seed).shuffle(&mut items);
            JinjaValue::from(items)
        }
        Err(_) => value,
    }
}

fn mandatory(value: JinjaValue, message: Option<String>) -> Result<JinjaValue, Error> {
    if value.is_undefined() {
        return Err(Error::new(
            ErrorKind::UndefinedError,
            message.unwrap_or_else(|| "Mandatory variable not defined.".to_owned()),
        ));
    }
    Ok(value)
}

fn type_debug(value: JinjaValue) -> &'static str {
    use minijinja::value::ValueKind;
    match value.kind() {
        ValueKind::Undefined => "Undefined",
        ValueKind::None => "NoneType",
        ValueKind::Bool => "bool",
        ValueKind::Number if value.is_integer() => "int",
        ValueKind::Number => "float",
        ValueKind::String => "str",
        ValueKind::Bytes => "bytes",
        ValueKind::Map => "dict",
        _ => "list",
    }
}

fn merge(base: &mut Value, other: &Value, recursive: bool) {
    let (Value::Map(base), Value::Map(other)) = (base, other) else {
        return;
    };
    for (key, value) in other {
        match (recursive, base.get_mut(key)) {
            (true, Some(existing)) if matches!((&*existing, value), (Value::Map(_), Value::Map(_))) => {
                merge(existing, value, true);
            }
            _ => {
                base.insert(key.clone(), value.clone());
            }
        }
    }
}

fn combine(value: JinjaValue, others: Rest<JinjaValue>, kwargs: Kwargs) -> Result<JinjaValue, Error> {
    let recursive = kwargs.get::<Option<bool>>("recursive")?.unwrap_or(false);
    kwargs.assert_all_used()?;
    let mut result = from_jinja(&value);
    for other in others.iter() {
        merge(&mut result, &from_jinja(other), recursive);
    }
    Ok(to_jinja(&result))
}

fn flatten_into(value: &Value, levels: Option<usize>, out: &mut Vec<Value>) {
    match value {
        Value::Seq(items) if levels != Some(0) => {
            for item in items {
                match item {
                    Value::Seq(_) => flatten_into(item, levels.map(|levels| levels - 1), out),
                    // Ansible drops nulls while flattening.
                    Value::Null => {}
                    other => out.push(other.clone()),
                }
            }
        }
        other => out.push(other.clone()),
    }
}

fn flatten(value: JinjaValue, kwargs: Kwargs) -> Result<JinjaValue, Error> {
    let levels = kwargs.get::<Option<usize>>("levels")?;
    kwargs.assert_all_used()?;
    let mut out = Vec::new();
    // The outer list itself does not count as a level.
    flatten_into(&from_jinja(&value), levels.map(|levels| levels + 1), &mut out);
    Ok(to_jinja(&Value::Seq(out)))
}

fn quote(value: JinjaValue) -> String {
    let text = text(&value);
    if text.is_empty() {
        return "''".to_owned();
    }
    if text.chars().all(|ch| ch.is_ascii_alphanumeric() || "@%+=:,./-_".contains(ch)) {
        return text;
    }
    format!("'{}'", text.replace('\'', "'\"'\"'"))
}

fn strftime(format: String, second: Option<JinjaValue>) -> Result<String, Error> {
    use std::fmt::Write as _;
    let moment = match second.filter(|second| !second.is_none()) {
        None => chrono::Local::now(),
        Some(second) => {
            let epoch = match from_jinja(&second) {
                Value::Int(number) => Some(number),
                Value::Float(number) => Some(number as i64),
                Value::Str(text) => crate::tools::py_int(&text),
                _ => None,
            }
            .ok_or_else(|| invalid(format!("Invalid value for epoch value ({second})")))?;
            chrono::DateTime::from_timestamp(epoch, 0)
                .ok_or_else(|| invalid(format!("Invalid value for epoch value ({second})")))?
                .with_timezone(&chrono::Local)
        }
    };
    let mut out = String::new();
    write!(out, "{}", moment.format(&format)).map_err(|_| invalid(format!("Invalid format string: {format}")))?;
    Ok(out)
}

fn comment(value: JinjaValue, kwargs: Kwargs) -> Result<String, Error> {
    let style = kwargs.get::<Option<String>>("style")?.unwrap_or_else(|| "plain".to_owned());
    let (beginning, decoration, end) = match style.as_str() {
        "plain" => ("", "# ", ""),
        "erlang" => ("", "% ", ""),
        "c" => ("", "// ", ""),
        "cblock" => ("/*", " * ", " */"),
        "xml" => ("<!--", " - ", "-->"),
        other => return Err(invalid(format!("Style {other} is not supported"))),
    };
    let decoration = kwargs.get::<Option<String>>("decoration")?.unwrap_or_else(|| decoration.to_owned());
    kwargs.assert_all_used()?;
    let mut lines: Vec<String> = Vec::new();
    if !beginning.is_empty() {
        lines.push(beginning.to_owned());
    }
    // A decoration line frames the text, as in ansible's default prefix/postfix.
    let frame = decoration.trim_end().to_owned();
    lines.push(frame.clone());
    lines.extend(text(&value).lines().map(|line| format!("{decoration}{line}").trim_end().to_owned()));
    lines.push(frame);
    if !end.is_empty() {
        lines.push(end.to_owned());
    }
    Ok(lines.join("\n"))
}

fn fileglob(pattern: String) -> Vec<String> {
    let path = Path::new(&pattern);
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
        return Vec::new();
    };
    let dir = if dir.as_os_str().is_empty() { Path::new(".") } else { dir };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<String> = entries
        .flatten()
        .filter(|entry| {
            entry.path().is_file()
                && crate::tools::fnmatch(&entry.file_name().to_string_lossy(), &name.to_string_lossy())
        })
        .map(|entry| entry.path().to_string_lossy().into_owned())
        .collect();
    found.sort();
    found
}

/// Register every filter on `env`.
pub(super) fn register(env: &mut Environment<'_>) {
    env.add_filter("to_yaml", to_yaml);
    env.add_filter("to_nice_yaml", to_nice_yaml);
    env.add_filter("from_yaml", from_yaml);
    env.add_filter("from_yaml_all", from_yaml_all);
    env.add_filter("to_json", to_json);
    env.add_filter("to_nice_json", to_nice_json);
    env.add_filter("from_json", from_json);
    env.add_filter("tojson", tojson);
    env.add_filter("bool", to_bool);
    env.add_filter("basename", |path: JinjaValue| posix_basename(&text(&path)).to_owned());
    env.add_filter("dirname", |path: JinjaValue| posix_dirname(&text(&path)));
    env.add_filter("expanduser", |path: JinjaValue| crate::dirs::expand_user(&text(&path)));
    env.add_filter("expandvars", |path: JinjaValue| expandvars(&text(&path)));
    env.add_filter("realpath", |path: JinjaValue| realpath(&text(&path)));
    env.add_filter("relpath", |path: JinjaValue, start: Option<String>| relpath(&text(&path), start.as_deref()));
    env.add_filter("splitext", |path: JinjaValue| {
        let (root, extension) = splitext(&text(&path));
        vec![root, extension]
    });
    env.add_filter("win_basename", |path: JinjaValue| windows_split(&text(&path)).1.to_owned());
    env.add_filter("win_dirname", |path: JinjaValue| windows_split(&text(&path)).0);
    env.add_filter("win_splitdrive", |path: JinjaValue| {
        let path = text(&path);
        let (drive, rest) = windows_splitdrive(&path);
        vec![drive.to_owned(), rest.to_owned()]
    });
    env.add_filter("b64encode", |value: JinjaValue| base64::engine::general_purpose::STANDARD.encode(text(&value)));
    env.add_filter("b64decode", |value: JinjaValue| -> Result<String, Error> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(text(&value))
            .map_err(|err| invalid(format!("Invalid base64 data: {err}")))?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    });
    env.add_filter("to_uuid", |value: JinjaValue| {
        uuid::Uuid::new_v5(&UUID_NAMESPACE, text(&value).as_bytes()).to_string()
    });
    env.add_filter("md5", |value: JinjaValue| hex(&Md5::digest(text(&value).as_bytes())));
    env.add_filter("sha1", |value: JinjaValue| hex(&Sha1::digest(text(&value).as_bytes())));
    env.add_filter("checksum", |value: JinjaValue| hex(&Sha1::digest(text(&value).as_bytes())));
    env.add_filter("hash", |value: JinjaValue, hashtype: Option<String>| {
        hash(&text(&value), hashtype.as_deref().unwrap_or("sha1")).map_or(JinjaValue::from(()), JinjaValue::from)
    });
    env.add_filter("regex_replace", regex_replace);
    env.add_filter("regex_escape", |value: JinjaValue| regex::escape(&text(&value)));
    env.add_filter("regex_search", regex_search);
    env.add_filter("regex_findall", regex_findall);
    env.add_filter("ternary", ternary);
    env.add_filter("ans_random", ans_random);
    env.add_filter("shuffle", shuffle);
    env.add_filter("mandatory", mandatory);
    env.add_filter("type_debug", type_debug);
    env.add_filter("combine", combine);
    env.add_filter("flatten", flatten);
    env.add_filter("quote", quote);
    env.add_filter("strftime", strftime);
    env.add_filter("comment", comment);
    env.add_filter("fileglob", fileglob);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_helpers_match_python() {
        assert_eq!(posix_basename("/absolute/path"), "path");
        assert_eq!(posix_dirname("/absolute/path"), "/absolute");
        assert_eq!(posix_dirname("/file"), "/");
        assert_eq!(posix_dirname("file"), "");
        assert_eq!(splitext("a/b.tar.gz"), ("a/b.tar".to_owned(), ".gz".to_owned()));
        assert_eq!(splitext(".bashrc"), (".bashrc".to_owned(), String::new()));
        assert_eq!(windows_split(r"C:\dir\file.txt"), (r"C:\dir".to_owned(), "file.txt"));
    }

    #[test]
    fn replacement_translation() {
        assert_eq!(python_replacement(r"\1-\g<name>$"), "${1}-${name}$$");
    }

    #[test]
    fn hashing() {
        assert_eq!(hash("a", "md5").as_deref(), Some("0cc175b9c0f1b6a831c399e269772661"));
        assert_eq!(hash("a", "nope"), None);
        assert_eq!(hash("a", "sha512").map(|digest| digest.len()), Some(128));
    }
}
