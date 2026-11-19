//! Command-line interface, mirroring `copier/_cli.py`.

use std::ffi::OsString;

/// Run the CLI with `args` (including the program name) and return the exit code.
///
/// Never exits the process, so tests can call it in-process and capture its output
/// with [`crate::output::capture`].
pub fn run<I, T>(args: I) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let _args: Vec<OsString> = args.into_iter().map(Into::into).collect();
    todo!("cli")
}
