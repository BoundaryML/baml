use anyhow::Result;

use futures::stream::{StreamExt, TryStreamExt};
use internal_baml_core::ir::repr::IntermediateRepr;
use internal_baml_jinja::RenderedChatMessage;
use std::ops::DerefMut;
use std::sync::Arc;
use tokio::sync::{Mutex, MutexGuard};

use crate::{internal::llm_client::SseResponse, FunctionResult, RuntimeContext};

use super::response::LLMResponse;

#[cfg(feature = "wasm")]
// JsFuture is !Send, so when building for WASM, we have to drop that requirement from StreamCallback
pub type StreamCallback = Box<dyn Fn(FunctionResult) -> Result<()> + Send>;
#[cfg(not(feature = "wasm"))]
pub type StreamCallback = Box<dyn Fn(FunctionResult) -> Result<()> + Send + Sync>;

/// Wrapper that holds a stream of responses from a BAML function call.
///
/// Needs to hold a reference to the IR so that it can parse each response from the LLM.
/// We decouple its lifetime from that of BamlRuntime because we want to make it easy for
/// users to cancel the stream.
pub struct FunctionResultStream {
    function_name: String,
    inner: Arc<Mutex<Option<SseResponse>>>,
    ir: Arc<IntermediateRepr>,
    pub on_event: Option<StreamCallback>,
    ctx: RuntimeContext,
}

#[cfg(feature = "wasm")]
// JsFuture is !Send, so when building for WASM, we have to drop that requirement from StreamCallback
static_assertions::assert_impl_all!(FunctionResultStream: Send);
#[cfg(not(feature = "wasm"))]
static_assertions::assert_impl_all!(FunctionResultStream: Send, Sync);

impl FunctionResultStream {
    pub fn from(
        function_name: String,
        inner: SseResponse,
        ir: Arc<IntermediateRepr>,
        ctx: RuntimeContext,
    ) -> Result<Self> {
        Ok(Self {
            function_name,
            inner: Arc::new(Mutex::new(Some(inner))),
            ir: ir,
            on_event: None,
            ctx,
        })
    }

    pub async fn run(&self, on_event: Option<StreamCallback>) -> Result<FunctionResult> {
        use internal_baml_core::ir::IRHelper;

        let Some(stream) =
            std::mem::replace(MutexGuard::deref_mut(&mut self.inner.lock().await), None)
        else {
            anyhow::bail!("Stream is already consumed");
        };

        let final_response = stream
            .stream()
            .await?
            .inspect(|event| log::debug!("Received event: {:#?}", event))
            .then(|fn_result| async {
                let response = fn_result?;

                let func = self.ir.find_function(self.function_name.as_str())?;
                // TODO: partial-ify func.output
                let parsed = jsonish::from_str(
                    &*self.ir,
                    &self.ctx.env,
                    func.output(),
                    response.content.as_str(),
                );

                if let Some(ref on_event) = on_event {
                    if let Ok(parsed) = parsed {
                        return match on_event(FunctionResult {
                            llm_response: LLMResponse::Success(response.clone()),
                            parsed: Some(Ok(parsed)),
                        }) {
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
        );
        Ok(FunctionResult {
            llm_response: LLMResponse::Success(final_response),
            parsed: Some(final_parsed),
        })
    }
}
