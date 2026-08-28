//! VM-level error and panic types.
//!
//! Lives in `bex_vm_types` (rather than `bex_vm`) so any crate that produces
//! these errors — sysop impls in particular — can construct them without
//! taking a dependency on the VM crate itself. The actual conversion from
//! these types to heap-allocated exception `Value`s
//! (`panic_to_exception_value` / `error_to_exception_value`) stays in
//! `bex_vm` because it needs VM state (heap, class table).

use thiserror::Error;

use crate::{BinOp, CmpOp, SysOpErrorCategory, UnaryOp, Value, types::Type};

/// A catchable BAML panic — maps 1:1 to a `baml.panics.*` class.
///
/// These are user-visible runtime errors (division by zero, index out of
/// bounds, etc.) that can be caught by `catch` handlers. The handler's
/// `ThrowIfPanic` instruction filters which panics are caught vs rethrown.
#[derive(Debug, Error, PartialEq, Clone)]
pub enum VmPanic {
    #[error("division by zero: {left:?} / {right:?}")]
    DivisionByZero { left: Value, right: Value },

    /// An `int` (i63) arithmetic operation overflowed the representable
    /// range `[INT_MIN, INT_MAX]`. Carries a human-readable description of
    /// the operation (e.g. `"4611686018427387903 + 1"`); built only on the
    /// cold overflow path, so the `String` alloc never touches hot code.
    #[error("integer overflow: {message}")]
    IntegerOverflow { message: String },

    // Raised by array and byte-array subscripting, so the message stays generic
    // ("index", not "array index").
    #[error("index out of bounds: {index} of {length}")]
    IndexOutOfBounds { index: i64, length: usize },

    #[error("invalid field access: field {field_index} of {field_count}")]
    InvalidFieldAccess {
        field_index: usize,
        field_count: usize,
    },

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

    /// A user-caused panic from `baml.sys.panic`, and the stdlib's panic of
    /// record for a user-violated native invariant (e.g. a reflection kind
    /// view's `_ty` field overwritten with a type of a different kind).
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

    /// The right operand of a bigint shift (`<<` / `>>`) was negative.
    /// Catchable because the count is a runtime `bigint` and the type
    /// system can't rule out negative values.
    #[error("negative bit shift: {message}")]
    NegativeBitShift { message: String },

    /// A host callable returned a value of the wrong type, or threw a value
    /// that does not match its declared `throws` contract `E`. Surfaces in
    /// BAML as `baml.panics.HostContractViolation`.
    ///
    /// `class_name` / `language` are populated when the violation arose from
    /// a host throw (echoing the offending host exception's identity) and
    /// `None` when it arose from a wrong-type return (no exception class to
    /// echo).
    #[error("host contract violation: {message} [class={class_name:?}, lang={language:?}]")]
    HostContractViolation {
        message: String,
        class_name: Option<String>,
        language: Option<String>,
    },
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

    /// An error value from the host language that has no direct BAML
    /// representation. The `handle` is the load-bearing field — it
    /// references the original host exception object via the
    /// process-global host-value table, so the originating runtime can
    /// recover the exact native exception on round-trip. The
    /// `class_name` / `message` / `language` / `traceback` fields are
    /// purely metadata for debugging, logging, and user-facing
    /// formatting — they do not participate in error matching or
    /// rehydration.
    ///
    /// Surfaces in BAML as a `baml.errors.HostCallable` Instance whose
    /// `_handle` field is materialized from `handle`. Engine-side
    /// failures with no underlying host exception (bridge serialization
    /// faults, missing-bridge errors, etc.) MUST use a different
    /// variant — they are not host-language errors and have nothing to
    /// rehydrate. Such SDK/bridge faults route through fatal
    /// [`VmInternalError::BridgeFailure`].
    #[error("host callable error: {message} [class={class_name}, lang={language:?}]")]
    HostCallable {
        class_name: String,
        message: String,
        traceback: Option<String>,
        language: Option<String>,
        /// Required: handle to the originating host exception object
        /// (registered in the per-bridge host-value table at the point
        /// of the throw). Materialized into the BAML class's `_handle`
        /// field on the way out so the originating runtime can resolve
        /// it back to the original native exception.
        handle: std::sync::Arc<bex_resource_types::HostValueArc>,
    },
}

