use wasm_bindgen::prelude::*;

/// Returns a test string for hot reload testing.
/// See typescript2/app-vscode-webview/src/hot-reload.hmr.test.ts
#[wasm_bindgen]
pub fn hot_reload_test_string() -> String {
    // BEGIN_VITE_HOT_RELOAD_TEST
    "injected for hot reload test, see hot-reload.hmr.test.ts".to_string()
    // END_VITE_HOT_RELOAD_TEST
}
