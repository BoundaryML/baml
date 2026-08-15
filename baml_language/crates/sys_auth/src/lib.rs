//! Provider authentication primitives for the BAML standard library.
//!
//! Two operations in the provider stack need cryptography, which BAML's stdlib
//! does not have: minting a Google Cloud `OAuth2` access token (RS256 JWT signing
//! over PKCS#8 RSA keys) and AWS `SigV4` request signing (HMAC-SHA256). Both are
//! thin adapters over the slim hand-forks (`google-cloud-auth`, `aws-config`,
//! `aws-sigv4`) — no provider request/response knowledge lives here; that is all
//! `.baml`.
//!
//! All IO (env, files, HTTP) is routed through BAML's [`RuntimeIo`] by an
//! internal `BamlAuthIo` adapter, so credential resolution stays inside the
//! sandbox.
//!
//! Exposed to BAML as the `ai.internal.*` sys-ops (see
//! `baml_builtins2/baml_std/ai/ns_internal/auth.baml`), which the `google` and
//! `aws` client packages wrap.

use std::sync::Arc;

use sys_types::runtime_io::RuntimeIo;

mod aws;
mod gcp;
mod io_bridge;
#[cfg(test)]
mod testing;

pub use aws::{AwsSignOptions, resolve_region, sign_request};
pub use gcp::{access_token, project_id, quota_project_id};
pub(crate) use io_bridge::BamlAuthIo;

/// Why an auth operation failed.
///
/// The split is the one callers act on: [`Self::Access`] means the credentials
/// are missing, malformed, or refused (never retry), [`Self::Io`] means the
/// transport or filesystem failed while resolving them (retry-safe). It maps
/// onto `baml.errors.AccessError` / `baml.errors.Io` at the sys-op boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    /// Credentials were absent, unreadable, malformed, or rejected.
    Access(String),
    /// An IO operation failed while resolving credentials.
    Io(String),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Access(m) | Self::Io(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for AuthError {}

/// Wrap a [`RuntimeIo`] so the forks' IO traits can be satisfied.
pub(crate) fn bridge(io: Arc<dyn RuntimeIo>) -> BamlAuthIo {
    BamlAuthIo { io }
}
