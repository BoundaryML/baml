//! Non-TLS cryptography through the replaceable `baml_crypto_provider` crate.
//! This facade exposes no TLS or backend-specific types.

use std::{
    fmt,
    sync::{Arc, OnceLock},
};

pub use baml_crypto_types::{
    AeadAlgorithm, AeadCipher, AeadError, CryptoError, CryptoProvider, SHA1_LEN, SHA256_LEN,
    Sha256Context,
};

/// The provider selected by the downstream build, initialized once on first use.
pub fn provider() -> &'static Arc<dyn CryptoProvider> {
    static PROVIDER: OnceLock<Arc<dyn CryptoProvider>> = OnceLock::new();
    PROVIDER.get_or_init(baml_crypto_provider::provider)
}

/// Compatibility hash for AWS SSO cache filenames.
pub fn sha1(data: &[u8]) -> Result<[u8; SHA1_LEN], CryptoError> {
    provider().sha1(data)
}

pub fn sha256(data: &[u8]) -> Result<[u8; SHA256_LEN], CryptoError> {
    let mut hasher = Sha256::new()?;
    hasher.update(data);
    Ok(hasher.finish())
}

pub struct Sha256(Box<dyn Sha256Context>);

impl Sha256 {
    pub fn new() -> Result<Self, CryptoError> {
        provider().sha256().map(Self)
    }

    pub fn update(&mut self, data: &[u8]) {
        self.0.update(data);
    }

    /// Return the digest and reset the hasher for the next message.
    pub fn finish(&mut self) -> [u8; SHA256_LEN] {
        self.0.finish()
    }
}

impl fmt::Debug for Sha256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Sha256").finish_non_exhaustive()
    }
}

pub fn hmac_sha256(key: &[u8], message: &[u8]) -> Result<[u8; SHA256_LEN], CryptoError> {
    provider().hmac_sha256(key, message)
}

pub fn sign_rs256(private_key_pem: &str, message: &[u8]) -> Result<Vec<u8>, CryptoError> {
    provider().sign_rs256(private_key_pem, message)
}

pub fn fill_random(buf: &mut [u8]) -> Result<(), CryptoError> {
    provider().fill_random(buf)
}

pub fn aead(algorithm: AeadAlgorithm, key: &[u8]) -> Result<Arc<dyn AeadCipher>, AeadError> {
    algorithm.validate_key(key)?;
    provider().aead(algorithm, key)
}

#[cfg(test)]
mod tests;
