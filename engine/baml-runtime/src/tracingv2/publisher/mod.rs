pub mod interface;
pub mod publisher;

pub use publisher::TracePublisher;

#[cfg(target_arch = "wasm32")]
pub mod publisher_wasm;

// For wasm32 builds, export publisher_wasm as publisher
#[cfg(target_arch = "wasm32")]
pub use publisher_wasm as publisher;
