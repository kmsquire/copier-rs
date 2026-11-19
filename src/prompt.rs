//! Interactive prompting. The terminal UI lives behind [`Prompter`] so the engine
//! can be driven by scripted answers.

use std::fmt;

use crate::questions::{DefaultRendered, PromptKind};
use crate::value::Value;

/// Why a prompt produced no answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptError {
    /// The user pressed Ctrl-C.
    Interrupted,
    /// Input ended; no interactive session is available.
    Eof,
}

/// Live check of a typed answer; `Err` carries the message to show.
pub type Validator<'a> = dyn Fn(&Value) -> Result<(), String> + 'a;

/// One option of a select or checkbox prompt.
#[derive(Debug, Clone, PartialEq)]
pub struct PromptChoice {
    /// Label shown to the user.
    pub name: String,
    /// Raw value the choice stands for.
    pub value: Value,
    /// Why it cannot be selected; empty when it can.
    pub disabled: String,
    /// Whether a checkbox starts ticked.
    pub checked: bool,
}

/// Everything needed to ask one question.
pub struct PromptSpec<'a> {
    /// Question name.
    pub name: String,
    /// Text shown to the user.
    pub message: String,
    /// Marker shown before the message.
    pub qmark: String,
    /// Kind of prompt.
    pub kind: PromptKind,
    /// Default shown in the prompt.
    pub default: DefaultRendered,
    /// Options of a select or checkbox prompt.
    pub choices: Vec<PromptChoice>,
    /// Whether the input spans several lines.
    pub multiline: bool,
    /// Hint shown in an empty input.
    pub placeholder: String,
    /// Question type (`str`, `yaml`, ...), for syntax-aware editing.
    pub type_name: String,
    /// Live check of the typed answer; `Err` carries the message to show.
    pub validator: Option<&'a Validator<'a>>,
}

impl PromptSpec<'_> {
    /// Check `answer` the way the prompt does while the user edits it.
    ///
    /// # Errors
    ///
    /// Returns the message to show the user.
    pub fn validate(&self, answer: &Value) -> Result<(), String> {
        self.validator.map_or(Ok(()), |validator| validator(answer))
    }
}

impl fmt::Debug for PromptSpec<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PromptSpec")
            .field("name", &self.name)
            .field("message", &self.message)
            .field("kind", &self.kind)
            .field("default", &self.default)
            .field("choices", &self.choices)
            .finish_non_exhaustive()
    }
}

/// Source of interactive answers.
pub trait Prompter {
    /// Ask a question and return the raw (uncast) answer.
    ///
    /// # Errors
    ///
    /// Returns why no answer was produced.
    fn ask(&mut self, spec: &PromptSpec<'_>) -> Result<Value, PromptError>;

    /// Ask a yes/no question.
    ///
    /// # Errors
    ///
    /// Returns why no answer was produced.
    fn confirm(&mut self, message: &str, default: bool) -> Result<bool, PromptError>;
}

/// A prompter for non-interactive runs: every prompt reports the end of input.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoPrompter;

impl Prompter for NoPrompter {
    fn ask(&mut self, _spec: &PromptSpec<'_>) -> Result<Value, PromptError> {
        Err(PromptError::Eof)
    }

    fn confirm(&mut self, _message: &str, _default: bool) -> Result<bool, PromptError> {
        Err(PromptError::Eof)
    }
}

/// The prompter the CLI uses.
pub fn terminal() -> NoPrompter {
    NoPrompter
}
