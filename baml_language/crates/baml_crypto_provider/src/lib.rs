//! The rustls crypto provider for BAML builds with the `external-crypto`
//! feature.
//!
//! This default supplies none: an `external-crypto` build that keeps it must
//! install a process-wide provider itself
//! ([`rustls::crypto::CryptoProvider::install_default`]) before BAML first
//! needs crypto, or every operation that does fails. Point the workspace's
//! `baml_crypto_provider` dependency at your own crate to have BAML install
//! yours on first use instead; see `README.md`.

/// The provider BAML installs as the process default when none is installed
/// yet. `None` leaves the process without one.
pub fn provider() -> Option<rustls::crypto::CryptoProvider> {
    None
}
