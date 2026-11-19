//! Rust port of [Copier](https://github.com/copier-org/copier), a library and CLI
//! for rendering project templates and keeping generated projects up to date.

#![warn(clippy::unwrap_used, missing_docs)]

pub mod cli;
mod dirs;
pub mod errors;
pub mod jinja;
pub mod json;
pub mod output;
pub mod pathspec;
pub mod prompt;
pub mod questions;
mod scoped;
pub mod settings;
pub mod subproject;
pub mod template;
pub mod tools;
pub mod value;
pub mod vcs;
pub mod version;
pub mod worker;
pub mod yaml;

pub use errors::{CopierError, Result};
pub use questions::load_answersfile_data;
pub use settings::{load_settings, Settings};
pub use value::Value;
pub use worker::{get_update_data, ConflictMode, Phase, UpdateData, VcsRef, Worker};

/// Version of Python Copier whose behavior this port reproduces; templates compare
/// `_min_copier_version` against it.
pub const COPIER_VERSION: &str = "9.19.1";
