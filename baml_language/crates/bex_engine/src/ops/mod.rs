//! Built-in external operations.
//!
//! This module contains implementations of external operations defined in
//! `baml_builtins` with the `#[external]` attribute.
//!
//! Operations are dispatched via static dispatch using the `ExternalOp` and
//! `SysOp` enums from `bex_vm_types`. See `BexEngine::execute_external_op`.

pub mod fs;
pub mod net;
pub mod sys;
