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
//! let engine = BexEngine::new(program, env_vars, SysOps::native())?;
//! ```

mod ops;
pub mod registry;

// Re-export types from sys_types for convenience
pub use sys_types::{CompletionHandle, OpError, SysOp, SysOpFn, SysOpResult, SysOps};

/// Extension trait to add `native()` constructor to `SysOps`.
pub trait SysOpsExt {
    /// Create a `SysOps` table with native Tokio-based implementations.
    ///
    /// This is the default provider for native Rust applications.
    fn native() -> Self;
}

impl SysOpsExt for SysOps {
    fn native() -> Self {
        SysOps {
            fs_open: ops::fs::open,
            fs_read: ops::fs::read,
            fs_close: ops::fs::close,
            net_connect: ops::net::connect,
            net_read: ops::net::read,
            net_close: ops::net::close,
            shell: ops::sys::shell,
            http_fetch: ops::http::fetch,
            http_response_text: ops::http::text,
            http_response_status: ops::http::status,
            http_response_ok: ops::http::ok,
            http_response_url: ops::http::url,
            http_response_headers: ops::http::headers,
        }
    }
}
