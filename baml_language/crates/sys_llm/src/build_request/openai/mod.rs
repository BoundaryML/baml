//! OpenAI-format HTTP request builders.
//!
//! Supports: `OpenAi`, `OpenAiGeneric`, `AzureOpenAi`, Ollama, `OpenRouter`,
//! and `OpenAiResponses` (Responses API).

mod chat_completions;
mod responses;

pub(crate) use chat_completions::OpenAiBuilder;
pub(crate) use responses::OpenAiResponsesBuilder;
