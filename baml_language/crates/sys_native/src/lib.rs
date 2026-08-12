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

pub mod host_dispatch;
mod host_impls;
#[cfg(feature = "bundle-http")]
mod http_server;
mod io_impls;
pub mod registry;
pub mod shell;

pub use sys_ops::{SysOps, io};
pub use sys_types::{CallId, CompletionHandle, OpError, SysOp, SysOpContext, VmInternalError};

// HTTPS — both the client (`fetch`/`send`) and the server (`build_acceptor`) —
// needs a rustls crypto provider. Require one at compile time so `bundle-http`
// can't silently ship without one, which would panic when building a rustls
// `ServerConfig`/`ClientConfig` at runtime.
#[cfg(all(
    feature = "bundle-http",
    not(feature = "aws-crypto"),
    not(feature = "ring-crypto")
))]
compile_error!(
    "feature `bundle-http` requires a rustls crypto provider: enable `aws-crypto` (the default) or `ring-crypto`"
);

#[cfg(all(feature = "bundle-http", feature = "ring-crypto"))]
pub(crate) fn ensure_rustls_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[cfg(all(
    feature = "bundle-http",
    not(feature = "ring-crypto"),
    feature = "aws-crypto"
))]
pub(crate) fn ensure_rustls_crypto_provider() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

#[cfg(not(feature = "bundle-http"))]
#[expect(dead_code)]
pub(crate) fn ensure_rustls_crypto_provider() {}

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
