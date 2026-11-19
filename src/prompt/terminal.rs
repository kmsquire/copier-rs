//! The interactive prompts of the command line.
//!
//! Wording, markers and key bindings follow the `questionary` prompts Python Copier
//! shows. On a real terminal a prompt is redrawn in place; when input or output is
//! redirected each state is printed as plain text, which keeps scripted sessions
//! readable.

use std::io::{IsTerminal, Stdin, Write};

use unicode_width::UnicodeWidthStr;

use super::keys::{Key, KeyReader};
use super::{PromptChoice, PromptError, PromptSpec, Prompter};
use crate::questions::{DefaultRendered, PromptKind};
use crate::value::Value;

const POINTER: &str = "»";
const CHECKED: &str = "●";
const UNCHECKED: &str = "○";
const SELECT_HINT: &str = "(Use arrow keys)";
const CHECKBOX_HINT: &str =
    "(Use arrow keys to move, <space> to select, <a> to toggle, <i> to invert)";
const MULTILINE_HINT: &str = " (Finish with 'Alt+Enter' or 'Esc then Enter')\n> ";

/// How a piece of a prompt is emphasized on a terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Style {
    Plain,
    Question,
    Answer,
    Hint,
    Disabled,
    Error,
}

impl Style {
    fn code(self) -> &'static str {
        match self {
            Self::Plain => "",
            Self::Question => "\x1b[1m",
            Self::Answer => "\x1b[1;33m",
            Self::Hint | Self::Disabled => "\x1b[2m",
            Self::Error => "\x1b[31m",
        }
    }
}

/// What a prompt currently shows: styled spans, and where the text cursor sits
/// (as a character offset into the plain text) if it is not at the end.
#[derive(Default)]
struct Frame {
    spans: Vec<(Style, String)>,
    cursor: Option<usize>,
}

impl Frame {
    fn push(&mut self, style: Style, text: impl Into<String>) {
        self.spans.push((style, text.into()));
    }

    fn text(&self) -> String {
        self.spans.iter().map(|(_, text)| text.as_str()).collect()
    }
}

/// Number of terminal rows `text` occupies and the column the cursor ends on.
fn extent(text: &str, columns: usize) -> (usize, usize) {
    let mut rows = 1;
    let mut column = 0;
    for line in text
        .split('\n')
        .enumerate()
        .map(|(index, line)| (index > 0, line))
    {
        if line.0 {
            rows += 1;
        }
        let width = line.1.width();
        // A full line wraps onto the next row only once more text follows.
        rows += width.saturating_sub(1) / columns.max(1);
        column = if width == 0 {
            0
        } else {
            (width - 1) % columns.max(1) + 1
        };
    }
    (rows, column)
}

/// Prompts on the process's standard input and output.
pub struct TerminalPrompter {
    keys: KeyReader<Stdin>,
    /// Whether prompts are drawn with cursor control (both ends are terminals).
    interactive: bool,
    /// Whether keys are read unbuffered. True whenever stdin is a terminal, even
    /// with output redirected, so Ctrl-C arrives as a key instead of leaving the
    /// prompt blocked on a line read.
    raw_input: bool,
    /// Rows between the top of the drawn frame and the cursor's row.
    rows_above_cursor: usize,
}

/// Puts the terminal in raw mode for the lifetime of one prompt.
struct RawMode(bool);

impl RawMode {
    fn enter(interactive: bool) -> Self {
        Self(interactive && crossterm::terminal::enable_raw_mode().is_ok())
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        if self.0 {
            let _ = crossterm::terminal::disable_raw_mode();
        }
    }
}

impl Default for TerminalPrompter {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalPrompter {
    /// A prompter reading keys from stdin and drawing on stdout.
    pub fn new() -> Self {
        let raw_input = std::io::stdin().is_terminal();
        let interactive = raw_input && std::io::stdout().is_terminal();
        Self {
            keys: KeyReader::new(std::io::stdin()),
            interactive,
            raw_input,
            rows_above_cursor: 0,
        }
    }

    fn write(&self, text: &str) {
        let mut stdout = std::io::stdout();
        // A closed stdout must not abort the run from inside a prompt.
        let _ = stdout.write_all(text.as_bytes());
        let _ = stdout.flush();
    }

