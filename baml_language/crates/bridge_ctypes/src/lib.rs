//! Shared type conversion utilities for BAML bridge (C FFI and WASM).
//!
//! This crate holds the common protobuf definitions and conversion logic
//! between host values and `BexExternalValue`, used by both `bridge_cffi`
//! and `bridge_wasm`.

mod error;
mod handle_table;
mod wire_depth;
pub use wire_depth::{
    ARGUMENT_DEPTH, RESULT_DEPTH, WIRE_NESTING_LIMIT, baml_ty_depth, inbound_value_depth,
};
mod host_call_completion;
mod host_value_registry;
mod invocation;
pub use host_call_completion::decode_host_call_completion;
pub use host_value_registry::HostValueRegistry;
pub use invocation::{
    OwnedHostArguments, OwnedHostInvocation, build_host_invocation, release_outbound_references,
};
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

/// The engine's reference to a host object, the dual of a [`TableKey`].
pub use bex_project::HostKey;
pub use error::CtypesError;
pub use handle_table::{
    CffiHandleTable, CffiHandleTableEntry, CffiHandleTableOptions, HANDLE_TABLE,
    InvocationStateHandle, TableKey, TraceReservationHandle,
};
pub use traceback::format_traceback_lines;
pub use ty_decode::{
    DecodedTypeArgs, proto_ty_args_to_named, proto_ty_def_to_external, proto_ty_def_to_portable,
    proto_ty_to_external, proto_ty_to_runtime_ty,
};
pub use ty_encode::{portable_type_def_to_proto, runtime_ty_to_proto_ty};
pub use utils::DecodeFromBuffer;
pub use value_decode::{InboundOwners, WireHandle, inbound_to_external, kwargs_to_bex_values};
pub use value_encode::{artifact_safe_outbound_bytes, build_to_host_call, external_to_outbound};
