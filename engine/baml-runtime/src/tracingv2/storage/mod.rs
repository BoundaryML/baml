pub mod interface;
pub mod llm_response_to_log_event;
pub mod storage;
pub use llm_response_to_log_event::make_trace_event_for_response;
pub use storage::TraceStorage;
// #[cfg(target_arch = "wasm32")]
// pub mod storage_wasm;

// For wasm32 builds, export storage_wasm as storage
// #[cfg(target_arch = "wasm32")]
// pub use storage_wasm as storage;
