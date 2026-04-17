/// Internal helper that constructs and sends a log event.
function _log(level: string, data: map<string, unknown>) -> null {
  baml.events.send("log", {
    level: level,
    data: data
  })
}

/// Log an info-level event with structured data.
function info(data: map<string, unknown>) -> null {
  _log("info", data)
}

/// Log a debug-level event with structured data.
function debug(data: map<string, unknown>) -> null {
  _log("debug", data)
}

/// Log a warning-level event with structured data.
function warn(data: map<string, unknown>) -> null {
  _log("warn", data)
}

/// Log an error-level event with structured data.
function error(data: map<string, unknown>) -> null {
  _log("error", data)
}