impl VmBamlError {
    /// Map this `baml.errors.*` value to its contract-level
    /// [`SysOpErrorCategory`] — the finite set of categories that sysop
    /// `#[throws(...)]` annotations reference.
    pub fn category(&self) -> SysOpErrorCategory {
        match self {
            Self::InvalidArgument { .. } => SysOpErrorCategory::InvalidArgument,
            Self::ParseError { .. } => SysOpErrorCategory::ParseError,
            Self::Io { .. } => SysOpErrorCategory::Io,
            Self::Timeout { .. } => SysOpErrorCategory::Timeout,
            Self::Unsupported { .. } => SysOpErrorCategory::Unsupported,
            Self::AccessError { .. } => SysOpErrorCategory::AccessError,
            Self::RenderPrompt { .. } => SysOpErrorCategory::RenderPrompt,
            Self::NotImplemented { .. } => SysOpErrorCategory::NotImplemented,
            Self::LlmClient { .. } => SysOpErrorCategory::LlmClient,
            Self::DevOther { .. } => SysOpErrorCategory::DevOther,
            Self::HostCallable { .. } => SysOpErrorCategory::HostCallable,
        }
    }
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

    #[error("unexpected constant kind: expected a TyTemplate constant at this index")]
    UnexpectedConstantKind,

    #[error("invalid compact opcode byte: {0}")]
    InvalidOpcode(u8),

    /// `StoreGlobal` was executed outside of an `$init` function. Globals are
    /// frozen post-`$init` (shared as `Arc<[Value]>` across VMs) and any
    /// post-init `StoreGlobal` violates that invariant.
    #[error("StoreGlobal executed outside of $init (globals are frozen post-init)")]
    StoreGlobalAfterInit,

