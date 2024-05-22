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
        ErrorCode, LLMCompleteResponse, LLMErrorResponse, SseResponseTrait,
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
        let (span, _ctx) =
            self.tracer
                .start_span(&self.function_name, self.ctx.clone(), &Default::default());
        let res = match self.provider.as_ref() {
            LLMPrimitiveProvider::OpenAI(c) => self.run_impl(c, on_event).await,
            LLMPrimitiveProvider::Anthropic(c) => self.run_impl(c, on_event).await,
        };

        let mut target_id = None;
        if let Some(span) = span {
            match self.tracer.finish_baml_span(span, &res).await {
                Ok(id) => target_id = id,
                Err(e) => log::debug!("Error during logging: {}", e),
            }
        };

        (res, target_id)
    }

    async fn run_impl<C: SseResponseTrait, F: Fn(FunctionResult) -> ()>(
        &self,
        c: &C,
        on_event: Option<F>,
    ) -> Result<FunctionResult> {
        let Ok(req) = c.build_request_for_stream(&self.prompt) else {
            return Ok(FunctionResult::new(
                self.scope.clone(),
                LLMResponse::LLMFailure(LLMErrorResponse {
                    client: self.provider.name().into(),
                    model: None,
                    prompt: self.prompt.clone(),
                    start_time: web_time::SystemTime::now(),
                    latency: web_time::Duration::ZERO,
                    message: "Failed to build stream request".into(),
                    code: ErrorCode::Other(4),
                }),
                None,
            ));
        };
        let (system_start, instant_start) = (web_time::SystemTime::now(), web_time::Instant::now());
        match req.send().await {
            Ok(resp) => {
                let status = resp.status();
                if status == 200 {
                    self.run_stream(
                        c.response_stream(resp, &self.prompt, system_start, instant_start),
                        on_event,
                    )
                    .await
                } else {
                    Ok(FunctionResult::new(
                        self.scope.clone(),
                        LLMResponse::LLMFailure(LLMErrorResponse {
                            client: self.provider.name().into(),
                            model: None,
                            prompt: self.prompt.clone(),
                            start_time: system_start,
                            latency: instant_start.elapsed(),
                            message: resp
                                .text()
                                .await
                                .unwrap_or("Failed to decode response".into()),
                            code: ErrorCode::from_status(status),
                        }),
                        None,
                    ))
                }
            }
            Err(e) => Ok(FunctionResult::new(
                self.scope.clone(),
                LLMResponse::LLMFailure(LLMErrorResponse {
                    client: self.provider.name().into(),
                    model: None,
                    prompt: self.prompt.clone(),
                    start_time: system_start,
                    latency: instant_start.elapsed(),
                    message: e.to_string(),
                    code: e
                        .status()
                        .map(ErrorCode::from_status)
                        .unwrap_or(ErrorCode::Other(3)),
                }),
                None,
            )),
        }
    }

    async fn run_stream<F>(
        &self,
        response_stream: impl Stream<Item = Result<LLMResponse>>,
        on_event: Option<F>,
    ) -> Result<FunctionResult>
    where
        F: Fn(FunctionResult) -> (),
    {
        let final_response = response_stream
            .inspect(|event| log::debug!("Received event: {:#?}", event))
            .map(|stream_part| match stream_part {
                Ok(LLMResponse::Success(response)) => {
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
                    Ok(LLMResponse::Success(response))
                }
                Ok(other) => Ok(other),
                Err(e) => Err(e),
            })
            .fold(None, |_, current| async { Some(current) })
            .await
            .ok_or_else(|| {
                log::info!("in the ok-or-else stream ended no responses");
                anyhow::anyhow!("Stream ended before receiving responses")
            })?;

        match final_response {
            Ok(LLMResponse::Success(response)) => {
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
            Ok(other) => Ok(FunctionResult::new(self.scope.clone(), other, None)),
            Err(e) => Ok(FunctionResult::new(
                self.scope.clone(),
                LLMResponse::OtherFailure(e.to_string()),
                None,
            )),
        }
    }
}
