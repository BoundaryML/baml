use anyhow::Result;
use async_openai::types::{
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
    ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs,
};
use async_openai::Client;
#[allow(unused_imports)]
use log;

use crate::{LlmClient, Prompt};

pub struct OpenaiClient {
    pub model: String,
}

impl LlmClient for OpenaiClient {
    async fn chat(&self, prompt: Prompt) -> Result<String> {
        let client = Client::new();

        let request = CreateChatCompletionRequestArgs::default()
            .model(self.model.clone())
            .messages(match prompt {
                Prompt::Completion(text) => vec![ChatCompletionRequestMessage::System(
                    ChatCompletionRequestSystemMessageArgs::default()
                        .content(text)
                        .build()?,
                )],
                Prompt::Chat(messages) => messages
                    .into_iter()
                    .map(|message| match message.role.as_str() {
                        "user" => Ok(ChatCompletionRequestMessage::User(
                            ChatCompletionRequestUserMessageArgs::default()
                                .content(message.content)
                                .build()?,
                        )),
                        "system" => Ok(ChatCompletionRequestMessage::System(
                            ChatCompletionRequestSystemMessageArgs::default()
                                .content(message.content)
                                .build()?,
                        )),
                        _ => Err(anyhow::anyhow!(
                            "Unexpected role (OpenAI only supports system and user in roles): {}",
                            message.role
                        )),
                    })
                    .collect::<Result<Vec<ChatCompletionRequestMessage>>>()?,
            })
            .max_tokens(40_u16)
            .build()?;

        let response = client.chat().create(request).await?;

        let Some(response) = response.choices.get(0) else {
            anyhow::bail!(
                "OpenAI response did not contain any choices {:#?}",
                response
            );
        };

        Ok(response
            .message
            .content
            .as_ref()
            .ok_or(anyhow::anyhow!(
                "OpenAI response did not contain any message content {:#?}",
                response
            ))?
            .clone())
    }
}