    /// A native resource handle (SSE stream, WebSocket stream, …) did not
    /// resolve in the registry that owns it. The sysop holds a live
    /// `ResourceHandle` for the whole call and the registry entry is removed
    /// only when the last handle drops, so a missing entry is a registry
    /// invariant violation rather than anything user code can provoke.
    #[error("resource handle {key} of kind `{kind}` did not resolve in its registry")]
    UnresolvedResourceHandle { kind: &'static str, key: usize },

    /// A bridge-layer fault that prevented a host operation from
    /// proceeding — e.g. `external_to_outbound` could not serialize a
    /// `BexExternalValue` (engine→bridge wire-encoding bug), or the
    /// host-side dispatch wrapper threw before scheduling work
    /// (host→bridge dispatch fault). The bridge is engine-owned
    /// infrastructure, so any failure inside it is treated as an
    /// engine bug and surfaces as a fatal internal error rather than a
    /// catchable `VmBamlError` or `VmPanic`.
    #[error("bridge failure: {message}")]
    BridgeFailure { message: String },

    /// A `VirtualCall` could not resolve an implementation of `method` for the
    /// receiver's runtime concrete type. The type checker only emits a virtual
    /// call once it has proved the receiver implements the interface, so the
    /// baked interface-impl registry being unable to resolve the method is a
    /// compiler/VM inconsistency, not a user-reachable condition.
    #[error("virtual call could not resolve interface method `{method}`")]
    UnresolvedVirtualCall { method: String },

    /// A `VirtualLoadField`/`VirtualStoreField` could not resolve interface field
    /// `field_index` of `interface` for the receiver's runtime concrete type — either
    /// the impl registry has no rule for the pair, or the rule's `field_links` table
    /// is shorter than the interface's declared field list. The type checker proves
    /// the receiver implements the interface before emitting the access, and E0124
    /// makes the table total, so either is a compiler/VM inconsistency rather than a
    /// user-reachable condition.
    #[error("virtual field access could not resolve field #{field_index} of `{interface}`")]
    UnresolvedVirtualFieldAccess {
        interface: String,
        field_index: usize,
    },

    /// A `LoadType` (or other materialization) could not fully realize a
    /// `TyTemplate` against the frame's realized type arguments — a frame
    /// reference out of range, or an associated-type projection that did not
    /// reduce. Runtime
    /// type arguments are realized, so the compiler guarantees such templates
    /// realize; a failure here is a compiler/VM inconsistency, surfaced loudly
    /// rather than erased to `unknown`.
    #[error("could not realize type template: {message}")]
    TypeSubstitution { message: String },
}

/// Any kind of virtual machine error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfilerErrorKind {
    Fresh,
    Rethrow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VmUnwindSource {
    Bytecode,
    NativeCall,
    EngineCall,
    FutureResume,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VmThrowSite {
    pub file_id: u32,
    pub line: u32,
    pub start_offset: u32,
    pub end_offset: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VmUnwindOrigin {
    pub throw_call_id: u64,
    pub throw_function_id: u32,
    pub throw_site: Option<VmThrowSite>,
    pub source: VmUnwindSource,
    pub selected_error: bool,
    pub manual_eligible: bool,
    pub origin_span_already_terminated: bool,
}

impl VmUnwindOrigin {
    #[must_use]
    pub const fn unresolved(source: VmUnwindSource) -> Self {
        Self {
            throw_call_id: 0,
            throw_function_id: 0,
            throw_site: None,
            source,
            selected_error: false,
            manual_eligible: false,
            origin_span_already_terminated: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VmThrown {
    pub value: Value,
    pub profiler_kind: ProfilerErrorKind,
    pub language_is_rethrow: bool,
    pub origin: VmUnwindOrigin,
}

impl VmThrown {
    #[must_use]
    pub const fn fresh(value: Value, source: VmUnwindSource) -> Self {
        Self {
            value,
            profiler_kind: ProfilerErrorKind::Fresh,
            language_is_rethrow: false,
            origin: VmUnwindOrigin::unresolved(source),
        }
    }

    #[must_use]
    pub const fn rethrow(value: Value, source: VmUnwindSource, language_is_rethrow: bool) -> Self {
        Self {
            value,
            profiler_kind: ProfilerErrorKind::Rethrow,
            language_is_rethrow,
            origin: VmUnwindOrigin::unresolved(source),
        }
    }

    #[must_use]
    pub const fn with_origin(mut self, origin: VmUnwindOrigin) -> Self {
        self.origin = origin;
        self
    }
}

/// Any kind of virtual machine error.
#[derive(Debug, Error, PartialEq, Clone)]
pub enum VmError {
    /// Catchable (panics and error values) — internal signal for exception unwinding.
    #[error("uncaught throw: {0:?}")]
    Thrown(VmThrown),
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

impl VmError {
    #[must_use]
    pub const fn thrown_fresh(value: Value) -> Self {
        Self::Thrown(VmThrown::fresh(value, VmUnwindSource::Bytecode))
    }
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
    Thrown {
        value: Value,
        profiler_kind: ProfilerErrorKind,
    },
}

impl VmRustFnError {
    #[must_use]
    pub const fn thrown_fresh(value: Value) -> Self {
        Self::Thrown {
            value,
            profiler_kind: ProfilerErrorKind::Fresh,
        }
    }

    #[must_use]
    pub const fn thrown_rethrow(value: Value) -> Self {
        Self::Thrown {
            value,
            profiler_kind: ProfilerErrorKind::Rethrow,
        }
    }
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
///
/// The per-frame format is intentionally mirrored by `bridge_ctypes`'s
/// `format_traceback_lines` (the per-frame wire form for the host); keep the
/// two in sync.
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
