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

mod epoch_guard;
mod external_value;
mod handle;
mod snapshot;

pub use epoch_guard::EpochGuard;
pub use external_value::ExternalValue;
pub use handle::{Handle, HandleInner, WeakHeapRef};
pub use snapshot::Snapshot;
