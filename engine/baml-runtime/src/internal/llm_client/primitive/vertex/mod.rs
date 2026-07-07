pub(crate) mod response_handler;
#[cfg(target_arch = "wasm32")]
pub(super) mod wasm_auth;
#[cfg(target_arch = "wasm32")]
pub(super) use wasm_auth as auth;

#[cfg(all(not(target_arch = "wasm32"), feature = "vertex"))]
pub(super) mod std_auth;
#[cfg(all(not(target_arch = "wasm32"), feature = "vertex"))]
pub(super) use std_auth as auth;

// Without the `vertex` feature the gcp_auth dependency tree is dropped; this
// stub keeps VertexClient compiling and fails at request time instead.
#[cfg(all(not(target_arch = "wasm32"), not(feature = "vertex")))]
pub(super) mod stub_auth;
#[cfg(all(not(target_arch = "wasm32"), not(feature = "vertex")))]
pub(super) use stub_auth as auth;

mod types;
mod vertex_client;
pub use vertex_client::VertexClient;
