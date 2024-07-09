use baml_runtime::{
    internal::llm_client::orchestrator::{ExecutionScope, OrchestrationScope},
    ChatMessagePart, RenderedPrompt,
};

use baml_types::{BamlMedia, BamlMediaType, MediaBase64};
use serde_wasm_bindgen::to_value;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(getter_with_clone)]
pub struct WasmScope {
    scope: OrchestrationScope,
}

#[wasm_bindgen(getter_with_clone)]
pub struct WasmPrompt {
    prompt: RenderedPrompt,
    pub client_name: String,
}

impl From<OrchestrationScope> for WasmScope {
    fn from(scope: OrchestrationScope) -> Self {
        WasmScope { scope }
    }
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
    pub fn is_audio(&self) -> bool {
        matches!(self.part, ChatMessagePart::Audio(_))
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
                BamlMedia::Url(BamlMediaType::Image, u) => u.url.clone(),
                BamlMedia::Base64(BamlMediaType::Image, b) => b.base64.clone(),
                _ => return None, // This will match any other case and return None
            })
        } else {
            None
        }
    }

    #[wasm_bindgen]
    pub fn as_audio(&self) -> Option<String> {
        if let ChatMessagePart::Audio(s) = &self.part {
            Some(match s {
                BamlMedia::Url(BamlMediaType::Audio, u) => u.url.clone(),
                BamlMedia::Base64(_, MediaBase64 { base64, media_type }) => {
                    format!("data:{};base64,{}", media_type, base64.clone())
                }
                _ => return None, // This will match any other case and return None
            })
        } else {
            None
        }
    }
}

#[wasm_bindgen]
impl WasmScope {
    #[wasm_bindgen]
    pub fn name(&self) -> String {
        self.scope.name()
    }

    #[wasm_bindgen]
    pub fn get_orchestration_scope_info(&self) -> JsValue {
        self.scope.to_js_value()
    }

    #[wasm_bindgen]
    pub fn iter_scopes(&self) -> ScopeIterator {
        ScopeIterator {
            scopes: self.scope.scope.clone(),
            index: 0,
        }
    }
}

#[wasm_bindgen]
pub struct ScopeIterator {
    scopes: Vec<ExecutionScope>,
    index: usize,
}

#[wasm_bindgen]
impl ScopeIterator {
    #[wasm_bindgen]
    pub fn next(&mut self) -> JsValue {
        if self.index < self.scopes.len() {
            let scope = &self.scopes[self.index];
            self.index += 1;
            to_value(scope).unwrap() // Use serde_wasm_bindgen::to_value
        } else {
            JsValue::NULL
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
