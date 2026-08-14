/// Log an info-level event with any BAML value.
///
/// The value is emitted unchanged as the log event's `data`.
/// Pass a map for structured fields, or pass a string, number, bool,
/// list, class instance, enum variant, or null to inspect that value directly.
function info(data: unknown) -> void {
    $compiler_intrinsic
}

/// Log a debug-level event with any BAML value.
///
/// The value is emitted unchanged as the log event's `data`.
/// Pass a map for structured fields, or pass a string, number, bool,
/// list, class instance, enum variant, or null to inspect that value directly.
function debug(data: unknown) -> void {
    $compiler_intrinsic
}

/// Log a warning-level event with any BAML value.
///
/// The value is emitted unchanged as the log event's `data`.
/// Pass a map for structured fields, or pass a string, number, bool,
/// list, class instance, enum variant, or null to inspect that value directly.
function warn(data: unknown) -> void {
    $compiler_intrinsic
}

/// Log an error-level event with any BAML value.
///
/// The value is emitted unchanged as the log event's `data`.
/// Pass a map for structured fields, or pass a string, number, bool,
/// list, class instance, enum variant, or null to inspect that value directly.
function error(data: unknown) -> void {
    $compiler_intrinsic
}
