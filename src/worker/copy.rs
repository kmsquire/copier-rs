//! Copy and recopy (`Worker.run_copy`, `Worker.run_recopy`).

use super::{Operation, Phase, Worker};
use crate::errors::{warn, CopierError, Result, WarningKind};
use crate::output;
use crate::template::Stage;
use crate::vcs;

impl Worker {
    /// Refuse templates that use unsafe features unless they are trusted.
    pub(super) fn check_unsafe(&self, operation: Operation) -> Result<()> {
        let template = self.template_ref()?;
        if self.unsafe_ || self.user_settings()?.is_trusted(&template.url) {
            return Ok(());
        }
        let mut features = Vec::new();
        let mut add = |feature: &str| {
            if !features.iter().any(|known| known == feature) {
                features.push(feature.to_owned());
            }
        };
        if !template.jinja_extensions()?.is_empty() {
            add("jinja_extensions");
        }
        if !template.tasks()?.is_empty() && !self.skip_tasks {
            add("tasks");
        }
        if let (Operation::Update, Some(old_template)) = (operation, self.subproject()?.template())
        {
            if !old_template.jinja_extensions()?.is_empty() {
                add("jinja_extensions");
            }
            if !old_template.tasks()?.is_empty() {
                add("tasks");
            }
            for stage in [Stage::Before, Stage::After] {
                if !template.migration_tasks(stage, old_template)?.is_empty() {
                    add("migrations");
                    break;
                }
            }
        }
        if features.is_empty() {
            return Ok(());
        }
        features.sort();
        Err(CopierError::UnsafeTemplate { features })
    }

    pub(super) fn copy(&mut self) -> Result<()> {
        let _operation = Operation::enter(Operation::Copy);
        self.reset_match_exclude();
        self.check_unsafe(Operation::Copy)?;
        let template = self.template()?;
        self.print_message(&template.message("before_copy")?)?;
        {
            let _phase = Phase::enter(Phase::Prompt);
            self.ask_questions()?;
        }
        let project = self.subproject()?.local_abspath.clone();
        let was_existing = project.exists();
        if let Err(error) = self.render_and_run_tasks() {
            if !was_existing && self.cleanup_on_error {
                match vcs::remove_dir_all(&project) {
                    Ok(()) => {}
                    // Nothing was created yet, so nothing to clean up.
                    Err(cleanup_error) if cleanup_error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(cleanup_error) => warn(
                        WarningKind::Cleanup,
                        format!("Failed to clean up {}: {cleanup_error}", project.display()),
                    ),
                }
            }
            return Err(error);
        }
        self.print_message(&template.message("after_copy")?)?;
        if !self.quiet {
            output::println("");
        }
        Ok(())
    }

    fn render_and_run_tasks(&self) -> Result<()> {
        let template = self.template_ref()?;
        if !self.quiet {
            let version = template
                .version()?
                .map_or_else(|| "None".to_owned(), |version| version.to_string());
            output::eprintln(&format!("\nCopying from template version {version}"));
        }
        {
            let _phase = Phase::enter(Phase::Render);
            self.render_template()?;
        }
        if !self.quiet {
            output::println("");
        }
        if !self.skip_tasks {
            let _phase = Phase::enter(Phase::Tasks);
            self.execute_tasks(&template.tasks()?)?;
        }
        Ok(())
    }

    pub(super) fn recopy(&mut self) -> Result<()> {
        let _operation = Operation::enter(Operation::Copy);
        let subproject = self.subproject()?;
        let Some(old_template) = subproject.template() else {
            return Err(CopierError::UserMessage(format!(
                "Cannot recopy because cannot obtain old template references from `{}`.",
                subproject.answers_relpath.display()
            )));
        };
        let mut worker = self.derive();
        worker.src_path = Some(old_template.url.clone());
        worker.copy()?;
        self.answers = worker.answers;
        Ok(())
    }
}
