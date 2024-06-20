use baml_runtime::{
    internal::llm_client::orchestrator::OrchestrationScope, ChatMessagePart, RenderedPrompt,
};

use baml_types::BamlMedia;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(getter_with_clone)]
pub struct WasmPrompt {
    prompt: RenderedPrompt,
    pub client_name: String,
}

impl From<(&RenderedPrompt, &OrchestrationScope)> for WasmPrompt {
    fn from((prompt, client_name): (&RenderedPrompt, &OrchestrationScope)) -> Self {
        WasmPrompt {
            prompt: prompt.clone(),
            client_name: client_name.name(),
        }
    }
}

#[wasm_bindgen(getter_with_clone)]
pub struct WasmChatMessage {
    #[wasm_bindgen(readonly)]
    pub role: String,
    #[wasm_bindgen(readonly)]
    pub parts: Vec<WasmChatMessagePart>,
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct WasmChatMessagePart {
    part: ChatMessagePart,
}

impl From<ChatMessagePart> for WasmChatMessagePart {
    fn from(part: ChatMessagePart) -> Self {
        WasmChatMessagePart { part }
    }
}

#[wasm_bindgen]
impl WasmChatMessagePart {
    #[wasm_bindgen]
    pub fn is_text(&self) -> bool {
        matches!(self.part, ChatMessagePart::Text(_))
    }

    #[wasm_bindgen]
    pub fn is_image(&self) -> bool {
        matches!(self.part, ChatMessagePart::Image(_))
    }

    #[wasm_bindgen]
    pub fn as_text(&self) -> Option<String> {
        if let ChatMessagePart::Text(s) = &self.part {
            Some(s.clone())
        } else {
            None
        }
    }

    #[wasm_bindgen]
    pub fn as_image(&self) -> Option<String> {
        if let ChatMessagePart::Image(s) = &self.part {
            Some(match s {
                BamlMedia::Url(u) => u.url.clone(),
                BamlMedia::Base64(b) => b.base64.clone(),
            })
        } else {
            None
        }
    }
}

#[wasm_bindgen]
impl WasmPrompt {
    #[wasm_bindgen]
    pub fn is_chat(&self) -> bool {
        matches!(self.prompt, RenderedPrompt::Chat(_))
    }

    #[wasm_bindgen]
    pub fn is_completion(&self) -> bool {
        matches!(self.prompt, RenderedPrompt::Completion(_))
    }

    #[wasm_bindgen]
    pub fn as_chat(&self) -> Option<Vec<WasmChatMessage>> {
        if let RenderedPrompt::Chat(s) = &self.prompt {
            Some(
                s.iter()
                    .map(|m| WasmChatMessage {
                        role: m.role.clone(),
                        parts: m.parts.iter().map(|p| p.clone().into()).collect(),
                    })
                    .collect(),
            )
        } else {
            None
        }
    }
}
