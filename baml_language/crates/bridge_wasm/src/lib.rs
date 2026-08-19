//! `bridge_wasm` - WASM bindings for BAML.
//!
//! This crate only supports the `wasm32-unknown-unknown` target. Use
//! `--target wasm32-unknown-unknown` when building.
//!
//! The browser playground/LSP host that lived here was built on the previous
//! project foundation and is being rebuilt on the `baml_lsp` owner/snapshot
//! runtime; until then this crate exports only its build identity.

use std::sync::Once;

use wasm_bindgen::prelude::*;

static LOGGER_INIT: Once = Once::new();

#[wasm_bindgen(start)]
pub fn start() {
    bex_project::register_inbound_union_ambiguity_policy(
        bex_project::InboundUnionAmbiguityPolicy::SelectDefault,
    )
    .expect("the browser TypeScript bridge must own the process-wide inbound policy");
    #[cfg(feature = "console_error_panic")]
    console_error_panic_hook::set_once();
    LOGGER_INIT.call_once(|| {
        let level = if cfg!(debug_assertions) {
            log::Level::Debug
        } else {
            log::Level::Info
        };
        wasm_logger::init(wasm_logger::Config::new(level));
    });
}

/// Get the version of the `bridge_wasm` crate.
#[wasm_bindgen]
pub fn version() -> String {
    baml_version::CANONICAL_VERSION.to_string()
}

/// Get the Git commit used to build the `bridge_wasm` crate.
#[wasm_bindgen(js_name = commitHash)]
pub fn commit_hash() -> String {
    env!("BRIDGE_WASM_GIT_SHA").to_string()
}

/// Returns the build timestamp (unix seconds) for hot-reload / build-identity checks.
#[wasm_bindgen(js_name = getBuildTime)]
pub fn get_build_time() -> String {
    env!("BRIDGE_WASM_BUILD_TS").to_string()
}
