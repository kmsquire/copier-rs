//! Decoding of terminal key presses from a byte stream.
//!
//! Works on any byte source, so the same code serves a real terminal in raw mode
//! and scripted input arriving through a pipe.

use std::io::Read;

/// One key press.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Key {
    Char(char),
    Enter,
    /// Alt+Enter, or Escape followed by Enter.
    AltEnter,
    Backspace,
    /// Alt+Backspace: delete the previous word.
    AltBackspace,
    Delete,
    Tab,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    CtrlC,
    CtrlD,
    /// A sequence this decoder does not know; ignored by prompts.
    Unknown,
}

/// Reads keys from a byte source; `None` means the input ended.
pub(super) struct KeyReader<R> {
    input: R,
}

impl<R: Read> KeyReader<R> {
    pub(super) fn new(input: R) -> Self {
        Self { input }
    }

    fn byte(&mut self) -> Option<u8> {
        let mut buffer = [0u8; 1];
        loop {
            match self.input.read(&mut buffer) {
                Ok(0) => return None,
                Ok(_) => return Some(buffer[0]),
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => return None,
            }
        }
    }

    /// The next key press.
    pub(super) fn read(&mut self) -> Option<Key> {
        let first = self.byte()?;
        Some(match first {
            b'\r' | b'\n' => Key::Enter,
            0x7f | 0x08 => Key::Backspace,
            b'\t' => Key::Tab,
            0x03 => Key::CtrlC,
            0x04 => Key::CtrlD,
            0x01 => Key::Home,
            0x05 => Key::End,
            0x1b => self.escape_sequence()?,
            byte if byte < 0x20 => Key::Unknown,
            byte => self.utf8_char(byte),
        })
    }

    fn escape_sequence(&mut self) -> Option<Key> {
        Some(match self.byte()? {
            b'\r' | b'\n' => Key::AltEnter,
            0x7f | 0x08 => Key::AltBackspace,
            b'[' | b'O' => {
                // CSI sequences end with a byte in `@`..=`~`; parameters come before.
                let mut parameters = Vec::new();
                let last = loop {
                    let byte = self.byte()?;
                    if (0x40..=0x7e).contains(&byte) {
                        break byte;
                    }
                    parameters.push(byte);
                };
                match (last, parameters.as_slice()) {
                    (b'A', _) => Key::Up,
                    (b'B', _) => Key::Down,
                    (b'C', _) => Key::Right,
                    (b'D', _) => Key::Left,
                    (b'H', _) | (b'~', b"1" | b"7") => Key::Home,
                    (b'F', _) | (b'~', b"4" | b"8") => Key::End,
                    (b'~', b"3") => Key::Delete,
                    _ => Key::Unknown,
                }
            }
            _ => Key::Unknown,
        })
    }

    fn utf8_char(&mut self, first: u8) -> Key {
        let extra = match first {
            0xc0..=0xdf => 1,
            0xe0..=0xef => 2,
            0xf0..=0xf7 => 3,
            _ => 0,
        };
        let mut bytes = vec![first];
        for _ in 0..extra {
            match self.byte() {
                Some(byte) => bytes.push(byte),
                None => return Key::Unknown,
            }
        }
        std::str::from_utf8(&bytes)
            .ok()
            .and_then(|text| text.chars().next())
            .map_or(Key::Unknown, Key::Char)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(input: &str) -> Vec<Key> {
        let mut reader = KeyReader::new(input.as_bytes());
        std::iter::from_fn(|| reader.read()).collect()
    }

    #[test]
    fn decodes_plain_and_escape_sequences() {
        assert_eq!(
            keys("aé\r\x1b[A\x1b[B\x1b\r\x1b\x7f\x08\t\x03 \x1b[3~\x1b[H"),
            [
                Key::Char('a'),
                Key::Char('é'),
                Key::Enter,
                Key::Up,
                Key::Down,
                Key::AltEnter,
                Key::AltBackspace,
                Key::Backspace,
                Key::Tab,
                Key::CtrlC,
                Key::Char(' '),
                Key::Delete,
                Key::Home,
            ]
        );
        assert_eq!(keys(""), []);
    }
}
