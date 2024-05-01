use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct WasmRuntimeContext {
    pub(crate) ctx: baml_runtime::RuntimeContext,
}

#[wasm_bindgen]
impl WasmRuntimeContext {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmRuntimeContext {
        WasmRuntimeContext {
            ctx: baml_runtime::RuntimeContext::new(),
        }
    }

    pub fn copy(&self) -> WasmRuntimeContext {
        WasmRuntimeContext {
            ctx: self.ctx.clone(),
        }
    }

    #[wasm_bindgen]
    pub fn set_env(&mut self, key: String, value: Option<String>) {
        match value {
            Some(v) => self.ctx.env.insert(key, v),
            None => self.ctx.env.remove(&key),
        };
    }

    #[wasm_bindgen]
    pub fn set_tag(&mut self, key: String, value: Option<String>) {
        match value {
            Some(v) => self.ctx.tags.insert(key, v.into()),
            None => self.ctx.tags.remove(&key),
        };
    }
}
