//! Debug logging infrastructure for compiler development.
//!
//! This module provides a thread-local debug log that can be used to collect
//! debug messages during compilation, which are then displayed in onionskin.
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

/// Push a debug message to the thread-local log.
/// This is typically called via the `baml_debug!` macro.
#[cfg(debug_assertions)]
pub fn push_debug(module: &'static str, msg: String) {
    DEBUG_LOG.with(|log| {
        log.borrow_mut().push(DebugMessage {
            module,
            message: msg,
        });
    });
}

/// Stub for release builds - does nothing.
#[cfg(not(debug_assertions))]
pub fn push_debug(_module: &'static str, _msg: String) {}

/// Drain all debug messages from the thread-local log.
/// Returns the messages and clears the log.
pub fn drain_debug_log() -> Vec<DebugMessage> {
    DEBUG_LOG.with(|log| log.borrow_mut().drain(..).collect())
}

/// Check if there are any debug messages pending.
pub fn has_debug_messages() -> bool {
    DEBUG_LOG.with(|log| !log.borrow().is_empty())
}

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
