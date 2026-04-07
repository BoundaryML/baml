use bex_vm_types::{BinOp, CmpOp, UnaryOp, Value, types::Type};
use thiserror::Error;

/// A catchable BAML panic — maps 1:1 to a `baml.panics.*` class.
///
/// These are user-visible runtime errors (division by zero, index out of
/// bounds, etc.) that can be caught by `catch` handlers. The handler's
/// `ThrowIfPanic` instruction filters which panics are caught vs rethrown.
#[derive(Debug, Error, PartialEq, Clone)]
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
    // ── Catchable panics ────────────────────────────────────────────────
    /// A BAML-level panic (converted to a `baml.panics.*` instance and
    /// routed through the exception table).
    #[error("{0}")]
    Panic(#[from] VmPanic),

    // ── Fatal errors (not catchable) ────────────────────────────────────
    #[error("{0}")]
    InternalError(#[from] VmInternalError),

    // ── Terminal errors ─────────────────────────────────────────────────
    /// Only exists in this form it if
    #[error("uncaught throw: {value:?}")]
    UnhandledThrow { value: Value },
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
