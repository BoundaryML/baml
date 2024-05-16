use anyhow::{Context, Result};
use baml_types::BamlValue;

use futures::{
    stream::{BoxStream, StreamExt, TryStreamExt},
    Stream,
};
use internal_baml_core::ir::repr::IntermediateRepr;
use internal_baml_jinja::RenderedChatMessage;
use std::sync::Arc;
use std::{collections::HashMap, ops::DerefMut};
use stream_cancel::{StreamExt as CancellableStreamExt, TakeUntilIf, Trigger, Tripwire};
use tokio::sync::Mutex;

use crate::{internal::llm_client::SseResponse, FunctionResult, RuntimeContext};

use super::response::LLMResponse;

#[cfg(feature = "wasm")]
// JsFuture is !Send, so when building for WASM, we have to drop that requirement from StreamCallback
pub type StreamCallback = Box<dyn Fn(FunctionResult) -> Result<()> + Send>;
#[cfg(not(feature = "wasm"))]
pub type StreamCallback = Box<dyn Fn(FunctionResult) -> Result<()> + Send + Sync>;

/// Wraps a stream_cancel::Trigger with an idempotent cancel.
#[derive(Clone)]
pub struct CancelStreamTrigger {
    trigger: Arc<Mutex<Option<Trigger>>>,
}

static_assertions::assert_impl_all!(CancelStreamTrigger: Send, Sync);

impl CancelStreamTrigger {
    pub async fn cancel(&self) {
        let mut locked_trigger = self.trigger.lock().await;
        let owned_trigger = core::mem::replace(locked_trigger.deref_mut(), None);
        match owned_trigger {
            Some(trigger) => trigger.cancel(),
            None => {
                log::warn!("Failed to cancel stream: trigger is None (was it already cancelled?)")
            }
        }
    }
}

/// Wrapper that holds a stream of responses from a BAML function call.
///
/// Needs to hold a reference to the IR so that it can parse each response from the LLM.
/// We decouple its lifetime from that of BamlRuntime because we want to make it easy for
/// users to cancel the stream.
pub struct FunctionResultStream {
    function_name: String,
    inner: SseResponse,
    ir: Arc<IntermediateRepr>,
    pub on_event: Option<StreamCallback>,
    tripwire: Tripwire,
    cancelme: CancelStreamTrigger,
    ctx: RuntimeContext,
}

static_assertions::assert_impl_all!(FunctionResultStream: Send);

impl FunctionResultStream {
    pub fn from(
        function_name: String,
        inner: SseResponse,
        ir: Arc<IntermediateRepr>,
        ctx: RuntimeContext,
    ) -> Result<Self> {
        let (trigger, tripwire) = Tripwire::new();
        Ok(Self {
            function_name,
            inner: inner,
            ir: ir,
            on_event: None,
            tripwire,
            cancelme: CancelStreamTrigger {
                trigger: Arc::new(Mutex::new(Some(trigger))),
            },
            ctx,
        })
    }

    pub async fn run(self) -> Result<FunctionResult> {
        use internal_baml_core::ir::IRHelper;
        let final_response = self
            .inner
            .stream()
            .await?
            .inspect(|event| log::debug!("Received event: {:#?}", event))
            .take_until_if(self.tripwire)
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

                if let Some(ref on_event) = self.on_event {
                    if let Ok(parsed) = parsed {
                        return match on_event(FunctionResult {
                            llm_response: LLMResponse::Success(response.clone()),
                            parsed: Some(Ok(parsed)),
                        }) {
                            Ok(()) => Ok(response),
                            Err(e) => {
                                log::debug!("User-provided on_event errored: {:?}", e);
                                Err(e)
                            }
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

    pub async fn get_cancel_trigger(&self) -> CancelStreamTrigger {
        self.cancelme.clone()
    }
}
