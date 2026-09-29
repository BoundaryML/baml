//! Integration-test provider: ring primitives, with no TLS or AEAD dependencies.
use std::sync::Arc;

use baml_crypto_types::{CryptoError, CryptoProvider, SHA256_LEN, Sha256Context};
use ring::{digest, hmac, rand::{SecureRandom, SystemRandom}, signature};
use zeroize::Zeroizing;

pub fn provider() -> Arc<dyn CryptoProvider> { Arc::new(RingProvider) }
struct RingProvider;
struct Hasher(digest::Context);

impl Sha256Context for Hasher {
    fn update(&mut self, data: &[u8]) { self.0.update(data); }
    fn finish(&mut self) -> [u8; SHA256_LEN] {
        let old = std::mem::replace(&mut self.0, digest::Context::new(&digest::SHA256));
        old.finish().as_ref().try_into().unwrap()
    }
}

impl CryptoProvider for RingProvider {
    fn sha256(&self) -> Result<Box<dyn Sha256Context>, CryptoError> {
        Ok(Box::new(Hasher(digest::Context::new(&digest::SHA256))))
    }
    fn hmac_sha256(&self, key: &[u8], message: &[u8]) -> Result<[u8; SHA256_LEN], CryptoError> {
        Ok(hmac::sign(&hmac::Key::new(hmac::HMAC_SHA256, key), message).as_ref().try_into().unwrap())
    }
    fn sign_rs256(&self, pem: &str, message: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let (label, der) = pem_rfc7468::decode_vec(pem.as_bytes())
            .map_err(|_| CryptoError::InvalidKey("invalid PEM".into()))?;
        let der = Zeroizing::new(der);
        let key = match label {
            "PRIVATE KEY" => signature::RsaKeyPair::from_pkcs8(&der),
            "RSA PRIVATE KEY" => signature::RsaKeyPair::from_der(&der),
            _ => return Err(CryptoError::InvalidKey("unsupported PEM label".into())),
        }.map_err(|_| CryptoError::InvalidKey("invalid RSA key".into()))?;
        let mut out = vec![0; key.public().modulus_len()];
        key.sign(&signature::RSA_PKCS1_SHA256, &SystemRandom::new(), message, &mut out)
            .map_err(|_| CryptoError::Failed("signing failed".into()))?;
        Ok(out)
    }
    fn fill_random(&self, out: &mut [u8]) -> Result<(), CryptoError> {
        SystemRandom::new().fill(out).map_err(|_| CryptoError::Failed("random failed".into()))
    }
}
