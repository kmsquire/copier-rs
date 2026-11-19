//! Ctrl-C handling.
//!
//! Without a handler, an interrupt kills the process on the spot and leaves
//! temporary clones and scratch repositories behind. Instead the interrupt is
//! recorded, the run notices it at the next safe point and fails with
//! [`CopierError::Interrupted`], and the usual unwinding removes the temporary
//! directories. Like Python Copier, an interrupted run leaves the destination as
//! it is.

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
        let _ = ctrlc::set_handler(raise);
    });
}

/// Record an interrupt, as if Ctrl-C had been pressed.
pub fn raise() {
    PENDING.store(true, Ordering::SeqCst);
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

/// Forget a pending interrupt once it has been reported to the caller.
pub(crate) fn clear() {
    PENDING.store(false, Ordering::SeqCst);
}
