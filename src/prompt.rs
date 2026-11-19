//! Interactive prompting. The terminal UI lives behind [`Prompter`] so the engine
//! can be driven by scripted answers.

use crate::value::Value;

/// Why a prompt produced no answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptError {
    /// The user pressed Ctrl-C.
    Interrupted,
    /// Input ended; no interactive session is available.
    Eof,
}

/// Everything needed to ask one question.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct PromptSpec {
    /// Question name.
    pub name: String,
    /// Text shown to the user.
    pub message: String,
}

/// Source of interactive answers.
pub trait Prompter {
    /// Ask a question and return the raw (uncast) answer.
    fn ask(&mut self, spec: &PromptSpec) -> Result<Value, PromptError>;

    /// Ask a yes/no question.
    fn confirm(&mut self, message: &str, default: bool) -> Result<bool, PromptError>;
}
