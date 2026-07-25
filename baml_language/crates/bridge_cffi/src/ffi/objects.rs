//! Object operations FFI entry points.

/// Flush the event sink. No-op: tracing/event production has been removed.
#[unsafe(no_mangle)]
pub extern "C" fn flush_events() {}
