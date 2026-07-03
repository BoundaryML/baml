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
        AsBexExternalValue, BexExternalValue, BexHeap, CallId, OpError, SysOpContext, SysOpFn,
        SysOpOutput, SysOpResult, VmBamlError, VmPanic, VmRustFnError,
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
                return SysOpOutput::err(VmBamlError::DevOther {
                    message: format!(
                        "Client resolve function {resolve_fn_name} matches multiple namespaced entries"
                    ),
                });
            }
            sys_types::ResolveOutcome::NotFound => {
                return SysOpOutput::err(VmBamlError::DevOther {
                    message: format!("Client resolve function not found: {resolve_fn_name}"),
                });
            }
        };
        SysOpOutput::ok(
            FunctionRef::<io::owned::llm::PrimitiveClient>::new(*global_index).into_external(),
        )
    }
}

fn shorthand_to_primitive_client(
    shorthand: &str,
) -> Result<io::owned::llm::PrimitiveClient, VmRustFnError> {
    let shorthand = shorthand.trim();
    let Some((provider, model)) = shorthand.split_once('/') else {
        return Err(VmBamlError::InvalidArgument {
            message: format!("Invalid short hand name: {shorthand}"),
        }
        .into());
    };
    if provider.is_empty() || model.is_empty() {
        return Err(VmBamlError::InvalidArgument {
            message: format!("Invalid short hand name: {shorthand}"),
        }
        .into());
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
        return_type: baml_type::RuntimeTy,
        ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::llm::PromptAst> {
        let old_client = match convert_io_primitive_client(&client) {
            Ok(c) => c,
            Err(e) => {
                return SysOpOutput::err(VmBamlError::InvalidArgument {
                    message: e.to_string(),
                });
            }
        };
        let args_ext = BexExternalValue::Map {
            key_type: baml_type::RuntimeTy::string(),
            value_type: baml_type::RuntimeTy::unknown(),
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
            .map_err(VmRustFnError::from),
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
            Err(e) => {
                return SysOpOutput::err(VmBamlError::InvalidArgument {
                    message: e.to_string(),
                });
            }
        };
        let prompt_ast = unwrap_prompt_ast(&prompt);
        SysOpOutput::Ready(
            sys_llm::execute_specialize_prompt_from_owned(&old_client, prompt_ast)
                .map(wrap_prompt_ast)
                .map_err(VmRustFnError::from),
        )
    }

    fn build_request(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        client: io::owned::llm::PrimitiveClient,
        prompt: io::owned::llm::PromptAst,
        return_type: baml_type::RuntimeTy,
        ctx: &SysOpContext,
    ) -> SysOpOutput<BexExternalValue> {
        let old_client = match convert_io_primitive_client(&client) {
            Ok(c) => c,
            Err(e) => {
                return SysOpOutput::err(VmBamlError::InvalidArgument {
                    message: e.to_string(),
                });
            }
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
            .map_err(VmRustFnError::from)
        })
    }

    fn parse(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        client: io::owned::llm::PrimitiveClient,
        response: String,
        type_arg_0: baml_type::RuntimeTy,
        ctx: &SysOpContext,
    ) -> SysOpOutput<BexExternalValue> {
        let old_client = match convert_io_primitive_client(&client) {
            Ok(c) => c,
            Err(e) => {
                return SysOpOutput::err(VmBamlError::InvalidArgument {
                    message: e.to_string(),
                });
            }
        };
        SysOpOutput::Ready(
            sys_llm::execute_parse_response_from_owned(&old_client, &response, &type_arg_0, ctx)
                .map(bex_external_types::AsBexExternalValue::into_bex_external_value)
                .map_err(VmRustFnError::from),
        )
    }

    fn build_request_stream(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        client: io::owned::llm::PrimitiveClient,
        prompt: io::owned::llm::PromptAst,
        return_type: baml_type::RuntimeTy,
        ctx: &SysOpContext,
    ) -> SysOpOutput<BexExternalValue> {
        let old_client = match convert_io_primitive_client(&client) {
            Ok(c) => c,
            Err(e) => {
                return SysOpOutput::err(VmBamlError::InvalidArgument {
                    message: e.to_string(),
                });
            }
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
            .map_err(VmRustFnError::from)
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
            Err(e) => SysOpOutput::err(VmRustFnError::from(e)),
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
            Err(e) => {
                return SysOpOutput::err(VmBamlError::InvalidArgument {
                    message: e.to_string(),
                });
            }
        };
        SysOpOutput::Ready(
            sys_llm::execute_validate_finish_reason(&old_client, &finish_reason)
                .map_err(VmRustFnError::from),
        )
    }
}

/// Look up an LLM function by name via the canonical
/// [`sys_types::resolve_name`] rule. The suffix-scan step handles
/// functions declared inside a user namespace (e.g. `ns_lorem/`) — the
/// synthesized companion passes the bare BAML identifier, not the FQN,
/// so without it a namespaced LLM function fails to resolve. Returns the
/// full `ResolveOutcome` (rather than collapsing to `Option`) so callers
/// can distinguish ambiguity from a true not-found in their error
/// messages: both still abort the sysop as a `DevOther`, but the
/// distinction matters for diagnosing synthesis / name-resolution bugs.
fn lookup_llm_function<'a>(
    function_name: &str,
    llm_functions: &'a std::collections::HashMap<String, LlmFunctionInfo>,
) -> sys_types::ResolveOutcome<'a, LlmFunctionInfo> {
    sys_types::resolve_name(llm_functions, function_name)
}

