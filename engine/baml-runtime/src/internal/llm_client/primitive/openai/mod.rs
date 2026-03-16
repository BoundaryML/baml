mod openai_client;
mod properties;
pub mod response_handler;
#[allow(dead_code)]
mod types;

#[cfg(not(target_arch = "wasm32"))]
pub(super) mod std_auth;

#[cfg(target_arch = "wasm32")]
pub(super) mod wasm_auth;

/// Unified alias: `azure_auth` resolves to the platform-appropriate module.
/// On native targets, this is `std_auth` (uses the `azure_identity` crate).
/// On WASM targets, this is `wasm_auth` (uses the JS callback bridge).
#[cfg(not(target_arch = "wasm32"))]
pub(super) use std_auth as azure_auth;

#[cfg(target_arch = "wasm32")]
pub(super) use wasm_auth as azure_auth;

pub use openai_client::OpenAIClient;
