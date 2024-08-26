use std::collections::HashMap;

use baml_runtime::{
    internal::llm_client::{
        orchestrator::{ExecutionScope, OrchestrationScope},
        AllowedMetadata,
    },
    ChatMessagePart, RenderedPrompt,
};
use serde::Serialize;
use serde_json::json;

use crate::runtime_wasm::ToJsValue;
use baml_types::{BamlMedia, BamlMediaContent, BamlMediaType, MediaBase64};
use serde_wasm_bindgen::to_value;
use wasm_bindgen::prelude::*;

use super::WasmFunction;

#[wasm_bindgen(getter_with_clone)]
pub struct WasmScope {
    scope: OrchestrationScope,
}

#[wasm_bindgen(getter_with_clone)]
pub struct WasmPrompt {
    prompt: RenderedPrompt,
    pub client_name: String,
    allowed: AllowedMetadata,
}

impl From<OrchestrationScope> for WasmScope {
    fn from(scope: OrchestrationScope) -> Self {
        WasmScope { scope }
    }
}

impl From<(&RenderedPrompt, &OrchestrationScope, &AllowedMetadata)> for WasmPrompt {
    fn from(
        (prompt, client_name, allowed): (&RenderedPrompt, &OrchestrationScope, &AllowedMetadata),
    ) -> Self {
        WasmPrompt {
            prompt: prompt.clone(),
            client_name: client_name.name(),
            allowed: allowed.clone(),
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
#[derive(Clone, Copy)]
pub enum WasmChatMessagePartMediaType {
    Url,
    File,
    Error,
}

#[wasm_bindgen(getter_with_clone)]
pub struct WasmChatMessagePartMedia {
    pub r#type: WasmChatMessagePartMediaType,
    pub content: String,
}

#[wasm_bindgen]
impl WasmChatMessagePart {
    #[wasm_bindgen]
    pub fn is_text(&self) -> bool {
        self.part.as_text().is_some()
    }

    #[wasm_bindgen]
    pub fn is_image(&self) -> bool {
        matches!(
            self.part.as_media().map(|s| s.media_type),
            Some(BamlMediaType::Image)
        )
    }

    #[wasm_bindgen]
    pub fn json_meta(&self, prompt: &WasmPrompt) -> Option<String> {
        match self.part.meta() {
            Some(meta) => {
                let (allowed, skipped): (Vec<_>, Vec<_>) = meta
                    .into_iter()
                    .partition(|(k, _)| prompt.allowed.is_allowed(k));

                let allowed: HashMap<_, _> = allowed.into_iter().collect();
                let skipped: HashMap<_, _> = skipped.into_iter().collect();

                Some(
                    json!({
                        "allowed": allowed,
                        "skipped": skipped,
                    })
                    .to_string(),
                )
            }
            None => None,
        }
    }

    #[wasm_bindgen]
    pub fn is_audio(&self) -> bool {
        matches!(
            self.part.as_media().map(|s| s.media_type),
            Some(BamlMediaType::Audio)
        )
    }

    #[wasm_bindgen]
    pub fn as_text(&self) -> Option<String> {
        self.part.as_text().map(|s| s.clone())
    }

    #[wasm_bindgen]
    pub fn as_media(&self) -> Option<WasmChatMessagePartMedia> {
        let Some(m) = self.part.as_media() else {
            return None;
        };
        Some(match &m.content {
            BamlMediaContent::Url(u) => WasmChatMessagePartMedia {
                r#type: WasmChatMessagePartMediaType::Url,
                content: u.url.clone(),
            },
            BamlMediaContent::Base64(MediaBase64 { base64 }) => WasmChatMessagePartMedia {
                r#type: WasmChatMessagePartMediaType::Url,
                content: format!(
                    "data:{};base64,{}",
                    m.mime_type.as_deref().unwrap_or("type/unknown"),
                    base64.clone()
                ),
            },
            BamlMediaContent::File(f) => match f.path() {
                Ok(path) => WasmChatMessagePartMedia {
                    r#type: WasmChatMessagePartMediaType::File,
                    content: path.to_string_lossy().into_owned(),
                },
                Err(e) => WasmChatMessagePartMedia {
                    r#type: WasmChatMessagePartMediaType::Error,
                    content: format!("Error resolving file '{}': {:#}", f.relpath.display(), e),
                },
            },
        })
    }
}

#[wasm_bindgen]
impl WasmScope {
    #[wasm_bindgen]
    pub fn name(&self) -> String {
        self.scope.name()
    }
    #[cfg(target_arch = "wasm32")]
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
            match to_value(scope) {
                Ok(value) => value,
                Err(_) => JsValue::NULL,
            }
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
