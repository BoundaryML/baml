use internal_baml_client_llm::{AnthropicClient, LlmClient, Message, OpenaiClient, Prompt};

use crate::errors::CliError;
use tokio::runtime::Runtime;

#[derive(clap::Args, Debug)]
pub struct CallArgs {}

pub fn run() -> Result<(), CliError> {
    let rt = Runtime::new()?;

    let prompt = Prompt::Chat(vec![
        Message {
            role: "system".to_string(),
            content: "You love talking about cats. Respond with a 10-word description of a cat if one is provided, else return [done]".to_string(),
        },
        Message {
            role: "user".to_string(),
            content: "doberman".to_string(),
        },
    ]);

    log::info!("Prompt: {:#?}", prompt);

    rt.block_on(async {
        let openai = OpenaiClient {
            model: "gpt-3.5-turbo".to_string(),
        };

        match openai.chat(prompt.clone()).await {
            Ok(m) => log::info!("OpenAI response:\n---\n{}\n---", m),
            Err(e) => log::error!("OpenAI error: {}", e),
        }
    });
    rt.block_on(async {
        let anthropic = AnthropicClient {};
        match anthropic.chat(prompt.clone()).await {
            Ok(m) => log::info!("Anthropic response:\n---\n{}\n---", m),
            Err(e) => log::error!("Anthropic error: {}", e),
        }
    });

    Ok(())
}
