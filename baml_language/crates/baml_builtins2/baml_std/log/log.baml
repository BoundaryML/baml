/// Log an info-level event.
///
/// Pass a string for a simple message or a map for structured data.
/// `log.info("hello")` is equivalent to `log.info({ message: "hello" })`.
function info(data: string | map<string, unknown>) -> void {
  $compiler_intrinsic
}

/// Log a debug-level event.
///
/// Pass a string for a simple message or a map for structured data.
function debug(data: string | map<string, unknown>) -> void {
  $compiler_intrinsic
}

/// Log a warning-level event.
///
/// Pass a string for a simple message or a map for structured data.
function warn(data: string | map<string, unknown>) -> void {
  $compiler_intrinsic
}

/// Log an error-level event.
///
/// Pass a string for a simple message or a map for structured data.
function error(data: string | map<string, unknown>) -> void {
  $compiler_intrinsic
}
