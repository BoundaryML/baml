use anyhow::Result;

use core::future::Future;
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
    FunctionResult, RuntimeContext,
};

use super::response::LLMResponse;

/// unused
pub type StreamCallback = ();

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
    pub on_event: Option<StreamCallback>,
    pub(crate) ctx: RuntimeContext,
}

#[cfg(feature = "wasm")]
// JsFuture is !Send, so when building for WASM, we have to drop that requirement from StreamCallback
static_assertions::assert_impl_all!(FunctionResultStream: Send);
#[cfg(not(feature = "wasm"))]
static_assertions::assert_impl_all!(FunctionResultStream: Send, Sync);

impl FunctionResultStream {
    pub async fn run<F, O>(&self, on_event: Option<F>) -> Result<FunctionResult>
    where
        F: Fn(FunctionResult) -> O,
        O: Future<Output = Result<()>>,
    {
        use internal_baml_core::ir::IRHelper;

        //let Some(stream) =
        //    std::mem::replace(MutexGuard::deref_mut(&mut self.inner.lock().await), None)
        //else {
        //    anyhow::bail!("Stream is already consumed");
        //};
        match self.provider.as_ref() {
            LLMPrimitiveProvider::OpenAI(c) => {
                let resp = c
                    .build_request_for_stream(&self.ctx, &self.prompt)?
                    .send()
                    .await?;
                self.run_internal(c.response_stream(resp), on_event).await
            }
            LLMPrimitiveProvider::Anthropic(c) => {
                let resp = c
                    .build_request_for_stream(&self.ctx, &self.prompt)?
                    .send()
                    .await?;
                log::info!("is the response coming back at all? if seeing this, yes");
                self.run_internal(c.response_stream(resp), on_event).await
            }
            _ => {
                anyhow::bail!("Only OpenAI supports streaming right now");
            }
        }
    }

    async fn run_internal<F, O>(
        &self,
        response_stream: impl Stream<Item = Result<LLMCompleteResponse>>,
        on_event: Option<F>,
    ) -> Result<FunctionResult>
    where
        F: Fn(FunctionResult) -> O,
        O: Future<Output = Result<()>>,
    {
        let final_response = response_stream
            .inspect(|event| log::debug!("Received event: {:#?}", event))
            .then(|fn_result| async {
                let response = fn_result?;

                let func = self.ir.find_function(self.function_name.as_str())?;
                let parsed = jsonish::from_str(
                    &*self.ir,
                    &self.ctx.env,
                    func.output(),
                    response.content.as_str(),
                    true,
                );

                if let Some(ref on_event) = on_event {
                    if let Ok(parsed) = parsed {
                        return match on_event(FunctionResult::new(
                            self.scope.clone(),
                            LLMResponse::Success(response.clone()),
                            Some(Ok(parsed)),
                        ))
                        .await
                        {
                            Ok(()) => Ok(response),
                            Err(e) => Err(e.context("Error in on_event callback")),
                        };
                    }
                }

                Ok(response)
            })
            .into_stream()
            .fold(None, |_, event| async { Some(event) })
            .await
            .ok_or(anyhow::anyhow!("Stream ended before receiving responses"))?
            .map_err(|e| e.context("Error while processing stream"))?;

        let func = self.ir.find_function(self.function_name.as_str())?;
        let final_parsed = jsonish::from_str(
            &*self.ir,
            &self.ctx.env,
            func.output(),
            final_response.content.as_str(),
            false,
        );
        Ok(FunctionResult::new(
            self.scope.clone(),
            LLMResponse::Success(final_response.clone()),
            Some(final_parsed),
        ))
    }
}
