use anyhow::Result;
use clust::{
    messages::{ClaudeModel, Message, MessagesRequestBuilder, SystemPrompt},
    Client,
};
#[allow(unused_imports)]
use log;

use crate::{LlmClient, Prompt};

pub struct AnthropicClient {}

impl LlmClient for AnthropicClient {
    async fn chat(&self, prompt: Prompt) -> Result<String> {
        let client = Client::from_env()?;

        let req =
            MessagesRequestBuilder::new_with_max_tokens(ClaudeModel::Claude3Sonnet20240229, 1024)?;

        let req = match prompt {
            Prompt::Chat(messages) => {
                let (req, messages) = if let Some(first) = messages.get(0) {
                    if first.role == "system" {
                        (
                            req.system(SystemPrompt::new(&first.content)),
                            &messages[1..],
                        )
                    } else {
                        (req, &messages[..])
                    }
                } else {
                    (req, &messages[..])
                };
                let messages = messages.iter().map(
                    |message| match message.role.as_str() {
                        "user" => Ok(Message::user(message.content.clone())),
                        "assistant" => Ok(Message::assistant(message.content.clone())),
                        // TODO: failing fast is not a great UX here, we should let the prompt go through anyways
                        "system" => {
                            Err(anyhow::anyhow!("Unexpected system message (only the first message may be a system in Anthropic): {}", message.content))
                        }
                        _ => {
                            Err(anyhow::anyhow!("Unexpected role (Anthropic only supports system, user, and assistant in roles): {}", message.role))
                        }
                    },
                ).collect::<Result<Vec<Message>>>()?;

                req.messages(messages)
            }
            Prompt::Completion(text) => req.system(SystemPrompt::new(text)),
        };

        let response = client.create_a_message(req.build()).await?;

        Ok(response.content.flatten_into_text()?.to_string())
    }
}
