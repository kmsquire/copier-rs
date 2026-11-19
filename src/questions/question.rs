//! One question of a template's questionnaire.

use minijinja::value::Value as JinjaValue;

use super::cast::{cast, py_eq, TYPE_NAMES};
use crate::errors::{CopierError, Result, TemplateErrorKind};
use crate::jinja::{to_jinja, Context, JinjaEnv};
use crate::settings::Settings;
use crate::tools::{cast_to_bool, force_str_end};
use crate::value::{Mapping, Value};
use crate::{json, yaml};

/// Names templates cannot use for questions because they are built-in variables.
const RESERVED_NAMES: &[&str] = &["now", "make_secret"];

/// Value `UNSET` renders to; a default that renders it means "no default".
const UNSET_MARKER: &str = "\u{1}copier-unset\u{1}";

/// One selectable option of a question.
#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    /// Label shown to the user.
    pub name: String,
    /// Raw (uncast) value.
    pub value: Value,
    /// Why the choice cannot be selected; empty when it can.
    pub disabled: String,
}

/// The default shown in a prompt.
#[derive(Debug, Clone, PartialEq)]
pub enum DefaultRendered {
    /// The question has no default.
    Missing,
    /// A yes/no default.
    Bool(bool),
    /// Text pre-filled in the input.
    Text(String),
    /// The pre-selected choice of a single-choice question.
    Choice(Choice),
    /// Nothing pre-filled (multiple choice, or no matching choice).
    None,
}

/// Which kind of prompt a question needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    /// Yes/no.
    Confirm,
    /// Pick one choice.
    Select,
    /// Pick any number of choices.
    Checkbox,
    /// Hidden text.
    Password,
    /// A filesystem path with completion.
    Path,
    /// Free text.
    Input,
}

/// What is already known about a question's answer, by source.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct KnownAnswers {
    /// Passed in up front (`--data` or the API).
    pub init: Option<Value>,
    /// Recorded by the previous run.
    pub last: Option<Value>,
    /// Default supplied by the API caller.
    pub user_default: Option<Value>,
}

/// A question, bound to the answers and render context of the moment it is asked.
pub struct Question<'a> {
    /// Name of the answer variable.
    pub var_name: String,
    /// Options to pick from: a list, a mapping, or a template rendering to YAML.
    pub choices: Value,
    /// Whether several choices may be picked.
    pub multiselect: bool,
    /// Default answer; `None` when the question has none.
    pub default: Option<Value>,
    /// Text shown instead of the variable name.
    pub help: String,
    /// Whether the input spans several lines (boolean or template).
    pub multiline: Value,
    /// Hint shown in an empty input.
    pub placeholder: String,
    /// Custom prompt marker.
    pub qmark: Option<String>,
    /// Whether the answer is kept out of the answers file and hidden while typing.
    pub secret: bool,
    /// Declared type (may be a template).
    pub type_name: String,
    /// Template rendering to an error message, or to nothing when the answer is valid.
    pub validator: String,
    /// Whether the question applies (boolean or template).
    pub when: Value,
    /// Whether the question is asked at all (boolean or template).
    pub ask: Value,
    known: KnownAnswers,
    context: Context,
    env: &'a JinjaEnv,
    settings: &'a Settings,
}

fn text_field(details: &Mapping, key: &str) -> Result<String> {
    match details.get(&Value::from(key)) {
        None => Ok(String::new()),
        Some(Value::Str(text)) => Ok(text.clone()),
        Some(_) => Err(CopierError::Validation(format!(
            "{key}\n  Input should be a valid string"
        ))),
    }
}

fn bool_field(details: &Mapping, key: &str) -> Result<bool> {
    match details.get(&Value::from(key)) {
        None => Ok(false),
        Some(Value::Bool(flag)) => Ok(*flag),
        Some(Value::Int(number @ (0 | 1))) => Ok(*number == 1),
        Some(Value::Str(text)) => match text.to_lowercase().as_str() {
            "1" | "true" | "yes" | "on" | "y" | "t" => Ok(true),
            "0" | "false" | "no" | "off" | "n" | "f" => Ok(false),
            _ => Err(CopierError::Validation(format!(
                "{key}\n  Input should be a valid boolean"
            ))),
        },
        Some(_) => Err(CopierError::Validation(format!(
            "{key}\n  Input should be a valid boolean"
        ))),
    }
}

