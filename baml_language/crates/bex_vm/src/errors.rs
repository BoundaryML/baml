use bex_vm_types::{BinOp, CmpOp, UnaryOp, Value, types::Type};
use thiserror::Error;

/// A catchable BAML panic — maps 1:1 to a `baml.panics.*` class.
///
/// These are user-visible runtime errors (division by zero, index out of
/// bounds, etc.) that can be caught by `catch` handlers. The handler's
/// `ThrowIfPanic` instruction filters which panics are caught vs rethrown.
#[derive(Debug, Error, PartialEq, Clone, Copy)]
pub enum VmPanic {
    #[error("division by zero: {left:?} / {right:?}")]
    DivisionByZero { left: Value, right: Value },

    #[error("array index out of bounds: {index} of {length}")]
    IndexOutOfBounds { index: i64, length: usize },

    #[error("key not found in map")]
    MapKeyNotFound,

    #[error("stack overflow")]
    StackOverflow,

    #[error("assertion failed")]
    AssertionFailed,

    #[error("unreachable code executed")]
    Unreachable,
}

/// An error value from the BAML standard library. Maps 1:1 to a `baml.errors.*` class.
#[derive(Debug, Error, PartialEq, Clone)]
pub enum VmBamlError {
    #[error("invalid argument: {message}")]
    InvalidArgument { message: String },

    #[error("I/O error: {message}")]
    Io { message: String },

    #[error("timeout: {message}")]
    Timeout {
        message: String,
        duration_ms: Option<i64>,
    },

    #[error("unsupported: {message}")]
    Unsupported { message: String },

    #[error("access error: {message}")]
    AccessError { message: String },

    #[error("render prompt: {message}")]
    RenderPrompt { message: String },

    #[error("not implemented: {message}")]
    NotImplemented { message: String },

    #[error("LLM client error: {message}")]
    LlmClient { message: String },

    #[error("developer error: {message}")]
    DevOther { message: String },

    #[error("host panic: {message}")]
    HostPanic { message: String },
}

/// The VM encountered an internal error. This typically indicates a bug in the VM or compiler.
/// These are always fatal: they cannot be caught in BAML code.
#[derive(Debug, Error, PartialEq, Clone)]
pub enum VmInternalError {
    #[error("invalid argument count: expected {expected}, got {got}")]
    InvalidArgumentCount { expected: usize, got: usize },

    #[error("unexpected empty eval stack")]
    UnexpectedEmptyStack,

    #[error("not enough items on stack: {0}")]
    NotEnoughItemsOnStack(usize),

    #[error("invalid object reference: {0}")]
    InvalidObjectRef(usize),

    #[error("type error: expected {expected}, got {got}")]
    TypeError { expected: Type, got: Type },

    #[error("cannot apply binary operation: {left} {op} {right}")]
    CannotApplyBinOp { left: Type, right: Type, op: BinOp },

    #[error("cannot apply comparison operation: {left} {op} {right}")]
    CannotApplyCmpOp { left: Type, right: Type, op: CmpOp },

    #[error("cannot apply unary operation: {op} {value}")]
    CannotApplyUnaryOp { op: UnaryOp, value: Type },

    #[error("jump offset overflowed instruction pointer")]
    InvalidJump,

    #[error("internal error: {0}")]
    InternalError(String),
}

/// Any kind of virtual machine error.
#[derive(Debug, Error, PartialEq, Clone)]
pub enum VmError {
    /// Catchable (panics and error values)
    #[error("uncaught throw: {0:?}")]
    Thrown(Value),
    /// Fatal VM errors
    #[error("{0}")]
    InternalError(#[from] VmInternalError),
}

/// An error returned by a Rust function. Will generally be turned into a [`VmError`].
/// This is separate from [`VmError`] so native Rust functions can return standard errors
/// without needing to handle heap allocation.
#[derive(Debug, Error, PartialEq, Clone)]
pub enum VmRustFnError {
    #[error("{0}")]
    Panic(#[from] VmPanic),
    #[error("{0}")]
    BamlError(#[from] VmBamlError),
    #[error("{0}")]
    InternalError(#[from] VmInternalError),
}

#[derive(Debug, Clone)]
pub struct ErrorLocation {
    pub function_name: String,
    pub function_span: baml_type::Span,
    pub error_line: usize,
}

#[derive(Debug, Clone)]
pub struct StackTrace {
    pub error: VmError,
    pub trace: Vec<ErrorLocation>,
}

impl std::fmt::Display for StackTrace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Traceback (most recent call last):")?;
        for location in &self.trace {
            writeln!(
                f,
                "  File \"{}\", line {}, in {}",
                location.function_span.file_id, location.error_line, location.function_name
            )?;
        }
        write!(f, "{}", self.error)
    }
}

impl std::error::Error for StackTrace {}
