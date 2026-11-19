//! Argument parsing with plumbum's rules: switches may appear anywhere, long
//! switches take `--name value` or `--name=value`, short ones `-n value` or
//! `-nvalue`, short flags combine (`-fq`), and `--` ends the switches.

use std::collections::HashMap;
use std::path::Path;

/// How a switch consumes its argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Flag,
    Value,
    List,
    Integer,
    ExistingFile,
    Choice(&'static [&'static str]),
}

/// One switch and all of its spellings; the last name is the canonical one.
#[derive(Debug, Clone, Copy)]
pub(super) struct Switch {
    names: &'static [&'static str],
    kind: Kind,
}

impl Switch {
    pub(super) const fn flag(names: &'static [&'static str]) -> Self {
        Self {
            names,
            kind: Kind::Flag,
        }
    }

    pub(super) const fn value(names: &'static [&'static str]) -> Self {
        Self {
            names,
            kind: Kind::Value,
        }
    }

    pub(super) const fn list(names: &'static [&'static str]) -> Self {
        Self {
            names,
            kind: Kind::List,
        }
    }

    pub(super) const fn integer(names: &'static [&'static str]) -> Self {
        Self {
            names,
            kind: Kind::Integer,
        }
    }

    pub(super) const fn existing_file(names: &'static [&'static str]) -> Self {
        Self {
            names,
            kind: Kind::ExistingFile,
        }
    }

    pub(super) const fn choice(
        names: &'static [&'static str],
        choices: &'static [&'static str],
    ) -> Self {
        Self {
            names,
            kind: Kind::Choice(choices),
        }
    }

    fn canonical(&self) -> &'static str {
        self.names[self.names.len() - 1]
    }
}

/// A sub-command: its switches, help text and how many positionals it takes.
pub(super) struct Spec {
    pub(super) name: &'static str,
    pub(super) help: &'static str,
    pub(super) switches: &'static [&'static [Switch]],
    /// Minimum and maximum number of positional arguments.
    pub(super) positionals: (usize, usize),
}

/// Why parsing stopped before producing a command to run.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum UsageError {
    /// `-h` / `--help`.
    Help,
    /// `--help-all`.
    HelpAll,
    /// `-v` / `--version`.
    Version,
    /// A malformed command line, with plumbum's message.
    Invalid(String),
}

/// The switches and positionals of one command line, keyed by canonical name.
#[derive(Debug, Default)]
pub(super) struct Parsed {
    values: HashMap<&'static str, Vec<String>>,
    pub(super) positionals: Vec<String>,
}

impl Parsed {
    /// Whether a flag was given.
    pub(super) fn flag(&self, name: &str) -> bool {
        self.values.contains_key(name)
    }

    /// The last value given for a switch.
    pub(super) fn value(&self, name: &str) -> Option<&str> {
        self.values
            .get(name)
            .and_then(|values| values.last())
            .map(String::as_str)
    }

    /// Every value given for a repeatable switch.
    pub(super) fn list(&self, name: &str) -> Vec<String> {
        self.values.get(name).cloned().unwrap_or_default()
    }
}

fn py_list(items: &[String]) -> String {
    let quoted: Vec<String> = items
        .iter()
        .map(|item| crate::value::py_str_repr(item))
        .collect();
    format!("[{}]", quoted.join(", "))
}

fn invalid<T>(message: String) -> Result<T, UsageError> {
    Err(UsageError::Invalid(message))
}

/// Check a switch argument against its type; `given` is the name as typed, without dashes.
fn validate(kind: Kind, given: &str, value: &str) -> Result<(), UsageError> {
    let quoted = crate::value::py_str_repr(value);
    match kind {
        Kind::Integer if crate::tools::py_int(value).is_none() => invalid(format!(
            "Argument of {given} expected to be <class 'int'>, not {quoted}:\n    ValueError(\"invalid literal for int() with base 10: {quoted}\")"
        )),
        Kind::ExistingFile if !Path::new(value).is_file() => invalid(format!(
            "Argument of {given} expected to be ExistingFile, not {quoted}:\n    ValueError('{value} is not a file')"
        )),
        Kind::Choice(choices) if !choices.contains(&value) => {
            let expected: Vec<String> = choices.iter().map(|choice| format!("'{choice}'")).collect();
            invalid(format!(
                "Argument of {given} expected to be {{{}}}, not {quoted}:\n    ValueError(\"Invalid value: {value} (Expected one of ({}))\")",
                choices.join(", "),
                expected.join(", ")
            ))
        }
        _ => Ok(()),
    }
}

