/// BAML runtime errors
///
/// Note: This is intentionally minimal. Expand with specific variants
/// (InitError, CallError, etc.) once the core functionality works.
#[derive(Debug, thiserror::Error)]
pub enum BamlError {
    #[error("{0}")]
    Internal(String),
}

impl BamlError {
    pub fn internal(msg: impl Into<String>) -> Self {
        BamlError::Internal(msg.into())
    }
}

/// Panics with a user-friendly error message for internal/unreachable errors.
///
/// This macro is used for situations that should never occur in practice -
/// bugs in the FFI boundary, protocol mismatches, etc. The error message
/// guides users to report the issue.
///
/// # Examples
/// ```ignore
/// // Simple message
/// baml_unreachable!("unexpected null pointer");
///
/// // With format args
/// baml_unreachable!("unknown object type: {:?}", obj_type);
/// ```
#[macro_export]
macro_rules! baml_unreachable {
    ($($arg:tt)*) => {{
        panic!(
            "\n\n\
            ========================================\n\
            BAML Internal Error\n\
            ========================================\n\n\
            {}\n\n\
            This is a bug in BAML. Please report it:\n\
            - GitHub: https://github.com/BoundaryML/baml/issues\n\
            - Discord: https://boundaryml.com/discord\n\n\
            Include this error message and steps to reproduce.\n\
            ========================================\n",
            format_args!($($arg)*)
        )
    }};
}
