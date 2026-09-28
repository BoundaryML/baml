//! A provider the host installs before BAML's first TLS call is the one BAML
//! uses. Its own binary, so no other test has installed a provider first.

#![cfg(any(feature = "aws-crypto", feature = "ring-crypto"))]

use rustls::crypto::CryptoProvider;

#[test]
fn keeps_the_host_provider() {
    #[cfg(feature = "ring-crypto")]
    let mut host = rustls::crypto::ring::default_provider();
    #[cfg(not(feature = "ring-crypto"))]
    let mut host = rustls::crypto::aws_lc_rs::default_provider();
    host.cipher_suites.truncate(1);
    host.install_default().unwrap();

    baml_tls::ensure_crypto_provider().unwrap();

    let installed = CryptoProvider::get_default().unwrap();
    assert_eq!(installed.cipher_suites.len(), 1);
}