    /// Show `frame`, replacing the previous one on a terminal.
    fn draw(&mut self, frame: &Frame, last: bool) {
        if !self.interactive {
            self.write(&format!("{}\r\n", frame.text().replace('\n', "\r\n")));
            return;
        }
        let columns = crossterm::terminal::size().map_or(80, |(columns, _)| usize::from(columns));
        let mut out = String::new();
        if self.rows_above_cursor > 0 {
            out.push_str(&format!("\x1b[{}A", self.rows_above_cursor));
        }
        out.push_str("\r\x1b[J");
        for (style, text) in &frame.spans {
            let text = text.replace('\n', "\r\n");
            if style.code().is_empty() {
                out.push_str(&text);
            } else {
                out.push_str(&format!("{}{text}\x1b[0m", style.code()));
            }
        }
        let text = frame.text();
        let (total_rows, _) = extent(&text, columns);
        if last {
            out.push_str("\r\n");
            self.rows_above_cursor = 0;
        } else {
            let (cursor_rows, cursor_column) = match frame.cursor {
                Some(offset) => extent(&text.chars().take(offset).collect::<String>(), columns),
                None => extent(&text, columns),
            };
            if total_rows > cursor_rows {
                out.push_str(&format!("\x1b[{}A", total_rows - cursor_rows));
            }
            out.push('\r');
            if cursor_column > 0 {
                out.push_str(&format!("\x1b[{cursor_column}C"));
            }
            self.rows_above_cursor = cursor_rows - 1;
        }
        self.write(&out);
    }

    fn header(spec: &PromptSpec<'_>) -> Frame {
        let mut frame = Frame::default();
        frame.push(Style::Plain, spec.qmark.clone());
        frame.push(Style::Question, format!(" {} ", spec.message));
        frame
    }

    fn finish(&mut self, mut frame: Frame, answer: &str) {
        frame.push(Style::Answer, answer);
        self.draw(&frame, true);
    }

    fn next_key(&mut self) -> Result<Key, PromptError> {
        match self.keys.read() {
            None | Some(Key::CtrlD) => Err(PromptError::Eof),
            Some(Key::CtrlC) => Err(PromptError::Interrupted),
            Some(key) => Ok(key),
        }
    }

    fn ask_confirm(&mut self, mut frame: Frame, default: bool) -> Result<bool, PromptError> {
        let mut prompt = Frame {
            spans: frame.spans.clone(),
            cursor: None,
        };
        prompt.push(Style::Hint, if default { "(Y/n)" } else { "(y/N)" });
        self.draw(&prompt, false);
        let answer = loop {
            match self.next_key()? {
                Key::Char('y' | 'Y') => break true,
                Key::Char('n' | 'N') => break false,
                Key::Enter => break default,
                _ => {}
            }
        };
        frame.cursor = None;
        self.finish(frame, if answer { "Yes" } else { "No" });
        Ok(answer)
    }

    fn ask_text(&mut self, spec: &PromptSpec<'_>) -> Result<Value, PromptError> {
        let masked = spec.kind == PromptKind::Password;
        let mut buffer: Vec<char> = match &spec.default {
            DefaultRendered::Text(text) => text.chars().collect(),
            _ => Vec::new(),
        };
        let mut cursor = buffer.len();
        let mut error: Option<String> = None;
        let mut header = Self::header(spec);
        if spec.multiline {
            header.push(Style::Hint, MULTILINE_HINT);
        }
        let shown = |buffer: &[char]| -> String {
            if masked {
                "*".repeat(buffer.len())
            } else {
                buffer.iter().collect()
            }
        };
        loop {
            let mut frame = Frame {
                spans: header.spans.clone(),
                cursor: None,
            };
            let prefix_chars = frame.text().chars().count();
            if buffer.is_empty() && !spec.placeholder.is_empty() {
                frame.push(Style::Hint, spec.placeholder.clone());
            } else {
                frame.push(Style::Plain, shown(&buffer));
            }
            frame.cursor = Some(prefix_chars + cursor);
            if let Some(message) = &error {
                frame.push(Style::Error, format!("\n{message}"));
            }
            self.draw(&frame, false);
            let key = self.next_key()?;
            let submit = match key {
                Key::Enter if spec.multiline => {
                    buffer.insert(cursor, '\n');
                    cursor += 1;
                    false
                }
                Key::Enter | Key::AltEnter => true,
                Key::Char(ch) => {
                    buffer.insert(cursor, ch);
                    cursor += 1;
                    false
                }
                Key::Backspace => {
                    if cursor > 0 {
                        cursor -= 1;
                        buffer.remove(cursor);
                    }
                    false
                }
                Key::AltBackspace => {
                    let end = cursor;
                    while cursor > 0 && buffer[cursor - 1].is_whitespace() {
                        cursor -= 1;
                    }
                    while cursor > 0 && !buffer[cursor - 1].is_whitespace() {
                        cursor -= 1;
                    }
                    buffer.drain(cursor..end);
                    false
                }
                Key::Delete => {
                    if cursor < buffer.len() {
                        buffer.remove(cursor);
                    }
                    false
                }
                Key::Left => {
                    cursor = cursor.saturating_sub(1);
                    false
                }
                Key::Right => {
                    cursor = (cursor + 1).min(buffer.len());
                    false
                }
                Key::Home => {
                    cursor = 0;
                    false
                }
                Key::End => {
                    cursor = buffer.len();
                    false
                }
                Key::Tab if spec.kind == PromptKind::Path => {
                    let completed = complete_path(&buffer.iter().collect::<String>());
                    buffer = completed.chars().collect();
                    cursor = buffer.len();
                    false
                }
                _ => false,
            };
            let text: String = buffer.iter().collect();
            error = spec.validate(&Value::Str(text.clone())).err();
            if submit && error.is_none() {
                self.finish(
                    Frame {
                        spans: header.spans,
                        cursor: None,
                    },
                    &shown(&buffer),
                );
                return Ok(Value::Str(text));
            }
        }
    }

