//! The rustls crypto provider behind every TLS connection BAML makes, and the
//! rest of its security-relevant crypto (see `primitives`).
//!
//! rustls does no cryptography itself; it calls a process-wide
//! [`CryptoProvider`]. A build picks where that provider comes from with one
//! of three features:
//!
//! - `aws-crypto` (the default): AWS-LC, bundled.
//! - `ring-crypto`: `ring`, bundled.
//! - `external-crypto`: nothing bundled. The host installs a provider before
//!   its first HTTPS call, or a replacement `baml_crypto_provider` crate
//!   supplies one. This is how a build puts TLS on a FIPS-validated module.
//!
//! Whichever features are on, a provider the host installed first is always
//! the one used. With none of them on, only a host-installed provider works.

mod primitives;

use std::fmt;

pub use primitives::{
    CryptoError, SHA256_LEN, Sha256, fill_random, hmac_sha256, provider, sha256, sign_rs256,
};
use rustls::crypto::CryptoProvider;

/// Makes sure the process has a default rustls crypto provider. Call it
/// before building any TLS client or server config.
///
/// A provider the host already installed is kept. Otherwise this installs
/// the one `baml_crypto_provider` supplies (`external-crypto`), else the
/// bundled one. It never panics: with no provider anywhere it returns
/// [`NoCryptoProvider`].
pub fn ensure_crypto_provider() -> Result<(), NoCryptoProvider> {
    if CryptoProvider::get_default().is_some() {
        return Ok(());
    }
    if let Some(provider) = supplied_provider().or_else(bundled_provider) {
        // An error means another thread installed one first; that one stands.
        let _ = provider.install_default();
    }
    match CryptoProvider::get_default() {
        Some(_) => Ok(()),
        None => Err(NoCryptoProvider),
    }
}

#[cfg(feature = "external-crypto")]
fn supplied_provider() -> Option<CryptoProvider> {
    baml_crypto_provider::provider()
}

#[cfg(not(feature = "external-crypto"))]
fn supplied_provider() -> Option<CryptoProvider> {
    None
}

// `ring-crypto` is opted into on top of the default `aws-crypto`, so it wins
// when both are on.
#[cfg(feature = "ring-crypto")]
#[allow(
    clippy::unnecessary_wraps,
    reason = "matches the variant for builds that bundle no provider"
)]
fn bundled_provider() -> Option<CryptoProvider> {
    Some(rustls::crypto::ring::default_provider())
}

#[cfg(all(feature = "aws-crypto", not(feature = "ring-crypto")))]
#[allow(
    clippy::unnecessary_wraps,
    reason = "matches the variant for builds that bundle no provider"
)]
fn bundled_provider() -> Option<CryptoProvider> {
    Some(rustls::crypto::aws_lc_rs::default_provider())
}

#[cfg(not(any(feature = "aws-crypto", feature = "ring-crypto")))]
fn bundled_provider() -> Option<CryptoProvider> {
    None
}

/// No rustls crypto provider is installed and this build bundles none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoCryptoProvider;

impl fmt::Display for NoCryptoProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(
            "no crypto provider: this BAML build links no crypto library (`external-crypto`). \
             Install a rustls CryptoProvider with `CryptoProvider::install_default` before BAML \
             first needs crypto, or build with a `baml_crypto_provider` crate that supplies one",
        )
    }
}

impl std::error::Error for NoCryptoProvider {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_the_bundled_provider_or_reports_none() {
        let supplied = supplied_provider().is_some();
        let result = ensure_crypto_provider();
        if supplied || cfg!(any(feature = "aws-crypto", feature = "ring-crypto")) {
            assert_eq!(result, Ok(()));
            assert!(CryptoProvider::get_default().is_some());
        } else {
            assert_eq!(result, Err(NoCryptoProvider));
            assert!(CryptoProvider::get_default().is_none());
        }
    }
}
