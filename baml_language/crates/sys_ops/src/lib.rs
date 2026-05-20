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

    pub use bex_heap::{AccessError, BexClass, BexValue, BuiltinClass, PermitProof};
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
// RuntimeIo adapter (generated from .baml files via baml_builtins2_codegen)
// ============================================================================
// Every `$rust_io_function` in the ns_* .baml files is used by
// `baml_builtins2_codegen` to generate the `RuntimeIo` trait (in
// `sys_types::runtime_io`). RuntimeIo is a flat, typed async interface to all
// sys-ops -- no VM plumbing (BexHeap, SysOpContext, CallId) in its signatures.
// Crates like `sys_llm` take `&dyn RuntimeIo` to call into the runtime IO
// layer (HTTP, env, filesystem, shell) without coupling to the VM.
//
// The generated `RuntimeIoAdapter` below bridges the trait to the underlying
// `SysOpFn` pointers by marshaling typed args through `BexExternalValue`.
//
// The trait carries `UnwindSafe + RefUnwindSafe` bounds because the AWS SDK's
// `HttpConnector` trait requires them on provider objects that store
// `Arc<dyn RuntimeIo>`. The adapter has a manual `impl UnwindSafe` -- this is
// safe because it holds only `Arc` clones (no interior mutability of its own)
// and we never catch panics across the SysOpFn boundary.
// ============================================================================

#[allow(
    dead_code,
    unreachable_pub,
    non_snake_case,
    unused_imports,
    unused_variables,
    clippy::all,
    clippy::redundant_closure_for_method_calls,
    clippy::used_underscore_binding,
    clippy::used_underscore_items
)]
mod io_adapter {
    use std::{future::Future, pin::Pin, sync::Arc};

    #[allow(unused_imports)]
    pub use bex_external_types::BexExternalAdt;
    pub use bex_heap::{BexValue, HeapPermitManager};
    pub use sys_types::{
        AsBexExternalValue, BexExternalValue, BexHeap, CallId, SysOpContext, SysOpFn, SysOpResult,
        runtime_io::*,
    };

    use super::io::SysOps;

    include!(concat!(env!("OUT_DIR"), "/io_adapter.rs"));
}
pub use io_adapter::build_runtime_io;

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
        // `client.name` is the bare BAML identifier (`StubClient`); the
        // synthesized `$new` function picks up the file's pkg + ns prefix
        // later, landing as e.g. `user.lorem.StubClient$new`. Try the
        // unqualified and `user.`-prefixed forms first, then fall back to
        // a suffix scan over `.{name}$new` for clients declared inside a
        // user namespace (`ns_<x>/`). Ambiguity surfaces as a hard error
        // — clients are required to be unique within a package, so two
        // matches mean a synthesis bug.
        let resolve_fn_name = format!("{}$new", client.name);
        let global_index = match sys_types::resolve_name(
            &ctx.function_global_indices,
            &resolve_fn_name,
        ) {
            sys_types::ResolveOutcome::Found(_, idx) => idx,
            sys_types::ResolveOutcome::Ambiguous => {
                return SysOpOutput::err(OpErrorKind::Other(format!(
                    "Client resolve function {resolve_fn_name} matches multiple namespaced entries"
                )));
            }
            sys_types::ResolveOutcome::NotFound => {
                return SysOpOutput::err(OpErrorKind::Other(format!(
                    "Client resolve function not found: {resolve_fn_name}"
                )));
            }
        };
        SysOpOutput::ok(
            FunctionRef::<io::owned::llm::PrimitiveClient>::new(*global_index).into_external(),
        )
    }
}

