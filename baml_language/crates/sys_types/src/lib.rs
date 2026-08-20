//! BEX Sys - System operations for the BEX runtime.
//!
//! This crate provides external I/O operations (file system, network, shell)
//! that the BEX engine can dispatch to. Operations receive and return
//! `BexExternalValue` directly.

use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use async_trait::async_trait;
// Re-export BexExternalValue and BexValue for ops
pub use bex_external_types::{AsBexExternalValue, BexExternalValue, Handle};
pub use bex_heap::BexHeap;
// Re-export SysOp for convenience
pub use bex_vm_types::SysOp;
pub use tokio_util::sync::CancellationToken;

pub mod sse;

/// Outcome of [`resolve_name`].
pub enum ResolveOutcome<'a, T> {
    /// A unique match was found via exact, `user.{name}`, or unambiguous
    /// suffix lookup. The `&str` is the canonical key in the map.
    Found(&'a str, &'a T),
    /// No exact or `user.{name}` match, and two or more keys end with
    /// `.{name}` — caller should treat as not found *or* as an error,
    /// depending on the surface (CLI: not found; client `$new`: error,
    /// since clients must be unique within a package).
    Ambiguous,
    /// No match at all.
    NotFound,
}

impl<'a, T> ResolveOutcome<'a, T> {
    /// Treat `Ambiguous` and `NotFound` identically. Right for surfaces
    /// where suffix-scan ambiguity is just "no match" UX.
    pub fn found(self) -> Option<(&'a str, &'a T)> {
        match self {
            Self::Found(k, v) => Some((k, v)),
            Self::Ambiguous | Self::NotFound => None,
        }
    }
}

/// Canonical function-name resolver shared by every lookup surface
/// (engine call dispatch, `baml run`/`baml pack` CLI, sysop LLM-function
/// + client-`$new` lookups).
///
/// All user code is namespaced under the `user` package, so user-defined
/// identifiers can be referred to either by their qualified name
/// (`user.main`) or their bare display name (`main`). LLM functions
/// declared inside a namespace (e.g. `ns_lorem/`) also need to resolve
/// from bare-name companion calls that pre-date the namespace prefix.
///
/// The rule, applied in order, is:
///   1. Exact match.
///   2. Try `user.{name}` (compiler2 always namespaces user code under
///      `user`).
///   3. Unambiguous suffix match on `.{name}`. Two or more suffix matches
///      yield `Ambiguous` so callers can choose between "not found" UX
///      and a hard error (synthesized constructors require uniqueness).
///
/// Lookups that pass a fully qualified name hit step 1 and never observe
/// 2 or 3. Lookups that pass a bare name fall through to whichever step
/// matches.
pub fn resolve_name<'a, T>(
    map: &'a std::collections::HashMap<String, T>,
    name: &str,
) -> ResolveOutcome<'a, T> {
    if let Some((k, v)) = map.get_key_value(name) {
        return ResolveOutcome::Found(k.as_str(), v);
    }
    let qualified = format!("user.{name}");
    if let Some((k, v)) = map.get_key_value(&qualified) {
        return ResolveOutcome::Found(k.as_str(), v);
    }
    let suffix = format!(".{name}");
    let mut matches = map.iter().filter(|(k, _)| k.ends_with(&suffix));
    let Some(first) = matches.next() else {
        return ResolveOutcome::NotFound;
    };
    if matches.next().is_some() {
        return ResolveOutcome::Ambiguous;
    }
    ResolveOutcome::Found(first.0.as_str(), first.1)
}

/// Types generated from the stdlib `.baml` sources.
/// NOTE: sys_ops also generates the same code via its own build.rs because the
/// generated IO traits contain blanket impls that must live in the crate that
/// defines the SysOps struct (orphan rule).
#[allow(warnings, clippy::all, clippy::pedantic)]
pub mod generated {

    pub use bex_external_types::{AsBexExternalValue, BexExternalValue};
    pub use bex_heap::{AccessError, BexClass, BexValue, BuiltinClass, PermitProof};
    pub use bex_vm_types::{self, SysOp};

    pub use crate::{BexHeap, CallId, OpError, SysOpContext, SysOpFn, SysOpOutput, SysOpResult};

    include!(concat!(env!("OUT_DIR"), "/io_generated.rs"));
}

/// Typed async IO trait generated from `.baml` `$rust_io_function` definitions.
///
/// Provides a clean async interface to all sys-ops without VM plumbing
/// (`BexHeap`, `SysOpContext`, `CallId`). The adapter implementation lives in
/// `sys_ops` and bridges to the `SysOpFn` pointers.
#[allow(warnings, clippy::all, clippy::pedantic)]
pub mod runtime_io {
    pub use bex_external_types::BexExternalValue;

    pub use super::generated::owned;

    include!(concat!(env!("OUT_DIR"), "/runtime_io.rs"));
}

// ============================================================================
// CallId — opaque per-call identifier
// ============================================================================

/// Opaque per-call identifier. Passed to every `sys_op` for call correlation.
///
/// The playground uses this to associate fetch logs with the function call
/// that triggered them. Callers that don't need tracking pass `CallId::next()`.
/// Use `CallId::next()` for a unique ID per call (e.g. from bridges with concurrent calls).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CallId(pub u64);

// Start at 1M to reserve lower IDs for internal/test use
static NEXT_CALL_ID: AtomicU64 = AtomicU64::new(1_000_000);

