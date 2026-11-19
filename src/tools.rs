//! Small helpers, mirroring `copier/_tools.py`.

use crate::value::Value;

/// Parse anything into a boolean the way Copier answers do.
///
/// Numbers are true when non-zero; `y/yes/t/true/on` and `n/no/f/false/off/~/null/none`
/// are recognized case-insensitively; anything else uses Python truthiness.
pub fn cast_to_bool(_value: &Value) -> bool {
    todo!("cast_to_bool")
}

/// Split NUL-delimited command output, dropping the trailing empty item.
pub fn split_nul(_text: &str) -> Vec<String> {
    todo!("split_nul")
}
