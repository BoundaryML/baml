use anyhow::Result;
use internal_baml_jinja::{ChatOptions, RenderedChatMessage};

use crate::{internal::llm_client::LLMResponse, RuntimeContext};

pub trait WithChat: Sync + Send {
    fn chat_options(&mut self, ctx: &RuntimeContext) -> Result<ChatOptions>;

    async fn chat(
        &mut self,
        ctx: &RuntimeContext,
        prompt: &Vec<RenderedChatMessage>,
    ) -> Result<LLMResponse>;
}

// pub trait WithChatStream: WithChat {
//     fn stream_chat(
//         &mut self,
//         ctx: &RuntimeContext,
//         prompt: &Vec<RenderedChatMessage>,
//     ) -> impl Stream<Item = Result<LLMStreamResponse>> {
//         stream! {
//             let response = self.chat(ctx, prompt).await?;
//             yield Ok(LLMStreamResponse {
//                 delta: response.content(),
//                 start_time_unix_ms: response.start_time_unix_ms,
//                 latency_ms: response.latency_ms,
//                 metadata: response.metadata,
//             });
//         }
//     }
// }

pub trait WithNoChat {}

impl<T> WithChat for T
where
    T: WithNoChat + Send + Sync,
{
    fn chat_options(&mut self, ctx: &RuntimeContext) -> Result<ChatOptions> {
        anyhow::bail!("Chat prompts are not supported by this provider")
    }

    async fn chat(
        &mut self,
        _: &RuntimeContext,
        _: &Vec<RenderedChatMessage>,
    ) -> Result<LLMResponse> {
        anyhow::bail!("Chat prompts are not supported by this provider")
    }
}
