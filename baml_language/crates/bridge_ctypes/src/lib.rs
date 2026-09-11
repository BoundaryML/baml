//! Shared type conversion utilities for BAML bridge (C FFI and WASM).
//!
//! This crate holds the common protobuf definitions and conversion logic
//! between host values and `BexExternalValue`, used by both `bridge_cffi`
//! and `bridge_wasm`.

mod error;
mod handle_table;
mod traceback;
mod ty_decode;
mod ty_encode;
mod utils;
mod value_decode;
mod value_encode;

/// Generated protobuf module (CFFI / host value types).
pub mod baml_bridge {
    pub mod cffi {
        #![allow(clippy::doc_markdown, clippy::empty_structs_with_brackets)]
        include!(concat!(env!("OUT_DIR"), "/baml_bridge.cffi.v1.rs"));
    }
}

pub use error::CtypesError;
pub use handle_table::{
    CffiHandleTable, CffiHandleTableEntry, CffiHandleTableOptions, HANDLE_TABLE, RuntimeOwner,
};
pub use traceback::format_traceback_lines;
pub use ty_decode::{
    DecodedTypeArgs, proto_ty_args_to_named, proto_ty_def_to_external, proto_ty_def_to_portable,
    proto_ty_to_external, proto_ty_to_runtime_ty,
};
pub use ty_encode::{portable_type_def_to_proto, runtime_ty_to_proto_ty};
pub use utils::DecodeFromBuffer;
pub use value_decode::{
    inbound_to_external, kwargs_to_bex_values, playground_run_args_to_bex_values,
};
pub use value_encode::{artifact_safe_outbound_bytes, build_to_host_call, external_to_outbound};
