//! Ctrl-C handling.
//!
//! Without a handler, an interrupt kills the process on the spot and leaves
//! temporary clones and scratch repositories behind. Instead the interrupt is
//! recorded, the run notices it at the next safe point and fails with
//! [`CopierError::Interrupted`], and the usual unwinding removes the temporary
//! directories. Like Python Copier, an interrupted run leaves the destination as
//! it is. Pressing Ctrl-C a second time exits immediately with status 130.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;

use crate::errors::{CopierError, Result};

static PENDING: AtomicBool = AtomicBool::new(false);
static INSTALL: Once = Once::new();

/// Start recording Ctrl-C instead of dying from it. Safe to call repeatedly; does
/// nothing when the host program already installed its own handler.
pub fn install() {
    INSTALL.call_once(|| {
        // Failing here means another handler owns the signal; it decides then.
        let _ = ctrlc::set_handler(|| {
            // A second Ctrl-C means the first was not acted on (a task that traps
            // the signal, a blocked read): give the user the usual way out.
            if PENDING.swap(true, Ordering::SeqCst) {
                std::process::exit(130);
            }
        });
    });
}

/// Ask the run in progress to stop at its next safe point, as Ctrl-C does. For
/// hosts that handle signals themselves and want to forward a cancellation.
pub fn raise() {
    PENDING.store(true, Ordering::SeqCst);
}

/// Finish a top-level run: consume a pending interrupt so it cannot leak into the
/// next run, and report it instead of whatever error it was mistaken for. A run
/// that completed keeps its result; the interrupt came too late to matter.
pub(crate) fn settle<T>(outcome: Result<T>) -> Result<T> {
    let interrupted = PENDING.swap(false, Ordering::SeqCst);
    match outcome {
        Err(_) if interrupted => Err(CopierError::Interrupted),
        other => other,
    }
}

/// Fail if an interrupt is pending. The interrupt stays pending, so it cannot be
/// lost in code that deliberately ignores a failing command.
pub(crate) fn check() -> Result<()> {
    if PENDING.load(Ordering::SeqCst) {
        Err(CopierError::Interrupted)
    } else {
        Ok(())
    }
}