/// Format a `lookup_llm_function` miss as a sysop error message,
/// distinguishing ambiguous from not-found.
fn llm_function_lookup_error(
    function_name: &str,
    outcome: &sys_types::ResolveOutcome<'_, LlmFunctionInfo>,
) -> VmBamlError {
    match outcome {
        sys_types::ResolveOutcome::Found(_, _) => {
            // Unreachable in practice — caller only invokes this on a miss.
            // We still produce a coherent message rather than panicking so
            // a future refactor can't accidentally trip on this.
            VmBamlError::DevOther {
                message: format!(
                    "internal: llm_function_lookup_error called with a Found \
                     outcome for `{function_name}`"
                ),
            }
        }
        sys_types::ResolveOutcome::NotFound => VmBamlError::DevOther {
            message: format!("LLM function not found: {function_name}"),
        },
        sys_types::ResolveOutcome::Ambiguous => VmBamlError::DevOther {
            message: format!(
                "LLM function name `{function_name}` is ambiguous: two or more \
                 namespaced functions end with `.{function_name}`. Pass a fully \
                 qualified name (e.g. `<pkg>.<ns>.{function_name}`) to disambiguate."
            ),
        },
    }
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
            return SysOpOutput::err(VmBamlError::DevOther {
                message: "Invalid stream accumulator handle".into(),
            });
        };
        match sys_llm::stream_accumulator::add_events(&handle, &events) {
            Ok(()) => SysOpOutput::ok(()),
            Err(e) => SysOpOutput::err(VmRustFnError::from(e)),
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
            return SysOpOutput::err(VmBamlError::DevOther {
                message: "Invalid stream accumulator handle".into(),
            });
        };
        match sys_llm::stream_accumulator::get_content(&handle) {
            Ok(content) => SysOpOutput::ok(content),
            Err(e) => SysOpOutput::err(VmRustFnError::from(e)),
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
            return SysOpOutput::err(VmBamlError::DevOther {
                message: "Invalid stream accumulator handle".into(),
            });
        };
        match sys_llm::stream_accumulator::is_done(&handle) {
            Ok(done) => SysOpOutput::ok(done),
            Err(e) => SysOpOutput::err(VmRustFnError::from(e)),
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
            return SysOpOutput::err(VmBamlError::DevOther {
                message: "Invalid stream accumulator handle".into(),
            });
        };
        match sys_llm::stream_accumulator::get_model(&handle) {
            Ok(model) => SysOpOutput::ok(model),
            Err(e) => SysOpOutput::err(VmRustFnError::from(e)),
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
            return SysOpOutput::err(VmBamlError::DevOther {
                message: "Invalid stream accumulator handle".into(),
            });
        };
        match sys_llm::stream_accumulator::get_finish_reason(&handle) {
            Ok(reason) => SysOpOutput::ok(reason),
            Err(e) => SysOpOutput::err(VmRustFnError::from(e)),
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
            return SysOpOutput::err(VmBamlError::DevOther {
                message: "Invalid stream accumulator handle".into(),
            });
        };
        match sys_llm::stream_accumulator::get_input_tokens(&handle) {
            Ok(tokens) => SysOpOutput::ok(tokens.map(u64::cast_signed)),
            Err(e) => SysOpOutput::err(VmRustFnError::from(e)),
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
            return SysOpOutput::err(VmBamlError::DevOther {
                message: "Invalid stream accumulator handle".into(),
            });
        };
        match sys_llm::stream_accumulator::get_output_tokens(&handle) {
            Ok(tokens) => SysOpOutput::ok(tokens.map(u64::cast_signed)),
            Err(e) => SysOpOutput::err(VmRustFnError::from(e)),
        }
    }
}

