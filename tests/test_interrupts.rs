//! Port of `tests/test_interrupts.py`.

mod common;

use std::collections::VecDeque;

use common::{assert_err, file_tree, v, worker, Tmp};
use copier::errors::CopierError;
use copier::prompt::{PromptError, PromptSpec, Prompter};
use copier::Value;
use indoc::indoc;
use serde_json::json;

/// Replays scripted prompt results in order, as Python's `side_effect` list does.
struct ScriptedPrompter {
    steps: VecDeque<Result<Value, PromptError>>,
}

impl ScriptedPrompter {
    fn new(steps: Vec<Result<Value, PromptError>>) -> Self {
        Self {
            steps: steps.into(),
        }
    }
}

impl Prompter for ScriptedPrompter {
    fn ask(&mut self, _spec: &PromptSpec) -> Result<Value, PromptError> {
        self.steps.pop_front().expect("no more scripted prompts")
    }

    fn confirm(&mut self, _message: &str, _default: bool) -> Result<bool, PromptError> {
        Err(PromptError::Interrupted)
    }
}

// We override the prompt method from questionary to raise this
// exception and expect our surrounding machinery to re-raise
// it as a CopierAnswersInterrupt.
// TODO(port): the `CopierAnswersInterrupt` parametrize case has no Rust input; the
// prompter can only yield `PromptError::Interrupted` (the `KeyboardInterrupt` case).
#[test]
#[ignore = "pending"]
fn test_keyboard_interrupt() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => indoc! {"
            question:
                type: str
        "},
    };

    assert_err!(
        worker(&src, &dst)
            .defaults(false)
            .prompter(Box::new(ScriptedPrompter::new(vec![Err(
                PromptError::Interrupted
            )])))
            .run_copy(),
        CopierError::AnswersInterrupt(_)
    );
}

#[test]
#[ignore = "pending"]
fn test_multiple_questions_interrupt() {
    let tmp = Tmp::new();
    let (src, dst) = tmp.src_dst();
    file_tree! {
        src.join("copier.yml") => indoc! {"
            question1:
                type: str
            question2:
                type: str
            question3:
                type: str
        "},
    };

    let worker = worker(&src, &dst)
        .defaults(false)
        .prompter(Box::new(ScriptedPrompter::new(vec![
            Ok(Value::from("foobar")),
            Ok(Value::from("yosemite")),
            Err(PromptError::Interrupted),
        ])));
    let template = worker.template().unwrap();

    let CopierError::AnswersInterrupt(interrupt) = worker
        .run_copy()
        .err()
        .expect("run_copy should be interrupted")
    else {
        panic!("expected CopierError::AnswersInterrupt");
    };
    assert_eq!(
        Value::Map(interrupt.answers.user.clone()),
        v(json!({
            "question1": "foobar",
            "question2": "yosemite",
        }))
    );
    assert_eq!(interrupt.template_url, template.url);
}
