/// Logs a structured event at the specified level
///
/// This can be used for structured logging that works with both regular and JSON formats.
/// When in JSON mode, the payload is serialized as a JSON object.
/// When in regular mode, the payload is formatted as a string.
///
/// # Example
///
/// ```ignore
/// use baml_log::event;
/// use baml_log::Level;
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct UserEvent {
///     user_id: String,
///     action: String,
/// }
///
/// // Log a structured event
/// event!(
///     Level::Info,
///     "user_action",
///     UserEvent {
///         user_id: "123".to_string(),
///         action: "login".to_string(),
///     }
/// );
/// ```
#[macro_export]
macro_rules! event {
    ($level:expr, $payload:expr) => {
        $crate::log_event_internal(
            $level,
            &$payload,
            Some(module_path!()),
            Some(file!()),
            Some(line!()),
        )
    };
}

#[macro_export]
macro_rules! elog {
    ($level:expr, $payload:expr) => {
        $crate::log_event_internal(
            $level,
            $payload,
            Some(module_path!()),
            Some(file!()),
            Some(line!()),
        )
    };
}

/// Log an event at the ERROR level
#[macro_export]
macro_rules! eerror {
    ($payload:expr) => {
        $crate::event!($crate::Level::Error, $payload)
    };
}

/// Log an event at the WARN level
#[macro_export]
macro_rules! ewarn {
    ($payload:expr) => {
        $crate::event!($crate::Level::Warn, $payload)
    };
}

/// Log an event at the INFO level
#[macro_export]
macro_rules! einfo {
    ($payload:expr) => {
        $crate::event!($crate::Level::Info, $payload)
    };
}

/// Log an event at the DEBUG level
#[macro_export]
macro_rules! edebug {
    ($payload:expr) => {
        $crate::event!($crate::Level::Debug, $payload)
    };
}

/// Log an event at the TRACE level
#[macro_export]
macro_rules! etrace {
    ($payload:expr) => {
        $crate::event!($crate::Level::Trace, $payload)
    };
}
