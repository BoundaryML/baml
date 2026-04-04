// ============================================================================
// IO pipeline (generated from .baml files via baml_builtins2_codegen)
// ============================================================================

// SysOps struct, IO traits (IoClassFsFile, IoNamespaceFs, etc.),
// view/owned types, from_impl, all_unsupported — all generated from
// `.baml` `$rust_io_function` definitions by `baml_builtins2_codegen`.
#[allow(
    dead_code,
    non_snake_case,
    unreachable_pub,
    unused_imports,
    unused_variables,
    unused_parens,
    clippy::all,
    clippy::wildcard_imports,
    clippy::pub_underscore_fields,
    clippy::used_underscore_binding,
    clippy::redundant_closure_for_method_calls,
    clippy::redundant_clone,
    clippy::used_underscore_items,
    clippy::implicit_clone
)]
pub mod io {
    use std::sync::Arc;

    pub use bex_heap::{AccessError, BexClass, BexValue, BuiltinClass, GcProtectedHeap};
    pub use bex_vm_types::SysOp;
    // Owned structs are generated once in sys_types and re-exported here
    // so that `io::owned::llm::*` paths continue to work.
    pub use sys_types::generated::owned;
    pub use sys_types::{
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
        return_type: baml_type::Ty,
        ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::llm::PromptAst> {
        let old_client = match convert_io_primitive_client(&client) {
            Ok(c) => c,
            Err(e) => return SysOpOutput::err(OpErrorKind::Other(e.to_string())),
        };
        let args_ext = BexExternalValue::Map {
            key_type: baml_type::Ty::string(),
            value_type: baml_type::Ty::unknown(),
            entries: args,
        };
        SysOpOutput::Ready(
            sys_llm::execute_render_prompt_from_owned(
                &old_client,
                &template,
                &args_ext,
                &return_type,
                ctx,
            )
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
        let old_client = match convert_io_primitive_client(&client) {
            Ok(c) => c,
            Err(e) => return SysOpOutput::err(OpErrorKind::Other(e.to_string())),
        };
        let prompt_ast = unwrap_prompt_ast(&prompt);
        SysOpOutput::Ready(
            sys_llm::execute_specialize_prompt_from_owned(&old_client, prompt_ast)
                .map(wrap_prompt_ast)
                .map_err(OpErrorKind::from),
        )
    }

    fn build_request(
        &self,
        heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        client: io::owned::llm::PrimitiveClient,
        prompt: io::owned::llm::PromptAst,
        ctx: &SysOpContext,
    ) -> SysOpOutput<BexExternalValue> {
        let old_client = match convert_io_primitive_client(&client) {
            Ok(c) => c,
            Err(e) => return SysOpOutput::err(OpErrorKind::Other(e.to_string())),
        };
        let prompt_ast = unwrap_prompt_ast(&prompt);
        let callbacks = build_io_callbacks(&ctx.io_callbacks, heap, ctx);
        SysOpOutput::async_op(async move {
            sys_llm::execute_build_request_from_owned(&old_client, prompt_ast, &callbacks)
                .await
                .map(|req| {
                    io::owned::http::Request {
                        method: req.method,
                        url: req.url,
                        headers: req.headers,
                        body: req.body,
                    }
                    .into_bex_external_value()
                })
                .map_err(OpErrorKind::from)
        })
    }

    fn parse(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        client: io::owned::llm::PrimitiveClient,
        response: String,
        type_def: baml_type::Ty,
        ctx: &SysOpContext,
    ) -> SysOpOutput<BexExternalValue> {
        let old_client = match convert_io_primitive_client(&client) {
            Ok(c) => c,
            Err(e) => return SysOpOutput::err(OpErrorKind::Other(e.to_string())),
        };
        SysOpOutput::Ready(
            sys_llm::execute_parse_response_from_owned(&old_client, &response, &type_def, ctx)
                .map(bex_external_types::AsBexExternalValue::into_bex_external_value)
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

    fn __sap_parse(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        json: String,
        ty: baml_type::Ty,
        ctx: &SysOpContext,
    ) -> SysOpOutput<BexExternalValue> {
        SysOpOutput::Ready(
            sys_llm::execute_sap_parse(&json, &ty, ctx, true).map_err(OpErrorKind::from),
        )
    }
}

/// Wrap a `bex_vm_types::PromptAst` (Arc) into the generated `owned::llm::PromptAst`.
fn wrap_prompt_ast(ast: bex_vm_types::PromptAst) -> io::owned::llm::PromptAst {
    io::owned::llm::PromptAst {
        _data: ast as std::sync::Arc<dyn std::any::Any + Send + Sync>,
    }
}

/// Unwrap the `_data` field of a generated `owned::llm::PromptAst` back to `bex_vm_types::PromptAst`.
#[allow(clippy::used_underscore_binding)]
fn unwrap_prompt_ast(owned: &io::owned::llm::PromptAst) -> bex_vm_types::PromptAst {
    owned
        ._data
        .clone()
        .downcast::<baml_builtins2::PromptAst>()
        .expect("PromptAst._data downcast failed: expected Arc<baml_builtins2::PromptAst>. This indicates a bug in wrap_prompt_ast or a type mismatch.")
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
) -> Result<sys_llm::baml_std::PrimitiveClient, sys_llm::baml_std::ClientError> {
    sys_llm::baml_std::PrimitiveClient::new(
        name.clone(),
        provider.clone(),
        sys_llm::baml_std::PrimitiveClientOptions {
            model: options.model.clone(),
            supports_streaming: options.supports_streaming,
            allowed_role_metadata: options.allowed_role_metadata.clone(),
            finish_reason_allow_list: options.finish_reason_allow_list.clone(),
            finish_reason_deny_list: options.finish_reason_deny_list.clone(),
            base_url: options.base_url.clone(),
            default_role: options.default_role.clone(),
            allowed_roles: options.allowed_roles.clone(),
            remap_roles: options.remap_roles.clone(),
            api_key: options.api_key.clone(),
            provider_options: options.provider_options.clone(),
            headers: options.headers.clone(),
            query_params: options.query_params.clone(),
            request_body: options.request_body.clone(),
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
    #[must_use]
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
    #[must_use]
    pub fn with_env<T: io::IoNamespaceEnv + Default + Send + Sync + 'static>(self) -> Self {
        self.with_env_instance(Arc::new(T::default()))
    }

    /// Override the `fs` namespace (including `fs.File` methods) with a pre-built instance.
    #[must_use]
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
    #[must_use]
    pub fn with_fs<T: io::IoNamespaceFs + Default + Send + Sync + 'static>(self) -> Self {
        self.with_fs_instance(Arc::new(T::default()))
    }

    /// Override the `http` namespace (including `http.Response` methods) with a pre-built instance.
    #[must_use]
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
    #[must_use]
    pub fn with_http<T: io::IoNamespaceHttp + Default + Send + Sync + 'static>(self) -> Self {
        self.with_http_instance(Arc::new(T::default()))
    }

    /// Override the `net` namespace (including `net.Socket` methods) with a pre-built instance.
    #[must_use]
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
    #[must_use]
    pub fn with_net<T: io::IoNamespaceNet + Default + Send + Sync + 'static>(self) -> Self {
        self.with_net_instance(Arc::new(T::default()))
    }

    /// Override the `sys` namespace with a pre-built instance.
    #[must_use]
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
    #[must_use]
    pub fn with_sys<T: io::IoNamespaceSys + Default + Send + Sync + 'static>(self) -> Self {
        self.with_sys_instance(Arc::new(T::default()))
    }
}

impl Default for IoSysOpsBuilder {
    fn default() -> Self {
        Self::new()
    }
}

use ::bex_heap::{BexExternalValue, BexHeap, BexValue};
use ::std::sync::Arc;
// Re-export io::SysOps as the primary SysOps type.
use ::sys_types::{
    AsBexExternalValue as _, CallId, FunctionRef, LlmFunctionInfo, OpErrorKind, SysOpContext,
    SysOpFn, SysOpIoCallbacks, SysOpOutput, SysOpResult,
};
pub use io::SysOps;

// ============================================================================
// SysOpFn -> BuildRequestCallbacks adapter
// ============================================================================

/// Resolve a `SysOpResult` (sync or async) into the contained `BexExternalValue`.
async fn resolve_sys_op_result(
    result: SysOpResult,
) -> Result<BexExternalValue, sys_llm::LlmOpError> {
    match result {
        SysOpResult::Ready(Ok(val)) => Ok(val),
        SysOpResult::Ready(Err(e)) => Err(sys_llm::LlmOpError::Other(format!("{e:?}"))),
        SysOpResult::Async(fut) => fut
            .await
            .map_err(|e| sys_llm::LlmOpError::Other(format!("{e:?}"))),
    }
}

/// Build [`sys_llm::BuildRequestCallbacks`] by wrapping the `SysOpFn` pointers
/// from [`SysOpIoCallbacks`]. Each callback marshals its args into
/// `BexExternalValue`, calls the corresponding `SysOpFn`, and extracts the result.
///
/// The `AssertUnwindSafe` wrappers are needed because `SysOpFn` and `BexHeap`
/// don't carry `UnwindSafe`/`RefUnwindSafe` bounds, but the AWS SDK callback
/// traits require them. This is safe because we never actually catch panics
/// across these boundaries.
fn build_io_callbacks(
    io: &SysOpIoCallbacks,
    heap: &Arc<BexHeap>,
    ctx: &SysOpContext,
) -> sys_llm::BuildRequestCallbacks {
    use std::panic::{RefUnwindSafe, UnwindSafe};

    /// Wraps captured state so closures satisfy `UnwindSafe + RefUnwindSafe`.
    /// SAFETY: We never catch panics across these closures; the bounds are only
    /// required by the AWS SDK's `HttpConnector` trait.
    #[derive(Clone)]
    struct Env {
        fn_ptr: SysOpFn,
        heap: Arc<BexHeap>,
        ctx: SysOpContext,
    }
    impl UnwindSafe for Env {}
    impl RefUnwindSafe for Env {}

    // -- env_read: String -> Option<String> ----------------------------------
    let env_read: sys_llm::EnvReadFn = {
        let env = Env {
            fn_ptr: io.env_get.clone(),
            heap: heap.clone(),
            ctx: ctx.clone(),
        };
        Arc::new(move |key: String| {
            let env = env.clone();
            Box::pin(async move {
                let arg = BexExternalValue::String(key);
                let result = (env.fn_ptr)(
                    &env.heap,
                    vec![BexValue::ExternalValue(&arg)],
                    &env.ctx,
                    CallId::next(),
                );
                let val = resolve_sys_op_result(result).await?;
                match val {
                    BexExternalValue::Null => Ok(None),
                    BexExternalValue::String(s) => Ok(Some(s)),
                    other => Err(sys_llm::LlmOpError::Other(format!(
                        "env.get returned unexpected type: {}",
                        other.type_name()
                    ))),
                }
            }) as std::pin::Pin<Box<dyn std::future::Future<Output = _> + Send>>
        })
    };

    // -- http_send: HttpRequest -> HttpSendResponse --------------------------
    let http_send: sys_llm::HttpSendFn = {
        let send_env = Env {
            fn_ptr: io.http_send.clone(),
            heap: heap.clone(),
            ctx: ctx.clone(),
        };
        let text_env = Env {
            fn_ptr: io.http_response_text.clone(),
            heap: heap.clone(),
            ctx: ctx.clone(),
        };
        Arc::new(move |req: sys_llm::baml_std::HttpRequest| {
            let send_env = send_env.clone();
            let text_env = text_env.clone();
            Box::pin(async move {
                // Convert HttpRequest to owned::http::Request -> BexExternalValue
                let io_req = io::owned::http::Request {
                    method: req.method,
                    url: req.url,
                    headers: req.headers,
                    body: req.body,
                };
                let arg = io_req.into_bex_external_value();

                // Call http.send
                let result = (send_env.fn_ptr)(
                    &send_env.heap,
                    vec![BexValue::ExternalValue(&arg)],
                    &send_env.ctx,
                    CallId::next(),
                );
                let response_val = resolve_sys_op_result(result).await?;

                // Extract status_code and headers from the response
                let response = io::owned::http::Response::from_external(response_val.clone())
                    .map_err(|e| {
                        sys_llm::LlmOpError::Other(format!("http.send response parse error: {e:?}"))
                    })?;
                let status_code = u16::try_from(response.status_code).map_err(|_| {
                    sys_llm::LlmOpError::Other(format!(
                        "http.send returned invalid status_code: {}",
                        response.status_code
                    ))
                })?;
                let headers = response.headers;

                // Call http.Response.text() to get the body
                let text_result = (text_env.fn_ptr)(
                    &text_env.heap,
                    vec![BexValue::ExternalValue(&response_val)],
                    &text_env.ctx,
                    CallId::next(),
                );
                let body_val = resolve_sys_op_result(text_result).await?;
                let body = match body_val {
                    BexExternalValue::String(s) => s,
                    other => {
                        return Err(sys_llm::LlmOpError::Other(format!(
                            "http.Response.text() returned unexpected type: {}",
                            other.type_name()
                        )));
                    }
                };

                Ok(sys_llm::HttpSendResponse {
                    status_code,
                    headers,
                    body,
                })
            }) as std::pin::Pin<Box<dyn std::future::Future<Output = _> + Send>>
        })
    };

    // -- fs_read: String -> Vec<u8> ------------------------------------------
    let fs_read: sys_llm::FsReadFn = {
        let open_env = Env {
            fn_ptr: io.fs_open.clone(),
            heap: heap.clone(),
            ctx: ctx.clone(),
        };
        let read_env = Env {
            fn_ptr: io.fs_file_read.clone(),
            heap: heap.clone(),
            ctx: ctx.clone(),
        };
        Arc::new(move |path: String| {
            let open_env = open_env.clone();
            let read_env = read_env.clone();
            Box::pin(async move {
                // fs.open(path) -> File
                let arg = BexExternalValue::String(path);
                let result = (open_env.fn_ptr)(
                    &open_env.heap,
                    vec![BexValue::ExternalValue(&arg)],
                    &open_env.ctx,
                    CallId::next(),
                );
                let file_val = resolve_sys_op_result(result).await?;
                io::owned::fs::File::from_external(file_val.clone()).map_err(|e| {
                    sys_llm::LlmOpError::Other(format!("fs.open returned unexpected type: {e:?}"))
                })?;

                // fs.File.read() -> String
                let result = (read_env.fn_ptr)(
                    &read_env.heap,
                    vec![BexValue::ExternalValue(&file_val)],
                    &read_env.ctx,
                    CallId::next(),
                );
                let content_val = resolve_sys_op_result(result).await?;
                match content_val {
                    BexExternalValue::String(s) => Ok(s.into_bytes()),
                    other => Err(sys_llm::LlmOpError::Other(format!(
                        "fs.File.read() returned unexpected type: {}",
                        other.type_name()
                    ))),
                }
            }) as std::pin::Pin<Box<dyn std::future::Future<Output = _> + Send>>
        })
    };

    // -- shell: String -> String ---------------------------------------------
    let shell: sys_llm::ShellFn = {
        let env = Env {
            fn_ptr: io.sys_shell.clone(),
            heap: heap.clone(),
            ctx: ctx.clone(),
        };
        Arc::new(move |command: String| {
            let env = env.clone();
            Box::pin(async move {
                let arg = BexExternalValue::String(command);
                let result = (env.fn_ptr)(
                    &env.heap,
                    vec![BexValue::ExternalValue(&arg)],
                    &env.ctx,
                    CallId::next(),
                );
                let val = resolve_sys_op_result(result).await?;
                match val {
                    BexExternalValue::String(s) => Ok(s),
                    other => Err(sys_llm::LlmOpError::Other(format!(
                        "sys.shell returned unexpected type: {}",
                        other.type_name()
                    ))),
                }
            }) as std::pin::Pin<Box<dyn std::future::Future<Output = _> + Send>>
        })
    };

    sys_llm::BuildRequestCallbacks {
        http_send,
        env_read,
        fs_read,
        shell,
    }
}

/// Builder for composing a [`SysOps`] table by overriding namespaces.
///
/// Starts with all operations returning `Unsupported` (except LLM, which uses
/// the blanket implementation), and allows selectively overriding namespaces.
pub type SysOpsBuilder = IoSysOpsBuilder;

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
        use sys_types::{OpErrorKind, SysOpResult};

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
        use sys_types::{OpError, OpErrorKind, SysOpResult};

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
        use sys_types::SysOpResult;

