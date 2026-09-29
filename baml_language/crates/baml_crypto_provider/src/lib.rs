//! Default non-TLS cryptography. Replace this crate to supply an organization's
//! implementation of the backend-independent `baml_crypto_types` contracts.
//! TLS policy and backend selection belong to the HTTP provider.

mod aead;
#[cfg(not(target_arch = "wasm32"))]
mod native;

use std::sync::Arc;

use baml_crypto_types::{AeadAlgorithm, AeadCipher, AeadError, CryptoProvider};
#[cfg(not(target_arch = "wasm32"))]
use baml_crypto_types::{CryptoError, SHA256_LEN, Sha256Context};

pub fn provider() -> Arc<dyn CryptoProvider> {
    Arc::new(DefaultProvider)
}

struct DefaultProvider;

impl CryptoProvider for DefaultProvider {
    #[cfg(not(target_arch = "wasm32"))]
    fn sha256(&self) -> Result<Box<dyn Sha256Context>, CryptoError> {
        Ok(native::sha256())
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn hmac_sha256(&self, key: &[u8], message: &[u8]) -> Result<[u8; SHA256_LEN], CryptoError> {
        Ok(native::hmac_sha256(key, message))
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn sign_rs256(&self, private_key_pem: &str, message: &[u8]) -> Result<Vec<u8>, CryptoError> {
        native::sign_rs256(private_key_pem, message)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn fill_random(&self, buf: &mut [u8]) -> Result<(), CryptoError> {
        native::fill_random(buf)
    }

    fn aead(&self, algorithm: AeadAlgorithm, key: &[u8]) -> Result<Arc<dyn AeadCipher>, AeadError> {
        aead::aead(algorithm, key)
    }
}
