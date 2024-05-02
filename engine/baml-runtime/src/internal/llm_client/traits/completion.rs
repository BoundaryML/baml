use anyhow::Result;
use internal_baml_jinja::CompletionOptions;

use crate::{internal::llm_client::LLMResponse, RuntimeContext};

pub trait WithCompletion: Sync + Send {
    fn completion_options(&self, ctx: &RuntimeContext) -> Result<CompletionOptions>;

    async fn completion(&self, ctx: &RuntimeContext, prompt: &String) -> Result<LLMResponse>;
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

    async fn completion(&self, _: &RuntimeContext, _: &String) -> Result<LLMResponse> {
        anyhow::bail!("Completion prompts are not supported by this provider")
    }
}