impl CallId {
    /// Returns a fresh call ID that is unique across the process. Use this from
    /// bridges (e.g. Python) when multiple overlapping calls can occur.
    #[inline]
    pub fn next() -> Self {
        CallId(NEXT_CALL_ID.fetch_add(1, Ordering::Relaxed))
    }
}

// ============================================================================
// Operation Errors
// ============================================================================

impl std::fmt::Display for CallId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CallId({})", self.0)
    }
}

/// Errors that can occur during external operation execution. Every error
/// is tied to the operation (`fn_name`) that was being called; the inner
/// `payload` is either a normal VM error (treated like a `$rust_function`
/// error or `throw` opcode and delivered through the VM's exception-
/// unwinding path) or a *host throw* — a decoded `BexExternalValue` that
/// the engine runs through `materialize_host_throw` for the
/// declared-throws contract check against the surrounding callable's `E`.
#[derive(Debug, PartialEq, Clone)]
pub struct OpError {
    pub fn_name: SysOp,
    pub payload: OpErrorPayload,
}

/// Payload of an [`OpError`] — exactly one of:
/// - [`Self::Vm`]: a normal sysop error (catchable `BamlError`, panic,
///   internal fault, or pre-built thrown value).
/// - [`Self::HostThrown`]: the bridge invoked a host callable and the
///   callable raised something the bridge decoded to a `BexExternalValue`.
///   The engine's `materialize_host_throw` reads this and either re-injects
///   the value as a catchable throw (on-contract for `E`) or escalates to
///   `baml.panics.HostContractViolation` (off-contract). The rehydration
///   handle, when set, lives on the value itself as the `_handle` field
///   of a `baml.errors.HostCallable` Instance.
#[derive(Debug, PartialEq, Clone)]
pub enum OpErrorPayload {
    Vm(bex_vm_types::errors::VmRustFnError),
    /// Boxed because `BexExternalValue` is ~3x the size of
    /// `VmRustFnError`; keeping it inline would bloat every `OpError` on
    /// the hot success path. The host-thrown payload is rare and the
    /// extra indirection only fires on the throw branch.
    HostThrown(Box<BexExternalValue>),
}

impl OpErrorPayload {
    /// Short, human-facing summary for `Display`. The engine reads the
    /// payload structurally — this is purely for diagnostic output.
    fn display_summary(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Vm(e) => std::fmt::Display::fmt(e, f),
            Self::HostThrown(boxed) => match boxed.as_ref() {
                BexExternalValue::Instance {
                    class_name, fields, ..
                } => {
                    let message = fields.get("message").and_then(|v| match v {
                        BexExternalValue::String(s) => Some(s.as_str()),
                        _ => None,
                    });
                    match message {
                        Some(m) => write!(f, "host thrown {class_name}: {m}"),
                        None => write!(f, "host thrown {class_name}"),
                    }
                }
                other => write!(f, "host thrown {other:?}"),
            },
        }
    }
}

impl std::fmt::Display for OpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "failed to call {}: ", self.fn_name)?;
        self.payload.display_summary(f)
    }
}

impl std::error::Error for OpError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.payload {
            OpErrorPayload::Vm(e) => e.source(),
            OpErrorPayload::HostThrown(_) => None,
        }
    }
}

impl OpError {
    fn cancelled(operation: SysOp) -> Self {
        Self {
            fn_name: operation,
            payload: OpErrorPayload::Vm(bex_vm_types::errors::VmRustFnError::Panic(
                bex_vm_types::errors::VmPanic::Cancelled,
            )),
        }
    }

    /// Construct an `OpError` for `fn_name` with any error convertible to
    /// `VmRustFnError`. The `Into` bound lets sysop impls pass
    /// `VmBamlError::X { ... }` / `VmPanic::X { ... }` (auto-converted via
    /// their `From` impls) or a `VmRustFnError` directly.
    pub fn new(fn_name: SysOp, kind: impl Into<bex_vm_types::errors::VmRustFnError>) -> Self {
        Self {
            fn_name,
            payload: OpErrorPayload::Vm(kind.into()),
        }
    }

    /// Construct an `OpError` carrying a host-throw payload — the host
    /// language invoked a callable and the callable raised something the
    /// bridge decoded to `value`. The engine routes this through
    /// `materialize_host_throw`.
    pub fn host_thrown_value(fn_name: SysOp, value: BexExternalValue) -> Self {
        Self {
            fn_name,
            payload: OpErrorPayload::HostThrown(Box::new(value)),
        }
    }
}

pub use bex_vm_types::{
    SysOpErrorCategory, SysOpPanicCategory,
    errors::{VmBamlError, VmInternalError, VmPanic, VmRustFnError},
};

/// Sysop-body error carrier: an [`OpError`] without the `fn_name`.
///
/// Used as the error type of [`SysOpOutput::Async`] futures so the body
/// never has to know which [`SysOp`] variant it belongs to; the glue layer
/// attaches `fn_name` at the [`SysOpOutput::into_result`] boundary.
#[derive(Debug)]
pub struct OpErrorBody {
    pub payload: OpErrorPayload,
}

impl OpErrorBody {
    /// Attach the originating [`SysOp`], yielding a full [`OpError`] ready
    /// for [`SysOpResult`].
    pub fn into_op_error(self, fn_name: SysOp) -> OpError {
        OpError {
            fn_name,
            payload: self.payload,
        }
    }
}

impl std::fmt::Display for OpErrorBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.payload.display_summary(f)
    }
}

