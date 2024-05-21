use anyhow::Result;

use futures::{
    stream::{StreamExt, TryStreamExt},
    Stream,
};
use internal_baml_core::ir::repr::IntermediateRepr;
use internal_baml_core::ir::IRHelper;

use std::sync::Arc;

use crate::{
    internal::llm_client::{
        orchestrator::{LLMPrimitiveProvider, OrchestrationScope},
        LLMCompleteResponse, SseResponseTrait,
    },
    tracing::BamlTracer,
    FunctionResult, RuntimeContext,
};

use super::response::{self, LLMResponse};

/// Wrapper that holds a stream of responses from a BAML function call.
///
/// Needs to hold a reference to the IR so that it can parse each response from the LLM.
/// We decouple its lifetime from that of BamlRuntime because we want to make it easy for
/// users to cancel the stream.
pub struct FunctionResultStream {
    pub(crate) provider: Arc<LLMPrimitiveProvider>,
    pub(crate) prompt: internal_baml_jinja::RenderedPrompt,
    pub(crate) function_name: String,
    pub(crate) scope: OrchestrationScope,
    //pub(crate) inner: Arc<Mutex<Option<SseResponse>>>,
    pub(crate) ir: Arc<IntermediateRepr>,
    pub(crate) ctx: RuntimeContext,
    pub(crate) tracer: Arc<BamlTracer>,
}

#[cfg(feature = "wasm")]
// JsFuture is !Send, so when building for WASM, we have to drop that requirement from StreamCallback
static_assertions::assert_impl_all!(FunctionResultStream: Send);
#[cfg(not(feature = "wasm"))]
static_assertions::assert_impl_all!(FunctionResultStream: Send, Sync);

impl FunctionResultStream {
    pub async fn run<F>(
        &mut self,
        on_event: Option<F>,
    ) -> (Result<FunctionResult>, Option<uuid::Uuid>)
    where
        F: Fn(FunctionResult) -> (),
    {
        let (span, ctx) =
            self.tracer
                .start_span(&self.function_name, self.ctx.clone(), &Default::default());
        let res = self.run_impl(on_event, ctx).await;

        let mut target_id = None;
        if let Some(span) = span {
            match self.tracer.finish_baml_span(span, &res).await {
                Ok(id) => target_id = id,
                Err(e) => log::debug!("Error during logging: {}", e),
            }
        };

        (res, target_id)
    }

    async fn run_impl<F>(
        &mut self,
        on_event: Option<F>,
        ctx: RuntimeContext,
    ) -> Result<FunctionResult>
    where
        F: Fn(FunctionResult) -> (),
    {
        match self.provider.as_ref() {
            LLMPrimitiveProvider::OpenAI(c) => {
                let req = c.build_request_for_stream(&ctx, &self.prompt)?;
                let (system_start, instant_start) =
                    (web_time::SystemTime::now(), web_time::Instant::now());
                let resp = req.send().await?;
                self.run_internal(
                    c.response_stream(resp, system_start, instant_start),
                    on_event,
                )
                .await
            }
            LLMPrimitiveProvider::Anthropic(c) => {
                let req = c.build_request_for_stream(&ctx, &self.prompt)?;
                let (system_start, instant_start) =
                    (web_time::SystemTime::now(), web_time::Instant::now());
                let resp = req.send().await?;
                self.run_internal(
                    c.response_stream(resp, system_start, instant_start),
                    on_event,
                )
                .await
            }
        }
    }

    async fn run_internal<F>(
        &self,
        response_stream: impl Stream<Item = Result<LLMCompleteResponse>>,
        on_event: Option<F>,
    ) -> Result<FunctionResult>
    where
        F: Fn(FunctionResult) -> (),
    {
        let final_response = response_stream
            .inspect(|event| log::debug!("Received event: {:#?}", event))
            .map(|stream_part| match stream_part {
                Ok(response) => {
                    let func = self.ir.find_function(self.function_name.as_str()).unwrap();
                    let parsed = jsonish::from_str(
                        self.ir.as_ref(),
                        &self.ctx.env,
                        func.output(),
                        response.content.as_str(),
                        true,
                    );

                    if let Some(on_event) = on_event.as_ref() {
                        on_event(FunctionResult::new(
                            self.scope.clone(),
                            LLMResponse::Success(response.clone()),
                            Some(parsed),
                        ));
                    }
                    Ok(response)
                }
                Err(e) => Err(e),
            })
            .fold(None, |_, current| async { Some(current) })
            .await
            .ok_or(anyhow::anyhow!("Stream ended before receiving responses"))?;

        match final_response {
            Ok(response) => {
                let func = self.ir.find_function(self.function_name.as_str())?;
                let final_parsed = jsonish::from_str(
                    &*self.ir,
                    &self.ctx.env,
                    func.output(),
                    response.content.as_str(),
                    false,
                );
                Ok(FunctionResult::new(
                    self.scope.clone(),
                    LLMResponse::Success(response),
                    Some(final_parsed),
                ))
            }
            Err(e) => Ok(FunctionResult::new(
                self.scope.clone(),
                LLMResponse::OtherFailure(e.to_string()),
                None,
            )),
        }
    }
}
