use crate::{
    bytecode::{BinOp, CmpOp, UnaryOp},
    types::{Type, Value},
};

/// Bug in the VM or somehow invalid source code got compiled and executed.
///
/// If the VM throws this it's either a bug in the compiler or in the VM itself.
#[derive(Debug, PartialEq, Clone)]
pub enum InternalError {
    /// The number of arguments passed to a function doesn't match the function
    /// arity.
    InvalidArgumentCount { expected: usize, got: usize },

    /// Attempt to access the top of the stack but it's empty.
    UnexpectedEmptyStack,

    /// Attempt to access a stack slot from the top of the stack,
    /// and stack doesn't have enough items.
    /// Argument is the amount of slots from the top of the stack (inclusive - 0 is top itself)
    /// that were queried.
    NotEnoughItemsOnStack(usize),

    /// Reference an object that does not exist in the object pool.
    /// Argument is the reference index.
    InvalidObjectRef(usize),

    /// Attempt to use a value but it's not the expected type.
    TypeError { expected: Type, got: Type },

    /// Attempt to apply a binary operation to two values of different types.
    CannotApplyBinOp { left: Type, right: Type, op: BinOp },

    /// Attempt to apply a comparison operation to two values of different types.
    CannotApplyCmpOp { left: Type, right: Type, op: CmpOp },

    /// Attempt to apply a unary operation to a value of the wrong type.
    CannotApplyUnaryOp { op: UnaryOp, value: Type },

    /// Array index out of bounds.
    ArrayIndexOutOfBounds { index: usize, length: usize },

    /// Array index is negative.
    ArrayIndexIsNegative(i64),

    /// Instruction pointer is negative.
    NegativeInstructionPtr(isize),
}

/// Errors that can happen at runtime.
///
/// Either logic errors in the user's source code or bugs in our compiler/VM
/// stack.
#[derive(Debug, PartialEq, Clone)]
pub enum RuntimeError {
    /// Ah yes, classic stack overflow.
    StackOverflow,

    /// User code triggered an assertion failure via the [`Instruction::Assert`] opcode.
    AssertionError,

    /// VM internal error.
    InternalError(InternalError),

    /// Map does not contain the requested key.
    NoSuchKeyInMap,

    /// Right hand side of division operation is zero.
    DivisionByZero { left: Value, right: Value },

    /// Any error, provide a custom message for this one.
    Other(String),
}

/// Any kind of virtual machine error.
#[derive(Debug, PartialEq, Clone)]
pub enum VmError {
    RuntimeError(RuntimeError),
}

impl From<RuntimeError> for VmError {
    fn from(error: RuntimeError) -> Self {
        VmError::RuntimeError(error)
    }
}

impl From<InternalError> for VmError {
    fn from(error: InternalError) -> Self {
        VmError::RuntimeError(RuntimeError::InternalError(error))
    }
}

impl std::error::Error for VmError {}

impl std::fmt::Display for VmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Figure out something nicer here.
        match self {
            VmError::RuntimeError(error) => write!(f, "{error:?}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ErrorLocation {
    pub function_name: String,
    pub function_span: internal_baml_diagnostics::Span,
    pub error_line: usize,
}

#[derive(Debug, Clone)]
pub struct StackTrace {
    pub error: VmError,
    pub trace: Vec<ErrorLocation>,
}

impl std::fmt::Display for StackTrace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Figure out something nicer here.
        f.write_str("Traceback (most recent call last):\n")?;

        for location in &self.trace {
            writeln!(
                f,
                "  File \"{}\", line {}, in {}",
                location.function_span.file_name(),
                location.error_line,
                location.function_name
            )?;
        }

        writeln!(f, "{}", self.error)
    }
}

impl std::fmt::Display for InternalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Internal VM Erorr: ")?;

        match self {
            InternalError::InvalidArgumentCount { expected, got } => {
                write!(
                    f,
                    "invalid argument count: expected {expected} arguments, got {got}"
                )
            }
            InternalError::UnexpectedEmptyStack => write!(f, "unexpected empty eval stack"),
            InternalError::NotEnoughItemsOnStack(count) => {
                write!(f, "not enough items on stack: {count}")
            }
            InternalError::InvalidObjectRef(index) => {
                write!(f, "invalid object reference: {index}")
            }
            InternalError::TypeError { expected, got } => {
                write!(f, "type error: expected {expected}, got {got}")
            }
            InternalError::CannotApplyBinOp { left, right, op } => {
                write!(f, "cannot apply binary operation: {left} {op} {right}")
            }
            InternalError::CannotApplyCmpOp { left, right, op } => {
                write!(f, "cannot apply comparison operation: {left} {op} {right}")
            }
            InternalError::CannotApplyUnaryOp { op, value } => {
                write!(f, "cannot apply unary operation: {op} {value}")
            }
            InternalError::ArrayIndexOutOfBounds { index, length } => {
                write!(f, "array index out of bounds: {index} of {length}")
            }
            InternalError::ArrayIndexIsNegative(index) => {
                write!(f, "array index is negative: {index}")
            }
            InternalError::NegativeInstructionPtr(ptr) => {
                write!(f, "negative instruction pointer: {ptr}")
            }
        }
    }
}

impl std::error::Error for StackTrace {}

impl std::error::Error for InternalError {}
