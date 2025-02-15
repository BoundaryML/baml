pub mod interface;
pub mod storage;
pub use storage::TraceStorage;

// #[cfg(target_arch = "wasm32")]
// pub mod storage_wasm;

// For wasm32 builds, export storage_wasm as storage
// #[cfg(target_arch = "wasm32")]
// pub use storage_wasm as storage;
