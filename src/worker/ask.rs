//! The questionnaire (`Worker._ask`).

use super::Worker;
use crate::errors::{AnswersInterrupt, CopierError, Result};
use crate::prompt::{PromptChoice, PromptError, PromptSpec};
use crate::questions::{py_eq, AnswersMap, DefaultRendered, PromptKind, Question};
use crate::tools::fnmatch;
use crate::value::{Mapping, Value};

/// What to do with a question once its definition has been examined.
enum Decision {
    /// Leave the answers as they are.
    Skip,
    /// Record this answer.
    Answer(Value),
}

/// Everything one question changes about the answers.
struct Outcome {
    decision: Decision,
    /// The recorded answer is invalid or no longer applies.
    forget_last: bool,
    /// The question does not apply, so its answer must not be recorded.
    hide: bool,
}

impl Worker {
    /// Collect the answers to the template's questions, prompting where needed.
    pub(super) fn ask_questions(&mut self) -> Result<()> {
        let template = self.template()?;
        let fresh = AnswersMap {
            user_defaults: self.user_defaults.clone(),
            init: self.data.clone(),
            last: self.subproject()?.last_answers().clone(),
            metadata: template.metadata()?,
            ..AnswersMap::default()
        };
        self.update_answers(|answers| *answers = fresh);
        for (key, details) in &template.questions_data()? {
            let var_name = key.py_str();
            let details = match details {
                Value::Map(details) => details.clone(),
                _ => Mapping::new(),
            };
            crate::interrupt::check()?;
            let outcome = self.examine(&var_name, &details)?;
            self.update_answers(|answers| {
                if outcome.forget_last {
                    answers.last.shift_remove(key);
                }
                if outcome.hide {
                    answers.hide(&var_name);
                }
                if let Decision::Answer(answer) = outcome.decision {
                    answers.user.insert(key.clone(), answer);
                }
            });
        }
        Ok(())
    }

    /// Work out what one question does to the answers, prompting if necessary.
    fn examine(&self, var_name: &str, details: &Mapping) -> Result<Outcome> {
        let key = Value::from(var_name);
        let settings = self.user_settings()?;
        let mut question = Question::new(
            var_name,
            details,
            self.answers.known(var_name),
            self.context()?,
            self.env()?,
            settings,
        )?;
        let mut outcome = Outcome {
            decision: Decision::Skip,
            forget_last: false,
            hide: false,
        };

        // A recorded answer that is no longer valid is forgotten.
        if let Some(last) = self.answers.last.get(&key) {
            let still_valid = question
                .parse_answer(last)
                .and_then(|answer| question.validate_answer(&answer))
                .is_ok();
            if !still_valid {
                question.forget_last();
                outcome.forget_last = true;
            }
        }
        let ask = question.get_ask()?;
        let applies = question.get_when()?;
        if !applies {
            outcome.hide = true;
            outcome.forget_last = true;
            question.forget_last();
            if question.get_default()?.is_none() {
                return Ok(outcome);
            }
        }
        let has_last = !outcome.forget_last && self.answers.last.contains_key(&key);
        outcome.decision = match self.decide(&question, var_name, ask, has_last)? {
            Some(decision) => decision,
            // A question that does not apply is never shown; it takes its default.
            None if !applies => question
                .get_default()?
                .map_or(Decision::Skip, Decision::Answer),
            None => self.prompt(&question, var_name)?,
        };
        Ok(outcome)
    }

    fn is_asked_explicitly(&self, var_name: &str) -> bool {
        self.ask.iter().any(|pattern| fnmatch(var_name, pattern))
    }

    /// Settle the question without prompting when the options allow it; `None`
    /// means the user must be asked.
    fn decide(
        &self,
        question: &Question<'_>,
        var_name: &str,
        ask: bool,
        has_last: bool,
    ) -> Result<Option<Decision>> {
        if self.is_asked_explicitly(var_name) {
            return Ok(None);
        }
        if let Some(given) = self.answers.init.get(&Value::from(var_name)) {
            let answer = question.parse_answer(given)?;
            question.validate_answer(&answer)?;
            return Ok(Some(Decision::Answer(answer)));
        }
        if has_last && (self.skip_answered || !ask) {
            return Ok(Some(Decision::Skip));
        }
        if !ask {
            return Ok(Some(
                question
                    .get_default()?
                    .map_or(Decision::Skip, Decision::Answer),
            ));
        }
        if self.defaults {
            return match question.get_default()? {
                Some(answer) => Ok(Some(Decision::Answer(answer))),
                None => Err(CopierError::Value(format!(
                    "Question \"{var_name}\" is required"
                ))),
            };
        }
        Ok(None)
    }

    fn prompt(&self, question: &Question<'_>, var_name: &str) -> Result<Decision> {
        let kind = question.prompt_kind()?;
        let default = question.get_default_rendered()?;
        let mut choices = Vec::new();
        if matches!(kind, PromptKind::Select | PromptKind::Checkbox) {
            let default_choices = match (kind, question.get_default()?) {
                (PromptKind::Checkbox, Some(Value::Seq(items))) => items,
                _ => Vec::new(),
            };
            for choice in question.formatted_choices()? {
                let value = question.cast_answer(&choice.value)?;
                choices.push(PromptChoice {
                    checked: default_choices.iter().any(|item| py_eq(item, &value)),
                    name: choice.name,
                    value: choice.value,
                    disabled: choice.disabled,
                });
            }
        }
        let validator = |answer: &Value| question.check_input(answer);
        let validated = matches!(
            kind,
            PromptKind::Input | PromptKind::Checkbox | PromptKind::Password | PromptKind::Path
        );
        let is_text = matches!(
            kind,
            PromptKind::Input | PromptKind::Password | PromptKind::Path
        );
        let spec = PromptSpec {
            name: var_name.to_owned(),
            message: question.get_message()?,
            qmark: question.get_qmark(),
            kind,
            // A yes/no prompt always shows a default.
            default: match (kind, default) {
                (PromptKind::Confirm, DefaultRendered::Missing) => DefaultRendered::Bool(false),
                (_, default) => default,
            },
            choices,
            multiline: is_text && question.get_multiline()?,
            placeholder: if is_text {
                question.get_placeholder()?
            } else {
                String::new()
            },
            type_name: question.get_type_name()?,
            validator: if validated { Some(&validator) } else { None },
        };
        let raw = self.prompter.borrow_mut().ask(&spec);
        match raw {
            Ok(raw) => Ok(Decision::Answer(question.cast_answer(&raw)?)),
            Err(PromptError::Eof) => Err(CopierError::InteractiveSession(
                "Use `--defaults` and/or `--data`/`--data-file`".to_owned(),
            )),
            Err(PromptError::Interrupted) => {
                Err(CopierError::AnswersInterrupt(Box::new(AnswersInterrupt {
                    answers: self.answers.clone(),
                    last_question: var_name.to_owned(),
                    template_url: self.template_ref()?.url.clone(),
                })))
            }
        }
    }
}
