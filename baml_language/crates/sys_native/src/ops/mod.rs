//! Shared helpers for native system operation implementations.
//!
//! The actual implementations live in the per-module trait impls
//! (`SysOpFs`, `SysOpHttp`, etc.) in `lib.rs`. This module just
//! provides shared utilities (e.g., HTTP send logic).

pub(crate) mod http;