/// Blanket impl — `StreamCache.new()` creates a SAP cache from a type descriptor.
/// Parameter order follows the BAML decl (`new(streaming, target)` — stream
/// type first, mirroring `StreamCache<TStream, TFinal>`).
impl<T> io::IoClassLlmStreamCache for T {
    fn new(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        stream_target: baml_type::RuntimeTy,
        target: baml_type::RuntimeTy,
        ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::llm::StreamCache> {
        let compiled =
            match ::bex_sap::CompiledSapModel::from_sys_op_context(ctx, target, stream_target) {
                Ok(compiled) => compiled,
                Err(e) => {
                    return SysOpOutput::err(VmBamlError::InvalidArgument {
                        message: e.to_string(),
                    });
                }
            };
        let sap = ::sys_llm::SapStreamCache::new(compiled);
        let data: std::sync::Arc<dyn std::any::Any + Send + Sync> = std::sync::Arc::new(sap);
        SysOpOutput::ok(io::owned::llm::StreamCache { _data: data })
    }
}

/// Blanket impl — `Context.output_format_with(...)` re-renders the return
/// type's schema with caller options (BEP-049 §10 / M5b.2). `Context._output_format`
/// carries the prebuilt schema as an opaque handle, so this only re-renders it.
impl<T> io::IoClassLlmContext for T {
    #[allow(clippy::too_many_arguments)]
    fn output_format_with(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        context: io::owned::llm::Context,
        prefix: Option<String>,
        or_splitter: Option<String>,
        enum_value_prefix: Option<String>,
        hoisted_class_prefix: Option<String>,
        always_hoist_enums: Option<bool>,
        quote_class_fields: Option<bool>,
        hoist_classes: Option<Vec<String>>,
        map_style: Option<String>,
        render_null_as: Option<String>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        // Render the prebuilt schema handle with the caller's options. The
        // `Option → RenderOptions` mapping lives inside sys_llm (those option
        // types are crate-internal there).
        let content = unwrap_output_format(&context._output_format);
        SysOpOutput::ok(sys_llm::render_output_format_content(
            &content,
            prefix,
            or_splitter,
            enum_value_prefix,
            hoisted_class_prefix,
            always_hoist_enums,
            quote_class_fields,
            hoist_classes,
            map_style,
            render_null_as,
        ))
    }
}

impl<T> io::IoNamespaceAi for T {
    fn prompt_to_messages(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        prompt: BexExternalValue,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<io::owned::ai::ChatMessage>> {
        // The cross-namespace param arrives untyped. Depending on the boundary it is
        // either the PromptAst ADT itself, or a `baml.llm.PromptAst` instance whose
        // `_data` field carries the handle (as an ADT or a raw RustData).
        fn extract_ast(v: &BexExternalValue) -> Option<bex_vm_types::PromptAst> {
            match v {
                BexExternalValue::Adt(::bex_external_types::BexExternalAdt::PromptAst(a)) => {
                    Some(a.clone())
                }
                BexExternalValue::RustData(data) => {
                    data.clone().downcast::<baml_builtins2::PromptAst>().ok()
                }
                BexExternalValue::Instance { fields, .. } => {
                    fields.get("_data").and_then(extract_ast)
                }
                BexExternalValue::Union { value, .. } => extract_ast(value),
                _ => None,
            }
        }
        let Some(ast) = extract_ast(&prompt) else {
            return SysOpOutput::err(VmBamlError::InvalidArgument {
                message: "prompt_to_messages: expected a baml.llm.PromptAst".to_string(),
            });
        };
        SysOpOutput::ok(prompt_ast_to_chat_messages(&ast))
    }
}

/// Convert a `PromptAst` into native `baml.ai.ChatMessage` values: roles preserved,
/// text and media parts interleaved. Role-less content becomes a `"user"` message
/// (the specialize pass has normally wrapped simples with the client default role
/// already, so this is a fallback).
fn prompt_ast_to_chat_messages(ast: &baml_builtins2::PromptAst) -> Vec<io::owned::ai::ChatMessage> {
    use baml_builtins2::{PromptAst, PromptAstSimple};

    fn empty_part() -> io::owned::ai::MessagePart {
        io::owned::ai::MessagePart {
            text: None,
            image: None,
            audio: None,
            pdf: None,
            video: None,
        }
    }

    fn text_part(t: String) -> io::owned::ai::MessagePart {
        let mut p = empty_part();
        p.text = Some(t);
        p
    }

    fn media_part(m: &std::sync::Arc<baml_builtins2::MediaValue>) -> io::owned::ai::MessagePart {
        // Runtime media values are `baml.media.*` instances carrying the MediaValue in
        // `_data` (mirrors bex_vm's `copy::media` constructors) — not a bare media ADT.
        fn media_instance(
            class_name: &str,
            m: &std::sync::Arc<baml_builtins2::MediaValue>,
        ) -> BexExternalValue {
            let mut fields = ::indexmap::IndexMap::new();
            fields.insert(
                "_data".to_string(),
                BexExternalValue::RustData(
                    m.clone() as std::sync::Arc<dyn std::any::Any + Send + Sync>
                ),
            );
            BexExternalValue::Instance {
                class_name: class_name.to_string(),
                type_args: vec![],
                fields,
            }
        }
        let mut p = empty_part();
        match m.kind {
            ::bex_external_types::MediaKind::Audio => {
                p.audio = Some(media_instance("baml.media.Audio", m));
            }
            ::bex_external_types::MediaKind::Pdf => {
                p.pdf = Some(media_instance("baml.media.Pdf", m));
            }
            ::bex_external_types::MediaKind::Video => {
                p.video = Some(media_instance("baml.media.Video", m));
            }
            // `Generic` ("could be any") defaults to the image slot — the dominant case.
            ::bex_external_types::MediaKind::Image | ::bex_external_types::MediaKind::Generic => {
                p.image = Some(media_instance("baml.media.Image", m));
            }
        }
        p
    }

    fn simple_to_parts(s: &PromptAstSimple, out: &mut Vec<io::owned::ai::MessagePart>) {
        match s {
            PromptAstSimple::String(t) => {
                if !t.is_empty() {
                    out.push(text_part(t.clone()));
                }
            }
            PromptAstSimple::Media(m) => out.push(media_part(m)),
            PromptAstSimple::Multiple(items) => {
                for it in items {
                    simple_to_parts(it, out);
                }
            }
        }
    }

    fn mk_message(role: &str, content: &PromptAstSimple) -> io::owned::ai::ChatMessage {
        let mut parts = Vec::new();
        simple_to_parts(content, &mut parts);
        if parts.is_empty() {
            // Preserve an empty message rather than dropping it.
            parts.push(text_part(String::new()));
        }
        io::owned::ai::ChatMessage {
            role: role.to_string(),
            parts,
        }
    }

    fn walk(ast: &PromptAst, out: &mut Vec<io::owned::ai::ChatMessage>) {
        match ast {
            PromptAst::Simple(s) => out.push(mk_message("user", s)),
            PromptAst::Message { role, content, .. } => out.push(mk_message(role, content)),
            PromptAst::Vec(items) => {
                for it in items {
                    walk(it, out);
                }
            }
        }
    }

    let mut out = Vec::new();
    walk(ast, &mut out);
    out
}

// ============================================================================
// `baml.schema` — JSON Schema lowering (P7)
// ============================================================================

impl<T> io::IoNamespaceSchema for T {
    fn json_schema(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        t: baml_type::RuntimeTy,
        strict: bool,
        ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        let mut ancestry: Vec<baml_type::Name> = Vec::new();
        match schema::ty_to_json_schema(&t, strict, ctx, &mut ancestry) {
            Ok(value) => {
                SysOpOutput::ok(serde_json::to_string(&value).unwrap_or_else(|_| "{}".to_string()))
            }
            Err(message) => SysOpOutput::err(VmBamlError::Unsupported { message }),
        }
    }
}

/// JSON Schema lowering of a `baml_type::RuntimeTy`. In `strict` mode the emitted
/// schema follows OpenAI structured-output rules: every object closes with
/// `"additionalProperties": false` and lists ALL fields in `required` (optional
/// BAML fields keep their `null`-inclusive union schema rather than being dropped
/// from `required`).
mod schema {
    use baml_type::RuntimeTy;
    use serde_json::{Value, json};
    use sys_types::SysOpContext;

