//! Media rewriting for LLM requests.

use crate::errors::MediaResolveError;

/// Trait for provider-specific requests that support media rewriting.
pub trait MediaRewritable {
    /// Check if this request has unresolved media.
    fn has_unresolved_media(&self) -> bool;

    /// Resolve all media (URLs to base64, files to base64).
    /// This is async because it may involve network requests.
    #[cfg(feature = "native")]
    fn resolve_media(&mut self) -> impl std::future::Future<Output = Result<(), MediaResolveError>> + Send;

    /// Resolve all media (WASM version).
    #[cfg(all(target_arch = "wasm32", not(feature = "native")))]
    fn resolve_media(&mut self) -> impl std::future::Future<Output = Result<(), MediaResolveError>>;
}
