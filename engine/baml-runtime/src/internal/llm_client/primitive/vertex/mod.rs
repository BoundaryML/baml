pub(crate) mod response_handler;
#[cfg(target_arch = "wasm32")]
pub(super) mod wasm_auth;
#[cfg(target_arch = "wasm32")]
pub(super) use wasm_auth as auth;

#[cfg(not(target_arch = "wasm32"))]
pub(super) mod std_auth;
#[cfg(not(target_arch = "wasm32"))]
pub(super) use std_auth as auth;

mod types;
mod vertex_client;
pub use vertex_client::VertexClient;