impl<'a> Question<'a> {
    /// Build the question `var_name` from its `copier.yml` definition.
    ///
    /// # Errors
    ///
    /// Returns [`CopierError::Value`] for reserved names and secret questions
    /// without a default, and [`CopierError::Validation`] for malformed definitions.
    pub fn new(
        var_name: &str,
        details: &Mapping,
        known: KnownAnswers,
        context: Context,
        env: &'a JinjaEnv,
        settings: &'a Settings,
    ) -> Result<Self> {
        const FIELDS: &[&str] = &[
            "choices",
            "multiselect",
            "default",
            "help",
            "multiline",
            "placeholder",
            "qmark",
            "secret",
            "type",
            "validator",
            "when",
            "ask",
        ];
        if RESERVED_NAMES.contains(&var_name) {
            return Err(CopierError::Value("Invalid question name".to_owned()));
        }
        if let Some(unknown) = details
            .keys()
            .map(Value::py_str)
            .find(|key| !FIELDS.contains(&key.as_str()))
        {
            return Err(CopierError::Validation(format!(
                "{unknown}\n  Unexpected keyword argument"
            )));
        }
        let get = |key: &str| details.get(&Value::from(key)).cloned();
        let default = get("default");
        let mut type_name = text_field(details, "type")?;
        if type_name.is_empty() {
            // The type follows the default's Python type; anything else is YAML.
            type_name = match default.as_ref().map(Value::py_type_name) {
                Some(name @ ("bool" | "float" | "int" | "str")) => name.to_owned(),
                _ => "yaml".to_owned(),
            };
        }
        let secret = bool_field(details, "secret")?;
        if secret && default.is_none() {
            return Err(CopierError::Value(
                "Secret question requires a default value".to_owned(),
            ));
        }
        Ok(Self {
            var_name: var_name.to_owned(),
            choices: get("choices").unwrap_or_else(|| Value::Seq(Vec::new())),
            multiselect: bool_field(details, "multiselect")?,
            default,
            help: text_field(details, "help")?,
            multiline: get("multiline").unwrap_or(Value::Bool(false)),
            placeholder: text_field(details, "placeholder")?,
            qmark: get("qmark")
                .filter(|value| !value.is_null())
                .map(|value| value.py_str()),
            secret,
            type_name,
            validator: text_field(details, "validator")?,
            when: get("when").unwrap_or(Value::Bool(true)),
            ask: get("ask").unwrap_or(Value::Bool(true)),
            known,
            context,
            env,
            settings,
        })
    }

    /// Forget the answer recorded by the previous run.
    pub fn forget_last(&mut self) {
        self.known.last = None;
    }

    /// Render `value` if it is a string, or each string of a list; other values
    /// pass through.
    ///
    /// # Errors
    ///
    /// Undefined variables become [`CopierError::UserMessage`]; other template
    /// failures are reported as they are.
    pub fn render_value(&self, value: &Value) -> Result<Value> {
        self.render_with(value, &self.context)
    }

    fn render_with(&self, value: &Value, context: &Context) -> Result<Value> {
        match value {
            Value::Str(source) => match self.env.render_str(source, context) {
                Ok(rendered) => Ok(Value::Str(rendered)),
                Err(CopierError::Template {
                    kind: TemplateErrorKind::Undefined,
                    message,
                }) => Err(CopierError::UserMessage(message)),
                Err(other) => Err(other),
            },
            Value::Seq(items) => Ok(Value::Seq(
                items
                    .iter()
                    .map(|item| self.render_value(item))
                    .collect::<Result<_>>()?,
            )),
            other => Ok(other.clone()),
        }
    }

    fn render_text(&self, value: &Value) -> Result<String> {
        Ok(self.render_value(value)?.py_str())
    }

    /// The question's type after rendering.
    ///
    /// # Errors
    ///
    /// Returns [`CopierError::InvalidType`] for unknown types.
    pub fn get_type_name(&self) -> Result<String> {
        let type_name = self.render_text(&Value::Str(self.type_name.clone()))?;
        if !TYPE_NAMES.contains(&type_name.as_str()) {
            return Err(CopierError::InvalidType(format!(
                "Unsupported type \"{type_name}\" in question \"{}\"",
                self.var_name
            )));
        }
        Ok(type_name)
    }

