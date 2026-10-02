/// Log an info-level event with any BAML value.
///
/// The value is emitted unchanged as the log event's `data`.
/// Pass a map for structured fields, or pass a string, number, bool,
/// list, class instance, enum variant, or null to inspect that value directly.
/// `event_name` labels the event; null or omitted still emits an unnamed log.
function info(data: unknown, event_name: string? = null) -> void {
    $compiler_intrinsic
}

/// Log a debug-level event with any BAML value.
///
/// The value is emitted unchanged as the log event's `data`.
/// Pass a map for structured fields, or pass a string, number, bool,
/// list, class instance, enum variant, or null to inspect that value directly.
/// `event_name` labels the event; null or omitted still emits an unnamed log.
function debug(data: unknown, event_name: string? = null) -> void {
    $compiler_intrinsic
}

/// Log a warning-level event with any BAML value.
///
/// The value is emitted unchanged as the log event's `data`.
/// Pass a map for structured fields, or pass a string, number, bool,
/// list, class instance, enum variant, or null to inspect that value directly.
/// `event_name` labels the event; null or omitted still emits an unnamed log.
function warn(data: unknown, event_name: string? = null) -> void {
    $compiler_intrinsic
}

/// Log an error-level event with any BAML value.
///
/// The value is emitted unchanged as the log event's `data`.
/// Pass a map for structured fields, or pass a string, number, bool,
/// list, class instance, enum variant, or null to inspect that value directly.
/// `event_name` labels the event; null or omitted still emits an unnamed log.
function error(data: unknown, event_name: string? = null) -> void {
    $compiler_intrinsic
}