/// Parse the arguments that follow the sub-command name.
pub(super) fn parse(spec: &Spec, args: &[String]) -> Result<Parsed, UsageError> {
    let find = |name: &str| {
        spec.switches
            .iter()
            .flat_map(|group| group.iter())
            .find(|switch| switch.names.contains(&name))
    };
    let mut parsed = Parsed::default();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if arg == "--" {
            parsed.positionals.extend(args.cloned());
            break;
        }
        // One token can hold several short flags; each round handles one switch.
        let mut token = arg.clone();
        loop {
            if token == "-" || !token.starts_with('-') {
                parsed.positionals.push(token);
                break;
            }
            let (name, attached, remaining_flags) = if token.starts_with("--") {
                match token.split_once('=') {
                    Some((name, value)) => (name.to_owned(), Some(value.to_owned()), None),
                    None => (token.clone(), None, None),
                }
            } else {
                let mut chars = token.chars().skip(1);
                let letter = chars.next().unwrap_or('-');
                let rest: String = chars.collect();
                (
                    format!("-{letter}"),
                    None,
                    (!rest.is_empty()).then_some(rest),
                )
            };
            match name.as_str() {
                "-h" | "--help" => return Err(UsageError::Help),
                "--help-all" => return Err(UsageError::HelpAll),
                "-v" | "--version" => return Err(UsageError::Version),
                _ => {}
            }
            let Some(switch) = find(&name) else {
                return invalid(format!("Unknown switch {name}"));
            };
            if switch.kind == Kind::Flag {
                if attached.is_some() {
                    return invalid(format!("Switch {name} does not take an argument"));
                }
                parsed.values.entry(switch.canonical()).or_default();
                match remaining_flags {
                    Some(rest) => {
                        token = format!("-{rest}");
                        continue;
                    }
                    None => break,
                }
            }
            // What follows a short switch in the same token is its argument.
            let value = match attached.or(remaining_flags) {
                Some(value) => value,
                None => match args.next() {
                    Some(value) => value.clone(),
                    None => return invalid(format!("Switch {name} requires an argument")),
                },
            };
            validate(switch.kind, name.trim_start_matches('-'), &value)?;
            parsed
                .values
                .entry(switch.canonical())
                .or_default()
                .push(value);
            break;
        }
    }
    let (minimum, maximum) = spec.positionals;
    if parsed.positionals.len() < minimum {
        return invalid(format!(
            "Expected at least {minimum} positional argument{}, got {}",
            if minimum == 1 { "" } else { "s" },
            py_list(&parsed.positionals)
        ));
    }
    if parsed.positionals.len() > maximum {
        return invalid(format!(
            "Expected at most {maximum} positional argument{}, got {}",
            if maximum == 1 { "" } else { "s" },
            py_list(&parsed.positionals)
        ));
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: Spec = Spec {
        name: "demo",
        help: "",
        switches: &[&[
            Switch::flag(&["-f", "--force"]),
            Switch::flag(&["-q", "--quiet"]),
            Switch::list(&["-d", "--data"]),
            Switch::value(&["-r", "--vcs-ref"]),
            Switch::integer(&["-c", "--context-lines"]),
            Switch::choice(&["-o", "--conflict"], &["rej", "inline"]),
        ]],
        positionals: (1, 2),
    };

    fn run(args: &[&str]) -> Result<Parsed, UsageError> {
        parse(
            &SPEC,
            &args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>(),
        )
    }

    fn message(args: &[&str]) -> String {
        match run(args) {
            Err(UsageError::Invalid(message)) => message,
            other => panic!("expected a usage error, got {other:?}"),
        }
    }

    #[test]
    fn switch_forms() {
        let parsed = run(&[
            "-fq",
            "src",
            "-d",
            "a=1",
            "--data=b=2",
            "-rHEAD",
            "dst",
            "--context-lines",
            "5",
        ])
        .unwrap();
        assert!(parsed.flag("--force") && parsed.flag("--quiet"));
        assert_eq!(parsed.list("--data"), ["a=1", "b=2"]);
        assert_eq!(parsed.value("--vcs-ref"), Some("HEAD"));
        assert_eq!(parsed.value("--context-lines"), Some("5"));
        assert_eq!(parsed.positionals, ["src", "dst"]);
        assert_eq!(run(&["--", "-x"]).unwrap().positionals, ["-x"]);
    }

    #[test]
    fn usage_errors_match_plumbum() {
        assert_eq!(
            message(&[]),
            "Expected at least 1 positional argument, got []"
        );
        assert_eq!(
            message(&["a", "b", "c"]),
            "Expected at most 2 positional arguments, got ['a', 'b', 'c']"
        );
        assert_eq!(message(&["--nope", "a"]), "Unknown switch --nope");
        assert_eq!(message(&["a", "-d"]), "Switch -d requires an argument");
        assert_eq!(
            message(&["--quiet=1", "a"]),
            "Switch --quiet does not take an argument"
        );
        assert_eq!(
            message(&["-o", "bad", "a"]),
            "Argument of o expected to be {rej, inline}, not 'bad':\n    ValueError(\"Invalid value: bad (Expected one of ('rej', 'inline'))\")"
        );
        assert_eq!(
            message(&["--context-lines", "x", "a"]),
            "Argument of context-lines expected to be <class 'int'>, not 'x':\n    ValueError(\"invalid literal for int() with base 10: 'x'\")"
        );
        assert_eq!(run(&["--help"]).unwrap_err(), UsageError::Help);
    }
}
