use anyhow::Result;
use async_stream::stream;
use async_trait::async_trait;
use futures::Stream;
use internal_baml_jinja::CompletionOptions;

use crate::{
    runtime::llm_client::{LLMResponse, LLMStreamResponse},
    RuntimeContext,
};

#[async_trait]
pub trait WithCompletion: Sync + Send {
    fn completion_options(&mut self, ctx: &RuntimeContext) -> Result<CompletionOptions>;

    async fn completion(&mut self, ctx: &RuntimeContext, prompt: &String) -> Result<LLMResponse>;
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

#[async_trait]
impl<T> WithCompletion for T
where
    T: WithNoCompletion + Send + Sync,
{
    fn completion_options(&mut self, ctx: &RuntimeContext) -> Result<CompletionOptions> {
        anyhow::bail!("Completion prompts are not supported by this provider")
    }

    async fn completion(&mut self, _: &RuntimeContext, _: &String) -> Result<LLMResponse> {
        anyhow::bail!("Completion prompts are not supported by this provider")
    }
}
