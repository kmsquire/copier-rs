//! Command-line interface, mirroring `copier/_cli.py`.
//!
//! Argument syntax, help texts and usage errors follow plumbum, the library the
//! Python CLI is built on, so existing scripts and muscle memory keep working.

mod args;

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use args::{parse, Parsed, Spec, Switch, UsageError};

use crate::errors::{CopierError, Result};
use crate::output;
use crate::value::{Mapping, Value};
use crate::worker::{get_update_data, ConflictMode, Worker};
use crate::{json, yaml};

const MAIN_HELP: &str = include_str!("help/main.txt");
const ALL_HELP: &str = include_str!("help/all.txt");
const BASH_COMPLETIONS: &str = include_str!("help/completions.bash");
const FISH_COMPLETIONS: &str = include_str!("help/completions.fish");

/// Switches every project command accepts.
const COMMON: &[Switch] = &[
    Switch::value(&["-a", "--answers-file"]),
    Switch::list(&["-x", "--exclude"]),
    Switch::value(&["-r", "--vcs-ref"]),
    Switch::flag(&["-n", "--pretend"]),
    Switch::list(&["-s", "--skip"]),
    Switch::flag(&["-q", "--quiet"]),
    Switch::flag(&["-g", "--prereleases"]),
    Switch::flag(&["--UNSAFE", "--trust"]),
    Switch::flag(&["-T", "--skip-tasks"]),
    Switch::list(&["-d", "--data"]),
    Switch::existing_file(&["--data-file"]),
];

const COPY: Spec = Spec {
    name: "copy",
    help: include_str!("help/copy.txt"),
    switches: &[
        COMMON,
        &[
            Switch::flag(&["-C", "--no-cleanup"]),
            Switch::flag(&["-l", "--defaults"]),
            Switch::flag(&["-f", "--force"]),
            Switch::flag(&["-w", "--overwrite"]),
            Switch::list(&["--ask"]),
        ],
    ],
    positionals: (2, 2),
};

const RECOPY: Spec = Spec {
    name: "recopy",
    help: include_str!("help/recopy.txt"),
    switches: &[
        COMMON,
        &[
            Switch::flag(&["-l", "--defaults"]),
            Switch::flag(&["-f", "--force"]),
            Switch::flag(&["-w", "--overwrite"]),
            Switch::flag(&["-A", "--skip-answered"]),
            Switch::list(&["--ask"]),
        ],
    ],
    positionals: (0, 1),
};

const UPDATE: Spec = Spec {
    name: "update",
    help: include_str!("help/update.txt"),
    switches: &[
        COMMON,
        &[
            Switch::choice(&["-o", "--conflict"], &["rej", "inline"]),
            Switch::integer(&["-c", "--context-lines"]),
            Switch::flag(&["-l", "-f", "--defaults"]),
            Switch::flag(&["-A", "--skip-answered"]),
            Switch::list(&["--ask"]),
        ],
    ],
    positionals: (0, 1),
};

const CHECK_UPDATE: Spec = Spec {
    name: "check-update",
    help: include_str!("help/check-update.txt"),
    switches: &[&[
        Switch::value(&["-a", "--answers-file"]),
        Switch::flag(&["-q", "--quiet"]),
        Switch::flag(&["-g", "--prereleases"]),
        Switch::choice(&["--output-format"], &["plain", "json"]),
    ]],
    positionals: (0, 1),
};

fn help(text: &str) -> String {
    text.replace("{version}", crate::COPIER_VERSION)
}

fn version_line() -> String {
    format!(
        "copier {} (copier-rs {})",
        crate::COPIER_VERSION,
        env!("CARGO_PKG_VERSION")
    )
}

fn usage_error(message: &str, help_text: &str) -> i32 {
    output::print(&format!("Error: {message}\n------\n{}", help(help_text)));
    2
}

fn completions(shell: Option<&str>) -> i32 {
    match shell {
        Some("bash") => output::print(BASH_COMPLETIONS),
        Some("fish") => output::print(FISH_COMPLETIONS),
        Some(other) => {
            return usage_error(
                &format!(
                    "Argument of completions expected to be {{bash, fish}}, not '{other}':\n    ValueError(\"Invalid value: {other} (Expected one of ('bash', 'fish'))\")"
                ),
                MAIN_HELP,
            );
        }
        None => return usage_error("Switch --completions requires an argument", MAIN_HELP),
    }
    0
}

/// Run the CLI with `args` (including the program name) and return the exit code.
///
/// Never exits the process, so tests can call it in-process and capture its output
/// with [`crate::output::capture`].
pub fn run<I, T>(args: I) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let args: Vec<String> = args
        .into_iter()
        .skip(1)
        .map(|arg| arg.into().to_string_lossy().into_owned())
        .collect();
    let Some((first, rest)) = args.split_first() else {
        output::print(&format!(
            "No sub-command given\n------\n{}",
            help(MAIN_HELP)
        ));
        return 1;
    };
    let spec = match first.as_str() {
        "-h" | "--help" => {
            output::print(&help(MAIN_HELP));
            return 0;
        }
        "--help-all" => {
            output::print(&help(ALL_HELP));
            return 0;
        }
        "-v" | "--version" => {
            output::println(&version_line());
            return 0;
        }
        "--completions" => return completions(rest.first().map(String::as_str)),
        "copy" => &COPY,
        "recopy" => &RECOPY,
        "update" => &UPDATE,
        "check-update" => &CHECK_UPDATE,
        switch if switch.starts_with('-') => {
            return match switch.strip_prefix("--completions=") {
                Some(shell) => completions(Some(shell)),
                None => usage_error(&format!("Unknown switch {switch}"), MAIN_HELP),
            };
        }
        other => {
            output::print(&format!(
                "Unknown sub-command '{other}'\n------\n{}",
                help(MAIN_HELP)
            ));
            return 1;
        }
    };
    let parsed = match parse(spec, rest) {
        Ok(parsed) => parsed,
        Err(UsageError::Help) => {
            output::print(&help(spec.help));
            return 0;
        }
        Err(UsageError::HelpAll) => {
            output::print(&help(ALL_HELP));
            return 0;
        }
        Err(UsageError::Version) => {
            output::println(&format!("copier {} {}", spec.name, crate::COPIER_VERSION));
            return 0;
        }
        Err(UsageError::Invalid(message)) => return usage_error(&message, spec.help),
    };
    let outcome = if spec.name == "check-update" {
        check_update(&parsed)
    } else {
        project_command(spec.name, &parsed)
    };
    match outcome {
        Ok(code) => code,
        Err(error) => report(&error),
    }
}