        let ops = SysOps::all_unsupported();
        let heap = test_heap();
        let ctx = test_ctx();

        // Test that get() returns the correct function pointer
        let fn_ptr = ops.get(SysOp::BamlFsOpen);
        let result = fn_ptr(&heap, vec![], &ctx, CallId::next());
        assert!(matches!(result, SysOpResult::Ready(Err(_))));
    }

    // ========================================================================
    // build_io_callbacks tests
    // ========================================================================

    /// Helper: build a `SysOpIoCallbacks` where every field is unsupported
    /// except the ones the caller overrides.
    fn test_io_callbacks() -> SysOpIoCallbacks {
        SysOpIoCallbacks::unsupported()
    }

    // -- env_read -------------------------------------------------------------

    #[tokio::test]
    async fn test_env_read_returns_string() {
        let heap = test_heap();
        let ctx = test_ctx();
        let mut io = test_io_callbacks();
        io.env_get = Arc::new(|_heap, _args, _ctx, _id| {
            SysOpResult::Ready(Ok(BexExternalValue::String("my_value".into())))
        });

        let cbs = build_io_callbacks(&io, &heap, &ctx);
        let result = (cbs.env_read)("MY_KEY".into()).await;
        assert_eq!(result.unwrap(), Some("my_value".to_string()));
    }

    #[tokio::test]
    async fn test_env_read_returns_none_for_null() {
        let heap = test_heap();
        let ctx = test_ctx();
        let mut io = test_io_callbacks();
        io.env_get =
            Arc::new(|_heap, _args, _ctx, _id| SysOpResult::Ready(Ok(BexExternalValue::Null)));

        let cbs = build_io_callbacks(&io, &heap, &ctx);
        let result = (cbs.env_read)("MISSING".into()).await;
        assert_eq!(result.unwrap(), None);
    }

    #[tokio::test]
    async fn test_env_read_rejects_wrong_type() {
        let heap = test_heap();
        let ctx = test_ctx();
        let mut io = test_io_callbacks();
        io.env_get =
            Arc::new(|_heap, _args, _ctx, _id| SysOpResult::Ready(Ok(BexExternalValue::Int(42))));

        let cbs = build_io_callbacks(&io, &heap, &ctx);
        let err = (cbs.env_read)("KEY".into()).await.unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("unexpected type"), "got: {msg}");
    }

    #[tokio::test]
    async fn test_env_read_propagates_upstream_error() {
        let heap = test_heap();
        let ctx = test_ctx();
        let mut io = test_io_callbacks();
        io.env_get = Arc::new(|_heap, _args, _ctx, _id| {
            SysOpResult::Ready(Err(sys_types::OpError::new(
                SysOp::BamlEnvGet,
                sys_types::OpErrorKind::Unsupported,
            )))
        });

        let cbs = build_io_callbacks(&io, &heap, &ctx);
        assert!((cbs.env_read)("KEY".into()).await.is_err());
    }

    // -- shell ----------------------------------------------------------------

    #[tokio::test]
    async fn test_shell_returns_string() {
        let heap = test_heap();
        let ctx = test_ctx();
        let mut io = test_io_callbacks();
        io.sys_shell = Arc::new(|_heap, _args, _ctx, _id| {
            SysOpResult::Ready(Ok(BexExternalValue::String("hello\n".into())))
        });

        let cbs = build_io_callbacks(&io, &heap, &ctx);
        let result = (cbs.shell)("echo hello".into()).await;
        assert_eq!(result.unwrap(), "hello\n");
    }

    #[tokio::test]
    async fn test_shell_rejects_wrong_type() {
        let heap = test_heap();
        let ctx = test_ctx();
        let mut io = test_io_callbacks();
        io.sys_shell =
            Arc::new(|_heap, _args, _ctx, _id| SysOpResult::Ready(Ok(BexExternalValue::Null)));

        let cbs = build_io_callbacks(&io, &heap, &ctx);
        let err = (cbs.shell)("cmd".into()).await.unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("unexpected type"), "got: {msg}");
    }

    #[tokio::test]
    async fn test_shell_propagates_upstream_error() {
        let heap = test_heap();
        let ctx = test_ctx();
        let mut io = test_io_callbacks();
        io.sys_shell = Arc::new(|_heap, _args, _ctx, _id| {
            SysOpResult::Ready(Err(sys_types::OpError::new(
                SysOp::BamlSysShell,
                sys_types::OpErrorKind::Unsupported,
            )))
        });

        let cbs = build_io_callbacks(&io, &heap, &ctx);
        assert!((cbs.shell)("cmd".into()).await.is_err());
    }

    // -- fs_read --------------------------------------------------------------

    #[tokio::test]
    async fn test_fs_read_returns_bytes() {
        let heap = test_heap();
        let ctx = test_ctx();
        let mut io = test_io_callbacks();

        // fs.open returns a File instance that fs.File.read receives
        io.fs_open = Arc::new(|_heap, _args, _ctx, _id| {
            use io::AsBexExternalValue;
            let file = io::owned::fs::File {
                _handle: std::sync::Arc::new("file_handle".to_string()),
            };
            SysOpResult::Ready(Ok(file.into_bex_external_value()))
        });
        io.fs_file_read = Arc::new(|_heap, _args, _ctx, _id| {
            SysOpResult::Ready(Ok(BexExternalValue::String("file contents".into())))
        });

        let cbs = build_io_callbacks(&io, &heap, &ctx);
        let result = (cbs.fs_read)("/tmp/test.txt".into()).await;
        assert_eq!(result.unwrap(), b"file contents");
    }

    #[tokio::test]
    async fn test_fs_read_rejects_wrong_type() {
        let heap = test_heap();
        let ctx = test_ctx();
        let mut io = test_io_callbacks();
        io.fs_open = Arc::new(|_heap, _args, _ctx, _id| {
            use io::AsBexExternalValue;
            let file = io::owned::fs::File {
                _handle: std::sync::Arc::new("handle".to_string()),
            };
            SysOpResult::Ready(Ok(file.into_bex_external_value()))
        });
        io.fs_file_read =
            Arc::new(|_heap, _args, _ctx, _id| SysOpResult::Ready(Ok(BexExternalValue::Int(999))));

        let cbs = build_io_callbacks(&io, &heap, &ctx);
        let err = (cbs.fs_read)("path".into()).await.unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("unexpected type"), "got: {msg}");
    }

    #[tokio::test]
    async fn test_fs_read_propagates_open_error() {
        let heap = test_heap();
        let ctx = test_ctx();
        let mut io = test_io_callbacks();
        io.fs_open = Arc::new(|_heap, _args, _ctx, _id| {
            SysOpResult::Ready(Err(sys_types::OpError::new(
                SysOp::BamlFsOpen,
                sys_types::OpErrorKind::Unsupported,
            )))
        });
        // fs_file_read should never be called
        io.fs_file_read = Arc::new(|_heap, _args, _ctx, _id| {
            panic!("should not be called when open fails");
        });

        let cbs = build_io_callbacks(&io, &heap, &ctx);
        assert!((cbs.fs_read)("path".into()).await.is_err());
    }

    // -- http_send ------------------------------------------------------------

    /// Build a fake `http.Response` as a `BexExternalValue::Instance`.
    fn fake_http_response_value(status: i64) -> BexExternalValue {
        use io::AsBexExternalValue;
        io::owned::http::Response {
            status_code: status,
            headers: indexmap::indexmap! {
                "content-type".to_string() => "application/json".to_string(),
            },
            url: "https://example.com".to_string(),
            _body: std::sync::Arc::new(()),
        }
        .into_bex_external_value()
    }

    #[tokio::test]
    async fn test_http_send_happy_path() {
        let heap = test_heap();
        let ctx = test_ctx();
        let mut io = test_io_callbacks();

        io.http_send = Arc::new(|_heap, _args, _ctx, _id| {
            SysOpResult::Ready(Ok(fake_http_response_value(200)))
        });
        io.http_response_text = Arc::new(|_heap, _args, _ctx, _id| {
            SysOpResult::Ready(Ok(BexExternalValue::String(r#"{"ok":true}"#.into())))
        });

        let cbs = build_io_callbacks(&io, &heap, &ctx);
        let resp = (cbs.http_send)(sys_llm::baml_std::HttpRequest {
            method: "GET".into(),
            url: "https://example.com".into(),
            headers: indexmap::IndexMap::new(),
            body: String::new(),
        })
        .await
        .unwrap();

        assert_eq!(resp.status_code, 200);
        assert_eq!(resp.body, r#"{"ok":true}"#);
        assert_eq!(
            resp.headers.get("content-type").map(String::as_str),
            Some("application/json")
        );
    }

    #[tokio::test]
    async fn test_http_send_rejects_wrong_body_type() {
        let heap = test_heap();
        let ctx = test_ctx();
        let mut io = test_io_callbacks();

        io.http_send = Arc::new(|_heap, _args, _ctx, _id| {
            SysOpResult::Ready(Ok(fake_http_response_value(200)))
        });
        io.http_response_text =
            Arc::new(|_heap, _args, _ctx, _id| SysOpResult::Ready(Ok(BexExternalValue::Int(42))));

        let cbs = build_io_callbacks(&io, &heap, &ctx);
        let result = (cbs.http_send)(sys_llm::baml_std::HttpRequest {
            method: "GET".into(),
            url: "https://example.com".into(),
            headers: indexmap::IndexMap::new(),
            body: String::new(),
        })
        .await;

        match result {
            Err(e) => {
                let msg = format!("{e:?}");
                assert!(msg.contains("unexpected type"), "got: {msg}");
            }
            Ok(_) => panic!("expected error for wrong body type"),
        }
    }

    #[tokio::test]
    async fn test_http_send_propagates_send_error() {
        let heap = test_heap();
        let ctx = test_ctx();
        let mut io = test_io_callbacks();

        io.http_send = Arc::new(|_heap, _args, _ctx, _id| {
            SysOpResult::Ready(Err(sys_types::OpError::new(
                SysOp::BamlHttpSend,
                sys_types::OpErrorKind::Unsupported,
            )))
        });
        io.http_response_text = Arc::new(|_heap, _args, _ctx, _id| {
            panic!("should not be called when send fails");
        });

        let cbs = build_io_callbacks(&io, &heap, &ctx);
        assert!(
            (cbs.http_send)(sys_llm::baml_std::HttpRequest {
                method: "GET".into(),
                url: "https://example.com".into(),
                headers: indexmap::IndexMap::new(),
                body: String::new(),
            })
            .await
            .is_err()
        );
    }
}
