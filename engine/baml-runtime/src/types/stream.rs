use anyhow::Result;

use futures::{stream::StreamExt, Stream};
use internal_baml_core::ir::repr::IntermediateRepr;
use internal_baml_core::ir::IRHelper;
use internal_baml_jinja::Type;

use std::{rc, sync::Arc};

use crate::{
    client_builder::ClientBuilder,
    internal::{
        llm_client::{
            orchestrator::{
                self, orchestrate_stream, LLMPrimitiveProvider, OrchestrationScope,
                OrchestratorNodeIterator,
            },
            ErrorCode, LLMErrorResponse,
        },
        prompt_renderer::PromptRenderer,
    },
    tracing::BamlTracer,
    type_builder::TypeBuilder,
    FunctionResult, RuntimeContext, RuntimeContextManager,
};

use super::response::LLMResponse;

/// Wrapper that holds a stream of responses from a BAML function call.
///
/// Needs to hold a reference to the IR so that it can parse each response from the LLM.
/// We decouple its lifetime from that of BamlRuntime because we want to make it easy for
/// users to cancel the stream.
pub struct FunctionResultStream {
    pub(crate) function_name: String,
    pub(crate) params: crate::BamlMap<String, crate::BamlValue>,
    pub(crate) renderer: PromptRenderer,
    pub(crate) ir: Arc<IntermediateRepr>,
    pub(crate) orchestrator: OrchestratorNodeIterator,
    pub(crate) tracer: Arc<BamlTracer>,
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
    pub async fn run<F>(
        &mut self,
        on_event: Option<F>,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        cb: Option<&ClientBuilder>,
    ) -> (Result<FunctionResult>, Option<uuid::Uuid>)
    where
        F: Fn(FunctionResult) -> (),
    {
        let mut local_orchestrator = Vec::new();
        std::mem::swap(&mut local_orchestrator, &mut self.orchestrator);

        let mut local_params = crate::BamlMap::new();
        std::mem::swap(&mut local_params, &mut self.params);

        let span = self
            .tracer
            .start_span(&self.function_name, ctx, &local_params);

        let rctx = ctx.create_ctx(tb, cb);
        let res = match rctx {
            Ok(rctx) => {
                let (history, _) = orchestrate_stream(
                    local_orchestrator,
                    self.ir.as_ref(),
                    &rctx,
                    &self.renderer,
                    &baml_types::BamlValue::Map(local_params),
                    |content| self.renderer.parse(content, true),
                    |content| self.renderer.parse(content, false),
                    on_event,
                )
                .await;

                FunctionResult::new_chain(history)
            }
            Err(e) => Err(e),
        };

        let mut target_id = None;
        if let Some(span) = span {
            #[cfg(not(target_arch = "wasm32"))]
            match self.tracer.finish_baml_span(span, ctx, &res) {
                Ok(id) => target_id = id,
                Err(e) => log::debug!("Error during logging: {}", e),
            }
            #[cfg(target_arch = "wasm32")]
            match self.tracer.finish_baml_span(span, ctx, &res).await {
                Ok(id) => target_id = id,
                Err(e) => log::debug!("Error during logging: {}", e),
            }
        };

        (res, target_id)
    }
}