/// Print an error the way the Python CLI does and return its exit code.
fn report(error: &CopierError) -> i32 {
    // Python shows a traceback for anything but these two families; the message
    // is what a user can act on.
    output::eprintln(&output::style::error(&error.to_string()));
    error.exit_code()
}

/// Build the up-front answers from `--data` and `--data-file`; `--data` wins.
fn collect_data(parsed: &Parsed) -> Result<Mapping> {
    let mut data = Mapping::new();
    for item in &parsed.list("--data") {
        let (key, value) = item.split_once('=').ok_or_else(|| {
            CopierError::Value(format!(
                "--data expects VARIABLE=VALUE, got {}",
                crate::value::py_str_repr(item)
            ))
        })?;
        data.insert(Value::from(key), Value::from(value));
    }
    if let Some(path) = parsed.value("--data-file") {
        let text = yaml::decode(&std::fs::read(path)?)
            .map_err(|err| CopierError::Value(err.to_string()))?;
        if let Value::Map(file_data) =
            yaml::load(&text).map_err(|err| CopierError::Value(err.to_string()))?
        {
            for (key, value) in file_data {
                data.entry(key).or_insert(value);
            }
        }
    }
    Ok(data)
}

fn project_command(name: &str, parsed: &Parsed) -> Result<i32> {
    let force = parsed.flag("--force");
    let mut worker = Worker::new()
        .data(Value::Map(collect_data(parsed)?))
        .exclude(parsed.list("--exclude"))
        .use_prereleases(parsed.flag("--prereleases"))
        .skip_if_exists(parsed.list("--skip"))
        .defaults(force || parsed.flag("--defaults"))
        .pretend(parsed.flag("--pretend"))
        .quiet(parsed.flag("--quiet"))
        .unsafe_(parsed.flag("--trust"))
        .skip_tasks(parsed.flag("--skip-tasks"))
        .ask(parsed.list("--ask"))
        .prompter(Box::new(crate::prompt::terminal()));
    if let Some(answers_file) = parsed.value("--answers-file") {
        worker = worker.answers_file(answers_file);
    }
    if let Some(vcs_ref) = parsed.value("--vcs-ref") {
        worker = worker.vcs_ref(vcs_ref);
    }
    match name {
        "copy" => {
            worker
                .src_path(&parsed.positionals[0])
                .dst_path(&parsed.positionals[1])
                .cleanup_on_error(!parsed.flag("--no-cleanup"))
                .overwrite(force || parsed.flag("--overwrite"))
                .run_copy()?;
        }
        "recopy" => {
            worker
                .dst_path(destination(parsed))
                .overwrite(force || parsed.flag("--overwrite"))
                .skip_answered(parsed.flag("--skip-answered"))
                .run_recopy()?;
        }
        _ => {
            let conflict = match parsed.value("--conflict") {
                Some("rej") => ConflictMode::Rej,
                _ => ConflictMode::Inline,
            };
            let context_lines = parsed
                .value("--context-lines")
                .and_then(|lines| lines.trim().parse().ok())
                .unwrap_or(3);
            worker
                .dst_path(destination(parsed))
                .overwrite(true)
                .conflict(conflict)
                .context_lines(context_lines)
                .skip_answered(parsed.flag("--skip-answered"))
                .run_update()?;
        }
    }
    Ok(0)
}

fn destination(parsed: &Parsed) -> PathBuf {
    PathBuf::from(parsed.positionals.first().map_or(".", String::as_str))
}

fn check_update(parsed: &Parsed) -> Result<i32> {
    let update = get_update_data(
        &destination(parsed),
        parsed.value("--answers-file").map(Path::new),
        parsed.flag("--prereleases"),
    )?;
    let quiet = parsed.flag("--quiet");
    if update.update_available && quiet {
        return Ok(2);
    }
    if quiet {
        return Ok(0);
    }
    if parsed.value("--output-format") == Some("json") {
        let report = Value::map([
            ("update_available", Value::Bool(update.update_available)),
            ("current_version", Value::from(update.current_version)),
            ("latest_version", Value::from(update.latest_version)),
        ]);
        output::println(&json::dumps(&report, &json::DumpOptions::default()));
    } else if update.update_available {
        output::println(&format!(
            "New template version available.\nCurrent version is {}, latest version is {}.",
            update.current_version, update.latest_version
        ));
    } else {
        output::println("Project is up-to-date!");
    }
    Ok(0)
}
