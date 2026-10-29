//! Error surface of the Rust SDK.
//!
//! Every generated function returns `Result<T, Error<E>>` where `E` is the
//! Rust translation of the function's BAML `throws` contract — errors are
//! typed values, not stringly exceptions. A function with no contract uses
//! the uninhabited [`Infallible`], so its `Thrown` arm is statically
//! impossible and every error-arm payload lands in [`Error::Runtime`].

use std::{convert::Infallible, fmt, sync::Arc};

use crate::encode::ArgumentError;

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
        /// The thrown value as the engine sent it, which a host callback
        /// forwards by returning this error.
        thrown: ThrownValue,
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
        /// The thrown value as the engine sent it, which a host callback
        /// forwards by returning this error.
        thrown: ThrownValue,
    },
    /// A non-exit BAML panic. Exit panics never surface as values — they
    /// terminate the process, matching the other SDKs.
    Panic {
        /// FQN of the panic value's class (`baml.panics.Cancelled`,
        /// `baml.panics.SdkPanic`, …), when the engine sent one.
        class_name: Option<String>,
        /// Best-effort rendered panic payload.
        message: String,
        /// BAML stack-trace lines, outermost first.
        trace: Vec<String>,
        /// The panic value as the engine sent it, which a host callback
        /// forwards by returning this error: a panic stays a panic.
        thrown: ThrownValue,
    },
    /// An argument the bridge could not encode for the wire (too deep, or
    /// holding a handle that could not be cloned), refused before the call
    /// was sent.
    Argument(ArgumentError),
    /// The engine returned a value the declared Rust type cannot represent
    /// — engine/codegen drift, or a json value [`Json`](crate::Json) cannot
    /// hold (see [`DecodeError::NotJson`]).
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
            Error::Thrown { value, trace, .. } => {
                write!(f, "BAML function threw: {value:?}")?;
                write_trace(f, trace)
            }
            Error::Runtime {
                class_name,
                message,
                trace,
                ..
            } => {
                match class_name {
                    Some(name) => write!(f, "BAML error [{name}]: {message}")?,
                    None => write!(f, "BAML error: {message}")?,
                }
                write_trace(f, trace)
            }
            Error::Panic {
                class_name,
                message,
                trace,
                ..
            } => {
                match class_name {
                    Some(name) => write!(f, "BAML panic [{name}]: {message}")?,
                    None => write!(f, "BAML panic: {message}")?,
                }
                write_trace(f, trace)
            }
            Error::Argument(e) => write!(f, "BAML SDK could not send the call: {e}"),
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

/// The source of a BAML error or panic is the value BAML threw: a host
/// callback that returns the error forwards that value (see
/// [`HostCallback`](crate::HostCallback)).
impl<E: fmt::Debug> std::error::Error for Error<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Thrown { thrown, .. }
            | Error::Runtime { thrown, .. }
            | Error::Panic { thrown, .. } => Some(thrown),
            Error::Argument(e) => Some(e),
            Error::Decode(e) => Some(e),
            Error::Sdk(e) => Some(e),
            Error::CalledSyncFromAsync => None,
        }
    }
}

/// The value a BAML call threw — an error or a panic — as the engine sent
/// it, owned by this process: the engine references inside it are released
/// when the last clone is dropped.
///
/// A host callback forwards a BAML error or panic by returning it: a closure
/// failing with [`Error`] (whose [`source`](std::error::Error::source) is
/// this value) or with this value itself sends it back to BAML as thrown, so
/// a panic stays a panic and a nested call's error keeps its class.
#[derive(Debug, Clone)]
pub struct ThrownValue(crate::dynamic::Value);

impl ThrownValue {
    pub(crate) fn new(value: crate::wire::BamlOutboundValue) -> Self {
        Self(crate::dynamic::Value::own(value))
    }

    /// The value, to decode or inspect.
    pub fn value(&self) -> &crate::dynamic::Value {
        &self.0
    }
}

impl fmt::Display for ThrownValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0.class_name() {
            Some(name) => write!(f, "the thrown BAML `{name}` value"),
            None => write!(f, "the thrown BAML value"),
        }
    }
}

impl std::error::Error for ThrownValue {}

impl<E> From<ArgumentError> for Error<E> {
    fn from(e: ArgumentError) -> Self {
        Error::Argument(e)
    }
}

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
    /// An instance of a generic class arrived at other type arguments than
    /// the expected type's. Generic arguments are invariant, so it is not an
    /// instance of that type.
    TypeArgsMismatch {
        /// Expected class FQN.
        class: &'static str,
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
    /// A `baml.json.json` value has no [`Json`](crate::Json) form: a
    /// `bigint` outside `i64`/`u64`, a non-finite float, or a value that is
    /// not JSON at all (class, enum, media, bytes, handle).
    NotJson {
        /// What arrived instead (a wire variant kind or a short reason).
        got: &'static str,
    },
    /// The engine could not clone a handle the value holds (it was
    /// released, or the runtime shut down), so the value cannot be decoded
    /// a second time.
    HandleClone,
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
            DecodeError::TypeArgsMismatch { class } => {
                write!(f, "an instance of {class} arrived at other type arguments")
            }
            DecodeError::UnknownEnumVariant { enum_fqn, got } => {
                write!(f, "enum {enum_fqn} has no variant `{got}`")
            }
            DecodeError::InvalidBigint { len } => {
                write!(f, "invalid bigint wire string ({len} bytes)")
            }
            DecodeError::NotJson { got } => {
                write!(f, "baml.json.json value has no serde_json form: {got}")
            }
            DecodeError::InvalidUnionOptionIndex {
                union,
                index,
                arm_count,
            } => write!(
                f,
                "union {union} selected option index {index}, but it has {arm_count} arms"
            ),
            DecodeError::HandleClone => {
                write!(f, "a handle the value holds could not be cloned")
            }
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