/// Blanket `?`-coercion: any error type that converts into
/// [`VmRustFnError`] (i.e. [`VmBamlError`], [`VmPanic`], [`VmRustFnError`]
/// itself, etc.) auto-converts into an [`OpErrorBody`] with payload
/// [`OpErrorPayload::Vm`].
impl<E: Into<bex_vm_types::errors::VmRustFnError>> From<E> for OpErrorBody {
    fn from(kind: E) -> Self {
        Self {
            payload: OpErrorPayload::Vm(kind.into()),
        }
    }
}

/// `?` conversion for sysop bodies that await a `SysOpResult::Async` future
/// (whose error type is the full [`OpError`]) inside an `async_op` closure
/// returning `Result<_, OpErrorBody>`. Preserves the payload verbatim
/// (host-throw value or VM error); `fn_name` is dropped because the
/// surrounding closure will have its own [`SysOp`] attached at the
/// [`SysOpOutput::into_result`] boundary.
impl From<OpError> for OpErrorBody {
    fn from(op_err: OpError) -> Self {
        Self {
            payload: op_err.payload,
        }
    }
}

// ============================================================================
// Contract Enforcement
// ============================================================================

/// A `sys_op` returned an error category not declared in its `#[throws(...)]` contract.
#[derive(Debug)]
pub struct ContractViolation {
    pub op: SysOp,
    pub actual_category: SysOpErrorCategory,
}

impl std::fmt::Display for ContractViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "sys_op contract violation: `{}` returned error category `{}` \
             which is not in its declared #[throws(...)] contract (allowed: {:?})",
            self.op,
            self.actual_category,
            self.op.allowed_error_categories()
        )
    }
}

/// Validate that a `sys_op` error conforms to its declared contract.
///
/// Returns `Ok(())` if the error category is in the allowed set, or
/// `Err(ContractViolation)` with details for the implementer.
///
/// Only the [`VmRustFnError::BamlError`] variant has a contract-level
/// category to check. [`VmRustFnError::Panic`] is governed by the parallel
/// `#[panics(...)]` contract (not enforced here yet);
/// [`VmRustFnError::Thrown`] carries a pre-built host-provided exception
/// `Value` whose type is checked against the callable's `E` in the engine,
/// not against a sysop category; and [`VmRustFnError::InternalError`] is a
/// fatal VM/engine fault outside the contract surface.
pub fn validate_sys_op_error(op: SysOp, err: &VmRustFnError) -> Result<(), ContractViolation> {
    let baml_err = match err {
        VmRustFnError::BamlError(b) => b,
        VmRustFnError::Panic(_)
        | VmRustFnError::Thrown { .. }
        | VmRustFnError::InternalError(_) => {
            return Ok(());
        }
    };
    let category = baml_err.category();
    let allowed = op.allowed_error_categories();
    if allowed.is_empty() || allowed.contains(&category) {
        Ok(())
    } else {
        Err(ContractViolation {
            op,
            actual_category: category,
        })
    }
}

// ============================================================================
// Operation Results
// ============================================================================

/// A boxed future for async operations. The error type is [`OpError`],
/// which wraps the sysop body's [`VmRustFnError`] together with the
/// originating [`SysOp`] for engine-side attribution. The body itself
/// works with the bare `VmRustFnError`; the `SysOp` is attached at the
/// `into_result` glue boundary (see [`SysOpOutput::into_result`]).
pub type OpFuture = Pin<Box<dyn Future<Output = Result<BexExternalValue, OpError>> + Send>>;

/// Result of a system operation - either immediate or async.
#[allow(clippy::large_enum_variant)]
pub enum SysOpResult {
    /// Operation completed synchronously with this result.
    Ready(Result<BexExternalValue, OpError>),
    /// Operation is async and needs to be awaited.
    Async(OpFuture),
}

// ============================================================================
// SysOpOutput — Clean return type for trait-based sys_op implementations
// ============================================================================

/// Clean return type for `sys_op` trait methods, generic over the success value.
///
/// Like [`SysOpResult`] but carries [`VmRustFnError`] without the
/// `SysOp`-attribution wrapper — the implementor never needs to specify
/// which [`SysOp`] variant they are. The generated glue code wraps this
/// into a full [`SysOpResult`] via [`into_result`](SysOpOutput::into_result),
/// which converts `T` into [`BexExternalValue`] using [`AsBexExternalValue`]
/// and attaches the `SysOp`.
#[allow(clippy::large_enum_variant)]
pub enum SysOpOutput<T = BexExternalValue> {
    /// Operation completed synchronously.
    Ready(Result<T, bex_vm_types::errors::VmRustFnError>),
    /// Operation is async. The future yields an [`OpErrorBody`] on failure
    /// so that bodies awaiting an inner [`SysOpResult::Async`] (whose
    /// error type is [`OpError`]) can propagate fields beyond `kind`
    /// — notably `host_thrown`, set by
    /// `sys_native::host_dispatch::complete_with_throw` — through `?`
    /// without going through a lossy intermediate type.
    ///
    /// Most sysop bodies just `?` from `VmBamlError`/`VmRustFnError`,
    /// which auto-convert via [`OpErrorBody`]'s blanket `From` impl with
    /// `host_thrown = None`. The host-callable sysop is the one site
    /// that actively threads a non-`None` `host_thrown`.
    Async(Pin<Box<dyn Future<Output = Result<T, OpErrorBody>> + Send>>),
}

impl<T> SysOpOutput<T> {
    /// Create a successful synchronous result.
    pub fn ok(value: T) -> Self {
        Self::Ready(Ok(value))
    }