    /// Lower `ty` to a JSON Schema value. Returns `Err(message)` for constructs
    /// with no JSON Schema representation (functions, opaque rust types,
    /// unresolved generic params) — the caller surfaces it as
    /// `baml.errors.Unsupported`.
    pub(super) fn ty_to_json_schema(
        ty: &RuntimeTy,
        strict: bool,
        ctx: &SysOpContext,
        ancestry: &mut Vec<baml_type::Name>,
    ) -> Result<Value, String> {
        match ty {
            RuntimeTy::Int { .. } | RuntimeTy::Bigint { .. } => Ok(json!({ "type": "integer" })),
            RuntimeTy::Float { .. } => Ok(json!({ "type": "number" })),
            RuntimeTy::String { .. } => Ok(json!({ "type": "string" })),
            RuntimeTy::Bool { .. } => Ok(json!({ "type": "boolean" })),
            RuntimeTy::Null { .. } => Ok(json!({ "type": "null" })),
            // Binary payloads travel as base64 strings on the JSON wire.
            RuntimeTy::Uint8Array { .. } => Ok(json!({ "type": "string" })),
            RuntimeTy::Literal(lit, _, _) => Ok(literal_schema(lit)),
            RuntimeTy::List(inner, _) => Ok(json!({
                "type": "array",
                "items": ty_to_json_schema(inner, strict, ctx, ancestry)?,
            })),
            RuntimeTy::Map { value, .. } => Ok(json!({
                "type": "object",
                "additionalProperties": ty_to_json_schema(value, strict, ctx, ancestry)?,
            })),
            RuntimeTy::Union(members, _) => union_schema(members, strict, ctx, ancestry),
            RuntimeTy::Enum(name, _) => enum_schema(name, ctx),
            RuntimeTy::Class(name, _, _) => class_schema(name, strict, ctx, ancestry),
            RuntimeTy::TypeAlias(name, _) => {
                if let Some(target) = find_type_alias_definition(ctx, name) {
                    ty_to_json_schema(&target.clone(), strict, ctx, ancestry)
                } else {
                    // Opaque / recursive alias (e.g. `baml.json.json`): accept any JSON.
                    Ok(json!({}))
                }
            }
            // `unknown` accepts any JSON value.
            RuntimeTy::BuiltinUnknown { .. } => Ok(json!({})),
            other => Err(format!(
                "json_schema: no JSON Schema representation for `{}`",
                other.render_user_facing()
            )),
        }
    }

    fn literal_schema(lit: &baml_base::Literal) -> Value {
        use baml_base::Literal;
        match lit {
            Literal::Int(i) => json!({ "type": "integer", "const": i }),
            Literal::Bigint(n) => json!({ "type": "integer", "const": n.to_string() }),
            Literal::Float(s) => {
                json!({ "type": "number", "const": s.parse::<f64>().unwrap_or(0.0) })
            }
            Literal::String(s) => json!({ "type": "string", "const": s }),
            Literal::Bool(b) => json!({ "type": "boolean", "const": b }),
        }
    }

    fn union_schema(
        members: &[RuntimeTy],
        strict: bool,
        ctx: &SysOpContext,
        ancestry: &mut Vec<baml_type::Name>,
    ) -> Result<Value, String> {
        let has_null = members.iter().any(RuntimeTy::is_null);
        let non_null: Vec<&RuntimeTy> = members.iter().filter(|m| !m.is_null()).collect();
        if non_null.is_empty() {
            return Ok(json!({ "type": "null" }));
        }
        let mut schemas: Vec<Value> = non_null
            .iter()
            .map(|m| ty_to_json_schema(m, strict, ctx, ancestry))
            .collect::<Result<Vec<_>, _>>()?;
        if schemas.len() == 1 {
            let base = schemas.pop().unwrap_or_else(|| json!({}));
            return Ok(if has_null { with_null(base) } else { base });
        }
        if has_null {
            schemas.push(json!({ "type": "null" }));
        }
        Ok(json!({ "anyOf": schemas }))
    }

    /// Widen a single-typed schema to also admit `null`. A schema whose `"type"`
    /// is a plain string becomes a `["<type>", "null"]` array (OpenAI strict's
    /// preferred nullable form); anything richer falls back to `anyOf`.
    fn with_null(base: Value) -> Value {
        if let Value::Object(obj) = &base {
            if let Some(Value::String(t)) = obj.get("type") {
                let mut widened = obj.clone();
                widened.insert("type".to_string(), json!([t, "null"]));
                return Value::Object(widened);
            }
        }
        json!({ "anyOf": [base, { "type": "null" }] })
    }

    fn enum_schema(name: &baml_type::TypeName, ctx: &SysOpContext) -> Result<Value, String> {
        let enum_def = find_enum_definition(ctx, name)
            .ok_or_else(|| format!("json_schema: unknown enum `{}`", name.display_name()))?;
        let variants: Vec<Value> = enum_def
            .variants
            .iter()
            .map(|v| json!(v.alias.clone().unwrap_or_else(|| v.name.clone())))
            .collect();
        Ok(json!({ "type": "string", "enum": variants }))
    }

    fn class_schema(
        name: &baml_type::TypeName,
        strict: bool,
        ctx: &SysOpContext,
        ancestry: &mut Vec<baml_type::Name>,
    ) -> Result<Value, String> {
        let key = name.display_name();
        // Recursive class: OpenAI strict mode has no `$defs`/`$ref` support here,
        // so a cycle degrades to a permissive object rather than diverging.
        if ancestry.contains(&key) {
            return Ok(json!({ "type": "object" }));
        }
        let class_def = find_class_definition(ctx, name)
            .ok_or_else(|| format!("json_schema: unknown class `{}`", name.display_name()))?;

        ancestry.push(key);
        let mut properties = serde_json::Map::new();
        let mut required: Vec<Value> = Vec::new();
        for field in &class_def.fields {
            if field.skip {
                continue;
            }
            let prop_name = field.alias.clone().unwrap_or_else(|| field.name.clone());
            let field_schema = ty_to_json_schema(&field.field_type, strict, ctx, ancestry)?;
            properties.insert(prop_name.clone(), field_schema);
            let is_optional = field.field_type.is_nullable_union() || field.field_type.is_null();
            // Strict mode: EVERY field is required (optionals keep their nullable
            // union schema). Otherwise only non-optional fields are required.
            if strict || !is_optional {
                required.push(json!(prop_name));
            }
        }
        ancestry.pop();

        let mut obj = serde_json::Map::new();
        obj.insert("type".to_string(), json!("object"));
        obj.insert("properties".to_string(), Value::Object(properties));
        obj.insert("required".to_string(), Value::Array(required));
        if strict {
            obj.insert("additionalProperties".to_string(), json!(false));
        }
        Ok(Value::Object(obj))
    }

    fn find_class_definition<'a>(
        ctx: &'a SysOpContext,
        type_name: &baml_type::TypeName,
    ) -> Option<&'a sys_types::ClassDefinition> {
        ctx.class_definitions.get(type_name).or_else(|| {
            let mut matches = ctx
                .class_definitions
                .iter()
                .filter(|(name, _)| name.display_name() == type_name.display_name())
                .map(|(_, def)| def);
            let first = matches.next()?;
            matches.next().is_none().then_some(first)
        })
    }