    fn ask_choices(&mut self, spec: &PromptSpec<'_>) -> Result<Value, PromptError> {
        let multiple = spec.kind == PromptKind::Checkbox;
        let enabled = |choice: &PromptChoice| choice.disabled.is_empty();
        let mut checked: Vec<bool> = spec
            .choices
            .iter()
            .map(|choice| choice.checked && enabled(choice))
            .collect();
        let default_index = match &spec.default {
            DefaultRendered::Choice(default) => spec
                .choices
                .iter()
                .position(|choice| choice.value == default.value),
            _ => None,
        };
        let mut pointer = default_index
            .filter(|index| enabled(&spec.choices[*index]))
            .or_else(|| spec.choices.iter().position(enabled))
            .unwrap_or(0);
        let mut error: Option<String> = None;
        let header = Self::header(spec);
        let step = |from: usize, forward: bool| -> usize {
            let count = spec.choices.len();
            let mut index = from;
            for _ in 0..count {
                index = if forward {
                    (index + 1) % count
                } else {
                    (index + count - 1) % count
                };
                if enabled(&spec.choices[index]) {
                    return index;
                }
            }
            from
        };
        loop {
            let mut frame = Frame {
                spans: header.spans.clone(),
                cursor: None,
            };
            frame.push(
                Style::Hint,
                if multiple { CHECKBOX_HINT } else { SELECT_HINT },
            );
            for (index, choice) in spec.choices.iter().enumerate() {
                let marker = if index == pointer { POINTER } else { " " };
                if !enabled(choice) {
                    frame.push(
                        Style::Disabled,
                        format!("\n   - {} ({})", choice.name, choice.disabled),
                    );
                } else if multiple {
                    let mark = if checked[index] { CHECKED } else { UNCHECKED };
                    frame.push(Style::Plain, format!("\n {marker} {mark} {}", choice.name));
                } else {
                    frame.push(Style::Plain, format!("\n {marker} {}", choice.name));
                }
            }
            if let Some(message) = &error {
                frame.push(Style::Error, format!("\n{message}"));
            }
            self.draw(&frame, false);
            if spec.choices.is_empty() {
                return Err(PromptError::Eof);
            }
            match self.next_key()? {
                Key::Up | Key::Char('k') => pointer = step(pointer, false),
                Key::Down | Key::Char('j') => pointer = step(pointer, true),
                Key::Char(' ') if multiple && enabled(&spec.choices[pointer]) => {
                    checked[pointer] = !checked[pointer]
                }
                Key::Char('a') if multiple => {
                    let all = spec
                        .choices
                        .iter()
                        .zip(&checked)
                        .all(|(choice, checked)| *checked || !enabled(choice));
                    for (index, choice) in spec.choices.iter().enumerate() {
                        checked[index] = !all && enabled(choice);
                    }
                }
                Key::Char('i') if multiple => {
                    for (index, choice) in spec.choices.iter().enumerate() {
                        checked[index] = !checked[index] && enabled(choice);
                    }
                }
                Key::Enter if multiple => {
                    let selected: Vec<&PromptChoice> = spec
                        .choices
                        .iter()
                        .zip(&checked)
                        .filter(|(_, checked)| **checked)
                        .map(|(choice, _)| choice)
                        .collect();
                    let answer =
                        Value::Seq(selected.iter().map(|choice| choice.value.clone()).collect());
                    match spec.validate(&answer) {
                        Err(message) => error = Some(message),
                        Ok(()) => {
                            let summary = match selected.as_slice() {
                                [only] => format!("[{}]", only.name),
                                many => format!("done ({} selections)", many.len()),
                            };
                            self.finish(
                                Frame {
                                    spans: header.spans,
                                    cursor: None,
                                },
                                &summary,
                            );
                            return Ok(answer);
                        }
                    }
                }
                Key::Enter if enabled(&spec.choices[pointer]) => {
                    let choice = &spec.choices[pointer];
                    self.finish(
                        Frame {
                            spans: header.spans,
                            cursor: None,
                        },
                        &choice.name,
                    );
                    return Ok(choice.value.clone());
                }
                _ => {}
            }
        }
    }
}

impl Prompter for TerminalPrompter {
    fn ask(&mut self, spec: &PromptSpec<'_>) -> Result<Value, PromptError> {
        let _raw = RawMode::enter(self.raw_input);
        self.rows_above_cursor = 0;
        match spec.kind {
            PromptKind::Confirm => {
                let default = matches!(spec.default, DefaultRendered::Bool(true));
                self.ask_confirm(Self::header(spec), default)
                    .map(Value::Bool)
            }
            PromptKind::Select | PromptKind::Checkbox => self.ask_choices(spec),
            PromptKind::Input | PromptKind::Password | PromptKind::Path => self.ask_text(spec),
        }
    }

