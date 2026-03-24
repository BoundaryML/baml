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
    pub type_alias_definitions: Arc<indexmap::IndexMap<baml_type::TypeName, baml_type::Ty>>,
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
            cancel: CancellationToken::new(),
            class_definitions: Arc::new(
                indexmap::IndexMap::<baml_type::TypeName, ClassDefinition>::new(),
            ),
            enum_definitions: Arc::new(
                indexmap::IndexMap::<baml_type::TypeName, EnumDefinition>::new(),
            ),
            type_alias_definitions: Arc::new(indexmap::IndexMap::<
                baml_type::TypeName,
                baml_type::Ty,
            >::new()),
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
// IO pipeline (generated from .baml files via baml_builtins2_codegen)
// ============================================================================

// SysOps struct, IO traits (IoClassFsFile, IoNamespaceFs, etc.),
// view/owned types, from_impl, all_unsupported — all generated from
// `.baml` `$rust_io_function` definitions by `baml_builtins2_codegen`.
#[allow(
    dead_code,
    unreachable_pub,
    unused_imports,
    unused_variables,
    clippy::all
)]
pub mod io {
    use std::sync::Arc;

    pub use bex_heap::{AccessError, BexClass, BexValue, BuiltinClass, GcProtectedHeap};
    pub use bex_vm_types::SysOp;

    pub use super::{
        AsBexExternalValue, BexExternalValue, BexHeap, CallId, OpError, OpErrorKind, SysOpContext,
        SysOpFn, SysOpOutput, SysOpResult,
    };

    include!(concat!(env!("OUT_DIR"), "/io_generated.rs"));
}

// ============================================================================
// Blanket IO LLM implementation (delegates to sys_llm)
// ============================================================================

impl<T> io::IoClassLlmClient for T {
    fn get_constructor(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        client: io::owned::llm::Client,
        ctx: &SysOpContext,
    ) -> SysOpOutput<BexExternalValue> {
        let resolve_fn_name = format!("{}$new", client.name);
        let global_index = ctx
            .function_global_indices
            .get(&resolve_fn_name)
            .or_else(|| {
                ctx.function_global_indices
                    .get(&format!("user.{resolve_fn_name}"))
            });
        let Some(global_index) = global_index else {
            return SysOpOutput::err(OpErrorKind::Other(format!(
                "Client resolve function not found: {resolve_fn_name}"
            )));
        };
        SysOpOutput::ok(
            FunctionRef::<io::owned::llm::PrimitiveClient>::new(*global_index).into_external(),
        )
    }
}

