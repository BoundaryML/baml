//! How BAML code reaches the network.
//!
//! Everything goes through the one transport [`provider`] returns: requests a
//! BAML program makes (LLM calls, `baml.http`, `baml.ws`, `baml.http.Server`)
//! and requests BAML makes on its own. The latter go through [`outbound`],
//! which names each destination so it can be audited and, in a
//! `no-phone-home` build, refused.

pub mod outbound;

use std::sync::{Arc, OnceLock};

pub use baml_http_types::*;

/// The process's network transport, from `baml_http_provider`.
pub fn provider() -> &'static Arc<dyn HttpProvider> {
    static PROVIDER: OnceLock<Arc<dyn HttpProvider>> = OnceLock::new();
    PROVIDER.get_or_init(baml_http_provider::provider)
}