    /// Cast `answer` to the question's type.
    ///
    /// # Errors
    ///
    /// Returns [`CopierError::InvalidType`] for values of the wrong kind and
    /// [`CopierError::Value`] for unparsable text.
    pub fn cast_answer(&self, answer: &Value) -> Result<Value> {
        let type_name = self.get_type_name()?;
        let structured = matches!(type_name.as_str(), "json" | "yaml");
        if answer.is_null() && !structured {
            return Err(CopierError::InvalidType(format!(
                "Invalid answer \"None\" of type \"<class 'NoneType'>\" to question \"{}\" of type \"{type_name}\"",
                self.var_name
            )));
        }
        let cast_one = |item: &Value| match cast(&type_name, item) {
            // Already-structured data needs no parsing.
            Err(CopierError::Type(_)) if structured => Ok(item.clone()),
            Err(CopierError::Type(message)) => Err(CopierError::InvalidType(message)),
            other => other,
        };
        match answer {
            Value::Seq(items) if self.multiselect => Ok(Value::Seq(
                items.iter().map(cast_one).collect::<Result<_>>()?,
            )),
            other => cast_one(other),
        }
    }

    /// The choices with names, values and disabled reasons rendered.
    ///
    /// # Errors
    ///
    /// Fails on malformed choices or values that cannot be cast to the question type.
    pub fn formatted_choices(&self) -> Result<Vec<Choice>> {
        let choices = match &self.choices {
            Value::Str(_) => {
                let rendered = self.render_text(&self.choices)?;
                yaml::load(&rendered).map_err(|err| CopierError::Value(err.to_string()))?
            }
            other => other.clone(),
        };
        let pairs: Vec<(Value, Value, bool)> = match choices {
            Value::Map(map) => map
                .into_iter()
                .map(|(name, value)| (name, value, true))
                .collect(),
            Value::Seq(items) => items
                .into_iter()
                .map(|item| match item {
                    Value::Seq(pair) => match <[Value; 2]>::try_from(pair) {
                        Ok([name, value]) => Ok((name, value, true)),
                        Err(pair) => Err(CopierError::Value(format!(
                            "{} values to unpack (expected 2)",
                            if pair.len() > 2 {
                                "too many"
                            } else {
                                "not enough"
                            }
                        ))),
                    },
                    single => Ok((single.clone(), single, false)),
                })
                .collect::<Result<_>>()?,
            Value::Null => Vec::new(),
            other => {
                return Err(CopierError::Type(format!(
                    "'{}' object is not iterable",
                    other.py_type_name()
                )));
            }
        };
        let mut result = Vec::with_capacity(pairs.len());
        for (name, mut value, is_pair) in pairs {
            let name = self.render_text(&name)?;
            let mut disabled = String::new();
            if let (true, Value::Map(spec)) = (is_pair, &value) {
                let Some(inner) = spec.get(&Value::from("value")).cloned() else {
                    return Err(CopierError::Key("Property 'value' is required".to_owned()));
                };
                match spec.get(&Value::from("validator")) {
                    None => {}
                    Some(validator @ Value::Str(_)) => disabled = self.render_text(validator)?,
                    Some(_) => {
                        return Err(CopierError::Value(
                            "Property 'validator' must be a string".to_owned(),
                        ))
                    }
                }
                value = inner;
            }
            // A choice without a value stands for its own label.
            let value = match self.render_value(&value)? {
                Value::Null => Value::Str(name.clone()),
                rendered => rendered,
            };
            let choice = Choice {
                name,
                value,
                disabled,
            };
            self.cast_answer(&choice.value)?;
            result.push(choice);
        }
        Ok(result)
    }

    /// Cast `answer` and check it against the choices.
    ///
    /// # Errors
    ///
    /// Returns [`CopierError::Value`] for answers that are not an enabled choice,
    /// besides the errors of [`Question::cast_answer`].
    pub fn parse_answer(&self, answer: &Value) -> Result<Value> {
        if !self.multiselect {
            return self.parse_single(answer);
        }
        let items: Vec<Value> = match answer {
            Value::Str(text) => yaml::list_item_sources(text)
                .map_err(|err| CopierError::Value(err.to_string()))?
                .ok_or_else(|| CopierError::Type(format!("Not a YAML list: {}", answer.py_repr())))?
                .into_iter()
                .map(Value::Str)
                .collect(),
            Value::Seq(items) => items.clone(),
            other => {
                return Err(CopierError::Type(format!(
                    "'{}' object is not iterable",
                    other.py_type_name()
                )));
            }
        };
        let parsed: Vec<Value> = items
            .iter()
            .map(|item| self.parse_single(item))
            .collect::<Result<_>>()?;
        // The result follows the order of the choices, not of the answer.
        let mut selected = Vec::new();
        for choice in self.formatted_choices()? {
            let value = self.cast_answer(&choice.value)?;
            if parsed.iter().any(|answer| py_eq(answer, &value)) {
                selected.push(value);
            }
        }
        Ok(Value::Seq(selected))
    }

