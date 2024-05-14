use anyhow::Result;
use internal_baml_jinja::CompletionOptions;

use crate::{
    internal::llm_client::{LLMResponse, LLMResponseStream},
    RuntimeContext,
};

type ResponseType = Result<LLMResponse>;

pub trait WithCompletion: Sync + Send {
    fn completion_options(&self, ctx: &RuntimeContext) -> Result<CompletionOptions>;

    #[allow(async_fn_in_trait)]
    async fn completion(&self, ctx: &RuntimeContext, prompt: &String) -> ResponseType;
}

pub trait WithStreamCompletion: Sync + Send {
    #[allow(async_fn_in_trait)]
    async fn stream_completion(&self, ctx: &RuntimeContext, prompt: &String) -> LLMResponseStream;
}

// pub trait WithCompletionStream: WithCompletion {
//     fn stream_completion(
//         &mut self,
//         ctx: &RuntimeContext,
//         prompt: &String,
//     ) -> impl Stream<Item = Result<LLMStreamResponse>> {
//         stream! {
//             let response = self.completion(ctx, prompt).await?;
//             yield Ok(LLMStreamResponse {
//                 delta: response.content,
//                 start_time_unix_ms: response.start_time_unix_ms,
//                 latency_ms: response.latency_ms,
//                 metadata: response.metadata,
//             });
//         }
//     }
// }

pub trait WithNoCompletion {}

impl<T> WithCompletion for T
where
    T: WithNoCompletion + Send + Sync,
{
    fn completion_options(&self, _ctx: &RuntimeContext) -> Result<CompletionOptions> {
        anyhow::bail!("Completion prompts are not supported by this provider")
    }

    // #[cfg(not(feature = "no_wasm"))]
    // #[allow(async_fn_in_trait)]
    // async fn completion(&self, _: &RuntimeContext, _: &String) -> ResponseType {
    //     Err(wasm_bindgen::JsValue::from_str(
    //         "Completion prompts are not supported by this provider",
    //     ))
    // }

    // #[cfg(feature = "no_wasm")]
    #[allow(async_fn_in_trait)]
    async fn completion(&self, _: &RuntimeContext, _: &String) -> ResponseType {
        anyhow::bail!("Completion prompts are not supported by this provider")
    }
}
