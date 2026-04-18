/// Log an info-level event with structured data.
/// Note: The compiler special-cases `log.info` to emit the `SendEvent`
/// VM instruction directly with event_name="log" and data={level:"info", data:...}.
// This is to preserve the source location of the call site.
function info(data: map<string, unknown>) -> null {
  null
}

/// Log a debug-level event with structured data.
/// Note: The compiler special-cases `log.debug` to emit the `SendEvent`
/// VM instruction directly with event_name="log" and data={level:"debug", data:...}.
// This is to preserve the source location of the call site.
function debug(data: map<string, unknown>) -> null {
  null
}

/// Log a warning-level event with structured data.
/// Note: The compiler special-cases `log.warn` to emit the `SendEvent`
/// VM instruction directly with event_name="log" and data={level:"warn", data:...}.
// This is to preserve the source location of the call site.
function warn(data: map<string, unknown>) -> null {
  null
}

/// Log an error-level event with structured data.
/// Note: The compiler special-cases `log.error` to emit the `SendEvent`
/// VM instruction directly with event_name="log" and data={level:"error", data:...}.
// This is to preserve the source location of the call site.
function error(data: map<string, unknown>) -> null {
  null
}
