use bex_vm_types::{BinOp, CmpOp, UnaryOp, Value, types::Type};
use thiserror::Error;

/// Bug in the VM or somehow invalid source code got compiled and executed.
///
/// If the VM throws this it's either a bug in the compiler or in the VM itself.
#[derive(Debug, Error, PartialEq, Clone)]
pub enum InternalError {
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

    #[error("array index out of bounds: {index} of {length}")]
    ArrayIndexOutOfBounds { index: usize, length: usize },

    #[error("array index is negative: {0}")]
    ArrayIndexIsNegative(i64),

    #[error("jump offset overflowed instruction pointer")]
    InvalidJump,
}

/// Errors that can happen at runtime.
///
/// Either logic errors in the user's source code or bugs in our compiler/VM
/// stack.
#[derive(Debug, Error, PartialEq, Clone)]
pub enum RuntimeError {
    #[error("stack overflow")]
    StackOverflow,

    #[error("assertion failed")]
    AssertionError,

    #[error("unreachable code executed")]
    Unreachable,

    #[error("{0}")]
    InternalError(#[from] InternalError),

    #[error("key not found in map")]
    NoSuchKeyInMap,

    #[error("division by zero: {left:?} / {right:?}")]
    DivisionByZero { left: Value, right: Value },

    #[error("uncaught throw: {value}")]
    UnhandledThrow { value: String },

    #[error("{0}")]
    Other(String),
}

/// Any kind of virtual machine error.
#[derive(Debug, Error, PartialEq, Clone)]
pub enum VmError {
    #[error("{0}")]
    RuntimeError(#[from] RuntimeError),
}

impl From<InternalError> for VmError {
    fn from(error: InternalError) -> Self {
        VmError::RuntimeError(RuntimeError::InternalError(error))
    }
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
