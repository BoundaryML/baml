/// Internal helper that constructs and sends a log event.
function _log(level: string, message: string, data: map<string, unknown>) -> null {
  baml.events.send("log", {
    level: level,
    message: message,
    data: data
  })
}

/// Log an info-level message with additional structured data.
function info(message: string, data: map<string, unknown>) -> null {
  _log("info", message, data)
}

/// Log an info-level message without additional data.
function info_simple(message: string) -> null {
  _log("info", message, {})
}

/// Log a debug-level message with additional structured data.
function debug(message: string, data: map<string, unknown>) -> null {
  _log("debug", message, data)
}

/// Log a debug-level message without additional data.
function debug_simple(message: string) -> null {
  _log("debug", message, {})
}

/// Log a warning-level message with additional structured data.
function warn(message: string, data: map<string, unknown>) -> null {
  _log("warn", message, data)
}

/// Log a warning-level message without additional data.
function warn_simple(message: string) -> null {
  _log("warn", message, {})
}

/// Log an error-level message with additional structured data.
function error(message: string, data: map<string, unknown>) -> null {
  _log("error", message, data)
}

/// Log an error-level message without additional data.
function error_simple(message: string) -> null {
  _log("error", message, {})
}
