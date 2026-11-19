//! Update (`Worker.run_update`) and update checks.

use std::path::Path;

use super::{UpdateData, Worker};
use crate::errors::{CopierError, Result};

impl Worker {
    pub(super) fn update(&mut self) -> Result<()> {
        Err(CopierError::UserMessage(
            "Updating is not implemented yet.".to_owned(),
        ))
    }
}

/// Report whether the project at `dst_path` can be updated to a newer template version.
///
/// # Errors
///
/// Fails when the project lacks the recorded template data needed to compare versions.
pub fn get_update_data(
    _dst_path: &Path,
    _answers_file: Option<&Path>,
    _use_prereleases: bool,
) -> Result<UpdateData> {
    Err(CopierError::UserMessage(
        "Checking for updates is not implemented yet.".to_owned(),
    ))
}
