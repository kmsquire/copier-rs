//! Per-thread run state that Python Copier keeps in context variables: the
//! current phase and the operation in progress.

use std::cell::Cell;

/// Stage of a run, exposed to templates as `_copier_phase`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Phase {
    /// Asking questions.
    Prompt,
    /// Running tasks.
    Tasks,
    /// Running migrations.
    Migrate,
    /// Rendering files.
    Render,
    /// Outside any phase.
    #[default]
    Undefined,
}

impl Phase {
    /// The name templates see.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Prompt => "prompt",
            Self::Tasks => "tasks",
            Self::Migrate => "migrate",
            Self::Render => "render",
            Self::Undefined => "undefined",
        }
    }

    /// The phase of the run in progress on this thread.
    pub fn current() -> Self {
        PHASE.with(Cell::get)
    }

    /// Enter `phase` until the returned guard is dropped.
    pub(crate) fn enter(phase: Self) -> PhaseGuard {
        PhaseGuard {
            previous: PHASE.with(|cell| cell.replace(phase)),
        }
    }
}

/// What a run is doing, exposed to templates as `_copier_operation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    /// Generating a project (also used by recopy).
    Copy,
    /// Updating a project.
    Update,
}

impl Operation {
    /// The name templates see.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Copy => "copy",
            Self::Update => "update",
        }
    }

    /// The operation in progress on this thread, if any.
    pub fn current() -> Option<Self> {
        OPERATION.with(Cell::get)
    }

    /// Start `operation` unless one is already in progress: the copies an update
    /// performs internally still count as part of the update.
    pub(crate) fn enter(operation: Self) -> OperationGuard {
        let previous = OPERATION.with(Cell::get);
        if previous.is_none() {
            OPERATION.with(|cell| cell.set(Some(operation)));
        }
        OperationGuard { previous }
    }
}

thread_local! {
    static PHASE: Cell<Phase> = const { Cell::new(Phase::Undefined) };
    static OPERATION: Cell<Option<Operation>> = const { Cell::new(None) };
}

/// Restores the previous phase when dropped.
pub(crate) struct PhaseGuard {
    previous: Phase,
}

impl Drop for PhaseGuard {
    fn drop(&mut self) {
        PHASE.with(|cell| cell.set(self.previous));
    }
}

/// Restores the previous operation when dropped.
pub(crate) struct OperationGuard {
    previous: Option<Operation>,
}

impl Drop for OperationGuard {
    fn drop(&mut self) {
        OPERATION.with(|cell| cell.set(self.previous));
    }
}