    fn parse_single(&self, answer: &Value) -> Result<Value> {
        let cast_answer = self.cast_answer(answer)?;
        let choices = self.formatted_choices()?;
        if choices.is_empty() {
            return Ok(cast_answer);
        }
        let mut choice_error = String::new();
        let mut valid = Vec::new();
        for choice in &choices {
            let value = self.cast_answer(&choice.value)?;
            if choice.disabled.is_empty() {
                valid.push(value.clone());
            }
            if py_eq(&cast_answer, &value) {
                if choice.disabled.is_empty() {
                    return Ok(cast_answer);
                }
                if choice_error.is_empty() {
                    choice_error.clone_from(&choice.disabled);
                }
            }
        }
        let detail = if choice_error.is_empty() {
            format!(
                "{} is not in {}",
                cast_answer.py_repr(),
                Value::Seq(valid).py_repr()
            )
        } else {
            choice_error
        };
        Err(CopierError::Value(format!(
            "Invalid choice for '{}': {detail}",
            self.var_name
        )))
    }

    /// Check `answer` with the question's validator.
    ///
    /// # Errors
    ///
    /// Returns [`CopierError::Value`] carrying the validator's message.
    pub fn validate_answer(&self, answer: &Value) -> Result<()> {
        let context = self.context.with(self.var_name.clone(), to_jinja(answer));
        let message = match self.render_with(&Value::Str(self.validator.clone()), &context) {
            Ok(rendered) => rendered.py_str().trim().to_owned(),
            Err(error) => error.to_string(),
        };
        if message.is_empty() {
            return Ok(());
        }
        Err(CopierError::Value(format!(
            "Validation error for question '{}': {message}",
            self.var_name
        )))
    }

    /// Whether the question applies.
    ///
    /// # Errors
    ///
    /// Fails when the `when` template cannot be rendered.
    pub fn get_when(&self) -> Result<bool> {
        Ok(cast_to_bool(&self.render_value(&self.when)?))
    }

    /// Whether the question is asked at all.
    ///
    /// # Errors
    ///
    /// Fails when the `ask` template cannot be rendered.
    pub fn get_ask(&self) -> Result<bool> {
        Ok(cast_to_bool(&self.render_value(&self.ask)?))
    }

    /// Whether the input spans several lines.
    ///
    /// # Errors
    ///
    /// Fails when the `multiline` template cannot be rendered.
    pub fn get_multiline(&self) -> Result<bool> {
        Ok(cast_to_bool(&self.render_value(&self.multiline)?))
    }

    /// The rendered placeholder.
    ///
    /// # Errors
    ///
    /// Fails when the template cannot be rendered.
    pub fn get_placeholder(&self) -> Result<String> {
        self.render_text(&Value::Str(self.placeholder.clone()))
    }

    /// The default answer, parsed and validated; `None` when there is none.
    ///
    /// Sources by priority: data passed in, the last answer, user defaults, then the
    /// settings' default or the template's, rendered.
    ///
    /// # Errors
    ///
    /// Fails when the default is not a valid answer.
    pub fn get_default(&self) -> Result<Option<Value>> {
        let key = Value::from(self.var_name.as_str());
        let known = self
            .known
            .init
            .as_ref()
            .or(self.known.last.as_ref())
            .or(self.known.user_default.as_ref());
        let result = match known {
            Some(value) => value.clone(),
            None => {
                let Some(default) = self.settings.defaults.get(&key).or(self.default.as_ref())
                else {
                    return Ok(None);
                };
                let context = self.context.with("UNSET", JinjaValue::from(UNSET_MARKER));
                let rendered = self.render_with(default, &context)?;
                if rendered
                    .as_str()
                    .is_some_and(|text| text.contains(UNSET_MARKER))
                {
                    return Ok(None);
                }
                rendered
            }
        };
        let result = self.parse_answer(&result)?;
        if self.get_when()? && !self.secret {
            self.validate_answer(&result)?;
        }
        Ok(Some(result))
    }