fn shorthand_to_primitive_client(
    shorthand: &str,
) -> Result<io::owned::llm::PrimitiveClient, OpErrorKind> {
    let shorthand = shorthand.trim();
    let Some((provider, model)) = shorthand.split_once('/') else {
        return Err(OpErrorKind::Other(format!(
            "Invalid short hand name: {shorthand}"
        )));
    };
    if provider.is_empty() || model.is_empty() {
        return Err(OpErrorKind::Other(format!(
            "Invalid short hand name: {shorthand}"
        )));
    }

    Ok(io::owned::llm::PrimitiveClient {
        name: shorthand.to_string(),
        provider: provider.to_string(),
        options: io::owned::llm::PrimitiveClientOptions {
            model: Some(model.to_string()),
            provider_options: BexExternalValue::Null,
            ..Default::default()
        },
    })
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
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        client: io::owned::llm::PrimitiveClient,
        prompt: io::owned::llm::PromptAst,
        return_type: baml_type::Ty,
        ctx: &SysOpContext,
    ) -> SysOpOutput<BexExternalValue> {
        let old_client = match convert_io_primitive_client(&client) {
            Ok(c) => c,
            Err(e) => return SysOpOutput::err(OpErrorKind::Other(e.to_string())),
        };
        let prompt_ast = unwrap_prompt_ast(&prompt);
        let io = ctx.runtime_io.clone();
        SysOpOutput::async_op(async move {
            sys_llm::execute_build_request_from_owned(
                &old_client,
                prompt_ast,
                &return_type,
                io.clone(),
            )
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
        type_arg_0: baml_type::Ty,
        ctx: &SysOpContext,
    ) -> SysOpOutput<BexExternalValue> {
        let old_client = match convert_io_primitive_client(&client) {
            Ok(c) => c,
            Err(e) => return SysOpOutput::err(OpErrorKind::Other(e.to_string())),
        };
        SysOpOutput::Ready(
            sys_llm::execute_parse_response_from_owned(&old_client, &response, &type_arg_0, ctx)
                .map(bex_external_types::AsBexExternalValue::into_bex_external_value)
                .map_err(OpErrorKind::from),
        )
    }

    fn build_request_stream(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        client: io::owned::llm::PrimitiveClient,
        prompt: io::owned::llm::PromptAst,
        return_type: baml_type::Ty,
        ctx: &SysOpContext,
    ) -> SysOpOutput<BexExternalValue> {
        let old_client = match convert_io_primitive_client(&client) {
            Ok(c) => c,
            Err(e) => return SysOpOutput::err(OpErrorKind::Other(e.to_string())),
        };
        let prompt_ast = unwrap_prompt_ast(&prompt);
        let io = ctx.runtime_io.clone();
        SysOpOutput::async_op(async move {
            sys_llm::execute_build_request_stream_from_owned(
                &old_client,
                prompt_ast,
                &return_type,
                io,
            )
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

    fn new_stream_accumulator(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        client: io::owned::llm::PrimitiveClient,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::llm::StreamAccumulator> {
        match sys_llm::stream_accumulator::new_accumulator(&client.provider) {
            Ok(handle) => {
                let handle: std::sync::Arc<dyn std::any::Any + Send + Sync> =
                    std::sync::Arc::new(handle);
                SysOpOutput::ok(io::owned::llm::StreamAccumulator { _handle: handle })
            }
            Err(e) => SysOpOutput::err(OpErrorKind::from(e)),
        }
    }

    fn validate_finish_reason(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        client: io::owned::llm::PrimitiveClient,
        finish_reason: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        let old_client = match convert_io_primitive_client(&client) {
            Ok(c) => c,
            Err(e) => return SysOpOutput::err(OpErrorKind::Other(e.to_string())),
        };
        SysOpOutput::Ready(
            sys_llm::execute_validate_finish_reason(&old_client, &finish_reason)
                .map_err(OpErrorKind::from),
        )
    }
}

/// Look up an LLM function by name via the canonical
/// [`sys_types::resolve_name`] rule. The suffix-scan step handles
/// functions declared inside a user namespace (e.g. `ns_lorem/`) — the
/// synthesized companion passes the bare BAML identifier, not the FQN,
/// so without it a namespaced LLM function fails to resolve. Ambiguity
/// collapses to `None` so the caller's "not found" error path surfaces.
fn lookup_llm_function<'a>(
    function_name: &str,
    llm_functions: &'a std::collections::HashMap<String, LlmFunctionInfo>,
) -> Option<&'a LlmFunctionInfo> {
    // Ambiguity collapses to `None` so the existing "function not found"
    // error path surfaces — cleaner than silently picking one match.
    sys_types::resolve_name(llm_functions, function_name)
        .found()
        .map(|(_k, info)| info)
}

/// Blanket impl — all types get real `StreamAccumulator` behavior via `sys_llm` delegation.
impl<T> io::IoClassLlmStreamAccumulator for T {
    fn add_events(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        accumulator: io::owned::llm::StreamAccumulator,
        events: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        let Ok(handle) = accumulator
            ._handle
            .downcast::<bex_resource_types::ResourceHandle>()
        else {
            return SysOpOutput::err(OpErrorKind::Other(
                "Invalid stream accumulator handle".into(),
            ));
        };
        match sys_llm::stream_accumulator::add_events(&handle, &events) {
            Ok(()) => SysOpOutput::ok(()),
            Err(e) => SysOpOutput::err(OpErrorKind::from(e)),
        }
    }

    fn content(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        accumulator: io::owned::llm::StreamAccumulator,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        let Ok(handle) = accumulator
            ._handle
            .downcast::<bex_resource_types::ResourceHandle>()
        else {
            return SysOpOutput::err(OpErrorKind::Other(
                "Invalid stream accumulator handle".into(),
            ));
        };
        match sys_llm::stream_accumulator::get_content(&handle) {
            Ok(content) => SysOpOutput::ok(content),
            Err(e) => SysOpOutput::err(OpErrorKind::from(e)),
        }
    }

    fn is_done(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        accumulator: io::owned::llm::StreamAccumulator,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<bool> {
        let Ok(handle) = accumulator
            ._handle
            .downcast::<bex_resource_types::ResourceHandle>()
        else {
            return SysOpOutput::err(OpErrorKind::Other(
                "Invalid stream accumulator handle".into(),
            ));
        };
        match sys_llm::stream_accumulator::is_done(&handle) {
            Ok(done) => SysOpOutput::ok(done),
            Err(e) => SysOpOutput::err(OpErrorKind::from(e)),
        }
    }

    fn model(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        accumulator: io::owned::llm::StreamAccumulator,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<String>> {
        let Ok(handle) = accumulator
            ._handle
            .downcast::<bex_resource_types::ResourceHandle>()
        else {
            return SysOpOutput::err(OpErrorKind::Other(
                "Invalid stream accumulator handle".into(),
            ));
        };
        match sys_llm::stream_accumulator::get_model(&handle) {
            Ok(model) => SysOpOutput::ok(model),
            Err(e) => SysOpOutput::err(OpErrorKind::from(e)),
        }
    }

    fn finish_reason(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        accumulator: io::owned::llm::StreamAccumulator,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<String>> {
        let Ok(handle) = accumulator
            ._handle
            .downcast::<bex_resource_types::ResourceHandle>()
        else {
            return SysOpOutput::err(OpErrorKind::Other(
                "Invalid stream accumulator handle".into(),
            ));
        };
        match sys_llm::stream_accumulator::get_finish_reason(&handle) {
            Ok(reason) => SysOpOutput::ok(reason),
            Err(e) => SysOpOutput::err(OpErrorKind::from(e)),
        }
    }

    fn input_tokens(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        accumulator: io::owned::llm::StreamAccumulator,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<i64>> {
        let Ok(handle) = accumulator
            ._handle
            .downcast::<bex_resource_types::ResourceHandle>()
        else {
            return SysOpOutput::err(OpErrorKind::Other(
                "Invalid stream accumulator handle".into(),
            ));
        };
        match sys_llm::stream_accumulator::get_input_tokens(&handle) {
            Ok(tokens) => SysOpOutput::ok(tokens.map(u64::cast_signed)),
            Err(e) => SysOpOutput::err(OpErrorKind::from(e)),
        }
    }

    fn output_tokens(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        accumulator: io::owned::llm::StreamAccumulator,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<i64>> {
        let Ok(handle) = accumulator
            ._handle
            .downcast::<bex_resource_types::ResourceHandle>()
        else {
            return SysOpOutput::err(OpErrorKind::Other(
                "Invalid stream accumulator handle".into(),
            ));
        };
        match sys_llm::stream_accumulator::get_output_tokens(&handle) {
            Ok(tokens) => SysOpOutput::ok(tokens.map(u64::cast_signed)),
            Err(e) => SysOpOutput::err(OpErrorKind::from(e)),
        }
    }
}

/// Blanket impl — `StreamCache.new()` creates a SAP cache from a type descriptor.
impl<T> io::IoClassLlmStreamCache for T {
    fn new(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        target: baml_type::Ty,
        stream_target: baml_type::Ty,
        ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::llm::StreamCache> {
        let compiled =
            match ::bex_sap::CompiledSapModel::from_sys_op_context(ctx, target, stream_target) {
                Ok(compiled) => compiled,
                Err(e) => return SysOpOutput::err(OpErrorKind::Other(e.to_string())),
            };
        let sap = ::sys_llm::SapStreamCache::new(compiled);
        let data: std::sync::Arc<dyn std::any::Any + Send + Sync> = std::sync::Arc::new(sap);
        SysOpOutput::ok(io::owned::llm::StreamCache { _data: data })
    }
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

    fn get_stream_return_type(
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
        SysOpOutput::ok(info.stream_return_type.clone())
    }

    fn from_shorthand(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        shorthand: String,
        ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::llm::PrimitiveClient> {
        // Shorthand has no syntactic api_key binding to populate at lower
        // time, so for the two providers whose env-var convention is
        // ubiquitous enough to be safe to assume — `OPENAI_API_KEY` for
        // openai and `ANTHROPIC_API_KEY` for anthropic — pull the key
        // from the environment here. Other providers (vertex, bedrock,
        // azure, …) need explicit configuration and stay untouched.
        let mut client = match shorthand_to_primitive_client(&shorthand) {
            Ok(c) => c,
            Err(e) => return SysOpOutput::err(e),
        };
        let env_var: Option<&'static str> = match client.provider.as_str() {
            "openai" | "openai-responses" => Some("OPENAI_API_KEY"),
            "anthropic" => Some("ANTHROPIC_API_KEY"),
            _ => None,
        };
        let Some(env_var) = env_var else {
            return SysOpOutput::ok(client);
        };
        let io = ctx.runtime_io.clone();
        SysOpOutput::async_op(async move {
            if let Ok(Some(val)) = io.env_get(env_var.to_string()).await {
                client.options.api_key = Some(val);
            }
            Ok(client)
        })
    }

    fn __sap_parse_final(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        json: String,
        cache: io::owned::llm::StreamCache,
        _type_arg_0: baml_type::Ty,
        _type_arg_1: baml_type::Ty,
        ctx: &SysOpContext,
    ) -> SysOpOutput<BexExternalValue> {
        let Ok(sap) = cache._data.downcast::<::sys_llm::SapStreamCache>() else {
            return SysOpOutput::err(OpErrorKind::Other(
                "Invalid StreamCache: expected SapStreamCache".into(),
            ));
        };
        SysOpOutput::Ready(
            sys_llm::execute_sap_parse_final(&json, &sap, ctx).map_err(OpErrorKind::from),
        )
    }

    fn __sap_parse_partial(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        json: String,
        cache: io::owned::llm::StreamCache,
        _type_arg_0: baml_type::Ty,
        _type_arg_1: baml_type::Ty,
        ctx: &SysOpContext,
    ) -> SysOpOutput<BexExternalValue> {
        let Ok(sap) = cache._data.downcast::<::sys_llm::SapStreamCache>() else {
            return SysOpOutput::err(OpErrorKind::Other(
                "Invalid StreamCache: expected SapStreamCache".into(),
            ));
        };
        let result = match sys_llm::execute_sap_parse_partial(&json, &sap, ctx) {
            Ok(Some(value)) => Ok(value),
            Ok(None) => Ok(BexExternalValue::instance(
                "baml.stream.StreamNoYield",
                ::indexmap::IndexMap::new(),
            )),
            Err(e) => Err(OpErrorKind::from(e)),
        };
        SysOpOutput::Ready(result)
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
            media_url_handler: options.media_url_handler.clone(),
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
    fn text(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: io::owned::fs::File,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
    fn bytes(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: io::owned::fs::File,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<u8>> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
    fn read(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: io::owned::fs::File,
        _n: i64,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
    fn read_bytes(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: io::owned::fs::File,
        _n: i64,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<u8>> {
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
    fn seek_from(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: io::owned::fs::File,
        _whence: BexExternalValue,
        _offset: i64,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
    fn write(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: io::owned::fs::File,
        _data: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
    fn write_bytes(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: io::owned::fs::File,
        _data: Vec<u8>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
}

impl io::IoNamespaceFs for DefaultIoOps {
    fn open(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _mode: BexExternalValue,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::fs::File> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }

    fn exists(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<bool> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }

    fn remove(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }

    fn size(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }

    fn read(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }

    fn write(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _content: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }

    fn write_bytes(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _content: Vec<u8>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }

    fn read_dir(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<io::owned::fs::DirEntry>> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }

    fn mkdir(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _options: io::owned::fs::MkdirOptions,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
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
    fn bytes(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _r: io::owned::http::Response,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<u8>> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
}

impl io::IoClassHttpSseStream for DefaultIoOps {
    fn next(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _s: io::owned::http::SseStream,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<String>> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
    fn close(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _s: io::owned::http::SseStream,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
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
    fn fetch_sse(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _req: io::owned::http::Request,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::http::SseStream> {
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
    fn write(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _s: io::owned::net::Socket,
        _data: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
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

impl io::IoClassNetTcpListener for DefaultIoOps {
    fn accept(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _l: io::owned::net::TcpListener,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::net::Socket> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
    fn close(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _l: io::owned::net::TcpListener,
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
    fn listen(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _addr: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::net::TcpListener> {
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

impl io::IoNamespaceIo for DefaultIoOps {
    fn input(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _prompt: Option<String>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
    fn print(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
    fn println(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
    fn eprint(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
    fn eprintln(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
}

impl io::IoNamespaceSys for DefaultIoOps {
    fn exec(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _program: String,
        _args: Option<Vec<String>>,
        _options: Option<io::owned::sys::ProcessOptions>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::sys::ShellOutput> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }

    fn shell(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _command: String,
        _options: Option<io::owned::sys::ProcessOptions>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::sys::ShellOutput> {
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
}

impl io::IoClassGlobGlob for DefaultIoOps {
    fn scan(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _glob: io::owned::glob::Glob,
        _root: BexExternalValue,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<String>> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }

    fn matches(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _glob: io::owned::glob::Glob,
        _path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<bool> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
}

impl io::IoNamespaceGlob for DefaultIoOps {
    fn new(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _pattern: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::glob::Glob> {
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
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_env_get(heap, permit, args, ctx, call_id)
            })
        };
        self
    }

    /// Override the `env` namespace with a default-constructible type.
    #[must_use]
    pub fn with_env<T: io::IoNamespaceEnv + Default + Send + Sync + 'static>(self) -> Self {
        self.with_env_instance(Arc::new(T::default()))
    }

    /// Override the `io` namespace with a pre-built instance.
    #[must_use]
    pub fn with_io_instance(
        mut self,
        instance: Arc<dyn io::IoNamespaceIo + Send + Sync + 'static>,
    ) -> Self {
        self.inner.baml_io_input = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_io_input(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_io_print = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_io_print(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_io_println = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_io_println(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_io_eprint = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_io_eprint(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_io_eprintln = {
            let t = instance;
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_io_eprintln(heap, permit, args, ctx, call_id)
            })
        };
        self
    }

    /// Override the `io` namespace with a default-constructible type.
    #[must_use]
    pub fn with_io<T: io::IoNamespaceIo + Default + Send + Sync + 'static>(self) -> Self {
        self.with_io_instance(Arc::new(T::default()))
    }

    /// Override the `fs` namespace (including `fs.File` methods) with a pre-built instance.
    #[must_use]
    pub fn with_fs_instance(
        mut self,
        instance: Arc<dyn io::IoNamespaceFs + Send + Sync + 'static>,
    ) -> Self {
        self.inner.baml_fs_open = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_fs_open(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_fs_exists = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_fs_exists(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_fs_remove = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_fs_remove(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_fs_size = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_fs_size(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_fs_read = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_fs_read(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_fs_write = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_fs_write(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_fs_write_bytes = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_fs_write_bytes(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_fs_file_text = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_fs_file_text(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_fs_file_bytes = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_fs_file_bytes(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_fs_file_read = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_fs_file_read(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_fs_file_read_bytes = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_fs_file_read_bytes(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_fs_file_close = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_fs_file_close(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_fs_file_seek_from = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_fs_file_seek_from(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_fs_file_write = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_fs_file_write(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_fs_file_write_bytes = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_fs_file_write_bytes(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_fs_read_dir = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_fs_read_dir(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_fs_mkdir = {
            let t = instance;
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_fs_mkdir(heap, permit, args, ctx, call_id)
            })
        };
        self
    }

    /// Override the `fs` namespace with a default-constructible type.
    #[must_use]
    pub fn with_fs<T: io::IoNamespaceFs + Default + Send + Sync + 'static>(self) -> Self {
        self.with_fs_instance(Arc::new(T::default()))
    }

    /// Override the `glob` namespace (including `glob.Glob` methods) with a pre-built instance.
    #[must_use]
    pub fn with_glob_instance(
        mut self,
        instance: Arc<dyn io::IoNamespaceGlob + Send + Sync + 'static>,
    ) -> Self {
        self.inner.baml_glob_new = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_glob_new(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_glob_glob_scan = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_glob_glob_scan(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_glob_glob_matches = {
            let t = instance;
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_glob_glob_matches(heap, permit, args, ctx, call_id)
            })
        };
        self
    }

    /// Override the `glob` namespace with a default-constructible type.
    #[must_use]
    pub fn with_glob<T: io::IoNamespaceGlob + Default + Send + Sync + 'static>(self) -> Self {
        self.with_glob_instance(Arc::new(T::default()))
    }

    /// Override the `http` namespace (including `http.Response` methods) with a pre-built instance.
    #[must_use]
    pub fn with_http_instance(
        mut self,
        instance: Arc<dyn io::IoNamespaceHttp + Send + Sync + 'static>,
    ) -> Self {
        self.inner.baml_http_fetch = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_http_fetch(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_http_send = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_http_send(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_http_response_text = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_http_response_text(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_http_response_bytes = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_http_response_bytes(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_http_fetch_sse = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_http_fetch_sse(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_http_ssestream_next = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_http_ssestream_next(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_http_ssestream_close = {
            let t = instance;
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_http_ssestream_close(heap, permit, args, ctx, call_id)
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
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_net_connect(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_net_listen = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_net_listen(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_net_socket_read = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_net_socket_read(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_net_socket_write = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_net_socket_write(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_net_socket_close = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_net_socket_close(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_net_tcplistener_accept = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_net_tcplistener_accept(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_net_tcplistener_close = {
            let t = instance;
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_net_tcplistener_close(heap, permit, args, ctx, call_id)
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
    #[allow(clippy::needless_pass_by_value)]
    #[must_use]
    pub fn with_sys_instance(
        mut self,
        instance: Arc<dyn io::IoNamespaceSys + Send + Sync + 'static>,
    ) -> Self {
        self.inner.baml_sys_exec = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_sys_exec(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_sys_shell = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_sys_shell(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_sys_sleep = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_sys_sleep(heap, permit, args, ctx, call_id)
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

use ::bex_heap::{BexExternalValue, BexHeap};
use ::std::sync::Arc;
// Re-export io::SysOps as the primary SysOps type.
use ::sys_types::{
    AsBexExternalValue as _, CallId, FunctionRef, LlmFunctionInfo, OpErrorKind, SysOpContext,
    SysOpOutput,
};
pub use io::SysOps;

/// Builder for composing a [`SysOps`] table by overriding namespaces.
///
/// Starts with all operations returning `Unsupported` (except LLM, which uses
/// the blanket implementation), and allows selectively overriding namespaces.
pub type SysOpsBuilder = IoSysOpsBuilder;

#[cfg(test)]
mod tests {
    use bex_heap::HeapPermit;
    use bex_vm_types::SysOp;

    use super::*;

    fn test_heap() -> Arc<BexHeap> {
        BexHeap::new(vec![])
    }

    fn test_ctx() -> SysOpContext {
        SysOpContext::empty()
    }

    async fn test_permit() -> bex_heap::ActiveHeapPermit<()> {
        bex_heap::HeapPermitManager::new()
            .new_permit(())
            .await
            .acquire()
            .await
    }

    #[tokio::test]
    async fn test_unsupported_returns_error() {
        use sys_types::{OpErrorKind, SysOpResult};

        let heap = test_heap();
        let ctx = test_ctx();
        let op = SysOps::unsupported(SysOp::BamlSysShell);
        let permit = test_permit().await;
        let result = op(&heap, permit.proof(), vec![], &ctx, CallId::next());
        match result {
            SysOpResult::Ready(Err(e)) => {
                assert!(matches!(e.kind, OpErrorKind::Unsupported));
                assert_eq!(e.fn_name, SysOp::BamlSysShell);
            }
            _ => panic!("Expected Unsupported error"),
        }
    }

    #[tokio::test]
    async fn test_all_unsupported() {
        use sys_types::{OpError, OpErrorKind, SysOpResult};

        let heap = test_heap();
        let ctx = test_ctx();
        let ops = SysOps::all_unsupported();
        let permit = test_permit().await;

        // Test fs_open returns Unsupported
        let result = (ops.baml_fs_open)(&heap, permit.proof(), vec![], &ctx, CallId::next());
        assert!(matches!(
            result,
            SysOpResult::Ready(Err(OpError {
                fn_name: SysOp::BamlFsOpen,
                kind: OpErrorKind::Unsupported,
            }))
        ));

        // Test shell returns Unsupported
        let result = (ops.baml_sys_shell)(&heap, permit.proof(), vec![], &ctx, CallId::next());
        assert!(matches!(
            result,
            SysOpResult::Ready(Err(OpError {
                fn_name: SysOp::BamlSysShell,
                kind: OpErrorKind::Unsupported,
            }))
        ));
    }

    #[tokio::test]
    async fn test_sys_ops_get() {
        use sys_types::SysOpResult;

        let ops = SysOps::all_unsupported();
        let heap = test_heap();
        let ctx = test_ctx();
        let permit = test_permit().await;

        // Test that get() returns the correct function pointer
        let fn_ptr = ops.get(SysOp::BamlFsOpen);
        let result = fn_ptr(&heap, permit.proof(), vec![], &ctx, CallId::next());
        assert!(matches!(result, SysOpResult::Ready(Err(_))));
    }

    #[test]
    fn test_shorthand_to_primitive_client_uses_first_slash_only() {
        let client = shorthand_to_primitive_client("openrouter/meta-llama/llama-3.1").unwrap();

        assert_eq!(client.name, "openrouter/meta-llama/llama-3.1");
        assert_eq!(client.provider, "openrouter");
        assert_eq!(
            client.options.model.as_deref(),
            Some("meta-llama/llama-3.1")
        );
    }

    #[test]
    fn test_shorthand_to_primitive_client_rejects_invalid_values() {
        let err = shorthand_to_primitive_client("openai").unwrap_err();
        assert_eq!(
            err,
            OpErrorKind::Other("Invalid short hand name: openai".to_string())
        );
    }
}
