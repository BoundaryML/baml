//! External types for BEX FFI boundary.
//!
//! This crate provides types that cross the boundary between BEX internals
//! and external code (Python bindings, JS bindings, C FFI, etc.).
//!
//! # Design Principles
//!
//! - **Internal vs External**: Internal VM code uses `ObjectIndex` for fast access.
//!   External code uses opaque `Handle` values that are validated before use.
//!
//! - **ExternalValue**: A self-contained value type that doesn't require heap access
//!   to inspect. Primitives are inlined, complex objects use `Handle`.
//!
//! - **RAII Handles**: Handles use `Arc` internally for automatic cleanup.
//!   Clone to share, drop to release.
//!
//! # Dependency Graph
//!
//! ```text
//! bex_vm_types ◄── bex_external_types ◄── bex_heap ◄── bex_vm ◄── bex_engine
//! (internal)       (external)              (memory)     (exec)     (async)
//! ```

mod bex_external_value;
mod handle;
mod host_return;
mod runtime_ty_identity;

pub use baml_type::MediaKind;
pub use bex_external_value::{
    AsBexExternalValue, BexExternalAdt, BexExternalValue, MEDIA_WRAPPER_DATA_FIELD,
    OpaqueExternalValue, RuntimeTy, ToBexExternalValue, TyAttr, TypeName, UnionMetadata,
    try_convert_rust_data,
};
pub use bex_resource_types::{
    HostReleaseFn, HostValueArc, HostValueKind, host_release_dispatch, host_value,
};
pub use bex_str::BexStr;
pub use handle::{Handle, HandleInner, WeakHeapRef};
pub use host_return::{
    HostReturnTypeError, is_canonical_json_alias, validate_host_return, value_satisfies_json,
};
pub use runtime_ty_identity::{runtime_ty_structurally_equal, selected_arm_equal};