    /// Create a synchronous error. The `Into` bound lets sysop impls pass
    /// `VmBamlError::X { ... }` / `VmPanic::X { ... }` / a pre-built
    /// `VmRustFnError` interchangeably.
    pub fn err(err: impl Into<bex_vm_types::errors::VmRustFnError>) -> Self {
        Self::Ready(Err(err.into()))
    }
}

impl<T: Send + 'static> SysOpOutput<T> {
    /// Create an async result from a future yielding [`VmRustFnError`].
    ///
    /// `VmRustFnError` is the right type because it carries every shape a
    /// sysop body might produce: a catchable `BamlError(VmBamlError)`, a
    /// `Panic(VmPanic)` (e.g. `Cancelled`), a fatal `InternalError`, or a
    /// pre-built `Thrown(Value)`. Restricting to `VmBamlError` would lose
    /// the panic and internal-error shapes.
    ///
    /// The `?` operator inside the closure auto-converts any error type
    /// with a `From` impl to `VmRustFnError` (e.g. `VmBamlError` /
    /// `VmPanic` / `VmInternalError`) — so propagation from helper
    /// functions is transparent. Direct `Err(VmBamlError::X { … })`
    /// returns still need an explicit `.into()` because `Err` doesn't
    /// trigger the `?`-coercion path.
    ///
    /// We deliberately keep the bound concrete (rather than `impl
    /// Into<VmRustFnError>`) because closure-body inference can't pick a
    /// unique error type when `?` is used with a generic outer type, which
    /// triggers `E0283` ambiguity.
    pub fn async_op(
        fut: impl Future<Output = Result<T, bex_vm_types::errors::VmRustFnError>> + Send + 'static,
    ) -> Self {
        Self::Async(Box::pin(
            async move { fut.await.map_err(OpErrorBody::from) },
        ))
    }

    /// Async constructor for bodies that need to thread a non-`None`
    /// `host_thrown` through to the engine. Only the host-callable sysop
    /// uses this; every other sysop calls [`Self::async_op`] and lets the
    /// blanket `VmRustFnError → OpErrorBody` conversion fill `host_thrown`
    /// with `None`.
    pub fn async_op_with_throw(
        fut: impl Future<Output = Result<T, OpErrorBody>> + Send + 'static,
    ) -> Self {
        Self::Async(Box::pin(fut))
    }
}

impl<T: AsBexExternalValue + Send + 'static> SysOpOutput<T> {
    /// Convert to [`SysOpResult`] by attaching the [`SysOp`] variant to errors
    /// and converting `T` into [`BexExternalValue`] via [`AsBexExternalValue`].
    ///
    /// This is called by generated glue code — implementors don't use this directly.
    pub fn into_result(self, op: SysOp) -> SysOpResult {
        match self {
            Self::Ready(Ok(v)) => SysOpResult::Ready(Ok(v.into_bex_external_value())),
            Self::Ready(Err(err)) => SysOpResult::Ready(Err(OpError::new(op, err))),
            Self::Async(fut) => SysOpResult::Async(Box::pin(async move {
                fut.await
                    .map(AsBexExternalValue::into_bex_external_value)
                    .map_err(|body| body.into_op_error(op))
            })),
        }
    }
}

impl<T: Send + 'static> SysOpOutput<T> {
    /// Convert to [`SysOpResult`] using a custom value mapping function.
    ///
    /// Used by generated glue code for return types that don't implement
    /// [`AsBexExternalValue`] directly (e.g. `Vec<ClassName>`).
    pub fn into_result_mapped(
        self,
        op: SysOp,
        f: impl Fn(T) -> BexExternalValue + Send + 'static,
    ) -> SysOpResult {
        match self {
            Self::Ready(Ok(v)) => SysOpResult::Ready(Ok(f(v))),
            Self::Ready(Err(err)) => SysOpResult::Ready(Err(OpError::new(op, err))),
            Self::Async(fut) => SysOpResult::Async(Box::pin(async move {
                fut.await.map(f).map_err(|body| body.into_op_error(op))
            })),
        }
    }
}

// ============================================================================
// System Operations Table
// ============================================================================

/// Function pointer type for system operations.
///
/// Each operation takes a heap reference, arguments, and a context reference,
/// returning a `SysOpResult` which is either an immediate result or a future to await.
///
/// The heap reference plus a [`PermitProof`](bex_heap::PermitProof) (proving GC-exclusion) lets ops
/// safely access instance fields via the heap accessor APIs.
/// Arguments are `BexValue` which can be either:
/// - `BexValue::External(...)` for primitives/strings copied from VM
/// - `BexValue::Opaque(Handle)` for heap objects (instances, arrays, maps)
///
/// The context reference provides engine-level information (e.g., function metadata)
/// that some `sys_ops` need. Ops that don't need it simply ignore the parameter.
pub type SysOpFn = Arc<
    dyn for<'a> Fn(
            &Arc<BexHeap>,
            bex_heap::PermitProof<'a>,
            Vec<bex_heap::BexValue<'a>>,
            &SysOpContext,
            CallId,
        ) -> SysOpResult
        + Send
        + Sync,
>;

// ============================================================================
// Engine Context for Sys Ops
// ============================================================================

