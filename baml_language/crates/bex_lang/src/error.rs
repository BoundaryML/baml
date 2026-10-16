//! The closed set of failures a shared builtin can report.

use std::fmt;

use crate::Panic;

/// What a shared (VM and native) builtin can fail with: a panic, or one of
/// the `baml.errors.*` classes it raises itself. Closed, so each backend
/// converts it into its own error type without downcasting: the VM into
/// `VmRustFnError`, native code into its `Thrown`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A `baml.panics.*` class.
    Panic(Panic),
    /// `baml.errors.InvalidArgument { message }`: an argument is out of range
    /// or otherwise invalid.
    InvalidArgument {
        /// Human-readable description, e.g.
        /// `"float.itrunc: cannot convert NaN to int"`.
        message: String,
    },
}

impl From<Panic> for Error {
    fn from(panic: Panic) -> Self {
        Self::Panic(panic)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Panic(panic) => fmt::Display::fmt(panic, f),
            Self::InvalidArgument { message } => f.write_str(message),
        }
    }
}

impl std::error::Error for Error {}
