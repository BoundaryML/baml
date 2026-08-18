//! Error surface of the Rust SDK.
//!
//! Every generated function returns `Result<T, Error<E>>` where `E` is the
//! Rust translation of the function's BAML `throws` contract — errors are
//! typed values, not stringly exceptions. A function with no contract uses
//! the uninhabited [`Infallible`], so its `Thrown` arm is statically
//! impossible and every error-arm payload lands in [`Error::Runtime`].

use std::{convert::Infallible, fmt, sync::Arc};

/// Error type of a generated BAML function, generic over the function's
/// declared `throws` type.
///
/// The wire error arm is *not* exclusively the declared throws type:
/// engine-infrastructure errors (`baml.errors.TypeMismatch`,
/// `baml.errors.InvalidArgument`, …) arrive on the same arm. Decoding
/// first tries the declared type (FQN-verified); anything else lands in
/// [`Error::Runtime`] with its class name, rendered message, and trace
/// preserved — nothing is erased, but only contract-declared values are
/// typed.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error<E = Infallible> {
    /// A value thrown by BAML code, decoded as the function's declared
    /// `throws` type.
    Thrown {
        /// The thrown BAML value. Boxed so a large user error class
        /// doesn't inflate every generated function's `Result` — the Ok
        /// path never pays for the throw payload's size.
        value: Box<E>,
        /// BAML stack-trace lines, outermost first.
        trace: Vec<String>,
    },
    /// An error-arm value that is *not* the declared throws type: an
    /// engine-infrastructure error or contract drift.
    Runtime {
        /// FQN of the thrown value's class, when it was a class instance.
        class_name: Option<String>,
        /// Best-effort rendered message (the value's `message` field or a
        /// bounded description).
        message: String,
        /// BAML stack-trace lines, outermost first.
        trace: Vec<String>,
    },
    /// A non-exit BAML panic. Exit panics never surface as values — they
    /// terminate the process, matching the other SDKs.
    Panic {
        /// Best-effort rendered panic payload.
        message: String,
        /// BAML stack-trace lines, outermost first.
        trace: Vec<String>,
    },
    /// The engine returned a value the declared Rust type cannot represent
    /// — engine/codegen drift, never a user-input condition.
    Decode(DecodeError),
    /// SDK-boundary failure: runtime not initialized, tokio setup, task
    /// join, envelope decode.
    Sdk(SdkError),
    /// A blocking (sync) generated function was called from inside an
    /// async runtime, where `block_on` would deadlock or panic. Call the
    /// `_async` sibling instead.
    CalledSyncFromAsync,
}

impl<E: fmt::Debug> fmt::Display for Error<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Thrown { value, trace } => {
                write!(f, "BAML function threw: {value:?}")?;
                write_trace(f, trace)
            }
            Error::Runtime {
                class_name,
                message,
                trace,
            } => {
                match class_name {
                    Some(name) => write!(f, "BAML error [{name}]: {message}")?,
                    None => write!(f, "BAML error: {message}")?,
                }
                write_trace(f, trace)
            }
            Error::Panic { message, trace } => {
                write!(f, "BAML panic: {message}")?;
                write_trace(f, trace)
            }
            Error::Decode(e) => write!(f, "failed to decode BAML result: {e}"),
            Error::Sdk(e) => write!(f, "BAML SDK error: {e}"),
            Error::CalledSyncFromAsync => write!(
                f,
                "a blocking BAML function was called from inside an async runtime; \
                 use its `_async` sibling instead"
            ),
        }
    }
}

impl<E: fmt::Debug> std::error::Error for Error<E> {}

fn write_trace(f: &mut fmt::Formatter<'_>, trace: &[String]) -> fmt::Result {
    for line in trace {
        write!(f, "\n  {line}")?;
    }
    Ok(())
}

