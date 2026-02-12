//! BEX Sys - System operations for the BEX runtime.
//!
//! This crate provides external I/O operations (file system, network, shell)
//! that the BEX engine can dispatch to. Operations receive and return
//! `BexExternalValue` directly.

use std::{future::Future, pin::Pin, sync::Arc};

// Re-export BexExternalValue and BexValue for ops
pub use bex_external_types::{AsBexExternalValue, BexExternalValue};
pub use bex_heap::BexHeap;
// Re-export SysOp for convenience
pub use bex_vm_types::SysOp;
// ============================================================================
// Operation Errors
// ============================================================================

/// Errors that can occur during external operation execution.
/// Every error is tied to the operation (`fn_name`) that was being called.
#[derive(Debug)]
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

/// Errors that can occur during external operation execution.
#[derive(Debug, thiserror::Error)]
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

    #[error("Not implemented: {message}")]
    NotImplemented { message: String },

    #[error("LLM client error: {message}")]
    LlmClientError { message: String },
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
    dyn for<'a> Fn(&Arc<BexHeap>, Vec<bex_heap::BexValue<'a>>, &SysOpContext) -> SysOpResult
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
pub struct SysOpContext {
    /// Pre-extracted LLM function metadata, keyed by function name.
    /// Used by LLM ops that need to look up function prompt templates, client names, etc.
    pub llm_functions: std::collections::HashMap<String, LlmFunctionInfo>,

    /// Maps function names to their global indices in the VM.
    /// Used by `get_client_function` to return `FunctionRef` values.
    pub function_global_indices: std::collections::HashMap<String, usize>,

    /// Pre-formatted Jinja `{% macro %}` definitions for all `template_strings`.
    /// Prepended to templates by `get_jinja_template`.
    pub template_strings_macros: String,
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

impl SysOpContext {
    /// Create an empty context (for testing or when no LLM functions exist).
    pub fn empty() -> Self {
        Self {
            llm_functions: std::collections::HashMap::new(),
            function_global_indices: std::collections::HashMap::new(),
            template_strings_macros: String::new(),
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
    ($({ $Variant:ident, $path:expr, $snake:ident, $uses_ctx:expr })*) => {
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
                    $( SysOp::$Variant => Arc::new(|_, _, _| SysOpResult::Ready(Err(OpError::unsupported(SysOp::$Variant)))), )*
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
        primitive_client: bex_heap::builtin_types::owned::LlmPrimitiveClient,
        template: String,
        args: BexExternalValue,
    ) -> SysOpOutput<bex_vm_types::PromptAst> {
        SysOpOutput::Ready(
            sys_llm::execute_render_prompt_from_owned(&primitive_client, &template, &args)
                .map_err(OpErrorKind::from),
        )
    }

    fn baml_llm_primitive_client_specialize_prompt(
        &self,
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
        primitive_client: bex_heap::builtin_types::owned::LlmPrimitiveClient,
        response: String,
        function_name: String,
        ctx: &SysOpContext,
    ) -> SysOpOutput {
        let Some(info) = ctx.llm_functions.get(&function_name) else {
            return SysOpOutput::err(OpErrorKind::Other(format!(
                "LLM function not found: {function_name}"
            )));
        };

        SysOpOutput::Ready(
            sys_llm::execute_parse_response_from_owned(
                &primitive_client,
                &response,
                &info.return_type,
            )
            .map_err(OpErrorKind::from),
        )
    }

    fn baml_llm_get_jinja_template(
        &self,
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

    fn baml_llm_get_client_function(
        &self,
        function_name: String,
        ctx: &SysOpContext,
    ) -> SysOpOutput {
        let Some(info) = ctx.llm_functions.get(&function_name) else {
            return SysOpOutput::err(OpErrorKind::Other(format!(
                "LLM function not found: {function_name}"
            )));
        };

        let resolve_fn_name = format!("{}.resolve", info.client_name);
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
        let result = op(&heap, vec![], &ctx);
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
        let result = (ops.baml_fs_open)(&heap, vec![], &ctx);
        assert!(matches!(
            result,
            SysOpResult::Ready(Err(OpError {
                fn_name: SysOp::BamlFsOpen,
                kind: OpErrorKind::Unsupported,
            }))
        ));

        // Test shell returns Unsupported
        let result = (ops.baml_sys_shell)(&heap, vec![], &ctx);
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
        let result = fn_ptr(&heap, vec![], &ctx);
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
}
