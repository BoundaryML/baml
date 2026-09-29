//! Separate binary: no other test may install a process TLS provider first.

#[test]
fn keeps_the_host_tls_provider() {
    #[cfg(target_os = "ios")]
    let mut host = rustls::crypto::ring::default_provider();
    #[cfg(not(target_os = "ios"))]
    let mut host = rustls::crypto::aws_lc_rs::default_provider();
    host.cipher_suites.truncate(1);
    host.install_default().unwrap();

    let config = baml_http_types::TlsConfig {
        cert_pem: vec![],
        key_pem: vec![],
        allow_tls1_2: false,
        handshake_timeout: None,
    };
    // Certificate validation initializes TLS before rejecting the empty PEM.
    assert!(baml_http_provider::provider().check_tls(&config).is_err());
    assert_eq!(
        rustls::crypto::CryptoProvider::get_default()
            .unwrap()
            .cipher_suites
            .len(),
        1
    );
}
