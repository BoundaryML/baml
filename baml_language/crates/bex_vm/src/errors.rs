//! VM error and panic types — relocated to `bex_vm_types::errors` so that
//! crates which produce these errors (e.g. sysop impls) can use them without
//! taking a dependency on the VM crate itself. This shim re-exports them so
//! existing `bex_vm::errors::X` paths keep working.

pub use bex_vm_types::errors::*;
