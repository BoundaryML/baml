//! WASM handle lifecycle — auto-released via `FinalizationRegistry`.

use bridge_ctypes::{HANDLE_TABLE, baml::cffi::BamlHandleType};
use js_sys::{Object, Reflect};
use wasm_bindgen::prelude::*;

fn type_name(ht: BamlHandleType) -> &'static str {
    match ht {
        BamlHandleType::HandleUnspecified => "unspecified",
        BamlHandleType::HandleUnknown => "unknown",
        BamlHandleType::ResourceFile => "file",
        BamlHandleType::ResourceSocket => "socket",
        BamlHandleType::ResourceHttpResponse => "http_response",
        BamlHandleType::FunctionRef => "function_ref",
        BamlHandleType::AdtMediaImage => "image",
        BamlHandleType::AdtMediaAudio => "audio",
        BamlHandleType::AdtMediaVideo => "video",
        BamlHandleType::AdtMediaPdf => "pdf",
        BamlHandleType::AdtMediaGeneric => "media",
        BamlHandleType::AdtPromptAst => "prompt_ast",
        BamlHandleType::AdtCollector => "collector",
        BamlHandleType::AdtType => "type",
    }
}

/// A reference to an opaque BAML value held in the engine's handle table.
///
/// When this object is garbage-collected by JS (via `FinalizationRegistry`),
/// `Drop` is called which releases the entry from the handle table.
///
/// Implements `toJSON()` so `JSON.stringify` produces
/// `{ "$handle": { "key": "42", "handle_type": "image" } }`.
#[wasm_bindgen]
pub struct BamlHandle {
    key: u64,
    handle_type: BamlHandleType,
}

#[wasm_bindgen]
impl BamlHandle {
    /// Construct from proto-decoded key + type tag.
    /// Key is passed as string (e.g. from `BigInt.toString()`) to avoid f64 precision loss.
    /// Invalid or non-numeric string becomes 0 (invalid sentinel).
    /// `handle_type` is the i32 proto enum value.
    #[wasm_bindgen(constructor)]
    pub fn new(key: &str, handle_type: i32) -> BamlHandle {
        let key = key.parse().unwrap_or(0);
        BamlHandle {
            key,
            handle_type: BamlHandleType::try_from(handle_type)
                .unwrap_or(BamlHandleType::HandleUnknown),
        }
    }

    /// Clone this handle — new key, same underlying value.
    #[wasm_bindgen(js_name = "cloneHandle")]
    pub fn clone_handle(&self) -> Result<BamlHandle, JsError> {
        let new_key = HANDLE_TABLE
            .clone_handle(self.key)
            .ok_or_else(|| JsError::new("Handle is no longer valid"))?;
        Ok(BamlHandle {
            key: new_key,
            handle_type: self.handle_type,
        })
    }

    /// The handle type tag as i32 (maps to `BamlHandleType` proto enum).
    #[wasm_bindgen(getter, js_name = "handleType")]
    pub fn handle_type_i32(&self) -> i32 {
        self.handle_type as i32
    }

    /// The handle key as decimal string (safe for u64; use BigInt(key) in JS if needed).
    #[wasm_bindgen(getter)]
    pub fn key(&self) -> String {
        self.key.to_string()
    }

    #[allow(clippy::doc_markdown)]
    /// The human-readable handle type name (e.g. "image", "prompt_ast").
    #[wasm_bindgen(getter, js_name = "typeName")]
    pub fn type_name_str(&self) -> String {
        type_name(self.handle_type).to_owned()
    }

    /// Called by `JSON.stringify` on the `handle` field of a `BamlJsHandle<T>`.
    /// Returns `{ key: "42", handle_type: "image" }` — the `$baml` wrapper is added by
    /// the decoder, so this only needs to serialize the handle-specific metadata.
    #[wasm_bindgen(js_name = "toJSON")]
    pub fn to_json(&self) -> JsValue {
        let obj = Object::new();
        let _ = Reflect::set(
            &obj,
            &"key".into(),
            &JsValue::from_str(&self.key.to_string()),
        );
        let _ = Reflect::set(
            &obj,
            &"handle_type".into(),
            &JsValue::from_str(type_name(self.handle_type)),
        );
        obj.into()
    }
}

impl Drop for BamlHandle {
    fn drop(&mut self) {
        HANDLE_TABLE.release(self.key);
    }
}