    /// The default as a prompt shows it.
    ///
    /// # Errors
    ///
    /// Fails like [`Question::get_default`].
    pub fn get_default_rendered(&self) -> Result<DefaultRendered> {
        let Some(default) = self.get_default()? else {
            return Ok(DefaultRendered::Missing);
        };
        let has_choices = match &self.choices {
            Value::Seq(items) => !items.is_empty(),
            Value::Map(map) => !map.is_empty(),
            other => other.is_truthy(),
        };
        if has_choices {
            if !self.multiselect {
                for choice in self.formatted_choices()? {
                    if py_eq(&choice.value, &default) {
                        return Ok(DefaultRendered::Choice(choice));
                    }
                }
            }
            return Ok(DefaultRendered::None);
        }
        let type_name = self.get_type_name()?;
        Ok(match (&default, type_name.as_str()) {
            (Value::Bool(flag), "bool") => DefaultRendered::Bool(*flag),
            (Value::Null, _) => DefaultRendered::Text(String::new()),
            (_, "json") => {
                let options = if self.get_multiline()? {
                    json::DumpOptions::indented(2)
                } else {
                    json::DumpOptions::default()
                };
                DefaultRendered::Text(json::dumps(&default, &options))
            }
            (_, "yaml") => {
                let flow = if self.get_multiline()? {
                    yaml::FlowStyle::Block
                } else {
                    yaml::FlowStyle::Flow
                };
                let options = yaml::DumpOptions {
                    flow,
                    allow_unicode: false,
                    ..yaml::DumpOptions::default()
                };
                let dumped = yaml::dump(&default, &options);
                DefaultRendered::Text(dumped.trim().trim_end_matches("\n...").trim().to_owned())
            }
            _ => DefaultRendered::Text(default.py_str()),
        })
    }

    /// The prompt text: the help when given, otherwise the name and type.
    ///
    /// # Errors
    ///
    /// Fails when a template cannot be rendered.
    pub fn get_message(&self) -> Result<String> {
        if !self.help.is_empty() {
            let rendered = self.render_text(&Value::Str(self.help.clone()))?;
            if !rendered.is_empty() {
                return Ok(format!("{}  ", force_str_end(&rendered, "\n")));
            }
        }
        let type_name = self.get_type_name()?;
        let mut message = self.var_name.clone();
        if type_name != "str" {
            message.push_str(&format!(" ({type_name})"));
        }
        Ok(format!("{message}\n  "))
    }

    /// The prompt marker: the custom one, or a microphone (a detective for secrets).
    pub fn get_qmark(&self) -> String {
        match &self.qmark {
            Some(qmark) if !qmark.is_empty() => qmark.clone(),
            _ if self.secret => "🕵️".to_owned(),
            _ => "🎤".to_owned(),
        }
    }

    /// Which prompt the question needs.
    ///
    /// # Errors
    ///
    /// Fails when the type cannot be rendered.
    pub fn prompt_kind(&self) -> Result<PromptKind> {
        let type_name = self.get_type_name()?;
        let has_choices = match &self.choices {
            Value::Seq(items) => !items.is_empty(),
            Value::Map(map) => !map.is_empty(),
            other => other.is_truthy(),
        };
        Ok(if type_name == "path" {
            PromptKind::Path
        } else if has_choices && self.multiselect {
            PromptKind::Checkbox
        } else if has_choices {
            PromptKind::Select
        } else if type_name == "bool" {
            PromptKind::Confirm
        } else if self.secret {
            PromptKind::Password
        } else {
            PromptKind::Input
        })
    }

    /// Check typed text the way the prompt does while the user edits it.
    ///
    /// # Errors
    ///
    /// Returns the message to show: `Invalid input` when the text cannot be parsed,
    /// otherwise the validator's.
    pub fn check_input(&self, answer: &Value) -> std::result::Result<(), String> {
        let parsed = self
            .parse_answer(answer)
            .map_err(|_| "Invalid input".to_owned())?;
        self.validate_answer(&parsed)
            .map_err(|error| error.to_string())
    }
}
