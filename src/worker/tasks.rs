//! Execution of template tasks and migrations (`Worker._execute_tasks`).

use std::process::Command as Process;

use minijinja::value::Value as JinjaValue;

use super::Worker;
use crate::errors::{CopierError, Result};
use crate::jinja::to_jinja;
use crate::output;
use crate::template::{Command, Task};
use crate::tools::cast_to_bool;
use crate::value::{py_str_repr, Value};

impl Worker {
    /// Run `tasks` in order inside the project, skipping those whose condition is false.
    pub(super) fn execute_tasks(&self, tasks: &[Task]) -> Result<()> {
        let project = &self.subproject()?.local_abspath;
        for (index, task) in tasks.iter().enumerate() {
            let mut extra: Vec<(String, JinjaValue)> = task
                .extra_vars
                .iter()
                .map(|(key, value)| (format!("_{}", key.py_str()), to_jinja(value)))
                .collect();
            extra.push(Self::operation_variable());
            let condition = match &task.condition {
                Value::Str(source) => Value::Str(self.render_string(source, &extra)?),
                other => other.clone(),
            };
            if !cast_to_bool(&condition) {
                continue;
            }
            let (program, arguments, shown) = match &task.cmd {
                Command::Shell(source) => {
                    let command = self.render_string(source, &extra)?;
                    let (shell, flag) = if cfg!(windows) {
                        ("cmd", "/C")
                    } else {
                        ("/bin/sh", "-c")
                    };
                    (
                        shell.to_owned(),
                        vec![flag.to_owned(), command.clone()],
                        command,
                    )
                }
                Command::Argv(parts) => {
                    let rendered: Vec<String> = parts
                        .iter()
                        .map(|part| self.render_string(part, &extra))
                        .collect::<Result<_>>()?;
                    let shown = format!(
                        "[{}]",
                        rendered
                            .iter()
                            .map(|part| py_str_repr(part))
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                    let mut rendered = rendered.into_iter();
                    (
                        rendered.next().unwrap_or_default(),
                        rendered.collect(),
                        shown,
                    )
                }
            };
            if !self.quiet {
                output::eprintln(&output::style::info(&format!(
                    " > Running task {} of {}: {shown}",
                    index + 1,
                    tasks.len()
                )));
            }
            if self.pretend {
                continue;
            }
            crate::interrupt::check()?;
            let working_directory = project
                .join(self.render_string(&task.working_directory.to_string_lossy(), &extra)?);
            let mut process = Process::new(&program);
            process.args(&arguments).current_dir(&working_directory);
            for (name, value) in &extra {
                let text = if value.is_undefined() || value.is_none() {
                    "None".to_owned()
                } else {
                    value
                        .as_str()
                        .map_or_else(|| value.to_string(), str::to_owned)
                };
                process.env(name.trim_start_matches('_').to_uppercase(), text);
            }
            let status = process.status().map_err(|error| {
                CopierError::Process(format!("Cannot run task {shown}: {error}"))
            })?;
            // A task killed by Ctrl-C is an interrupt, not a task failure.
            crate::interrupt::check()?;
            if !status.success() {
                let command = match &task.cmd {
                    Command::Shell(_) => py_str_repr(&shown),
                    Command::Argv(_) => shown,
                };
                return Err(CopierError::Task {
                    command,
                    returncode: status.code().unwrap_or(-1),
                });
            }
        }
        Ok(())
    }
}
