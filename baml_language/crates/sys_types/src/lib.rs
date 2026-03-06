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

// Re-export BexExternalValue and BexValue for ops
pub use bex_external_types::{AsBexExternalValue, BexExternalValue};
pub use bex_heap::BexHeap;
// Re-export SysOp for convenience
pub use bex_vm_types::SysOp;
pub use tokio_util::sync::CancellationToken;

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

static NEXT_CALL_ID: AtomicU64 = AtomicU64::new(0);

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

/// Errors that can occur during external operation execution.
/// Every error is tied to the operation (`fn_name`) that was being called.
#[derive(Debug, PartialEq)]
pub struct OpError {
    pub fn_name: SysOp,
    pub kind: OpErrorKind,
}

impl std::fmt::Display for OpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "failed to call {}: {}", self.fn_name, self.kind)
    }
}

impl std::error::Error for OpError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.kind.source()
    }
}

impl OpError {
    fn unsupported(operation: SysOp) -> Self {
        Self {
            fn_name: operation,
            kind: OpErrorKind::Unsupported,
        }
    }

    fn cancelled(operation: SysOp) -> Self {
        Self {
            fn_name: operation,
            kind: OpErrorKind::Cancelled,
        }
    }

    pub fn new(fn_name: SysOp, kind: OpErrorKind) -> Self {
        Self { fn_name, kind }
    }
}

pub use bex_vm_types::{SysOpErrorCategory, SysOpPanicCategory};

// ============================================================================
// Operation Errors
// ============================================================================

/// Errors that can occur during external operation execution.
#[derive(Debug, PartialEq, thiserror::Error)]
pub enum OpErrorKind {
    #[error("Invalid number of arguments: expected {expected}, got {actual}")]
    InvalidArgumentCount { expected: usize, actual: usize },

    #[error("Invalid argument at position {position}: expected {expected}, got {actual}")]
    InvalidArgument {
        position: usize,
        expected: &'static str,
        actual: String,
    },

    #[error("{0}")]
    Other(String),

    #[error("Expected {expected}, got {actual}")]
    TypeError {
        expected: &'static str,
        actual: String,
    },