    fn find_enum_definition<'a>(
        ctx: &'a SysOpContext,
        type_name: &baml_type::TypeName,
    ) -> Option<&'a sys_types::EnumDefinition> {
        ctx.enum_definitions.get(type_name).or_else(|| {
            let mut matches = ctx
                .enum_definitions
                .iter()
                .filter(|(name, _)| name.display_name() == type_name.display_name())
                .map(|(_, def)| def);
            let first = matches.next()?;
            matches.next().is_none().then_some(first)
        })
    }

    fn find_type_alias_definition<'a>(
        ctx: &'a SysOpContext,
        type_name: &baml_type::TypeName,
    ) -> Option<&'a RuntimeTy> {
        ctx.type_alias_definitions.get(type_name).or_else(|| {
            let mut matches = ctx
                .type_alias_definitions
                .iter()
                .filter(|(name, _)| name.display_name() == type_name.display_name())
                .map(|(_, ty)| ty);
            let first = matches.next()?;
            matches.next().is_none().then_some(first)
        })
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
        // Aligned with `get_constructor`: the function name passed here is
        // synthesised by the compiler from the call site, so a missing entry
        // indicates a build artifact mismatch (a synthesis bug), not a
        // user-recoverable argument error. Ambiguity is surfaced separately
        // (rather than collapsed to "not found") so debuggers see the
        // actual failure mode.
        let outcome = lookup_llm_function(&function_name, &ctx.llm_functions);
        let sys_types::ResolveOutcome::Found(_, info) = outcome else {
            return SysOpOutput::err(llm_function_lookup_error(&function_name, &outcome));
        };
        let dedented = sys_llm::preprocess_template(&info.prompt_template);
        let template = if ctx.template_strings_macros.is_empty() {
            dedented
        } else {
            format!("{}\n{}", ctx.template_strings_macros, dedented)
        };
        SysOpOutput::ok(template)
    }

    fn assemble_prompt_ast(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        parts: Vec<String>,
        values: Vec<BexExternalValue>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::llm::PromptAst> {
        // BEP-049 §10 (M5d): structural PromptAst assembly — no magic delimiters.
        let ast = std::sync::Arc::new(assemble_prompt_ast_impl(&parts, &values)).merge_adjacent();
        SysOpOutput::ok(wrap_prompt_ast(ast))
    }

    fn render_output_format(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        return_type: baml_type::RuntimeTy,
        ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        // BEP-049 §10 (M5b): the `ctx.output_format` schema string.
        SysOpOutput::ok(sys_llm::render_output_format(&return_type, ctx))
    }

    fn build_output_format(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        return_type: baml_type::RuntimeTy,
        ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::llm::OutputFormat> {
        // BEP-049 §10 (M5b.2): build the opaque schema handle `Context._output_format`
        // carries; `output_format_with(...)` renders it with caller options.
        let content = sys_llm::build_output_format_content(&return_type, ctx);
        SysOpOutput::ok(wrap_output_format(std::sync::Arc::new(content)))
    }

    fn get_return_type(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        function_name: String,
        ctx: &SysOpContext,
    ) -> SysOpOutput<baml_type::RuntimeTy> {
        let outcome = lookup_llm_function(&function_name, &ctx.llm_functions);
        let sys_types::ResolveOutcome::Found(_, info) = outcome else {
            return SysOpOutput::err(llm_function_lookup_error(&function_name, &outcome));
        };
        SysOpOutput::ok(info.return_type.clone())
    }

    fn prompt_to_text(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        prompt: io::owned::llm::PromptAst,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        let ast = unwrap_prompt_ast(&prompt);
        SysOpOutput::ok(prompt_ast_to_text(&ast))
    }

    fn prompt_has_media(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        prompt: io::owned::llm::PromptAst,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<bool> {
        let ast = unwrap_prompt_ast(&prompt);
        SysOpOutput::ok(prompt_ast_has_media(&ast))
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
            // Body never errors; annotate the error type so the
            // `Result<_, VmRustFnError>` return contract is locked in.
            Ok::<_, bex_vm_types::errors::VmRustFnError>(client)
        })
    }

    fn __sap_parse_final(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        json: String,
        cache: io::owned::llm::StreamCache,
        _type_arg_0: baml_type::RuntimeTy,
        _type_arg_1: baml_type::RuntimeTy,
        ctx: &SysOpContext,
    ) -> SysOpOutput<BexExternalValue> {
        let Ok(sap) = cache._data.downcast::<::sys_llm::SapStreamCache>() else {
            return SysOpOutput::err(VmBamlError::DevOther {
                message: "Invalid StreamCache: expected SapStreamCache".into(),
            });
        };
        SysOpOutput::Ready(
            sys_llm::execute_sap_parse_final(&json, &sap, ctx).map_err(VmRustFnError::from),
        )
    }

    fn __sap_parse_partial(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        json: String,
        cache: io::owned::llm::StreamCache,
        _type_arg_0: baml_type::RuntimeTy,
        _type_arg_1: baml_type::RuntimeTy,
        ctx: &SysOpContext,
    ) -> SysOpOutput<BexExternalValue> {
        let Ok(sap) = cache._data.downcast::<::sys_llm::SapStreamCache>() else {
            return SysOpOutput::err(VmBamlError::DevOther {
                message: "Invalid StreamCache: expected SapStreamCache".into(),
            });
        };
        let result = match sys_llm::execute_sap_parse_partial(&json, &sap, ctx) {
            Ok(Some(value)) => Ok(value),
            Ok(None) => Ok(BexExternalValue::instance(
                "baml.stream.StreamNoYield",
                ::indexmap::IndexMap::new(),
            )),
            Err(e) => Err(VmRustFnError::from(e)),
        };
        SysOpOutput::Ready(result)
    }
}

