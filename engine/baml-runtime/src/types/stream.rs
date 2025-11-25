use std::{collections::HashMap, sync::Arc};

use anyhow::Result;
use baml_types::{
    ir_type::TypeNonStreaming,
    tracing::events::{FunctionEnd, FunctionStart, TraceData, TraceEvent},
    BamlValueWithMeta, TypeIR,
};
use internal_baml_core::ir::repr::IntermediateRepr;
use serde_json::json;
use stream_cancel::Tripwire;

use crate::{
    client_registry::ClientRegistry,
    internal::{
        llm_client::orchestrator::{orchestrate_stream, OrchestratorNodeIterator},
        prompt_renderer::PromptRenderer,
    },
    tracing::BamlTracer,
    tracingv2::storage::storage::{Collector, BAML_TRACER},
    type_builder::TypeBuilder,
    FunctionResult, IntoBamlError, PreparedFunctionArgs, RuntimeContextManager, TripWire,
};

/// Wrapper that holds a stream of responses from a BAML function call.
///
/// Needs to hold a reference to the IR so that it can parse each response from the LLM.
/// We decouple its lifetime from that of BamlRuntime because we want to make it easy for
/// users to cancel the stream.
pub struct FunctionResultStream {
    pub(crate) function_name: String,
    pub(crate) prepared_func: PreparedFunctionArgs,
    pub(crate) renderer: PromptRenderer,
    pub(crate) ir: Arc<IntermediateRepr>,
    pub(crate) orchestrator: OrchestratorNodeIterator,
    pub(crate) tracer: Arc<BamlTracer>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) tokio_runtime: Arc<tokio::runtime::Runtime>,
    pub(crate) collectors: Vec<Arc<Collector>>,
    pub(crate) tags: Option<HashMap<String, String>>,
    pub(crate) cancel_tripwire: Arc<TripWire>,
}

#[cfg(target_arch = "wasm32")]
// JsFuture is !Send, so when building for WASM, we have to drop that requirement from StreamCallback
static_assertions::assert_impl_all!(FunctionResultStream: Send);
#[cfg(not(target_arch = "wasm32"))]
static_assertions::assert_impl_all!(FunctionResultStream: Send, Sync);

/*
let func = self.get_function(&function_name, &ctx)?;
let baml_args = self.ir().check_function_params(&func, &params)?;

let renderer = PromptRenderer::from_function(&func)?;
let client_name = renderer.client_name().to_string();

let orchestrator = self.orchestration_graph(&client_name, &ctx)?;
let first = orchestrator.first().ok_or(anyhow::anyhow!(
    "No orchestrator nodes found for client {}",
    client_name
))?;
first.provider.clone();
first.provider.render_prompt(&renderer, &ctx, &baml_args)?;
first.scope.clone();
*/

impl FunctionResultStream {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn run_sync<F, G>(
        &mut self,
        on_tick: Option<G>,
        on_event: Option<F>,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        cb: Option<&ClientRegistry>,
        env_vars: HashMap<String, String>,
    ) -> (Result<FunctionResult>, baml_ids::FunctionCallId)
    where
        F: Fn(FunctionResult),
        G: Fn(),
    {
        let rt = self.tokio_runtime.clone();
        let fut = self.run(on_tick, on_event, ctx, tb, cb, env_vars);
        rt.block_on(fut)
    }

    pub async fn run<F, G>(
        &mut self,
        on_tick: Option<G>,
        on_event: Option<F>,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        cb: Option<&ClientRegistry>,
        env_vars: HashMap<String, String>,
    ) -> (Result<FunctionResult>, baml_ids::FunctionCallId)
    where
        F: Fn(FunctionResult),
        G: Fn(),
    {
        let mut local_orchestrator = Vec::new();
        std::mem::swap(&mut local_orchestrator, &mut self.orchestrator);

        // let mut local_params = crate::BamlMap::new();
        // std::mem::swap(&mut local_params, &mut self.params);

        let call = self.tracer.start_call(
            &self.function_name,
            ctx,
            &self.prepared_func.value,
            true,
            true,
            (!self.collectors.is_empty()).then(|| self.collectors.clone()),
            self.tags.as_ref(),
        );
        let rctx = ctx.create_ctx(tb, cb, env_vars, call.new_call_id_stack.clone());
        let res = match rctx {
            Ok(rctx) => {
                async {
                    let (history, _) = orchestrate_stream(
                        local_orchestrator,
                        self.ir.as_ref(),
                        &rctx,
                        &self.renderer,
                        &baml_types::BamlValue::Map(self.prepared_func.value.clone()),
                        on_tick,
                        |content| self.renderer.parse(self.ir.as_ref(), &rctx, content, true),
                        |content| self.renderer.parse(self.ir.as_ref(), &rctx, content, false),
                        on_event,
                        self.cancel_tripwire.trip_wire(),
                    )
                    .await;

                    FunctionResult::new_chain(history)
                }
                .await
            }
            Err(e) => Err(e),
        };

        let curr_call_id = call.curr_call_id();
        let call_stack = call.new_call_id_stack.clone();
        let function_type = call.function_type.clone();
        #[cfg(not(target_arch = "wasm32"))]
        match self.tracer.finish_baml_call(call, ctx, &res) {
            Ok(id) => {}
            Err(e) => log::debug!("Error during logging: {e}"),
        }
        #[cfg(target_arch = "wasm32")]
        match self.tracer.finish_baml_call(call, ctx, &res).await {
            Ok(id) => {}
            Err(e) => log::debug!("Error during logging: {e}"),
        }

        let trace_event = TraceEvent::new_function_end(
            call_stack,
            match &res {
                Ok(result) => match result.result_with_constraints_content() {
                    Ok(value) => Ok(value
                        .0
                        .map_meta(|f| f.3.to_non_streaming_type(self.ir.as_ref()))),
                    Err(e) => Err((&e).to_baml_error()),
                },
                Err(e) => Err(e.to_baml_error()),
            },
            function_type,
        );
        BAML_TRACER.lock().unwrap().put(Arc::new(trace_event));

        (res, curr_call_id)
    }
}
