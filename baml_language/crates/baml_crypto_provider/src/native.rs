//! AWS-LC on native targets, ring on iOS. No TLS types cross this boundary.

#[cfg(not(target_os = "ios"))]
use aws_lc_rs as backend;
use backend::{
    digest, hmac,
    rand::{SecureRandom, SystemRandom},
    signature,
};
use baml_crypto_types::{CryptoError, SHA1_LEN, SHA256_LEN, Sha256Context};
#[cfg(target_os = "ios")]
use ring as backend;
use zeroize::Zeroizing;

struct Sha256(digest::Context);

pub(super) fn sha1(data: &[u8]) -> [u8; SHA1_LEN] {
    let mut output = [0; SHA1_LEN];
    output.copy_from_slice(digest::digest(&digest::SHA1_FOR_LEGACY_USE_ONLY, data).as_ref());
    output
}

pub(super) fn sha256() -> Box<dyn Sha256Context> {
    Box::new(Sha256(digest::Context::new(&digest::SHA256)))
}

impl Sha256Context for Sha256 {
    fn update(&mut self, data: &[u8]) {
        self.0.update(data);
    }

    fn finish(&mut self) -> [u8; SHA256_LEN] {
        let context = std::mem::replace(&mut self.0, digest::Context::new(&digest::SHA256));
        let mut output = [0; SHA256_LEN];
        output.copy_from_slice(context.finish().as_ref());
        output
    }
}

pub(super) fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; SHA256_LEN] {
    let key = hmac::Key::new(hmac::HMAC_SHA256, key);
    let mut output = [0; SHA256_LEN];
    output.copy_from_slice(hmac::sign(&key, message).as_ref());
    output
}

pub(super) fn sign_rs256(private_key_pem: &str, message: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let (label, der) = pem_rfc7468::decode_vec(private_key_pem.as_bytes())
        .map_err(|_| CryptoError::InvalidKey("expected an unencrypted PEM private key".into()))?;
    let der = Zeroizing::new(der);
    let key = match label {
        "PRIVATE KEY" => signature::RsaKeyPair::from_pkcs8(&der),
        "RSA PRIVATE KEY" => signature::RsaKeyPair::from_der(&der),
        _ => {
            return Err(CryptoError::InvalidKey(
                "expected a PKCS#8 or PKCS#1 RSA private key".into(),
            ));
        }
    }
    .map_err(|_| CryptoError::InvalidKey("RSA private key rejected".into()))?;
    #[cfg(not(target_os = "ios"))]
    let signature_len = key.public_modulus_len();
    #[cfg(target_os = "ios")]
    let signature_len = key.public().modulus_len();
    let mut output = vec![0; signature_len];
    key.sign(
        &signature::RSA_PKCS1_SHA256,
        &SystemRandom::new(),
        message,
        &mut output,
    )
    .map_err(|_| CryptoError::Failed("RS256 signing failed".into()))?;
    Ok(output)
}

pub(super) fn fill_random(buf: &mut [u8]) -> Result<(), CryptoError> {
    SystemRandom::new()
        .fill(buf)
        .map_err(|_| CryptoError::Failed("secure random source failed".into()))
}
