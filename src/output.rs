//! Process output with optional per-thread capture, so library callers and tests
//! can observe exactly what the CLI would print.

use std::cell::RefCell;
use std::io::Write;

/// Text written to the standard streams while a capture was active.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Captured {
    /// Everything written to stdout.
    pub stdout: String,
    /// Everything written to stderr.
    pub stderr: String,
}

thread_local! {
    static CAPTURE: RefCell<Option<Captured>> = const { RefCell::new(None) };
}

/// Run `f`, returning its result and everything it printed on this thread.
///
/// Output of child processes (tasks, git) is not captured.
pub fn capture<T>(f: impl FnOnce() -> T) -> (T, Captured) {
    let previous = CAPTURE.with(|slot| slot.borrow_mut().replace(Captured::default()));
    let result = f();
    let captured = CAPTURE.with(|slot| std::mem::replace(&mut *slot.borrow_mut(), previous));
    (result, captured.unwrap_or_default())
}

/// Write `text` to stdout without a trailing newline.
pub fn print(text: &str) {
    let captured = CAPTURE.with(|slot| match slot.borrow_mut().as_mut() {
        Some(captured) => {
            captured.stdout.push_str(text);
            true
        }
        None => false,
    });
    if !captured {
        // A closed pipe must not turn into a panic.
        let _ = std::io::stdout().write_all(text.as_bytes());
    }
}

/// Write `text` to stderr without a trailing newline.
pub fn eprint(text: &str) {
    let captured = CAPTURE.with(|slot| match slot.borrow_mut().as_mut() {
        Some(captured) => {
            captured.stderr.push_str(text);
            true
        }
        None => false,
    });
    if !captured {
        let _ = std::io::stderr().write_all(text.as_bytes());
    }
}

/// Write `text` and a newline to stdout.
pub fn println(text: &str) {
    print(text);
    print("\n");
}

/// Write `text` and a newline to stderr.
pub fn eprintln(text: &str) {
    eprint(text);
    eprint("\n");
}
