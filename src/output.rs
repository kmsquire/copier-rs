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

/// ANSI styling of messages, matching the colors Python Copier prints.
pub mod style {
    use std::io::IsTerminal;

    const FG_RESET: &str = "\x1b[39m";

    /// Whether optional colors are emitted: only when stdout is a terminal, the
    /// rule Python Copier inherits from plumbum.
    fn enabled() -> bool {
        std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
    }

    fn paint(code: &str, text: &str) -> String {
        if enabled() {
            format!("\x1b[{code}m{text}{FG_RESET}")
        } else {
            text.to_owned()
        }
    }

    /// A warning (yellow).
    pub fn warn(text: &str) -> String {
        paint("33", text)
    }

    /// An informational note (cyan).
    pub fn info(text: &str) -> String {
        paint("36", text)
    }

    /// An error (red).
    pub fn error(text: &str) -> String {
        paint("31", text)
    }
}

/// Emphasis of the action word in a status line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Bright green.
    Ok,
    /// Bright yellow.
    Warning,
    /// Cyan.
    Ignore,
    /// Bright red.
    Danger,
}

impl Status {
    fn codes(self) -> &'static str {
        match self {
            Self::Ok => "\x1b[32m\x1b[1m",
            Self::Warning => "\x1b[33m\x1b[1m",
            Self::Ignore => "\x1b[36m",
            Self::Danger => "\x1b[31m\x1b[1m",
        }
    }
}

const STATUS_RESET: &str = "\x1b[39m\x1b[0m";
const HLINE: &str = "------------------------------------------";

/// Print a status line to stderr: the action right-aligned to `indent` columns
/// and always colored, then two spaces and the message.
pub fn printf(action: &str, message: &str, status: Status, indent: usize) {
    eprintln(&format!(
        "{}{action:>indent$}{STATUS_RESET}  {message}",
        status.codes()
    ));
}

/// Print an error banner to stderr: the action, a rule, the detail, a rule.
pub fn printf_exception(detail: &str, action: &str, message: &str) {
    eprintln("");
    printf(action, message, Status::Danger, 0);
    eprintln(HLINE);
    eprintln(detail);
    eprintln(HLINE);
}