/// A type that is able to spawn new VMs then return the value.
/// Generally this is `BexEngine`.
///
/// This is used by sys ops that want to run code in a separate VM/thread.
///
/// This needs to be a separate trait because `sys_types` cannot import `bex_engine`
/// due to a circular dependency.
#[async_trait]
pub trait VmSpawner<E: Send + Sync + 'static = Box<dyn Send + Sync + 'static>>:
    Send + Sync
{
    /// Spawn a a new VM with the given function name and arguments.
    ///
    /// Generally just calls `BexEngine::call_function`.
    async fn spawn_with_function(
        self: Arc<Self>,
        function_name: String,
        args: Vec<BexExternalValue>,
        cancel: CancellationToken,
    ) -> Result<BexExternalValue, E>;

    /// Spawn a new VM that invokes a callable BAML value — a raw function,
    /// closure, or bound method — referenced by a [`bex_external_types::Handle`]
    /// obtained from a sys-op's `function`-typed argument, with `args`, and
    /// return its result.
    ///
    /// Unlike [`Self::spawn_with_function`], the callee need not be a named
    /// global — this is how a sys-op invokes a BAML callback (e.g. an HTTP
    /// server `handler`). Generally just calls `BexEngine::call_callable`.
    async fn spawn_with_callable(
        self: Arc<Self>,
        callable: bex_external_types::Handle,
        args: Vec<BexExternalValue>,
        cancel: CancellationToken,
    ) -> Result<BexExternalValue, E>;
}

/// Context available to `sys_ops` that need engine-level information.
///
/// Most `sys_ops` don't need this — only those marked with `#[uses(engine_ctx)]`
/// in the DSL use it. The engine populates this at construction time.
///
/// All `sys_ops` receive `&SysOpContext` for signature uniformity (keeps `SysOpFn`
/// as a plain `fn` pointer). Ops that don't use it ignore the parameter.
///
/// # Per-call fields
///
/// [`SysOpContext`] is the per-call version of [`EngineSysOpContext`].
// Manual Clone impl: the derive would add an unnecessary `E: Clone` bound,
// but no field actually stores `E` directly (only `Arc<dyn VmSpawner<E>>`).
pub struct SysOpContext<E: Send + Sync + 'static = Box<dyn Send + Sync + 'static>> {
    /// Pre-extracted LLM function metadata, keyed by function name.
    /// Used by LLM ops that need to look up function prompt templates, client names, etc.
    pub llm_functions: Arc<std::collections::HashMap<String, LlmFunctionInfo>>,

    /// Maps function names to their global indices in the VM.
    /// Used by `resolve_client` to return `FunctionRef` values.
    pub function_global_indices: Arc<std::collections::HashMap<String, usize>>,

    /// Per-call cancellation token.
    ///
    /// Defaults to a never-cancelled token for the shared engine context.
    /// In `execute_sys_op`, a per-call clone is created with the real token.
    pub cancel: CancellationToken,

    /// Pre-extracted class definitions for output format rendering.
    /// Keyed by class name.
    pub class_definitions: Arc<indexmap::IndexMap<baml_type::TypeName, ClassDefinition>>,

    /// Pre-extracted enum definitions for output format rendering.
    /// Keyed by enum name.
    pub enum_definitions: Arc<indexmap::IndexMap<baml_type::TypeName, EnumDefinition>>,

    /// Recursive type alias definitions for output format rendering.
    /// Only recursive aliases are stored (non-recursive ones are expanded inline).
    /// Maps alias name → target type.
    pub type_alias_definitions: Arc<indexmap::IndexMap<baml_type::TypeName, baml_type::RuntimeTy>>,

    /// Can be used to spawn new VMs.
    pub spawner: Arc<dyn VmSpawner<E>>,

    /// Typed async IO interface for calling back into the runtime IO layer.
    /// Built once by the engine from the `SysOps` table and shared across calls.
    pub runtime_io: Arc<dyn runtime_io::RuntimeIo>,
}

impl<E: Send + Sync + 'static> Clone for SysOpContext<E> {
    fn clone(&self) -> Self {
        Self {
            llm_functions: self.llm_functions.clone(),
            function_global_indices: self.function_global_indices.clone(),
            cancel: self.cancel.clone(),
            class_definitions: self.class_definitions.clone(),
            enum_definitions: self.enum_definitions.clone(),
            type_alias_definitions: self.type_alias_definitions.clone(),
            spawner: self.spawner.clone(),
            runtime_io: self.runtime_io.clone(),
        }
    }
}

/// The shared part of [`SysOpContext`]. Used in `sys_ops` that need engine-level information.
/// When passing to a sys op, convert to [`SysOpContext`] with `to_op_context`.
#[derive(Clone)]
pub struct EngineSysOpContext {
    /// Pre-extracted LLM function metadata, keyed by function name.
    /// Used by LLM ops that need to look up function prompt templates, client names, etc.
    pub llm_functions: Arc<std::collections::HashMap<String, LlmFunctionInfo>>,

    /// Maps function names to their global indices in the VM.
    /// Used by `resolve_client` to return `FunctionRef` values.
    pub function_global_indices: Arc<std::collections::HashMap<String, usize>>,

    /// Pre-extracted class definitions for output format rendering.
    /// Keyed by class name.
    pub class_definitions: Arc<indexmap::IndexMap<baml_type::TypeName, ClassDefinition>>,

    /// Pre-extracted enum definitions for output format rendering.
    /// Keyed by enum name.
    pub enum_definitions: Arc<indexmap::IndexMap<baml_type::TypeName, EnumDefinition>>,

    /// Recursive type alias definitions for output format rendering.
    /// Only recursive aliases are stored (non-recursive ones are expanded inline).
    /// Maps alias name → target type.
    pub type_alias_definitions: Arc<indexmap::IndexMap<baml_type::TypeName, baml_type::RuntimeTy>>,

