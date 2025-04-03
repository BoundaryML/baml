#[cfg(target_arch = "wasm32")]
pub mod runtime_wasm;

use internal_baml_core::internal_baml_schema_ast::{format_schema, FormatOptions};
use std::env;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn version() -> String {
    // register_panic_hook();
    env!("CARGO_PKG_VERSION").to_string()
}

#[wasm_bindgen]
pub fn format_document(path: String, text: String) -> Option<String> {
    log::info!("Trying to format document (rust): {}", path);
    match format_schema(
        &text,
        FormatOptions {
            indent_width: 2,
            fail_on_unhandled_rule: false,
        },
    ) {
        Ok(formatted) => {
            log::info!("Formatted document: {}", formatted);
            Some(formatted)
        }
        Err(e) => {
            log::error!("Failed to format document: {} {:?}", path, e);
            None
        }
    }
}