    fn confirm(&mut self, message: &str, default: bool) -> Result<bool, PromptError> {
        let _raw = RawMode::enter(self.raw_input);
        self.rows_above_cursor = 0;
        let mut frame = Frame::default();
        frame.push(Style::Plain, "?");
        frame.push(Style::Question, format!(" {message} "));
        self.ask_confirm(frame, default)
    }
}

/// Complete a partially typed filesystem path as far as it is unambiguous.
fn complete_path(typed: &str) -> String {
    let expanded = crate::dirs::expand_user(typed);
    let (dir, prefix) = match expanded.rfind('/') {
        Some(index) => (&expanded[..=index], &expanded[index + 1..]),
        None => ("", expanded.as_str()),
    };
    let Ok(entries) = std::fs::read_dir(if dir.is_empty() { "." } else { dir }) else {
        return typed.to_owned();
    };
    let mut matches: Vec<(String, bool)> = entries
        .flatten()
        .map(|entry| {
            (
                entry.file_name().to_string_lossy().into_owned(),
                entry.path().is_dir(),
            )
        })
        .filter(|(name, _)| name.starts_with(prefix))
        .collect();
    matches.sort();
    match matches.as_slice() {
        [] => typed.to_owned(),
        [(name, is_dir)] => format!("{dir}{name}{}", if *is_dir { "/" } else { "" }),
        many => {
            let first = &many[0].0;
            let common = many.iter().fold(first.len(), |length, (name, _)| {
                first
                    .chars()
                    .zip(name.chars())
                    .take(length)
                    .take_while(|(a, b)| a == b)
                    .count()
            });
            format!("{dir}{}", first.chars().take(common).collect::<String>())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extent_counts_rows_and_columns() {
        assert_eq!(extent("abc", 80), (1, 3));
        assert_eq!(extent("ab\ncd\n", 80), (3, 0));
        assert_eq!(extent("abcdef", 4), (2, 2));
        assert_eq!(extent("abcd", 4), (1, 4));
    }

    #[test]
    fn path_completion() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("alpha")).unwrap();
        std::fs::write(dir.path().join("alpine.txt"), "").unwrap();
        std::fs::write(dir.path().join("beta.txt"), "").unwrap();
        let base = dir.path().to_string_lossy().into_owned();
        assert_eq!(
            complete_path(&format!("{base}/b")),
            format!("{base}/beta.txt")
        );
        assert_eq!(complete_path(&format!("{base}/a")), format!("{base}/alp"));
        assert_eq!(
            complete_path(&format!("{base}/alpha")),
            format!("{base}/alpha/")
        );
        assert_eq!(complete_path(&format!("{base}/zzz")), format!("{base}/zzz"));
    }
}