/// Role name of an interpolated value if it is a `baml.llm.Role` instance (set
/// by the in-template `role(...)` constructor), else `None`.
fn prompt_role_name(v: &BexExternalValue) -> Option<String> {
    if let BexExternalValue::Instance {
        class_name, fields, ..
    } = v
        && (class_name == "baml.llm.Role" || class_name.ends_with(".Role"))
        && let Some(BexExternalValue::String(name)) = fields.get("name")
    {
        return Some(name.to_string());
    }
    None
}

/// Best-effort text form of a non-`Role` interpolated value (M5d slice: scalars;
/// media / complex values are deferred).
fn prompt_value_text(v: &BexExternalValue) -> String {
    match v {
        BexExternalValue::String(s) => s.to_string(),
        BexExternalValue::Int(i) => i.to_string(),
        BexExternalValue::Float(f) => f.to_string(),
        BexExternalValue::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

/// BEP-049 §10 (M5d): fold a tagged template's `parts`/`values` into a
/// `PromptAst`. Walks them interleaved — a `Role` value starts a new chat
/// message; strings (and scalar values) accumulate into the current message's
/// content. With no `Role` values the whole template is a single
/// `PromptAst::Simple`. Mirrors the message-folding of `parse_chat_prompt` but
/// off the structured arrays instead of magic-delimiter string parsing.
/// Flatten a `PromptAst` into a single plain-text string: every message's textual
/// content, concatenated with a newline between messages. Media parts are skipped and
/// role structure is dropped (v1 bridge for a string-taking BAML provider).
fn prompt_ast_to_text(ast: &baml_builtins2::PromptAst) -> String {
    use baml_builtins2::{PromptAst, PromptAstSimple};
    fn simple_text(s: &PromptAstSimple, out: &mut String) {
        match s {
            PromptAstSimple::String(t) => out.push_str(t),
            PromptAstSimple::Media(_) => {}
            PromptAstSimple::Multiple(items) => {
                for it in items {
                    simple_text(it, out);
                }
            }
        }
    }
    fn walk(a: &PromptAst, out: &mut String) {
        match a {
            PromptAst::Simple(s) => simple_text(s, out),
            PromptAst::Message { content, .. } => {
                simple_text(content, out);
                out.push('\n');
            }
            PromptAst::Vec(items) => {
                for it in items {
                    walk(it, out);
                }
            }
        }
    }
    let mut out = String::new();
    walk(ast, &mut out);
    out.trim_end().to_string()
}

/// True iff the `PromptAst` contains any media part (image/audio/pdf/video).
fn prompt_ast_has_media(ast: &baml_builtins2::PromptAst) -> bool {
    use baml_builtins2::{PromptAst, PromptAstSimple};
    fn simple_has(s: &PromptAstSimple) -> bool {
        match s {
            PromptAstSimple::String(_) => false,
            PromptAstSimple::Media(_) => true,
            PromptAstSimple::Multiple(items) => items.iter().any(|it| simple_has(it)),
        }
    }
    match ast {
        PromptAst::Simple(s) => simple_has(s),
        PromptAst::Message { content, .. } => simple_has(content),
        PromptAst::Vec(items) => items.iter().any(|it| prompt_ast_has_media(it)),
    }
}

/// A media value interpolated into a tagged-template prompt: the bare media ADT, a
/// `baml.media.*` wrapper instance (unwrapped via `_data`), or either inside a union.
fn prompt_value_media(v: &BexExternalValue) -> Option<std::sync::Arc<baml_builtins2::MediaValue>> {
    match v {
        BexExternalValue::Adt(::bex_external_types::BexExternalAdt::Media(m)) => Some(m.clone()),
        BexExternalValue::Union { value, .. } => prompt_value_media(value),
        BexExternalValue::Instance {
            class_name, fields, ..
        } if class_name.starts_with("baml.media.") => {
            fields.get("_data").and_then(prompt_value_media)
        }
        _ => None,
    }
}

fn assemble_prompt_ast_impl(
    parts: &[String],
    values: &[BexExternalValue],
) -> baml_builtins2::PromptAst {
    use baml_builtins2::{PromptAst, PromptAstSimple};

    // Flush accumulated text into the message's simple-part list.
    fn flush_text(
        text: &mut String,
        simples: &mut Vec<std::sync::Arc<baml_builtins2::PromptAstSimple>>,
    ) {
        if !text.is_empty() {
            simples.push(std::sync::Arc::new(PromptAstSimple::String(
                std::mem::take(text),
            )));
        }
    }

    // Close the current message's content. Text-only messages collapse to the same
    // single-`String` shape as the pre-media fold (BEP-049 M5d), so prompt
    // byte-equivalence is unchanged; media interleaves as `Multiple` parts.
    fn close_content(
        text: &mut String,
        simples: &mut Vec<std::sync::Arc<baml_builtins2::PromptAstSimple>>,
    ) -> std::sync::Arc<baml_builtins2::PromptAstSimple> {
        if simples.is_empty() {
            return std::sync::Arc::new(PromptAstSimple::String(std::mem::take(text)));
        }
        flush_text(text, simples);
        if simples.len() == 1 {
            simples.pop().expect("non-empty")
        } else {
            std::sync::Arc::new(PromptAstSimple::Multiple(std::mem::take(simples)))
        }
    }

    let mk_msg =
        |role: String, content: std::sync::Arc<PromptAstSimple>| -> std::sync::Arc<PromptAst> {
            std::sync::Arc::new(PromptAst::Message {
                role,
                content,
                // `metadata` is `serde_json::Value`, not a direct dep here, so it
                // can't be named for `Value::default()`; its `Default` is
                // `Value::Null`. Role metadata threading lands later.
                #[allow(clippy::default_trait_access)]
                metadata: Default::default(),
            })
        };
    let mut messages: Vec<std::sync::Arc<PromptAst>> = Vec::new();
    let mut current_role: Option<String> = None;
    let mut simples: Vec<std::sync::Arc<PromptAstSimple>> = Vec::new();
    let mut text = String::new();
    for (i, value) in values.iter().enumerate() {
        if let Some(p) = parts.get(i) {
            text.push_str(p);
        }
        if let Some(role) = prompt_role_name(value) {
            if let Some(prev) = current_role.take() {
                let content = close_content(&mut text, &mut simples);
                messages.push(mk_msg(prev, content));
            }
            current_role = Some(role);
        } else if let Some(media) = prompt_value_media(value) {
            flush_text(&mut text, &mut simples);
            simples.push(std::sync::Arc::new(PromptAstSimple::Media(media)));
        } else {
            text.push_str(&prompt_value_text(value));
        }
    }
    if let Some(p) = parts.get(values.len()) {
        text.push_str(p);
    }
    match current_role {
        Some(role) => {
            let content = close_content(&mut text, &mut simples);
            messages.push(mk_msg(role, content));
            if messages.len() == 1 {
                std::sync::Arc::try_unwrap(messages.pop().expect("non-empty"))
                    .unwrap_or_else(|arc| (*arc).clone())
            } else {
                PromptAst::Vec(messages)
            }
        }
        None => PromptAst::Simple(close_content(&mut text, &mut simples)),
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

/// Wrap an `OutputFormatContent` into the generated `owned::llm::OutputFormat` handle.
fn wrap_output_format(
    content: std::sync::Arc<sys_llm::OutputFormatContent>,
) -> io::owned::llm::OutputFormat {
    io::owned::llm::OutputFormat {
        _data: content as std::sync::Arc<dyn std::any::Any + Send + Sync>,
    }
}

/// Unwrap a generated `owned::llm::OutputFormat` handle back to its `OutputFormatContent`.
#[allow(clippy::used_underscore_binding)]
fn unwrap_output_format(
    owned: &io::owned::llm::OutputFormat,
) -> std::sync::Arc<sys_llm::OutputFormatContent> {
    owned
        ._data
        .clone()
        .downcast::<sys_llm::OutputFormatContent>()
        .expect("OutputFormat._data downcast failed: expected Arc<OutputFormatContent>. This indicates a bug in wrap_output_format or a type mismatch.")
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
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn bytes(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: io::owned::fs::File,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<u8>> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn read(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: io::owned::fs::File,
        _n: i64,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn read_bytes(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: io::owned::fs::File,
        _n: i64,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<u8>> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn close(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: io::owned::fs::File,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
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
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn write(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: io::owned::fs::File,
        _data: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn write_bytes(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: io::owned::fs::File,
        _data: Vec<u8>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
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
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn exists(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<bool> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn remove(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn remove_dir(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn remove_dir_all(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn size(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn read(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn write(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _content: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn write_bytes(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _content: Vec<u8>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn read_dir(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<io::owned::fs::DirEntry>> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn mkdir(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _options: io::owned::fs::MkdirOptions,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
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
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn bytes(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _r: io::owned::http::Response,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<u8>> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn new(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _status_code: i64,
        _headers: indexmap::IndexMap<String, String>,
        _body: Vec<u8>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::http::Response> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn new_streaming(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _status_code: i64,
        _headers: indexmap::IndexMap<String, String>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::http::Response> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn write(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _r: io::owned::http::Response,
        _data: Vec<u8>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn end(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _r: io::owned::http::Response,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

impl io::IoClassHttpTlsConfig for DefaultIoOps {
    fn _new(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _cert_pem: Vec<u8>,
        _key_pem: Vec<u8>,
        _allow_tls1_2: bool,
        _handshake_timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::http::TlsConfig> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

impl io::IoClassHttpServer for DefaultIoOps {
    fn bind(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _addr: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::http::Server> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn _serve(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _server: io::owned::http::Server,
        _handler: bex_external_types::Handle,
        _tls_config: Option<io::owned::http::TlsConfig>,
        _allow_http1: bool,
        _allow_http2: bool,
        _max_body_size: i64,
        _max_connections: i64,
        _header_read_timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
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
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn close(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _s: io::owned::http::SseStream,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

impl io::IoNamespaceHttp for DefaultIoOps {
    fn _fetch(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _url: String,
        _timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::http::Response> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn _send(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _req: io::owned::http::Request,
        _timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::http::Response> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn fetch_sse(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _req: io::owned::http::Request,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::http::SseStream> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

impl io::IoClassWsWsStream for DefaultIoOps {
    fn send(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _s: io::owned::ws::WsStream,
        _text: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn next(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _s: io::owned::ws::WsStream,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<String>> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn close(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _s: io::owned::ws::WsStream,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

impl io::IoNamespaceWs for DefaultIoOps {
    fn connect(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _url: String,
        _headers: indexmap::IndexMap<String, String>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::ws::WsStream> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

impl io::IoClassNetTcpStream for DefaultIoOps {
    fn _connect(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _addr: String,
        _timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::net::TcpStream> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn _read(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _s: io::owned::net::TcpStream,
        _timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<u8>> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn _write(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _s: io::owned::net::TcpStream,
        _data: Vec<u8>,
        _timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn close(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _s: io::owned::net::TcpStream,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

impl io::IoClassNetTcpListener for DefaultIoOps {
    fn bind(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _addr: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::net::TcpListener> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn accept(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _l: io::owned::net::TcpListener,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::net::TcpStream> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn close(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _l: io::owned::net::TcpListener,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

impl io::IoClassNetUdpSocket for DefaultIoOps {
    fn bind(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _addr: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::net::UdpSocket> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn _send_to(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _s: io::owned::net::UdpSocket,
        _data: Vec<u8>,
        _addr: String,
        _timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn _recv_from(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _s: io::owned::net::UdpSocket,
        _timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::net::Datagram> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn close(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _s: io::owned::net::UdpSocket,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

impl io::IoNamespaceNet for DefaultIoOps {}

impl io::IoNamespaceEnv for DefaultIoOps {
    fn get(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _key: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<String>> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
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
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn print(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn println(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn eprint(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn eprintln(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
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
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn shell(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _command: String,
        _options: Option<io::owned::sys::ProcessOptions>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::sys::ShellOutput> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn sleep(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _delay: BexExternalValue,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
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
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn matches(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _glob: io::owned::glob::Glob,
        _path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<bool> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
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
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

impl io::IoNamespaceHost for DefaultIoOps {
    fn call_host_value(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _handle: BexExternalValue,
        _args: Vec<BexExternalValue>,
        _type_arg_0: baml_type::RuntimeTy,
        _type_arg_1: baml_type::RuntimeTy,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<BexExternalValue> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

impl io::IoClassTimeInstant for DefaultIoOps {
    fn now(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::time::Instant> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

impl io::IoNamespaceTime for DefaultIoOps {
    fn system_timezone(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn _tz_offset_at(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _timezone: String,
        _at_ns: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<i64>> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn _tz_to_instant(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _timezone: String,
        _civil_ns: Arc<num_bigint::BigInt>,
        _disambiguation: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<Arc<num_bigint::BigInt>>> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
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
        self.inner.baml_http__fetch = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_http__fetch(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_http__send = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_http__send(heap, permit, args, ctx, call_id)
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
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_http_ssestream_close(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_http_response_new = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_http_response_new(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_http_response_new_streaming = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_http_response_new_streaming(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_http_response_write = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_http_response_write(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_http_response_end = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_http_response_end(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_http_tlsconfig__new = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_http_tlsconfig__new(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_http_server_bind = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_http_server_bind(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_http_server__serve = {
            let t = instance;
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_http_server__serve(heap, permit, args, ctx, call_id)
            })
        };
        self
    }

    /// Override the `http` namespace with a default-constructible type.
    #[must_use]
    pub fn with_http<T: io::IoNamespaceHttp + Default + Send + Sync + 'static>(self) -> Self {
        self.with_http_instance(Arc::new(T::default()))
    }

    /// Override the `net` namespace (`TcpStream` / `TcpListener` / `UdpSocket`
    /// methods) with a pre-built instance.
    #[must_use]
    pub fn with_net_instance(
        mut self,
        instance: Arc<dyn io::IoNamespaceNet + Send + Sync + 'static>,
    ) -> Self {
        self.inner.baml_net_tcpstream__connect = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_net_tcpstream__connect(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_net_tcpstream__read = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_net_tcpstream__read(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_net_tcpstream__write = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_net_tcpstream__write(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_net_tcpstream_close = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_net_tcpstream_close(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_net_tcplistener_bind = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_net_tcplistener_bind(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_net_tcplistener_accept = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_net_tcplistener_accept(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_net_tcplistener_close = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_net_tcplistener_close(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_net_udpsocket_bind = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_net_udpsocket_bind(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_net_udpsocket__send_to = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_net_udpsocket__send_to(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_net_udpsocket__recv_from = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_net_udpsocket__recv_from(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_net_udpsocket_close = {
            let t = instance;
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_net_udpsocket_close(heap, permit, args, ctx, call_id)
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

    /// Override the `host` namespace (host-callable dispatch) with a pre-built instance.
    ///
    /// Only the WASM bridge uses this builder method: it composes its `SysOps`
    /// here and injects its JS dispatch impl explicitly, wiring the
    /// [`io::IoNamespaceHost::call_host_value`] sysop to a bridge-specific
    /// dispatch implementation that fires the host-language callable. The
    /// native bridges (Python, Node, Go) instead wire dispatch through
    /// `sys_native::NativeSysOps` (passed to [`SysOps::from_impl`]) and never
    /// call this method.
    #[must_use]
    pub fn with_host_instance(
        mut self,
        instance: Arc<dyn io::IoNamespaceHost + Send + Sync + 'static>,
    ) -> Self {
        self.inner.baml_host_call_host_value = {
            let t = instance;
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_host_call_host_value(heap, permit, args, ctx, call_id)
            })
        };
        self
    }

    /// Override the `host` namespace with a default-constructible type.
    #[must_use]
    pub fn with_host<T: io::IoNamespaceHost + Default + Send + Sync + 'static>(self) -> Self {
        self.with_host_instance(Arc::new(T::default()))
    }

    /// Override the `time` namespace (`Instant.now`) with a pre-built instance.
    #[must_use]
    pub fn with_time_instance(
        mut self,
        instance: Arc<dyn io::IoNamespaceTime + Send + Sync + 'static>,
    ) -> Self {
        self.inner.baml_time_instant_now = {
            let t = instance;
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_time_instant_now(heap, permit, args, ctx, call_id)
            })
        };
        self
    }

    /// Override the `time` namespace with a default-constructible type.
    #[must_use]
    pub fn with_time<T: io::IoNamespaceTime + Default + Send + Sync + 'static>(self) -> Self {
        self.with_time_instance(Arc::new(T::default()))
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
    AsBexExternalValue as _, CallId, FunctionRef, LlmFunctionInfo, SysOpContext, SysOpOutput,
    VmBamlError, VmRustFnError,
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
        use bex_vm_types::errors::{VmBamlError, VmRustFnError};
        use sys_types::SysOpResult;

        let heap = test_heap();
        let ctx = test_ctx();
        let op = SysOps::unsupported(SysOp::BamlSysShell);
        let permit = test_permit().await;
        let result = op(&heap, permit.proof(), vec![], &ctx, CallId::next());
        match result {
            SysOpResult::Ready(Err(e)) => {
                assert!(matches!(
                    e.payload,
                    sys_types::OpErrorPayload::Vm(VmRustFnError::BamlError(
                        VmBamlError::Unsupported { .. }
                    ))
                ));
                assert_eq!(e.fn_name, SysOp::BamlSysShell);
            }
            _ => panic!("Expected Unsupported error"),
        }
    }

    #[tokio::test]
    async fn test_all_unsupported() {
        use bex_vm_types::errors::{VmBamlError, VmRustFnError};
        use sys_types::{OpError, SysOpResult};

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
                payload: sys_types::OpErrorPayload::Vm(VmRustFnError::BamlError(
                    VmBamlError::Unsupported { .. }
                )),
            }))
        ));

        // Test shell returns Unsupported
        let result = (ops.baml_sys_shell)(&heap, permit.proof(), vec![], &ctx, CallId::next());
        assert!(matches!(
            result,
            SysOpResult::Ready(Err(OpError {
                fn_name: SysOp::BamlSysShell,
                payload: sys_types::OpErrorPayload::Vm(VmRustFnError::BamlError(
                    VmBamlError::Unsupported { .. }
                )),
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
            VmRustFnError::from(VmBamlError::InvalidArgument {
                message: "Invalid short hand name: openai".to_string()
            })
        );
    }
}
