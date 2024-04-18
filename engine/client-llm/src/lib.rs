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
pub fn lint(input: String) -> String {
    register_panic_hook();
    baml_fmt::lint(input)
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
