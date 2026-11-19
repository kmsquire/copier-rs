//! YAML input and output compatible with PyYAML, which Python Copier uses.
//!
//! Loading follows PyYAML's `safe_load` exactly (YAML 1.1 scalar resolution, merge
//! keys, duplicate keys), because it decides answer values. Dumping follows PyYAML's
//! layout and quoting for ordinary data but does not fold long lines.

mod emitter;
mod loader;
mod resolver;

pub use emitter::{dump, DumpOptions, FlowStyle};
pub use loader::{decode, list_item_sources, load, load_all, load_all_with, LoadError, TaggedNode, YamlError};
