//! Debug logging infrastructure for compiler development.
//!
//! This module provides a thread-local debug log that can be used to collect
//! debug messages during compilation for development tools and diagnostics.
//!
//! In release builds, all debug logging is compiled away to nothing.

use std::cell::RefCell;

/// A debug message with its source module path.
#[derive(Debug, Clone)]
pub struct DebugMessage {
    /// The module path where this message originated (e.g., "`baml_compiler_tir::infer`")
    pub module: &'static str,
    /// The actual debug message
    pub message: String,
}

thread_local! {
    static DEBUG_LOG: RefCell<Vec<DebugMessage>> = const { RefCell::new(Vec::new()) };
}

/// Stub for release builds - does nothing.
#[cfg(not(debug_assertions))]
pub fn push_debug(_module: &'static str, _msg: String) {}

/// Debug logging macro that automatically captures the crate/module path.
///
/// In release builds, this compiles to nothing (zero cost).
///
/// # Example
///
/// ```ignore
/// use baml_base::baml_debug;
///
/// fn resolve_field(ty: &str, field: &str) {
///     baml_debug!("Resolving {}.{}", ty, field);
/// }
/// ```
#[macro_export]
macro_rules! baml_debug {
    ($($arg:tt)*) => {
        #[cfg(debug_assertions)]
        {
            $crate::debug_log::push_debug(module_path!(), format!($($arg)*))
        }
    };
}
