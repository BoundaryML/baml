//! Shared type conversion utilities for BAML bridge (C FFI and WASM).
//!
//! This crate holds the common protobuf definitions and conversion logic
//! between host values and `BexExternalValue`, used by both `bridge_cffi`
//! and `bridge_wasm`.

mod error;
mod handle_table;
mod traceback;
mod utils;
mod value_decode;
mod value_encode;

/// Generated protobuf module (CFFI / host value types).
pub mod baml_core {
    pub mod cffi {
        #![allow(clippy::doc_markdown, clippy::empty_structs_with_brackets)]
        include!(concat!(env!("OUT_DIR"), "/baml_core.cffi.v1.rs"));
    }
}

pub use error::CtypesError;
pub use handle_table::{
    CffiHandleTable, CffiHandleTableEntry, CffiHandleTableOptions, HANDLE_TABLE,
};
pub use traceback::format_traceback_lines;
pub use utils::DecodeFromBuffer;
pub use value_decode::{inbound_to_external, kwargs_to_bex_values};
pub use value_encode::{build_to_host_call, external_to_outbound};
