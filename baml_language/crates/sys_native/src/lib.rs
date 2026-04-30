//! Native Tokio-based system operations provider.
//!
//! This crate provides `SysOps::native()` via an extension trait, which returns
//! a function pointer table with Tokio-based implementations for all system operations.
//!
//! # Usage
//!
//! ```ignore
//! use sys_native::SysOpsExt;
//! use bex_engine::BexEngine;
//!
//! let engine = BexEngine::new(program, SysOps::native())?;
//! ```

mod io_impls;
pub mod registry;
pub mod shell;

pub use sys_ops::{SysOps, io};
pub use sys_types::{CallId, CompletionHandle, OpError, SysOp, SysOpContext};

/// The native Tokio-based `sys_op` provider.
///
/// Implements IO traits (`IoNamespaceFs`, `IoNamespaceHttp`, etc.) with clean
/// typed signatures. The generated glue handles arg extraction and error wrapping.
pub struct NativeSysOps;

impl Default for NativeSysOps {
    fn default() -> Self {
        Self
    }
}

impl io::IoPackageBaml for NativeSysOps {}

// ============================================================================
// Extension trait
// ============================================================================

/// Extension trait to add `native()` constructor to `SysOps`.
pub trait SysOpsExt {
    fn native() -> Self;
}

impl SysOpsExt for SysOps {
    fn native() -> Self {
        SysOps::from_impl(NativeSysOps)
    }
}