/// A wire value did not match the expected static type. With no runtime
/// typemap in the Rust SDK, decode is driven entirely by the declared
/// type; any mismatch is engine/codegen drift and fails loudly.
///
/// Payloads carry bounded metadata only (variant kinds, names, lengths) —
/// never full values.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecodeError {
    /// The value's wire variant does not match the expected type.
    WrongType {
        /// The expected type, as baked into the generated impl.
        expected: &'static str,
        /// The wire variant kind that arrived.
        got: &'static str,
    },
    /// A required class field was absent from the wire value.
    MissingField {
        /// Expected class FQN.
        class: &'static str,
        /// Missing field name.
        field: &'static str,
    },
    /// A nominally-typed wire value (class or enum) does not match the
    /// expected type's FQN.
    FqnMismatch {
        /// Expected FQN, as baked into the generated impl.
        expected: &'static str,
        /// FQN that arrived on the wire.
        got: String,
    },
    /// A `baml.errors.HostCallable`'s `_handle` did not resolve to a live
    /// same-process host value. Always a bridge-lifetime bug: exactly one
    /// bridge exists per process, and the registry entry must outlive
    /// every possible arrival of the value.
    DeadHostHandle {
        /// The unresolvable handle key.
        key: u64,
    },
    /// A `baml.errors.HostCallable` was decoded into a concrete
    /// `HostCallable<T>`, but the rehydrated original host error is not a
    /// `T`. Not engine drift — the value is a genuine host error of some
    /// *other* type; `decode_result` folds this into [`Error::Runtime`],
    /// the same as any non-declared error-arm value. Decode into the
    /// erased `HostCallable` and `downcast` to inspect the real type.
    HostCallableTypeMismatch {
        /// The `type_name` of the expected `T`.
        expected: &'static str,
    },
    /// An enum value's variant does not exist on the expected enum.
    UnknownEnumVariant {
        /// Expected enum FQN.
        enum_fqn: &'static str,
        /// Variant value that arrived on the wire.
        got: String,
    },
    /// A bigint wire string failed to parse as a base sixteen integer.
    InvalidBigint {
        /// Length of the offending wire string.
        len: usize,
    },
    /// A decoded media descriptor cannot be safely passed back through the C ABI.
    InvalidMedia {
        /// The invalid descriptor field (`source` or `MIME type`).
        field: &'static str,
    },
    /// A union envelope selected an arm outside the generated union's range.
    InvalidUnionOptionIndex {
        /// Generated union type being decoded.
        union: &'static str,
        /// Index carried by the wire envelope.
        index: u32,
        /// Number of representable generated arms.
        arm_count: usize,
    },
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::WrongType { expected, got } => {
                write!(f, "expected {expected}, got wire variant {got}")
            }
            DecodeError::MissingField { class, field } => {
                write!(f, "class {class} is missing required field `{field}`")
            }
            DecodeError::FqnMismatch { expected, got } => {
                write!(f, "expected {expected}, got {got}")
            }
            DecodeError::UnknownEnumVariant { enum_fqn, got } => {
                write!(f, "enum {enum_fqn} has no variant `{got}`")
            }
            DecodeError::InvalidBigint { len } => {
                write!(f, "invalid bigint wire string ({len} bytes)")
            }
            DecodeError::InvalidMedia { field } => {
                write!(f, "media {field} contains an interior NUL byte")
            }
            DecodeError::InvalidUnionOptionIndex {
                union,
                index,
                arm_count,
            } => write!(
                f,
                "union {union} selected option index {index}, but it has {arm_count} arms"
            ),
            DecodeError::DeadHostHandle { key } => {
                write!(
                    f,
                    "host-value handle {key} did not resolve to a live entry \
                     (bridge-lifetime bug)"
                )
            }
            DecodeError::HostCallableTypeMismatch { expected } => {
                write!(f, "host error is not the expected {expected}")
            }
        }
    }
}

impl std::error::Error for DecodeError {}

impl<E> From<DecodeError> for Error<E> {
    fn from(e: DecodeError) -> Self {
        Error::Decode(e)
    }
}

/// SDK-boundary failure. `Clone` so a failed lazy initialization can be
/// stored once and returned from every subsequent call.
#[derive(Debug, Clone)]
pub struct SdkError {
    message: Arc<str>,
}

impl SdkError {
    pub fn new(message: impl fmt::Display) -> Self {
        Self {
            message: message.to_string().into(),
        }
    }
}

impl fmt::Display for SdkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for SdkError {}

impl<E> From<SdkError> for Error<E> {
    fn from(e: SdkError) -> Self {
        Error::Sdk(e)
    }
}