    /// Typed async IO interface, built from the `SysOps` table at engine init.
    pub runtime_io: Arc<dyn runtime_io::RuntimeIo>,
}

/// Pre-extracted metadata for an LLM function.
///
/// This is built during engine construction by reading function objects from the heap,
/// so that LLM `sys_ops` don't need to access raw heap pointers.
pub struct LlmFunctionInfo {
    /// The client name (e.g., `"MyClient"`) declared in the function.
    pub client_name: String,
    /// The expected return type, used for response parsing.
    pub return_type: baml_type::RuntimeTy,
}

/// Pre-extracted class definition for output format rendering.
#[derive(Clone, Debug)]
pub struct ClassDefinition {
    pub name: String,
    pub description: Option<String>,
    pub alias: Option<String>,
    pub fields: Vec<ClassFieldDefinition>,
}

/// A field in a pre-extracted class definition.
#[derive(Clone, Debug)]
pub struct ClassFieldDefinition {
    pub name: String,
    pub field_type: baml_type::RuntimeTy,
    /// Symbolic field type with class-level generic references preserved.
    ///
    /// `field_type` is intentionally erased for some runtime paths, so
    /// output-format rendering uses this template when a class is visited
    /// with concrete type arguments. `None` is retained for synthetic/test
    /// definitions that only provide an already-realized `field_type`.
    pub field_template: Option<baml_type::TyTemplate>,
    pub description: Option<String>,
    pub alias: Option<String>,
    pub skip: bool,
}

/// Pre-extracted enum definition for output format rendering.
#[derive(Clone, Debug)]
pub struct EnumDefinition {
    pub name: String,
    pub description: Option<String>,
    pub alias: Option<String>,
    pub variants: Vec<EnumVariantDefinition>,
}

/// A variant in a pre-extracted enum definition.
#[derive(Clone, Debug)]
pub struct EnumVariantDefinition {
    pub name: String,
    pub description: Option<String>,
    pub alias: Option<String>,
}

impl SysOpContext {
    /// Create an empty context (for testing or when no LLM functions exist).
    pub fn empty() -> Self {
        struct NeverSpawner;
        #[async_trait]
        impl VmSpawner for NeverSpawner {
            async fn spawn_with_function(
                self: Arc<Self>,
                _function_name: String,
                _args: Vec<BexExternalValue>,
                _cancel: CancellationToken,
            ) -> Result<BexExternalValue, Box<dyn Send + Sync + 'static>> {
                Err(Box::new(
                    "VmSpawner::spawn_with_function called on NeverSpawner (empty/test context)",
                ))
            }

            async fn spawn_with_callable(
                self: Arc<Self>,
                _callable: bex_external_types::Handle,
                _args: Vec<BexExternalValue>,
                _cancel: CancellationToken,
            ) -> Result<BexExternalValue, Box<dyn Send + Sync + 'static>> {
                Err(Box::new(
                    "VmSpawner::spawn_with_callable called on NeverSpawner (empty/test context)",
                ))
            }
        }
        Self {
            llm_functions: Arc::new(std::collections::HashMap::new()),
            function_global_indices: Arc::new(std::collections::HashMap::new()),
            cancel: CancellationToken::new(),
            class_definitions: Arc::new(
                indexmap::IndexMap::<baml_type::TypeName, ClassDefinition>::new(),
            ),
            enum_definitions: Arc::new(
                indexmap::IndexMap::<baml_type::TypeName, EnumDefinition>::new(),
            ),
            type_alias_definitions: Arc::new(indexmap::IndexMap::<
                baml_type::TypeName,
                baml_type::RuntimeTy,
            >::new()),
            spawner: Arc::new(NeverSpawner),
            runtime_io: Arc::new(runtime_io::NoopRuntimeIo),
        }
    }
}

impl EngineSysOpContext {
    /// Convert to [`SysOpContext`] for passing to a sys op.
    pub fn to_op_context(
        &self,
        cancel: CancellationToken,
        spawner: Arc<dyn VmSpawner>,
    ) -> SysOpContext {
        SysOpContext {
            llm_functions: self.llm_functions.clone(),
            function_global_indices: self.function_global_indices.clone(),
            cancel,
            class_definitions: self.class_definitions.clone(),
            enum_definitions: self.enum_definitions.clone(),
            type_alias_definitions: self.type_alias_definitions.clone(),
            spawner,
            runtime_io: self.runtime_io.clone(),
        }
    }
}

// ============================================================================
// FunctionRef<T> -- Typed wrapper for VM function references
// ============================================================================

/// Typed wrapper for VM function references.
///
/// The phantom type parameter `T` represents the return type of the referenced
/// function. It provides no runtime checking, but ensures the impl author
/// declares what kind of function they're returning — preventing accidental
/// misuse of the `BexExternalValue` escape hatch.
pub struct FunctionRef<T> {
    /// The global index into the VM's globals array.
    pub global_index: usize,
    _phantom: std::marker::PhantomData<T>,
}

