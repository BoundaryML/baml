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

    #[error("operation cancelled")]
    Cancelled,

    /// A user-caused panic from `baml.sys.panic`.
    #[error("baml.sys.panic: {message}")]
    UserPanic { message: String },

    /// A clean process-termination request from `baml.sys.exit(code)`.
    ///
    /// Catchable in user code as `baml.panics.Exit` — patterned after
    /// Python's `SystemExit`: code can intercept it for cleanup or
    /// testing, and if nothing catches it the engine surfaces the code
    /// as `EngineError::Exit` and the host terminates with it.
    ///
    /// BAML `int` is `i64`, so the signal carries the full value the
    /// user wrote; the host narrows to `i32` for `std::process::exit`.
    #[error("baml.sys.exit({code})")]
    Exit { code: i64 },

    /// The graceful-ish way to handle potential OOM errors, instead of hard-crashing.
    #[error("memory allocation failed: {message}")]
    AllocFailure { message: String },

    /// A required host resource is unavailable — e.g. the OS entropy source
    /// returned an error in a sandboxed runtime. Catchable so user code can
    /// fall back gracefully instead of aborting the host process.
    #[error("host resource '{resource}' unavailable: {message}")]
    HostUnavailable { resource: String, message: String },
}

/// An error value from the BAML standard library. Maps 1:1 to a `baml.errors.*` class.
#[derive(Debug, Error, PartialEq, Clone)]
pub enum VmBamlError {
    #[error("invalid argument: {message}")]
    InvalidArgument { message: String },

    #[error("parse error: {message}")]
    ParseError { message: String },

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

    /// A Rust type error during downcasting from an `Arc<dyn Any + Send + Sync>`.
    /// The message currently uses typeids which are not very human-readable.
    #[error("rust type error during downcasting: expected typeid {expected:?}, got typeid {got:?}")]
    RustTypeError {
        expected: ::core::any::TypeId,
        got: ::core::any::TypeId,
    },

    #[error("cannot apply binary operation: {left} {op} {right}")]
    CannotApplyBinOp { left: Type, right: Type, op: BinOp },

    #[error("cannot apply comparison operation: {left} {op} {right}")]
    CannotApplyCmpOp { left: Type, right: Type, op: CmpOp },

    #[error("cannot apply unary operation: {op} {value}")]
    CannotApplyUnaryOp { op: UnaryOp, value: Type },

    #[error("jump offset overflowed instruction pointer")]
    InvalidJump,

    #[error("missing native function: {name}")]
    MissingNativeFunction { name: String },

    /// We expected a function to return [`crate::vm::VmExecState::Complete`],
    /// but it returned a different (yielding) variant.
    #[error(
        "Expected a function to return completed, but it instead yielded at some incomplete state."
    )]
    ExpectedCompletion,

    #[error("Invalid watch filter")]
    InvalidFilter,

    #[error("Invalid manual notify")]
    InvalidManualNotify,

    #[error("unexpected constant kind: expected a TyTemplate constant at this index")]
    UnexpectedConstantKind,

    #[error("invalid compact opcode byte: {0}")]
    InvalidOpcode(u8),

    /// `StoreGlobal` was executed outside of an `$init` function. Globals are
    /// frozen post-`$init` (shared as `Arc<[Value]>` across VMs) and any
    /// post-init `StoreGlobal` violates that invariant.
    #[error("StoreGlobal executed outside of $init (globals are frozen post-init)")]
    StoreGlobalAfterInit,
}

/// Any kind of virtual machine error.
#[derive(Debug, Error, PartialEq, Clone)]
pub enum VmError {
    /// Catchable (panics and error values) — internal signal for exception unwinding.
    #[error("uncaught throw: {0:?}")]
    Thrown(Value),
    /// An exception that escaped all catch handlers, with captured stack trace.
    #[error("uncaught throw: {value:?}")]
    ThrownUnhandled {
        value: Value,
        trace: Vec<StackFrame>,
    },
    /// Fatal VM errors
    #[error("{0}")]
    InternalError(#[from] VmInternalError),
    /// Fatal VM error with captured stack trace.
    /// Produced by `exec()` wrapper from `InternalError`.
    #[error("{}", format_internal_error(source, trace))]
    TracedInternalError {
        source: VmInternalError,
        trace: Vec<StackFrame>,
    },
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
    /// A pre-built exception `Value` to throw directly as a catchable error.
    ///
    /// Used by native functions that need to throw user-defined class instances
    /// (e.g. `baml.json.JsonParseError`) without going through the
    /// `VmPanic` / `VmBamlError` enumeration machinery.
    #[error("thrown value")]
    Thrown(Value),
}

#[derive(Debug, Clone, PartialEq)]
pub struct StackFrame {
    pub function_name: String,
    /// Filesystem path of the source file containing this function.
    /// Empty string for builtins and synthesized functions.
    pub file_path: String,
    pub function_span: baml_type::Span,
    pub error_line: usize,
}

fn format_internal_error(err: &VmInternalError, trace: &[StackFrame]) -> String {
    use std::fmt::Write;
    let mut out = format_traceback(
        trace
            .iter()
            .map(|f| (f.file_path.as_str(), f.error_line, f.function_name.as_str())),
    );
    write!(out, "VM internal error: {err}").unwrap();
    out
}

/// Format a traceback header from an iterator of `(file, line, function_name)` tuples.
///
/// Produces the Python-style format:
/// ```text
/// Traceback (most recent call last):
///   File "test.baml", line 3, in user.inner
///   File "test.baml", line 7, in user.main
/// ```
///
/// Returns an empty string when `frames` is empty.
pub fn format_traceback<'a>(frames: impl Iterator<Item = (&'a str, usize, &'a str)>) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    for (file, line, function_name) in frames {
        if out.is_empty() {
            writeln!(out, "Traceback (most recent call last):").unwrap();
        }
        writeln!(out, "  File \"{file}\", line {line}, in {function_name}").unwrap();
    }
    out
}