/// Blanket impl — all types get real LLM behavior via `sys_llm` delegation.
/// Uses new IO traits from the `io` module.
impl<T> io::IoClassLlmPrimitiveClient for T {
    fn render_prompt(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        client: io::owned::llm::PrimitiveClient,
        template: String,
        args: indexmap::IndexMap<String, BexExternalValue>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::llm::PromptAst> {
        let old_client = convert_io_primitive_client(&client);
        let args_ext = BexExternalValue::Map {
            key_type: baml_type::Ty::string(),
            value_type: baml_type::Ty::unknown(),
            entries: args,
        };
        SysOpOutput::Ready(
            sys_llm::execute_render_prompt_from_owned(&old_client, &template, &args_ext)
                .map(wrap_prompt_ast)
                .map_err(OpErrorKind::from),
        )
    }

    fn specialize_prompt(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        client: io::owned::llm::PrimitiveClient,
        prompt: io::owned::llm::PromptAst,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::llm::PromptAst> {
        let old_client = convert_io_primitive_client(&client);
        let prompt_ast = unwrap_prompt_ast(&prompt);
        SysOpOutput::Ready(
            sys_llm::execute_specialize_prompt_from_owned(&old_client, prompt_ast)
                .map(wrap_prompt_ast)
                .map_err(OpErrorKind::from),
        )
    }

    fn build_request(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        client: io::owned::llm::PrimitiveClient,
        prompt: io::owned::llm::PromptAst,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<BexExternalValue> {
        let old_client = convert_io_primitive_client(&client);
        let prompt_ast = unwrap_prompt_ast(&prompt);
        SysOpOutput::Ready(
            sys_llm::execute_build_request_from_owned(&old_client, prompt_ast)
                .map(|req| {
                    io::owned::http::Request {
                        method: req.method,
                        url: req.url,
                        headers: req.headers,
                        body: req.body,
                    }
                    .into_bex_external_value()
                })
                .map_err(OpErrorKind::from),
        )
    }

    fn parse(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        client: io::owned::llm::PrimitiveClient,
        response: String,
        type_def: baml_type::Ty,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<BexExternalValue> {
        let old_client = convert_io_primitive_client(&client);
        SysOpOutput::Ready(
            sys_llm::execute_parse_response_from_owned(&old_client, &response, &type_def)
                .map(|v| v.into_bex_external_value())
                .map_err(OpErrorKind::from),
        )
    }
}

/// Look up an LLM function by name, trying the bare name first then "user.{name}".
fn lookup_llm_function<'a>(
    function_name: &str,
    llm_functions: &'a std::collections::HashMap<String, LlmFunctionInfo>,
) -> Option<&'a LlmFunctionInfo> {
    llm_functions
        .get(function_name)
        .or_else(|| llm_functions.get(&format!("user.{function_name}")))
}

impl<T> io::IoNamespaceLlm for T {
    fn get_jinja_template(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        function_name: String,
        ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        let Some(info) = lookup_llm_function(&function_name, &ctx.llm_functions) else {
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

    fn get_return_type(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        function_name: String,
        ctx: &SysOpContext,
    ) -> SysOpOutput<baml_type::Ty> {
        let Some(info) = lookup_llm_function(&function_name, &ctx.llm_functions) else {
            return SysOpOutput::err(OpErrorKind::Other(format!(
                "LLM function not found: {function_name}"
            )));
        };
        SysOpOutput::ok(info.return_type.clone())
    }
}

/// Wrap a `bex_vm_types::PromptAst` (Arc) into the generated `owned::llm::PromptAst`.
fn wrap_prompt_ast(ast: bex_vm_types::PromptAst) -> io::owned::llm::PromptAst {
    io::owned::llm::PromptAst {
        _data: ast as std::sync::Arc<dyn std::any::Any + Send + Sync>,
    }
}

/// Unwrap the `_data` field of a generated `owned::llm::PromptAst` back to `bex_vm_types::PromptAst`.
fn unwrap_prompt_ast(owned: &io::owned::llm::PromptAst) -> bex_vm_types::PromptAst {
    owned
        ._data
        .clone()
        .downcast::<baml_builtins::PromptAst>()
        .expect("PromptAst _data should be Arc<baml_builtins::PromptAst>")
}

/// Convert the generated IO `PrimitiveClient` to the `sys_llm::baml_std::PrimitiveClient`.
///
/// With typed owned fields, both structs have the same field types so this is
/// a direct field-by-field clone.
fn convert_io_primitive_client(
    io::owned::llm::PrimitiveClient {
        name,
        provider,
        options,
    }: &io::owned::llm::PrimitiveClient,
) -> sys_llm::baml_std::PrimitiveClient {
    sys_llm::baml_std::PrimitiveClient::new(
        name.clone(),
        provider.clone(),
        sys_llm::baml_std::PrimitiveClientOptions {
            model: options.model.clone(),
            base_url: options.base_url.clone(),
            default_role: options.default_role.clone(),
            allowed_roles: options.allowed_roles.clone(),
            remap_roles: options.remap_roles.clone(),
            api_key: options.api_key.clone(),
            headers: options.headers.clone(),
            query_params: options.query_params.clone(),
            request_body: options.request_body.clone(),
            ..Default::default()
        },
    )
}

// ============================================================================
// IoSysOpsBuilder — Compose an io::SysOps table by overriding namespaces
// ============================================================================

/// Default provider for the IO pipeline — non-LLM ops return `Unsupported`,
/// LLM ops use the blanket `impl<T> IoClassLlmPrimitiveClient/IoNamespaceLlm for T`.
struct DefaultIoOps;

impl io::IoClassFsFile for DefaultIoOps {
    fn read(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: io::owned::fs::File,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
    fn close(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: io::owned::fs::File,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
}

impl io::IoNamespaceFs for DefaultIoOps {
    fn open(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::fs::File> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
}

impl io::IoClassHttpResponse for DefaultIoOps {
    fn text(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _r: io::owned::http::Response,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
}

impl io::IoNamespaceHttp for DefaultIoOps {
    fn fetch(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _url: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::http::Response> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
    fn send(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _req: io::owned::http::Request,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::http::Response> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
}

impl io::IoClassNetSocket for DefaultIoOps {
    fn read(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _s: io::owned::net::Socket,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
    fn close(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _s: io::owned::net::Socket,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
}

impl io::IoNamespaceNet for DefaultIoOps {
    fn connect(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _addr: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::net::Socket> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
}

impl io::IoNamespaceEnv for DefaultIoOps {
    fn get(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _key: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<String>> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
}

impl io::IoNamespaceSys for DefaultIoOps {
    fn shell(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _command: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
    fn sleep(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _ms: i64,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
    fn panic(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _msg: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
}

impl io::IoPackageBaml for DefaultIoOps {}

/// Builder for composing an [`io::SysOps`] table by overriding namespaces.
///
/// Starts with all operations returning `Unsupported` (except LLM, which uses
/// the blanket implementation), and allows selectively overriding namespaces:
///
/// ```ignore
/// let ops = IoSysOpsBuilder::new()
///     .with_http_instance(Arc::new(my_http_impl))
///     .with_env_instance(Arc::new(my_env_impl))
///     .build();
/// ```
pub struct IoSysOpsBuilder {
    inner: io::SysOps,
}

impl IoSysOpsBuilder {
    /// Create a new builder with all operations defaulting to `Unsupported`,
    /// except LLM ops which use the real blanket implementation.
    pub fn new() -> Self {
        Self {
            inner: io::SysOps::from_impl(DefaultIoOps),
        }
    }

    /// Consume the builder and return the composed [`io::SysOps`] table.
    pub fn build(self) -> io::SysOps {
        self.inner
    }

    /// Override the `env` namespace with a pre-built instance.
    pub fn with_env_instance(
        mut self,
        instance: Arc<dyn io::IoNamespaceEnv + Send + Sync + 'static>,
    ) -> Self {
        self.inner.baml_env_get = {
            let t = instance;
            Arc::new(move |heap, args, ctx, call_id| {
                t.__glue_baml_env_get(heap, args, ctx, call_id)
            })
        };
        self
    }

    /// Override the `env` namespace with a default-constructible type.
    pub fn with_env<T: io::IoNamespaceEnv + Default + Send + Sync + 'static>(self) -> Self {
        self.with_env_instance(Arc::new(T::default()))
    }

    /// Override the `fs` namespace (including `fs.File` methods) with a pre-built instance.
    pub fn with_fs_instance(
        mut self,
        instance: Arc<dyn io::IoNamespaceFs + Send + Sync + 'static>,
    ) -> Self {
        self.inner.baml_fs_open = {
            let t = instance.clone();
            Arc::new(move |heap, args, ctx, call_id| {
                t.__glue_baml_fs_open(heap, args, ctx, call_id)
            })
        };
        self.inner.baml_fs_file_read = {
            let t = instance.clone();
            Arc::new(move |heap, args, ctx, call_id| {
                t.__glue_baml_fs_file_read(heap, args, ctx, call_id)
            })
        };
        self.inner.baml_fs_file_close = {
            let t = instance;
            Arc::new(move |heap, args, ctx, call_id| {
                t.__glue_baml_fs_file_close(heap, args, ctx, call_id)
            })
        };
        self
    }

    /// Override the `fs` namespace with a default-constructible type.
    pub fn with_fs<T: io::IoNamespaceFs + Default + Send + Sync + 'static>(self) -> Self {
        self.with_fs_instance(Arc::new(T::default()))
    }

    /// Override the `http` namespace (including `http.Response` methods) with a pre-built instance.
    pub fn with_http_instance(
        mut self,
        instance: Arc<dyn io::IoNamespaceHttp + Send + Sync + 'static>,
    ) -> Self {
        self.inner.baml_http_fetch = {
            let t = instance.clone();
            Arc::new(move |heap, args, ctx, call_id| {
                t.__glue_baml_http_fetch(heap, args, ctx, call_id)
            })
        };
        self.inner.baml_http_send = {
            let t = instance.clone();
            Arc::new(move |heap, args, ctx, call_id| {
                t.__glue_baml_http_send(heap, args, ctx, call_id)
            })
        };
        self.inner.baml_http_response_text = {
            let t = instance;
            Arc::new(move |heap, args, ctx, call_id| {
                t.__glue_baml_http_response_text(heap, args, ctx, call_id)
            })
        };
        self
    }

    /// Override the `http` namespace with a default-constructible type.
    pub fn with_http<T: io::IoNamespaceHttp + Default + Send + Sync + 'static>(self) -> Self {
        self.with_http_instance(Arc::new(T::default()))
    }

    /// Override the `net` namespace (including `net.Socket` methods) with a pre-built instance.
    pub fn with_net_instance(
        mut self,
        instance: Arc<dyn io::IoNamespaceNet + Send + Sync + 'static>,
    ) -> Self {
        self.inner.baml_net_connect = {
            let t = instance.clone();
            Arc::new(move |heap, args, ctx, call_id| {
                t.__glue_baml_net_connect(heap, args, ctx, call_id)
            })
        };
        self.inner.baml_net_socket_read = {
            let t = instance.clone();
            Arc::new(move |heap, args, ctx, call_id| {
                t.__glue_baml_net_socket_read(heap, args, ctx, call_id)
            })
        };
        self.inner.baml_net_socket_close = {
            let t = instance;
            Arc::new(move |heap, args, ctx, call_id| {
                t.__glue_baml_net_socket_close(heap, args, ctx, call_id)
            })
        };
        self
    }

    /// Override the `net` namespace with a default-constructible type.
    pub fn with_net<T: io::IoNamespaceNet + Default + Send + Sync + 'static>(self) -> Self {
        self.with_net_instance(Arc::new(T::default()))
    }

    /// Override the `sys` namespace with a pre-built instance.
    pub fn with_sys_instance(
        mut self,
        instance: Arc<dyn io::IoNamespaceSys + Send + Sync + 'static>,
    ) -> Self {
        self.inner.baml_sys_shell = {
            let t = instance.clone();
            Arc::new(move |heap, args, ctx, call_id| {
                t.__glue_baml_sys_shell(heap, args, ctx, call_id)
            })
        };
        self.inner.baml_sys_sleep = {
            let t = instance.clone();
            Arc::new(move |heap, args, ctx, call_id| {
                t.__glue_baml_sys_sleep(heap, args, ctx, call_id)
            })
        };
        self.inner.baml_sys_panic = {
            let t = instance;
            Arc::new(move |heap, args, ctx, call_id| {
                t.__glue_baml_sys_panic(heap, args, ctx, call_id)
            })
        };
        self
    }

    /// Override the `sys` namespace with a default-constructible type.
    pub fn with_sys<T: io::IoNamespaceSys + Default + Send + Sync + 'static>(self) -> Self {
        self.with_sys_instance(Arc::new(T::default()))
    }
}

impl Default for IoSysOpsBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// Re-export io::SysOps as the primary SysOps type.
pub use io::SysOps;

/// Builder for composing a [`SysOps`] table by overriding namespaces.
///
/// Starts with all operations returning `Unsupported` (except LLM, which uses
/// the blanket implementation), and allows selectively overriding namespaces.
pub type SysOpsBuilder = IoSysOpsBuilder;

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
        let op = bex_vm_types::sys_op_for_path("baml.env.get").unwrap();
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
            SysOp::BamlEnvGet,
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
}
