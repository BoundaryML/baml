mod runtime_wasm;

use std::{env, panic};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
    /// This function registers the reason for a Wasm panic via the
    /// JS function `globalThis.PRISMA_WASM_PANIC_REGISTRY.set_message()`
    #[wasm_bindgen(js_namespace = ["global", "PRISMA_WASM_PANIC_REGISTRY"], js_name = "set_message")]
    fn prisma_set_wasm_panic_message(s: &str);
}

/// Registers a singleton panic hook that will register the reason for the Wasm panic in JS.
/// Without this, the panic message would be lost: you'd see `RuntimeError: unreachable` message in JS,
/// with no reference to the Rust function and line that panicked.
/// This function should be manually called before any other public function in this module.
/// Note: no method is safe to call after a panic has occurred.
fn register_panic_hook() {
    use std::sync::Once;
    static SET_HOOK: Once = Once::new();

    SET_HOOK.call_once(|| {
        panic::set_hook(Box::new(|info| {
            let message = &info.to_string();
            log(message);
            prisma_set_wasm_panic_message(message);
        }));
    });
}

#[wasm_bindgen]
pub fn call_llm(input: String) -> String {
    register_panic_hook();
    baml_fmt::call_llm(input)
}

#[wasm_bindgen]
pub fn validate(params: String) -> Result<(), JsError> {
    register_panic_hook();
    baml_fmt::validate(params).map_err(|e| JsError::new(&e))
}

#[wasm_bindgen]
pub fn version() -> String {
    register_panic_hook();
    env!("CARGO_PKG_VERSION").to_string()
}

// #[wasm_bindgen]
// pub fn native_types(input: String) -> String {
//     register_panic_hook();
//     prisma_fmt::native_types(input)
// }

// #[wasm_bindgen]
// pub fn referential_actions(input: String) -> String {
//     register_panic_hook();
//     prisma_fmt::referential_actions(input)
// }

// #[wasm_bindgen]
// pub fn preview_features() -> String {
//     register_panic_hook();
//     prisma_fmt::preview_features()
// }

/// The API is modelled on an LSP [completion
/// request](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/specification-3-16.md#textDocument_completion).
/// Input and output are both JSON, the request being a `CompletionParams` object and the response
/// being a `CompletionList` object.
// #[wasm_bindgen]
// pub fn text_document_completion(schema: String, params: String) -> String {
//     register_panic_hook();
//     prisma_fmt::text_document_completion(schema, &params)
// }

/// This API is modelled on an LSP [code action
/// request](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/specification-3-16.md#textDocument_codeAction=).
/// Input and output are both JSON, the request being a
/// `CodeActionParams` object and the response being a list of
/// `CodeActionOrCommand` objects.
// #[wasm_bindgen]
// pub fn code_actions(schema: String, params: String) -> String {
//     register_panic_hook();
//     prisma_fmt::code_actions(schema, &params)
// }

/// Trigger a panic inside the wasm module. This is only useful in development for testing panic
/// handling.
#[wasm_bindgen]
pub fn debug_panic() {
    register_panic_hook();
    panic!("This is the panic triggered by `baml_fmt::debug_panic()`");
}

#[wasm_bindgen]
pub fn enable_logs() {
    wasm_logger::init(wasm_logger::Config::default());
}