impl<T> FunctionRef<T> {
    /// Create a new function reference with the given global index.
    pub fn new(global_index: usize) -> Self {
        Self {
            global_index,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Convert to `BexExternalValue::FunctionRef`.
    pub fn into_external(self) -> BexExternalValue {
        BexExternalValue::FunctionRef {
            global_index: self.global_index,
        }
    }
}
// ============================================================================
// Async Completion Utilities
// ============================================================================

/// Handle for completing an async operation from external code.
///
/// This is used for FFI async bridging - the host language receives this handle
/// and calls `complete()` when the operation finishes.
///
/// # Example
///
/// ```ignore
/// // In the binding code:
/// let (result, handle) = SysOpResult::pending();
/// spawn_python_task(move || {
///     let data = python_http_get(url);
///     handle.complete(Ok(BexExternalValue::String(data)));
/// });
/// return result;  // Returns the future to the engine
/// ```
pub struct CompletionHandle(tokio::sync::oneshot::Sender<Result<BexExternalValue, OpError>>);

impl CompletionHandle {
    /// Complete the async operation with the given result.
    ///
    /// This resolves the future returned by `SysOpResult::pending()`.
    pub fn complete(self, result: Result<BexExternalValue, OpError>) {
        // Ignore send error - receiver was dropped (operation cancelled)
        let _ = self.0.send(result);
    }
}

impl SysOpResult {
    /// Create a pending async result that can be completed externally.
    ///
    /// Returns a tuple of:
    /// - `SysOpResult::Async` containing the future
    /// - `CompletionHandle` to complete the operation
    ///
    /// The future will resolve when `handle.complete()` is called.
    pub fn pending(operation: SysOp) -> (Self, CompletionHandle) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let future =
            Box::pin(async move { rx.await.unwrap_or(Err(OpError::cancelled(operation))) });
        (SysOpResult::Async(future), CompletionHandle(tx))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn map(pairs: &[(&str, i32)]) -> HashMap<String, i32> {
        pairs.iter().map(|(k, v)| ((*k).to_string(), *v)).collect()
    }

    #[test]
    fn resolve_name_exact_match_wins() {
        let m = map(&[("main", 1), ("user.main", 2)]);
        match resolve_name(&m, "main") {
            ResolveOutcome::Found(k, v) => {
                assert_eq!(k, "main");
                assert_eq!(*v, 1);
            }
            _ => panic!("expected Found(main, 1)"),
        }
    }

    #[test]
    fn resolve_name_user_prefix_fallback() {
        let m = map(&[("user.main", 2), ("baml.json.serialize", 3)]);
        match resolve_name(&m, "main") {
            ResolveOutcome::Found(k, v) => {
                assert_eq!(k, "user.main");
                assert_eq!(*v, 2);
            }
            _ => panic!("expected Found(user.main, 2)"),
        }
    }

    #[test]
    fn resolve_name_suffix_scan_unique() {
        // Namespaced function — bare lookup falls through to suffix scan.
        let m = map(&[("user.lorem.Summarize", 7), ("baml.json.serialize", 3)]);
        match resolve_name(&m, "Summarize") {
            ResolveOutcome::Found(k, v) => {
                assert_eq!(k, "user.lorem.Summarize");
                assert_eq!(*v, 7);
            }
            _ => panic!("expected Found(user.lorem.Summarize, 7)"),
        }
    }

    #[test]
    fn resolve_name_suffix_scan_ambiguous() {
        let m = map(&[("a.Foo", 1), ("b.Foo", 2)]);
        assert!(matches!(resolve_name(&m, "Foo"), ResolveOutcome::Ambiguous));
    }

    #[test]
    fn resolve_name_not_found() {
        let m = map(&[("user.main", 1)]);
        assert!(matches!(resolve_name(&m, "Nope"), ResolveOutcome::NotFound));
    }

    /// Exact match must win over a suffix collision elsewhere in the map,
    /// so a fully qualified call can't be diverted by an unrelated `.{name}`
    /// key. Regression: this is the property `BexEngine::lookup_function`
    /// relies on for stdlib calls like `baml.json.serialize`.
    #[test]
    fn resolve_name_exact_beats_suffix() {
        let m = map(&[("baml.json.serialize", 1), ("user.lorem.serialize", 2)]);
        match resolve_name(&m, "baml.json.serialize") {
            ResolveOutcome::Found(k, v) => {
                assert_eq!(k, "baml.json.serialize");
                assert_eq!(*v, 1);
            }
            _ => panic!("expected Found(baml.json.serialize, 1)"),
        }
    }

    #[test]
    fn resolve_outcome_found_collapses_ambiguous_to_none() {
        let m = map(&[("a.Foo", 1), ("b.Foo", 2)]);
        assert!(resolve_name(&m, "Foo").found().is_none());
        assert!(resolve_name(&m, "Nope").found().is_none());
        assert!(resolve_name(&m, "a.Foo").found().is_some());
    }

    #[tokio::test]
    async fn test_completion_handle() {
        let (result, handle) = SysOpResult::pending(SysOp::BamlSysShell);

        // Complete in another task
        tokio::spawn(async move {
            handle.complete(Ok(BexExternalValue::String("done".into())));
        });

        // Await the result
        match result {
            SysOpResult::Async(fut) => {
                let value = fut.await.unwrap();
                assert!(matches!(value, BexExternalValue::String(s) if s.as_str() == "done"));
            }
            SysOpResult::Ready(_) => panic!("Expected Async result"),
        }
    }

    // ========================================================================
    // Contract enforcement tests
    // ========================================================================

    #[test]
    fn contract_allows_declared_category() {
        // `baml.http.fetch` is a thin BAML wrapper; the sys-op (which carries
        // the `throws Io | Timeout` contract) is `baml.http._fetch`.
        let op = bex_vm_types::sys_op_for_path("baml.http._fetch").unwrap();
        let err: bex_vm_types::errors::VmRustFnError = bex_vm_types::errors::VmBamlError::Timeout {
            message: "timed out".into(),
            duration_ms: Some(30_000),
        }
        .into();
        assert!(validate_sys_op_error(op, &err).is_ok());
    }

    #[test]
    fn contract_rejects_undeclared_category() {
        let op = bex_vm_types::sys_op_for_path("baml.env.get").unwrap();
        let err: bex_vm_types::errors::VmRustFnError =
            bex_vm_types::errors::VmBamlError::LlmClient {
                message: "bad".into(),
            }
            .into();
        let result = validate_sys_op_error(op, &err);
        assert!(result.is_err());
        let violation = result.unwrap_err();
        assert_eq!(violation.actual_category, SysOpErrorCategory::LlmClient);
    }

    #[test]
    fn contract_allows_devother_when_declared() {
        // `baml.http.fetch` is a thin BAML wrapper; the sys-op (which carries
        // the `throws Io | Timeout` contract) is `baml.http._fetch`.
        let op = bex_vm_types::sys_op_for_path("baml.http._fetch").unwrap();
        let err: bex_vm_types::errors::VmRustFnError =
            bex_vm_types::errors::VmBamlError::DevOther {
                message: "some debug detail".into(),
            }
            .into();
        let result = validate_sys_op_error(op, &err);
        assert!(
            result.is_err(),
            "DevOther should be rejected when not in #[throws]"
        );
    }

    /// `Panic` / `Thrown` / `InternalError` variants of `VmRustFnError` are
    /// outside the `#[throws(...)]` contract surface; validation must let
    /// them pass even on an op that declares no throws categories.
    #[test]
    fn contract_skips_non_baml_error_variants() {
        let op = bex_vm_types::sys_op_for_path("baml.env.get").unwrap();
        for err in [
            bex_vm_types::errors::VmRustFnError::Panic(bex_vm_types::errors::VmPanic::Cancelled),
            bex_vm_types::errors::VmRustFnError::thrown_fresh(bex_vm_types::Value::NULL),
            bex_vm_types::errors::VmRustFnError::InternalError(
                bex_vm_types::errors::VmInternalError::UnexpectedEmptyStack,
            ),
        ] {
            assert!(
                validate_sys_op_error(op, &err).is_ok(),
                "non-BamlError variants must skip contract validation"
            );
        }
    }

    /// An empty `allowed_error_categories` list means "accept any category".
    /// `call_host_value`'s codegen relies on this (it emits `&[]` when the
    /// `throws` contains a generic param like `E` — see
    /// `codegen_io::error_cat_arms`). Anchor that semantics here so a
    /// reviewer who reads `validate_sys_op_error` in isolation sees the
    /// contract is intentional, not a missing check.
    #[test]
    fn contract_empty_allow_list_accepts_any_category() {
        let op = bex_vm_types::sys_op_for_path("baml.host.call_host_value").unwrap();
        // `call_host_value` declares `throws E` (generic only), so codegen
        // emits an empty allow-list.
        assert!(op.allowed_error_categories().is_empty());
        for err in [
            bex_vm_types::errors::VmBamlError::Io {
                message: "io".into(),
            },
            bex_vm_types::errors::VmBamlError::LlmClient {
                message: "llm".into(),
            },
            bex_vm_types::errors::VmBamlError::Unsupported {
                message: "u".into(),
            },
            bex_vm_types::errors::VmBamlError::DevOther {
                message: "x".into(),
            },
        ] {
            let rust_err: bex_vm_types::errors::VmRustFnError = err.into();
            assert!(
                validate_sys_op_error(op, &rust_err).is_ok(),
                "empty allow-list must accept every BAML-error category"
            );
        }
    }

    #[test]
    fn all_sys_ops_have_contract_metadata() {
        use bex_vm_types::SysOp;
        let ops = [SysOp::BamlFsOpen, SysOp::BamlHttpFetch, SysOp::BamlEnvGet];
        for op in ops {
            let cats = op.allowed_error_categories();
            let panics = op.allowed_panic_categories();
            assert!(
                !cats.is_empty() || !panics.is_empty(),
                "sys_op {op} should have at least one contract category",
            );
        }
    }

    /// Every `VmBamlError` variant must map to some `SysOpErrorCategory`
    /// (used by `validate_sys_op_error` for the `#[throws(...)]` contract
    /// check). Exercises each variant once; the assertion is just that
    /// `category()` doesn't panic on any branch.
    #[test]
    fn category_mapping_covers_all_variants() {
        use bex_vm_types::errors::VmBamlError;
        let variants = vec![
            VmBamlError::InvalidArgument {
                message: "bad arg".into(),
            },
            VmBamlError::ParseError {
                message: "parse fail".into(),
            },
            VmBamlError::Io {
                message: "io error".into(),
            },
            VmBamlError::Timeout {
                message: "t".into(),
                duration_ms: Some(1_000),
            },
            VmBamlError::Unsupported {
                message: "u".into(),
            },
            VmBamlError::AccessError {
                message: "a".into(),
            },
            VmBamlError::RenderPrompt {
                message: "r".into(),
            },
            VmBamlError::NotImplemented {
                message: "n".into(),
            },
            VmBamlError::LlmClient {
                message: "l".into(),
            },
            VmBamlError::DevOther {
                message: "test".into(),
            },
            VmBamlError::HostCallable {
                class_name: "C".into(),
                message: "hc".into(),
                traceback: None,
                language: None,
                handle: bex_external_types::HostValueArc::new(
                    1,
                    bex_external_types::HostValueKind::Opaque,
                ),
            },
        ];
        for v in &variants {
            let _ = v.category();
        }
    }
}