    #[error("Expected resource of type {expected}")]
    ResourceTypeMismatch { expected: &'static str },

    #[error("Operation not supported on this platform")]
    Unsupported,

    #[error("Render prompt error: {0}")]
    RenderPrompt(String),

    #[error("Access error: {0}")]
    AccessError(#[from] bex_heap::AccessError),

    #[error("Operation cancelled")]
    Cancelled,

    #[error("Operation cancelled after {duration:?}: {message}")]
    Timeout {
        message: String,
        duration: std::time::Duration,
    },

    #[error("Not implemented: {message}")]
    NotImplemented { message: String },

    #[error("LLM client error: {message}")]
    LlmClientError { message: String },
}

impl OpErrorKind {
    /// Map this rich error to its contract-level category.
    pub fn category(&self) -> SysOpErrorCategory {
        match self {
            Self::InvalidArgumentCount { .. }
            | Self::InvalidArgument { .. }
            | Self::TypeError { .. }
            | Self::ResourceTypeMismatch { .. } => SysOpErrorCategory::InvalidArgument,
            Self::Other(_) => SysOpErrorCategory::DevOther,
            Self::Unsupported => SysOpErrorCategory::Unsupported,
            Self::RenderPrompt(_) => SysOpErrorCategory::RenderPrompt,
            Self::AccessError(_) => SysOpErrorCategory::AccessError,
            Self::Cancelled => SysOpErrorCategory::Io,
            Self::Timeout { .. } => SysOpErrorCategory::Timeout,
            Self::NotImplemented { .. } => SysOpErrorCategory::NotImplemented,
            Self::LlmClientError { .. } => SysOpErrorCategory::LlmClient,
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
pub fn validate_sys_op_error(op: SysOp, kind: &OpErrorKind) -> Result<(), ContractViolation> {
    let category = kind.category();
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

impl From<sys_llm::LlmOpError> for OpErrorKind {
    fn from(e: sys_llm::LlmOpError) -> Self {
        match e {
            sys_llm::LlmOpError::TypeError { expected, actual } => {
                OpErrorKind::TypeError { expected, actual }
            }
            sys_llm::LlmOpError::RenderPrompt(msg) => OpErrorKind::RenderPrompt(msg),
            sys_llm::LlmOpError::Other(msg) => OpErrorKind::Other(msg),
            sys_llm::LlmOpError::ParseResponseError(e) => {
                OpErrorKind::LlmClientError { message: e }
            }
            sys_llm::LlmOpError::NotImplemented { message } => {
                OpErrorKind::NotImplemented { message }
            }
        }
    }
}

// ============================================================================
// Operation Results
// ============================================================================

/// A boxed future for async operations.
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
/// Like [`SysOpResult`] but uses [`OpErrorKind`] instead of [`OpError`] —
/// the implementor never needs to specify which [`SysOp`] variant they are.
/// The generated glue code wraps this into a full [`SysOpResult`] via
/// [`into_result`](SysOpOutput::into_result), which converts `T` into
/// [`BexExternalValue`] using [`AsBexExternalValue`].
///
/// # Example
///
/// ```ignore
/// impl SysOpFs for MyProvider {
///     fn baml_fs_open(path: String) -> SysOpOutput<FsFile> {
///         SysOpOutput::async_op(async move {
///             let file = File::open(&path).await
///                 .map_err(|e| OpErrorKind::Other(format!("open failed: {e}")))?;
///             let handle = REGISTRY.register_file(file, path);
///             Ok(FsFile { _handle: handle })
///         })
///     }
/// }
/// ```
#[allow(clippy::large_enum_variant)]
pub enum SysOpOutput<T = BexExternalValue> {
    /// Operation completed synchronously.
    Ready(Result<T, OpErrorKind>),
    /// Operation is async.
    Async(Pin<Box<dyn Future<Output = Result<T, OpErrorKind>> + Send>>),
}

impl<T> SysOpOutput<T> {
    /// Create a successful synchronous result.
    pub fn ok(value: T) -> Self {
        Self::Ready(Ok(value))
    }

    /// Create a synchronous error.
    pub fn err(kind: OpErrorKind) -> Self {
        Self::Ready(Err(kind))
    }
}

impl<T: Send + 'static> SysOpOutput<T> {
    /// Create an async result from a future.
    pub fn async_op(fut: impl Future<Output = Result<T, OpErrorKind>> + Send + 'static) -> Self {
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
            Self::Ready(Err(kind)) => SysOpResult::Ready(Err(OpError::new(op, kind))),
            Self::Async(fut) => SysOpResult::Async(Box::pin(async move {
                fut.await
                    .map(AsBexExternalValue::into_bex_external_value)
                    .map_err(|kind| OpError::new(op, kind))
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
/// The heap reference allows ops to access instance fields via `with_gc_protection`.
/// Arguments are `BexValue` which can be either:
/// - `BexValue::External(...)` for primitives/strings copied from VM
/// - `BexValue::Opaque(Handle)` for heap objects (instances, arrays, maps)
///
/// The context reference provides engine-level information (e.g., function metadata)
/// that some `sys_ops` need. Ops that don't need it simply ignore the parameter.
pub type SysOpFn = Arc<
    dyn for<'a> Fn(&Arc<BexHeap>, Vec<bex_heap::BexValue<'a>>, &SysOpContext, CallId) -> SysOpResult
        + Send
        + Sync,
>;

// ============================================================================
// Engine Context for Sys Ops
// ============================================================================

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
/// The `cancel` field is per-call, not per-engine. All other fields are
/// `Arc`-wrapped so that [`with_cancel`](Self::with_cancel) is O(1) — just
/// reference-count increments, no data cloning. This is necessary because
/// `SysOpFn` takes a single `&SysOpContext`; splitting into shared + per-call
/// parts would require changing that signature and the proc macro codegen.
#[derive(Clone)]
pub struct SysOpContext {
    /// Pre-extracted LLM function metadata, keyed by function name.
    /// Used by LLM ops that need to look up function prompt templates, client names, etc.
    pub llm_functions: Arc<std::collections::HashMap<String, LlmFunctionInfo>>,

    /// Maps function names to their global indices in the VM.
    /// Used by `resolve_client` to return `FunctionRef` values.
    pub function_global_indices: Arc<std::collections::HashMap<String, usize>>,

    /// Pre-formatted Jinja `{% macro %}` definitions for all `template_strings`.
    /// Prepended to templates by `get_jinja_template`.
    pub template_strings_macros: Arc<String>,

    /// Client metadata for building full client trees, keyed by client name.
    /// Used by `get_client` to recursively construct `LlmClient` with sub-clients and retry policies.
    pub client_metadata: Arc<std::collections::HashMap<String, ClientBuildMeta>>,

    /// Atomic round-robin counters, keyed by client name.
    /// Used by `round_robin_next` to cycle through sub-clients.
    pub round_robin_counters:
        Arc<std::collections::HashMap<String, std::sync::Arc<std::sync::atomic::AtomicUsize>>>,

    /// Per-call cancellation token.
    ///
    /// Defaults to a never-cancelled token for the shared engine context.
    /// In `execute_sys_op`, a per-call clone is created with the real token.
    pub cancel: CancellationToken,

    /// Pre-extracted class definitions for output format rendering.
    /// Keyed by class name (e.g., "Person").
    pub class_definitions: Arc<indexmap::IndexMap<String, ClassDefinition>>,

    /// Pre-extracted enum definitions for output format rendering.
    /// Keyed by enum name (e.g., "Color").
    pub enum_definitions: Arc<indexmap::IndexMap<String, EnumDefinition>>,

    /// Recursive type alias definitions for output format rendering.
    /// Only recursive aliases are stored (non-recursive ones are expanded inline).
    /// Maps alias name → target type.
    pub type_alias_definitions: Arc<indexmap::IndexMap<String, baml_type::Ty>>,
}

/// Pre-extracted metadata for building a Client tree at runtime.
///
/// Populated from HIR `Client` and `RetryPolicy` items during compilation.
/// Used by `get_client` to recursively build `LlmClient` objects.
#[derive(Debug, Clone)]
pub struct ClientBuildMeta {
    /// The client type (`Primitive`, `Fallback`, `RoundRobin`).
    pub client_type: bex_heap::builtin_types::owned::LlmClientType,
    /// Sub-client names (for composite clients: fallback/round-robin).
    pub sub_client_names: Vec<String>,
    /// Retry policy, if one was specified.
    pub retry_policy: Option<bex_heap::builtin_types::owned::LlmRetryPolicy>,
    /// Optional round-robin start index used to initialize the RR counter.
    pub round_robin_start: Option<usize>,
}

/// Pre-extracted metadata for an LLM function.
///
/// This is built during engine construction by reading function objects from the heap,
/// so that LLM `sys_ops` don't need to access raw heap pointers.
pub struct LlmFunctionInfo {
    /// The Jinja prompt template for this function.
    pub prompt_template: String,
    /// The client name (e.g., `"MyClient"`) declared in the function.
    pub client_name: String,
    /// The expected return type, used for response parsing.
    pub return_type: baml_type::Ty,
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
    pub field_type: baml_type::Ty,
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
        Self {
            llm_functions: Arc::new(std::collections::HashMap::new()),
            function_global_indices: Arc::new(std::collections::HashMap::new()),
            template_strings_macros: Arc::new(String::new()),
            client_metadata: Arc::new(std::collections::HashMap::new()),
            round_robin_counters: Arc::new(std::collections::HashMap::new()),
            cancel: CancellationToken::new(),
            class_definitions: Arc::new(indexmap::IndexMap::new()),
            enum_definitions: Arc::new(indexmap::IndexMap::new()),
            type_alias_definitions: Arc::new(indexmap::IndexMap::new()),
        }
    }

    /// Create a per-call clone with the given cancellation token.
    ///
    /// All `Arc`-wrapped fields are shared (just reference-count increments).
    #[must_use]
    pub fn with_cancel(&self, cancel: CancellationToken) -> Self {
        Self {
            cancel,
            ..self.clone()
        }
    }
}

// ============================================================================
// FunctionRef<T> — Typed wrapper for VM function references
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
// SysOps Table (generated from for_all_sys_ops!)
// ============================================================================

/// Table of system operation implementations.
///
/// Generated from `#[sys_op]` definitions in `baml_builtins::with_builtins!`.
/// This struct has one field per `sys_op`, ensuring complete coverage.
///
/// This struct is passed to `BexEngine::new()` and determines how system
/// operations are executed. Different providers (native Tokio, WASM, FFI)
/// can supply different implementations.
///
/// # Example
///
/// ```ignore
/// // Using the native Tokio provider
/// let sys_ops = sys_types_native::SysOps::native();
/// let engine = BexEngine::new(program, sys_ops)?;
/// ```
macro_rules! define_sys_ops_struct {
    ($({ $Variant:ident, $path:expr, $snake:ident, $uses_ctx:expr, [$($throw_cat:ident),*], [$($panic_cat:ident),*] })*) => {
        #[derive(Clone)]
        pub struct SysOps {
            $( pub $snake: SysOpFn, )*
        }

        impl SysOps {
            /// Look up the function for a given `SysOp`.
            pub fn get(&self, op: SysOp) -> &SysOpFn {
                match op {
                    $( SysOp::$Variant => &self.$snake, )*
                }
            }

            /// Create a function that always returns `OpError::Unsupported` for a given op.
            ///
            /// Useful for providers that don't support certain operations.
            pub fn unsupported(operation: SysOp) -> SysOpFn {
                match operation {
                    $( SysOp::$Variant => Arc::new(|_, _, _, _| SysOpResult::Ready(Err(OpError::unsupported(SysOp::$Variant)))), )*
                }
            }

            /// Create a `SysOps` table where all operations return `Unsupported`.
            ///
            /// Useful as a base for providers that only implement some operations.
            pub fn all_unsupported() -> Self {
                Self {
                    $( $snake: Self::unsupported(SysOp::$Variant), )*
                }
            }
        }
    };
}

baml_builtins::for_all_sys_ops!(define_sys_ops_struct);

// ============================================================================
// Per-module sys_op traits (generated from DSL definitions)
// ============================================================================

// Generates: SysOpFs, SysOpSys, SysOpNet, SysOpHttp, SysOpLlm traits
// and SysOps::from_impl<T>() constructor.
baml_builtins::with_builtins!(baml_builtins_macros::generate_sys_op_traits);

// ============================================================================
// SysOpsBuilder — Compose a SysOps table by overriding modules independently
// ============================================================================

/// Builder for composing a [`SysOps`] table by overriding individual modules.
///
/// Starts with all operations returning `Unsupported` (except LLM, which uses
/// the blanket implementation), and allows selectively overriding modules:
///
/// ```ignore
/// // Use with_http::<T>() when T implements Default; use with_http_instance for pre-built instances.
/// let ops = SysOpsBuilder::new()
///     .with_http_instance(Arc::new(my_http_impl))
///     .build();
/// ```
pub struct SysOpsBuilder {
    inner: SysOps,
}

/// Default provider — all trait methods return `Unsupported` via defaults.
/// `SysOpLlm` is provided by the blanket `impl<T> SysOpLlm for T`.
struct DefaultOps;

impl Default for DefaultOps {
    fn default() -> Self {
        Self
    }
}

impl SysOpFs for DefaultOps {}
impl SysOpSys for DefaultOps {}
impl SysOpNet for DefaultOps {}
impl SysOpHttp for DefaultOps {}
impl SysOpEnv for DefaultOps {}

impl SysOpsBuilder {
    /// Create a new builder with all operations defaulting to `Unsupported`,
    /// except LLM ops which use the real blanket implementation.
    pub fn new() -> Self {
        Self {
            inner: SysOps::from_impl::<DefaultOps>(),
        }
    }

    /// Consume the builder and return the composed [`SysOps`] table.
    pub fn build(self) -> SysOps {
        self.inner
    }
}

impl Default for SysOpsBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Blanket SysOpLlm implementation (delegates to sys_llm)
// ============================================================================

/// Blanket implementation of `SysOpLlm` for all types.
///
/// Every type gets the real LLM behavior via `sys_llm::execute_*` functions.
/// When future cross-op calls are needed (e.g., HTTP for media URL resolution),
/// the bound can be tightened to `impl<T: SysOpHttp> SysOpLlm for T` and
/// closures can be passed to the `execute_*` functions.
impl<T> SysOpLlm for T {
    fn baml_llm_primitive_client_render_prompt(
        &self,
        _call_id: CallId,
        primitive_client: bex_heap::builtin_types::owned::LlmPrimitiveClient,
        template: String,
        args: BexExternalValue,
        return_type: baml_type::Ty,
        ctx: &SysOpContext,
    ) -> SysOpOutput<bex_vm_types::PromptAst> {
        let output_format = build_output_format(
            &return_type,
            &ctx.class_definitions,
            &ctx.enum_definitions,
            &ctx.type_alias_definitions,
        );
        SysOpOutput::Ready(
            sys_llm::execute_render_prompt_from_owned(
                &primitive_client,
                &template,
                &args,
                output_format,
            )
            .map_err(OpErrorKind::from),
        )
    }

    fn baml_llm_primitive_client_specialize_prompt(
        &self,
        _call_id: CallId,
        primitive_client: bex_heap::builtin_types::owned::LlmPrimitiveClient,
        prompt: bex_vm_types::PromptAst,
    ) -> SysOpOutput<bex_vm_types::PromptAst> {
        SysOpOutput::Ready(
            sys_llm::execute_specialize_prompt_from_owned(&primitive_client, prompt)
                .map_err(OpErrorKind::from),
        )
    }

    fn baml_llm_primitive_client_build_request(
        &self,
        _call_id: CallId,
        primitive_client: bex_heap::builtin_types::owned::LlmPrimitiveClient,
        prompt: bex_vm_types::PromptAst,
    ) -> SysOpOutput<bex_heap::builtin_types::owned::HttpRequest> {
        SysOpOutput::Ready(
            sys_llm::execute_build_request_from_owned(&primitive_client, prompt)
                .map_err(OpErrorKind::from),
        )
    }

    fn baml_llm_primitive_client_parse(
        &self,
        _call_id: CallId,
        primitive_client: bex_heap::builtin_types::owned::LlmPrimitiveClient,
        response: String,
        type_def: baml_type::Ty,
    ) -> SysOpOutput {
        SysOpOutput::Ready(
            sys_llm::execute_parse_response_from_owned(&primitive_client, &response, &type_def)
                .map_err(OpErrorKind::from),
        )
    }

    fn baml_llm_get_return_type(
        &self,
        _call_id: CallId,
        function_name: String,
        ctx: &SysOpContext,
    ) -> SysOpOutput<baml_type::Ty> {
        let Some(info) = ctx.llm_functions.get(&function_name) else {
            return SysOpOutput::err(OpErrorKind::Other(format!(
                "LLM function not found: {function_name}"
            )));
        };
        SysOpOutput::ok(info.return_type.clone())
    }

    fn baml_llm_get_jinja_template(
        &self,
        _call_id: CallId,
        function_name: String,
        ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        let Some(info) = ctx.llm_functions.get(&function_name) else {
            return SysOpOutput::err(OpErrorKind::Other(format!(
                "LLM function not found: {function_name}"
            )));
        };
        let dedented = sys_llm::preprocess_template(&info.prompt_template);
        let template = if ctx.template_strings_macros.is_empty() {
            dedented
        } else {
            format!("{}\n{}", ctx.template_strings_macros, dedented)
        };
        SysOpOutput::ok(template)
    }

    fn baml_llm_build_primitive_client(
        &self,
        _call_id: CallId,
        name: String,
        provider: String,
        default_role: String,
        allowed_roles: BexExternalValue,
        options: BexExternalValue,
    ) -> SysOpOutput<bex_heap::builtin_types::owned::LlmPrimitiveClient> {
        // Extract allowed_roles from BexExternalValue::Array
        let allowed_roles = match &allowed_roles {
            BexExternalValue::Array { items, .. } => {
                match items
                    .iter()
                    .map(|v| match v {
                        BexExternalValue::String(s) => Ok(s.clone()),
                        _ => Err(OpErrorKind::TypeError {
                            expected: "string",
                            actual: v.type_name().to_string(),
                        }),
                    })
                    .collect::<Result<Vec<_>, _>>()
                {
                    Ok(v) => v,
                    Err(e) => return SysOpOutput::err(e),
                }
            }
            _ => {
                return SysOpOutput::err(OpErrorKind::TypeError {
                    expected: "array",
                    actual: allowed_roles.type_name().to_string(),
                });
            }
        };

        // Extract options from BexExternalValue::Map
        let BexExternalValue::Map {
            entries: options, ..
        } = options
        else {
            return SysOpOutput::err(OpErrorKind::TypeError {
                expected: "map",
                actual: options.type_name().to_string(),
            });
        };

        SysOpOutput::ok(bex_heap::builtin_types::owned::LlmPrimitiveClient {
            name,
            provider,
            default_role,
            allowed_roles,
            options,
        })
    }

    fn baml_llm_get_client(
        &self,
        _call_id: CallId,
        function_name: String,
        ctx: &SysOpContext,
    ) -> SysOpOutput<bex_heap::builtin_types::owned::LlmClient> {
        let Some(info) = ctx.llm_functions.get(&function_name) else {
            return SysOpOutput::err(OpErrorKind::Other(format!(
                "LLM function not found: {function_name}"
            )));
        };

        match build_client_tree(&info.client_name, &ctx.client_metadata) {
            Ok(client) => SysOpOutput::ok(client),
            Err(e) => SysOpOutput::err(OpErrorKind::Other(e)),
        }
    }

    fn baml_llm_resolve_client(
        &self,
        _call_id: CallId,
        client_name: String,
        ctx: &SysOpContext,
    ) -> SysOpOutput {
        let resolve_fn_name = format!("{client_name}.resolve");
        let Some(global_index) = ctx.function_global_indices.get(&resolve_fn_name) else {
            return SysOpOutput::err(OpErrorKind::Other(format!(
                "Client resolve function not found: {resolve_fn_name}"
            )));
        };

        SysOpOutput::ok(
            FunctionRef::<bex_heap::builtin_types::owned::LlmPrimitiveClient>::new(*global_index)
                .into_external(),
        )
    }

    fn baml_llm_round_robin_next(
        &self,
        _call_id: CallId,
        client_name: String,
        ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        let Some(counter) = ctx.round_robin_counters.get(&client_name).cloned() else {
            return SysOpOutput::err(OpErrorKind::Other(format!(
                "Round-robin counter not found for client: {client_name}"
            )));
        };
        let val = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        #[allow(clippy::cast_possible_wrap)]
        SysOpOutput::ok(val as i64)
    }

    fn baml_llm_round_robin_peek(
        &self,
        _call_id: CallId,
        client_name: String,
        ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        let Some(counter) = ctx.round_robin_counters.get(&client_name).cloned() else {
            return SysOpOutput::err(OpErrorKind::Other(format!(
                "Round-robin counter not found for client: {client_name}"
            )));
        };
        let val = counter.load(std::sync::atomic::Ordering::SeqCst);
        #[allow(clippy::cast_possible_wrap)]
        SysOpOutput::ok(val as i64)
    }
}

// ============================================================================
// Cycle Detection for Recursive Classes
// ============================================================================

/// Find all classes that participate in cycles using Tarjan's SCC algorithm.
/// Returns the set of class names that are recursive (part of any cycle).
fn find_recursive_classes(
    classes: &indexmap::IndexMap<String, sys_llm::OutputClass>,
) -> std::collections::HashSet<String> {
    use std::collections::{HashMap, HashSet};

    // Build dependency graph: for each class, find which other collected classes
    // its fields reference.
    let class_names: HashSet<&str> = classes.keys().map(String::as_str).collect();
    let mut graph: HashMap<&str, HashSet<&str>> = HashMap::new();

    for (name, cls) in classes {
        let mut deps = HashSet::new();
        for field in &cls.fields {
            collect_class_refs(&field.field_type, &class_names, &mut deps);
        }
        graph.insert(name.as_str(), deps);
    }

    // Run Tarjan's SCC
    let sccs = tarjan_scc(&graph);

    // Collect all classes in any non-trivial SCC (size > 1 or self-referencing)
    let mut recursive = HashSet::new();
    for scc in sccs {
        if scc.len() > 1 {
            for name in scc {
                recursive.insert(name.to_string());
            }
        } else if scc.len() == 1 {
            let name = scc[0];
            // Single-node SCC: check for self-edge
            if let Some(deps) = graph.get(name) {
                if deps.contains(name) {
                    recursive.insert(name.to_string());
                }
            }
        }
    }

    recursive
}

/// Extract class name references from a type, filtering to only collected classes.
fn collect_class_refs<'a>(
    ty: &'a baml_type::Ty,
    known_classes: &std::collections::HashSet<&str>,
    refs: &mut std::collections::HashSet<&'a str>,
) {
    match ty {
        baml_type::Ty::Class(tn, _) => {
            let name = tn.display_name.as_str();
            if known_classes.contains(name) {
                refs.insert(name);
            }
        }
        baml_type::Ty::Optional(inner, _) | baml_type::Ty::List(inner, _) => {
            collect_class_refs(inner, known_classes, refs);
        }
        baml_type::Ty::Map { key, value, .. } => {
            collect_class_refs(key, known_classes, refs);
            collect_class_refs(value, known_classes, refs);
        }
        baml_type::Ty::Union(variants, _) => {
            for v in variants {
                collect_class_refs(v, known_classes, refs);
            }
        }
        _ => {}
    }
}

/// Tarjan's strongly connected components algorithm.
/// Returns all SCCs (including trivial single-node ones).
fn tarjan_scc<'a>(
    graph: &std::collections::HashMap<&'a str, std::collections::HashSet<&'a str>>,
) -> Vec<Vec<&'a str>> {
    use std::collections::HashMap;

    struct TarjanState<'a> {
        index_counter: usize,
        stack: Vec<&'a str>,
        on_stack: HashMap<&'a str, bool>,
        index: HashMap<&'a str, usize>,
        lowlink: HashMap<&'a str, usize>,
        result: Vec<Vec<&'a str>>,
    }

    fn strongconnect<'a>(
        v: &'a str,
        graph: &HashMap<&'a str, std::collections::HashSet<&'a str>>,
        state: &mut TarjanState<'a>,
    ) {
        state.index.insert(v, state.index_counter);
        state.lowlink.insert(v, state.index_counter);
        state.index_counter += 1;
        state.stack.push(v);
        state.on_stack.insert(v, true);

        if let Some(neighbors) = graph.get(v) {
            for &w in neighbors {
                if !state.index.contains_key(w) {
                    strongconnect(w, graph, state);
                    let w_low = state.lowlink[w];
                    let v_low = state.lowlink[v];
                    if w_low < v_low {
                        state.lowlink.insert(v, w_low);
                    }
                } else if state.on_stack.get(w).copied().unwrap_or(false) {
                    let w_idx = state.index[w];
                    let v_low = state.lowlink[v];
                    if w_idx < v_low {
                        state.lowlink.insert(v, w_idx);
                    }
                }
            }
        }

        if state.lowlink[v] == state.index[v] {
            let mut scc = Vec::new();
            loop {
                let w = state.stack.pop().unwrap();
                state.on_stack.insert(w, false);
                scc.push(w);
                if w == v {
                    break;
                }
            }
            state.result.push(scc);
        }
    }

    let mut state = TarjanState {
        index_counter: 0,
        stack: Vec::new(),
        on_stack: HashMap::new(),
        index: HashMap::new(),
        lowlink: HashMap::new(),
        result: Vec::new(),
    };

    for &v in graph.keys() {
        if !state.index.contains_key(v) {
            strongconnect(v, graph, &mut state);
        }
    }

    state.result
}

// ============================================================================
// Output Format Builder
// ============================================================================

/// Walk the type graph and build an `OutputFormatContent` with all referenced
/// class and enum definitions populated.
fn build_output_format(
    return_type: &baml_type::Ty,
    class_defs: &indexmap::IndexMap<String, ClassDefinition>,
    enum_defs: &indexmap::IndexMap<String, EnumDefinition>,
    type_alias_defs: &indexmap::IndexMap<String, baml_type::Ty>,
) -> sys_llm::OutputFormatContent {
    let mut content = sys_llm::OutputFormatContent::new(return_type.clone());

    let mut visited = std::collections::HashSet::new();
    let mut stack = vec![return_type.clone()];

    while let Some(ty) = stack.pop() {
        match &ty {
            baml_type::Ty::Class(tn, _) => {
                let name = tn.display_name.as_str();
                if visited.insert(name.to_string()) {
                    if let Some(cls_def) = class_defs.get(name) {
                        for field in &cls_def.fields {
                            if !field.skip {
                                stack.push(field.field_type.clone());
                            }
                        }
                        content = content.with_class(sys_llm::OutputClass {
                            name: cls_def.name.clone(),
                            alias: cls_def.alias.clone(),
                            description: cls_def.description.clone(),
                            fields: cls_def
                                .fields
                                .iter()
                                .filter(|f| !f.skip)
                                .map(|f| sys_llm::OutputClassField {
                                    name: f.name.clone(),
                                    alias: f.alias.clone(),
                                    field_type: f.field_type.clone(),
                                    description: f.description.clone(),
                                })
                                .collect(),
                        });
                    }
                }
            }
            baml_type::Ty::Enum(tn, _) => {
                let name = tn.display_name.as_str();
                if visited.insert(name.to_string()) {
                    if let Some(enum_def) = enum_defs.get(name) {
                        content = content.with_enum(sys_llm::OutputEnum {
                            name: enum_def.name.clone(),
                            alias: enum_def.alias.clone(),
                            description: enum_def.description.clone(),
                            values: enum_def
                                .variants
                                .iter()
                                .map(|v| sys_llm::OutputEnumValue {
                                    name: v.name.clone(),
                                    alias: v.alias.clone(),
                                    description: v.description.clone(),
                                })
                                .collect(),
                        });
                    }
                }
            }
            baml_type::Ty::TypeAlias(tn, _) => {
                let name = tn.display_name.as_str();
                if visited.insert(name.to_string()) {
                    if let Some(target) = type_alias_defs.get(name) {
                        // Walk into the target type to collect referenced classes/enums
                        stack.push(target.clone());
                        // Register as recursive type alias for hoisted rendering
                        content =
                            content.with_recursive_type_alias(name.to_string(), target.clone());
                    }
                }
            }
            baml_type::Ty::List(inner, _) | baml_type::Ty::Optional(inner, _) => {
                stack.push(*inner.clone());
            }
            baml_type::Ty::Map { key, value, .. } => {
                stack.push(*key.clone());
                stack.push(*value.clone());
            }
            baml_type::Ty::Union(variants, _) => {
                stack.extend(variants.iter().cloned());
            }
            _ => {}
        }
    }

    // Detect recursive classes and mark them for hoisting.
    // Sort alphabetically for deterministic output order.
    let recursive = find_recursive_classes(&content.classes);
    let mut recursive_ordered: Vec<String> = recursive.into_iter().collect();
    recursive_ordered.sort();
    for name in recursive_ordered {
        content = content.with_recursive_class(name);
    }

    content
}

// ============================================================================
// Client Tree Builder
// ============================================================================

/// Recursively build a `LlmClient` tree from `ClientBuildMeta`.
///
/// For primitive clients, this returns a leaf node.
/// For composite clients (fallback/round-robin), this recursively builds
/// sub-client trees from the metadata.
fn build_client_tree(
    client_name: &str,
    metadata: &std::collections::HashMap<String, ClientBuildMeta>,
) -> Result<bex_heap::builtin_types::owned::LlmClient, String> {
    let Some(meta) = metadata.get(client_name) else {
        return Err(format!("Client not found: {client_name}"));
    };

    let sub_clients = meta
        .sub_client_names
        .iter()
        .map(|sub_name| build_client_tree(sub_name, metadata))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(bex_heap::builtin_types::owned::LlmClient {
        name: client_name.to_string(),
        client_type: meta.client_type,
        sub_clients,
        retry: meta.retry_policy.clone(),
    })
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
    use bex_vm_types::SysOp;

    use super::*;

    fn test_heap() -> Arc<BexHeap> {
        BexHeap::new(vec![])
    }

    fn test_ctx() -> SysOpContext {
        SysOpContext::empty()
    }

    #[test]
    fn test_unsupported_returns_error() {
        let heap = test_heap();
        let ctx = test_ctx();
        let op = SysOps::unsupported(SysOp::BamlSysShell);
        let result = op(&heap, vec![], &ctx, CallId::next());
        match result {
            SysOpResult::Ready(Err(e)) => {
                assert!(matches!(e.kind, OpErrorKind::Unsupported));
                assert_eq!(e.fn_name, SysOp::BamlSysShell);
            }
            _ => panic!("Expected Unsupported error"),
        }
    }

    #[test]
    fn test_all_unsupported() {
        let heap = test_heap();
        let ctx = test_ctx();
        let ops = SysOps::all_unsupported();

        // Test fs_open returns Unsupported
        let result = (ops.baml_fs_open)(&heap, vec![], &ctx, CallId::next());
        assert!(matches!(
            result,
            SysOpResult::Ready(Err(OpError {
                fn_name: SysOp::BamlFsOpen,
                kind: OpErrorKind::Unsupported,
            }))
        ));

        // Test shell returns Unsupported
        let result = (ops.baml_sys_shell)(&heap, vec![], &ctx, CallId::next());
        assert!(matches!(
            result,
            SysOpResult::Ready(Err(OpError {
                fn_name: SysOp::BamlSysShell,
                kind: OpErrorKind::Unsupported,
            }))
        ));
    }

    #[test]
    fn test_sys_ops_get() {
        let ops = SysOps::all_unsupported();
        let heap = test_heap();
        let ctx = test_ctx();

        // Test that get() returns the correct function pointer
        let fn_ptr = ops.get(SysOp::BamlFsOpen);
        let result = fn_ptr(&heap, vec![], &ctx, CallId::next());
        assert!(matches!(result, SysOpResult::Ready(Err(_))));
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
                assert!(matches!(value, BexExternalValue::String(s) if s == "done"));
            }
            SysOpResult::Ready(_) => panic!("Expected Async result"),
        }
    }

    // ========================================================================
    // Contract enforcement tests
    // ========================================================================

    #[test]
    fn contract_allows_declared_category() {
        let op = bex_vm_types::sys_op_for_path("baml.http.fetch").unwrap();
        let err = OpErrorKind::Timeout {
            message: "timed out".into(),
            duration: std::time::Duration::from_secs(30),
        };
        assert!(validate_sys_op_error(op, &err).is_ok());
    }

    #[test]
    fn contract_rejects_undeclared_category() {
        let op = bex_vm_types::sys_op_for_path("env.get").unwrap();
        let err = OpErrorKind::LlmClientError {
            message: "bad".into(),
        };
        let result = validate_sys_op_error(op, &err);
        assert!(result.is_err());
        let violation = result.unwrap_err();
        assert_eq!(violation.actual_category, SysOpErrorCategory::LlmClient);
    }

    #[test]
    fn contract_allows_devother_when_declared() {
        let op = bex_vm_types::sys_op_for_path("baml.http.fetch").unwrap();
        let err = OpErrorKind::Other("some debug detail".into());
        let result = validate_sys_op_error(op, &err);
        assert!(
            result.is_err(),
            "DevOther should be rejected when not in #[throws]"
        );
    }

    #[test]
    fn all_sys_ops_have_contract_metadata() {
        use bex_vm_types::SysOp;
        let ops = [
            SysOp::BamlFsOpen,
            SysOp::BamlHttpFetch,
            SysOp::BamlSysPanic,
            SysOp::EnvGet,
        ];
        for op in ops {
            let cats = op.allowed_error_categories();
            let panics = op.allowed_panic_categories();
            assert!(
                !cats.is_empty() || !panics.is_empty(),
                "sys_op {op} should have at least one contract category",
            );
        }
    }

    #[test]
    fn category_mapping_covers_all_variants() {
        let variants = vec![
            OpErrorKind::InvalidArgumentCount {
                expected: 1,
                actual: 2,
            },
            OpErrorKind::InvalidArgument {
                position: 0,
                expected: "string",
                actual: "int".into(),
            },
            OpErrorKind::Other("test".into()),
            OpErrorKind::TypeError {
                expected: "int",
                actual: "string".into(),
            },
            OpErrorKind::ResourceTypeMismatch { expected: "File" },
            OpErrorKind::Unsupported,
            OpErrorKind::RenderPrompt("err".into()),
            OpErrorKind::Cancelled,
            OpErrorKind::Timeout {
                message: "t".into(),
                duration: std::time::Duration::from_secs(1),
            },
            OpErrorKind::NotImplemented {
                message: "n".into(),
            },
            OpErrorKind::LlmClientError {
                message: "l".into(),
            },
        ];
        for v in &variants {
            let _ = v.category();
        }
    }

    // ========================================================================
    // build_output_format tests
    // ========================================================================

    fn make_class_defs(defs: Vec<ClassDefinition>) -> indexmap::IndexMap<String, ClassDefinition> {
        defs.into_iter().map(|d| (d.name.clone(), d)).collect()
    }

    fn make_enum_defs(defs: Vec<EnumDefinition>) -> indexmap::IndexMap<String, EnumDefinition> {
        defs.into_iter().map(|d| (d.name.clone(), d)).collect()
    }

    #[test]
    fn output_format_primitive_int() {
        let ty = baml_type::Ty::Int {
            attr: baml_type::TyAttr::default(),
        };
        let content = build_output_format(
            &ty,
            &indexmap::IndexMap::new(),
            &indexmap::IndexMap::new(),
            &indexmap::IndexMap::new(),
        );
        let rendered = content.render(&sys_llm::RenderOptions::default()).unwrap();
        assert_eq!(rendered, Some("Answer as an int".to_string()));
    }

    #[test]
    fn output_format_string_returns_none() {
        let ty = baml_type::Ty::String {
            attr: baml_type::TyAttr::default(),
        };
        let content = build_output_format(
            &ty,
            &indexmap::IndexMap::new(),
            &indexmap::IndexMap::new(),
            &indexmap::IndexMap::new(),
        );
        let rendered = content.render(&sys_llm::RenderOptions::default()).unwrap();
        assert_eq!(rendered, None);
    }

    #[test]
    fn output_format_class_renders_schema() {
        let class_defs = make_class_defs(vec![ClassDefinition {
            name: "Person".to_string(),
            description: None,
            alias: None,
            fields: vec![
                ClassFieldDefinition {
                    name: "name".to_string(),
                    field_type: baml_type::Ty::String {
                        attr: baml_type::TyAttr::default(),
                    },
                    description: None,
                    alias: None,
                    skip: false,
                },
                ClassFieldDefinition {
                    name: "age".to_string(),
                    field_type: baml_type::Ty::Int {
                        attr: baml_type::TyAttr::default(),
                    },
                    description: None,
                    alias: None,
                    skip: false,
                },
            ],
        }]);

        let ty = baml_type::Ty::Class(
            baml_type::TypeName::local("Person".into()),
            baml_type::TyAttr::default(),
        );
        let content = build_output_format(
            &ty,
            &class_defs,
            &indexmap::IndexMap::new(),
            &indexmap::IndexMap::new(),
        );
        let rendered = content.render(&sys_llm::RenderOptions::default()).unwrap();
        assert!(rendered.is_some());
        let text = rendered.unwrap();
        assert!(text.contains("name: string"), "got: {text}");
        assert!(text.contains("age: int"), "got: {text}");
    }

    #[test]
    fn output_format_enum_renders_values() {
        let enum_defs = make_enum_defs(vec![EnumDefinition {
            name: "Color".to_string(),
            description: None,
            alias: None,
            variants: vec![
                EnumVariantDefinition {
                    name: "Red".to_string(),
                    description: None,
                    alias: None,
                },
                EnumVariantDefinition {
                    name: "Green".to_string(),
                    description: Some("Like grass".to_string()),
                    alias: None,
                },
                EnumVariantDefinition {
                    name: "Blue".to_string(),
                    description: None,
                    alias: None,
                },
            ],
        }]);

        let ty = baml_type::Ty::Enum(
            baml_type::TypeName::local("Color".into()),
            baml_type::TyAttr::default(),
        );
        let content = build_output_format(
            &ty,
            &indexmap::IndexMap::new(),
            &enum_defs,
            &indexmap::IndexMap::new(),
        );
        let rendered = content.render(&sys_llm::RenderOptions::default()).unwrap();
        assert!(rendered.is_some());
        let text = rendered.unwrap();
        assert!(text.contains("Red"), "got: {text}");
        assert!(text.contains("- Green: Like grass"), "got: {text}");
        assert!(text.contains("Blue"), "got: {text}");
    }

    #[test]
    fn output_format_nested_class_resolves() {
        let class_defs = make_class_defs(vec![
            ClassDefinition {
                name: "Address".to_string(),
                description: None,
                alias: None,
                fields: vec![ClassFieldDefinition {
                    name: "city".to_string(),
                    field_type: baml_type::Ty::String {
                        attr: baml_type::TyAttr::default(),
                    },
                    description: None,
                    alias: None,
                    skip: false,
                }],
            },
            ClassDefinition {
                name: "Person".to_string(),
                description: None,
                alias: None,
                fields: vec![ClassFieldDefinition {
                    name: "address".to_string(),
                    field_type: baml_type::Ty::Class(
                        baml_type::TypeName::local("Address".into()),
                        baml_type::TyAttr::default(),
                    ),
                    description: None,
                    alias: None,
                    skip: false,
                }],
            },
        ]);

        let ty = baml_type::Ty::List(
            Box::new(baml_type::Ty::Class(
                baml_type::TypeName::local("Person".into()),
                baml_type::TyAttr::default(),
            )),
            baml_type::TyAttr::default(),
        );
        let content = build_output_format(
            &ty,
            &class_defs,
            &indexmap::IndexMap::new(),
            &indexmap::IndexMap::new(),
        );
        // Both Person and Address should be resolved
        assert!(content.find_class("Person").is_some());
        assert!(content.find_class("Address").is_some());
    }

    #[test]
    fn output_format_self_referencing_class_no_infinite_loop() {
        let class_defs = make_class_defs(vec![ClassDefinition {
            name: "Node".to_string(),
            description: None,
            alias: None,
            fields: vec![
                ClassFieldDefinition {
                    name: "value".to_string(),
                    field_type: baml_type::Ty::Int {
                        attr: baml_type::TyAttr::default(),
                    },
                    description: None,
                    alias: None,
                    skip: false,
                },
                ClassFieldDefinition {
                    name: "child".to_string(),
                    field_type: baml_type::Ty::Optional(
                        Box::new(baml_type::Ty::Class(
                            baml_type::TypeName::local("Node".into()),
                            baml_type::TyAttr::default(),
                        )),
                        baml_type::TyAttr::default(),
                    ),
                    description: None,
                    alias: None,
                    skip: false,
                },
            ],
        }]);

        let ty = baml_type::Ty::Class(
            baml_type::TypeName::local("Node".into()),
            baml_type::TyAttr::default(),
        );
        // Should not infinite loop
        let content = build_output_format(
            &ty,
            &class_defs,
            &indexmap::IndexMap::new(),
            &indexmap::IndexMap::new(),
        );
        assert!(content.find_class("Node").is_some());
    }

    // ========================================================================
    // Cycle detection unit tests
    // ========================================================================

    fn mk_output_class(name: &str, fields: Vec<(&str, baml_type::Ty)>) -> sys_llm::OutputClass {
        sys_llm::OutputClass {
            name: name.to_string(),
            description: None,
            alias: None,
            fields: fields
                .into_iter()
                .map(|(n, t)| sys_llm::OutputClassField {
                    name: n.to_string(),
                    alias: None,
                    field_type: t,
                    description: None,
                })
                .collect(),
        }
    }

    fn ty_int() -> baml_type::Ty {
        baml_type::Ty::Int {
            attr: baml_type::TyAttr::default(),
        }
    }
    fn ty_class(name: &str) -> baml_type::Ty {
        baml_type::Ty::Class(
            baml_type::TypeName::local(name.into()),
            baml_type::TyAttr::default(),
        )
    }
    fn ty_optional(inner: baml_type::Ty) -> baml_type::Ty {
        baml_type::Ty::Optional(Box::new(inner), baml_type::TyAttr::default())
    }

    #[test]
    fn test_find_recursive_classes_non_recursive() {
        let mut classes = indexmap::IndexMap::new();
        classes.insert("A".to_string(), mk_output_class("A", vec![("x", ty_int())]));
        classes.insert("B".to_string(), mk_output_class("B", vec![("y", ty_int())]));
        let recursive = find_recursive_classes(&classes);
        assert!(recursive.is_empty());
    }

    #[test]
    fn test_find_recursive_classes_self_referential() {
        let mut classes = indexmap::IndexMap::new();
        classes.insert(
            "Node".to_string(),
            mk_output_class(
                "Node",
                vec![("data", ty_int()), ("next", ty_optional(ty_class("Node")))],
            ),
        );
        let recursive = find_recursive_classes(&classes);
        assert_eq!(recursive.len(), 1);
        assert!(recursive.contains("Node"));
    }

    #[test]
    fn test_find_recursive_classes_mutual() {
        let mut classes = indexmap::IndexMap::new();
        classes.insert(
            "A".to_string(),
            mk_output_class("A", vec![("b", ty_class("B"))]),
        );
        classes.insert(
            "B".to_string(),
            mk_output_class("B", vec![("a", ty_class("A"))]),
        );
        let recursive = find_recursive_classes(&classes);
        assert_eq!(recursive.len(), 2);
        assert!(recursive.contains("A"));
        assert!(recursive.contains("B"));
    }

    #[test]
    fn test_find_recursive_classes_referencing_recursive() {
        // LinkedList references Node, but LinkedList itself is not recursive
        let mut classes = indexmap::IndexMap::new();
        classes.insert(
            "Node".to_string(),
            mk_output_class(
                "Node",
                vec![("data", ty_int()), ("next", ty_optional(ty_class("Node")))],
            ),
        );
        classes.insert(
            "LinkedList".to_string(),
            mk_output_class("LinkedList", vec![("head", ty_optional(ty_class("Node")))]),
        );
        let recursive = find_recursive_classes(&classes);
        assert_eq!(recursive.len(), 1);
        assert!(recursive.contains("Node"));
        assert!(!recursive.contains("LinkedList"));
    }

    #[test]
    fn test_find_recursive_classes_chain_no_cycle() {
        let mut classes = indexmap::IndexMap::new();
        classes.insert(
            "A".to_string(),
            mk_output_class("A", vec![("b", ty_class("B"))]),
        );
        classes.insert(
            "B".to_string(),
            mk_output_class("B", vec![("c", ty_class("C"))]),
        );
        classes.insert("C".to_string(), mk_output_class("C", vec![("x", ty_int())]));
        let recursive = find_recursive_classes(&classes);
        assert!(recursive.is_empty());
    }

    // ========================================================================
    // Integration test: build_output_format detects recursive classes
    // ========================================================================

    #[test]
    fn test_build_output_format_detects_self_referential() {
        let class_defs = indexmap::IndexMap::from([(
            "Node".to_string(),
            ClassDefinition {
                name: "Node".to_string(),
                description: None,
                alias: None,
                fields: vec![
                    ClassFieldDefinition {
                        name: "data".to_string(),
                        field_type: ty_int(),
                        description: None,
                        alias: None,
                        skip: false,
                    },
                    ClassFieldDefinition {
                        name: "next".to_string(),
                        field_type: ty_optional(ty_class("Node")),
                        description: None,
                        alias: None,
                        skip: false,
                    },
                ],
            },
        )]);

        let ty = ty_class("Node");
        let content = build_output_format(
            &ty,
            &class_defs,
            &indexmap::IndexMap::new(),
            &indexmap::IndexMap::new(),
        );
        assert!(content.recursive_classes.contains("Node"));
    }

    #[test]
    fn test_build_output_format_detects_mutual_recursion() {
        let class_defs = indexmap::IndexMap::from([
            (
                "Tree".to_string(),
                ClassDefinition {
                    name: "Tree".to_string(),
                    description: None,
                    alias: None,
                    fields: vec![
                        ClassFieldDefinition {
                            name: "data".to_string(),
                            field_type: ty_int(),
                            description: None,
                            alias: None,
                            skip: false,
                        },
                        ClassFieldDefinition {
                            name: "children".to_string(),
                            field_type: ty_class("Forest"),
                            description: None,
                            alias: None,
                            skip: false,
                        },
                    ],
                },
            ),
            (
                "Forest".to_string(),
                ClassDefinition {
                    name: "Forest".to_string(),
                    description: None,
                    alias: None,
                    fields: vec![ClassFieldDefinition {
                        name: "trees".to_string(),
                        field_type: baml_type::Ty::List(
                            Box::new(ty_class("Tree")),
                            baml_type::TyAttr::default(),
                        ),
                        description: None,
                        alias: None,
                        skip: false,
                    }],
                },
            ),
        ]);

        let ty = ty_class("Tree");
        let content = build_output_format(
            &ty,
            &class_defs,
            &indexmap::IndexMap::new(),
            &indexmap::IndexMap::new(),
        );
        assert!(content.recursive_classes.contains("Tree"));
        assert!(content.recursive_classes.contains("Forest"));
    }

    #[test]
    fn test_build_output_format_full_pipeline_renders_hoisted() {
        let class_defs = indexmap::IndexMap::from([
            (
                "Node".to_string(),
                ClassDefinition {
                    name: "Node".to_string(),
                    description: None,
                    alias: None,
                    fields: vec![
                        ClassFieldDefinition {
                            name: "data".to_string(),
                            field_type: ty_int(),
                            description: None,
                            alias: None,
                            skip: false,
                        },
                        ClassFieldDefinition {
                            name: "next".to_string(),
                            field_type: ty_optional(ty_class("Node")),
                            description: None,
                            alias: None,
                            skip: false,
                        },
                    ],
                },
            ),
            (
                "LinkedList".to_string(),
                ClassDefinition {
                    name: "LinkedList".to_string(),
                    description: None,
                    alias: None,
                    fields: vec![
                        ClassFieldDefinition {
                            name: "head".to_string(),
                            field_type: ty_optional(ty_class("Node")),
                            description: None,
                            alias: None,
                            skip: false,
                        },
                        ClassFieldDefinition {
                            name: "len".to_string(),
                            field_type: ty_int(),
                            description: None,
                            alias: None,
                            skip: false,
                        },
                    ],
                },
            ),
        ]);

        let ty = ty_class("LinkedList");
        let content = build_output_format(
            &ty,
            &class_defs,
            &indexmap::IndexMap::new(),
            &indexmap::IndexMap::new(),
        );
        let rendered = content.render(&sys_llm::RenderOptions::default()).unwrap();

        let rendered = rendered.unwrap();
        // Node should be hoisted (defined at top, referenced by name)
        assert!(rendered.contains("Node {"));
        assert!(rendered.contains("head: Node or null,"));
        // LinkedList should be inline (not hoisted)
        assert!(!rendered.contains("LinkedList {"));
    }

    // ========================================================================
    // Tarjan SCC unit tests
    // ========================================================================

    #[test]
    fn test_tarjan_self_cycle() {
        use std::collections::{HashMap, HashSet};
        let mut graph: HashMap<&str, HashSet<&str>> = HashMap::new();
        graph.insert("A", HashSet::from(["A"]));
        let sccs = tarjan_scc(&graph);
        assert_eq!(sccs.len(), 1);
        assert_eq!(sccs[0], vec!["A"]);
    }

    #[test]
    fn test_tarjan_multi_scc() {
        use std::collections::{HashMap, HashSet};
        let mut graph: HashMap<&str, HashSet<&str>> = HashMap::new();
        graph.insert("A", HashSet::from(["B"]));
        graph.insert("B", HashSet::from(["A", "C"]));
        graph.insert("C", HashSet::from(["D"]));
        graph.insert("D", HashSet::from(["C"]));
        let sccs = tarjan_scc(&graph);
        // Should have 2 SCCs: {A, B} and {C, D}
        assert_eq!(sccs.len(), 2);
        let mut scc_sets: Vec<HashSet<&str>> = sccs
            .iter()
            .map(|scc| scc.iter().copied().collect::<HashSet<&str>>())
            .collect();
        scc_sets.sort_by_key(std::collections::HashSet::len);
        assert_eq!(scc_sets[0], HashSet::from(["C", "D"]));
        assert_eq!(scc_sets[1], HashSet::from(["A", "B"]));
    }

    #[test]
    fn test_tarjan_no_cycles() {
        use std::collections::{HashMap, HashSet};
        let mut graph: HashMap<&str, HashSet<&str>> = HashMap::new();
        graph.insert("A", HashSet::from(["B"]));
        graph.insert("B", HashSet::from(["C"]));
        graph.insert("C", HashSet::new());
        let sccs = tarjan_scc(&graph);
        // All SCCs should be trivial (size 1)
        assert_eq!(sccs.len(), 3);
        for scc in &sccs {
            assert_eq!(scc.len(), 1);
        }
    }
}
