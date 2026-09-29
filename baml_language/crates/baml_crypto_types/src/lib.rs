//! Backend-independent contracts for BAML's non-TLS cryptography.
//!
//! Implementations live in `baml_crypto_provider`. This crate contains no
//! cryptographic implementation or backend dependency.

use std::{fmt, sync::Arc};

pub const SHA1_LEN: usize = 20;
pub const SHA256_LEN: usize = 32;

/// An initialized, incremental SHA-256 computation. After successful creation,
/// updates and finalization are infallible. `finish` resets to an empty message.
pub trait Sha256Context: Send + 'static {
    fn update(&mut self, data: &[u8]);
    fn finish(&mut self) -> [u8; SHA256_LEN];
}

/// A non-TLS crypto failure. Messages must not contain key material or plaintext.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CryptoError {
    Unsupported(&'static str),
    InvalidKey(String),
    Failed(String),
}

impl fmt::Display for CryptoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(operation) => {
                write!(f, "crypto provider does not support {operation}")
            }
            Self::InvalidKey(message) => write!(f, "invalid key: {message}"),
            Self::Failed(message) => write!(f, "crypto provider error: {message}"),
        }
    }
}

impl std::error::Error for CryptoError {}

/// Operations used outside the HTTP transport's TLS stack. Unsupported
/// capabilities return errors; BAML never substitutes a different backend.
pub trait CryptoProvider: Send + Sync + 'static {
    /// SHA-1 for compatibility with the AWS CLI's SSO cache filenames.
    /// This is not used for signatures or integrity verification.
    fn sha1(&self, _data: &[u8]) -> Result<[u8; SHA1_LEN], CryptoError> {
        Err(CryptoError::Unsupported("SHA-1 for AWS SSO cache lookup"))
    }

    fn sha256(&self) -> Result<Box<dyn Sha256Context>, CryptoError> {
        Err(CryptoError::Unsupported("SHA-256"))
    }

    fn hmac_sha256(&self, _key: &[u8], _message: &[u8]) -> Result<[u8; SHA256_LEN], CryptoError> {
        Err(CryptoError::Unsupported("HMAC-SHA256"))
    }

    /// RSASSA-PKCS1-v1_5 with SHA-256. Accept an unencrypted PKCS#8 or PKCS#1
    /// PEM private key and return the raw signature bytes.
    fn sign_rs256(&self, _private_key_pem: &str, _message: &[u8]) -> Result<Vec<u8>, CryptoError> {
        Err(CryptoError::Unsupported("RS256 signing"))
    }

    /// Fill the complete buffer with cryptographically secure random bytes.
    /// The caller discards the buffer on error.
    fn fill_random(&self, _buf: &mut [u8]) -> Result<(), CryptoError> {
        Err(CryptoError::Unsupported("secure randomness"))
    }

    fn aead(
        &self,
        algorithm: AeadAlgorithm,
        _key: &[u8],
    ) -> Result<Arc<dyn AeadCipher>, AeadError> {
        Err(AeadError::Unsupported(algorithm))
    }
}

/// The algorithms exposed by `baml.crypto`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AeadAlgorithm {
    Aes128GcmSiv,
    Aes256GcmSiv,
    ChaCha20Poly1305,
    XChaCha20Poly1305,
}

impl AeadAlgorithm {
    pub fn name(self) -> &'static str {
        match self {
            Self::Aes128GcmSiv => "AES-128-GCM-SIV",
            Self::Aes256GcmSiv => "AES-256-GCM-SIV",
            Self::ChaCha20Poly1305 => "ChaCha20-Poly1305",
            Self::XChaCha20Poly1305 => "XChaCha20-Poly1305",
        }
    }

    pub fn key_len(self) -> usize {
        match self {
            Self::Aes128GcmSiv => 16,
            _ => 32,
        }
    }

    pub fn nonce_len(self) -> usize {
        match self {
            Self::XChaCha20Poly1305 => 24,
            _ => 12,
        }
    }

    pub fn tag_len(self) -> usize {
        16
    }

    pub fn max_plaintext(self) -> u64 {
        match self {
            // RFC 8452 §6: https://www.rfc-editor.org/rfc/rfc8452#section-6
            Self::Aes128GcmSiv | Self::Aes256GcmSiv => 1 << 36,
            // RFC 8439 §2.8: block zero derives the authentication key, leaving
            // 2^32 - 1 blocks of 64 bytes. XChaCha uses the same counter layout.
            // https://www.rfc-editor.org/rfc/rfc8439#section-2.8
            Self::ChaCha20Poly1305 | Self::XChaCha20Poly1305 => (1 << 38) - 64,
        }
    }

    pub fn max_aad(self) -> u64 {
        match self {
            Self::Aes128GcmSiv | Self::Aes256GcmSiv => 1 << 36,
            Self::ChaCha20Poly1305 | Self::XChaCha20Poly1305 => u64::MAX,
        }
    }

    pub fn validate_key(self, key: &[u8]) -> Result<(), AeadError> {
        if key.len() != self.key_len() {
            return Err(AeadError::InvalidArgument(format!(
                "{}: key must be exactly {} bytes, got {}",
                self.name(),
                self.key_len(),
                key.len()
            )));
        }
        Ok(())
    }

    fn validate_nonce(self, nonce: &[u8]) -> Result<(), AeadError> {
        if nonce.len() != self.nonce_len() {
            return Err(AeadError::InvalidArgument(format!(
                "{}: nonce must be exactly {} bytes, got {}",
                self.name(),
                self.nonce_len(),
                nonce.len()
            )));
        }
        Ok(())
    }

    pub fn validate_encrypt(
        self,
        nonce: &[u8],
        plaintext: &[u8],
        aad: &[u8],
    ) -> Result<(), AeadError> {
        self.validate_nonce(nonce)?;
        if plaintext.len() as u64 > self.max_plaintext() || aad.len() as u64 > self.max_aad() {
            return Err(AeadError::InvalidArgument(format!(
                "{}: plaintext ({} bytes) must be at most {} bytes and aad ({} bytes) at most {} bytes",
                self.name(),
                plaintext.len(),
                self.max_plaintext(),
                aad.len(),
                self.max_aad()
            )));
        }
        Ok(())
    }

    pub fn validate_decrypt(
        self,
        nonce: &[u8],
        ciphertext: &[u8],
        aad: &[u8],
    ) -> Result<(), AeadError> {
        self.validate_nonce(nonce)?;
        if ciphertext.len() < self.tag_len() {
            return Err(AeadError::CiphertextTooShort);
        }
        let max_ciphertext = self.max_plaintext().saturating_add(self.tag_len() as u64);
        if ciphertext.len() as u64 > max_ciphertext || aad.len() as u64 > self.max_aad() {
            return Err(AeadError::InvalidArgument(format!(
                "{}: ciphertext ({} bytes) must be at most {max_ciphertext} bytes and aad ({} bytes) at most {} bytes",
                self.name(),
                ciphertext.len(),
                aad.len(),
                self.max_aad()
            )));
        }
        Ok(())
    }
}

/// Errors distinguish invalid arguments, unsupported algorithms, operational
/// failures, and rejected ciphertext. Never include secret data in messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AeadError {
    Unsupported(AeadAlgorithm),
    InvalidArgument(String),
    /// The ciphertext does not contain a complete authentication tag.
    CiphertextTooShort,
    /// Do not distinguish wrong keys, nonces, AAD, or altered ciphertext.
    AuthenticationFailed,
    /// A backend failure unrelated to authentication or argument validation.
    Failed(String),
}

impl fmt::Display for AeadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(algorithm) => write!(
                f,
                "{} is unavailable in this crypto provider",
                algorithm.name()
            ),
            Self::InvalidArgument(message) | Self::Failed(message) => f.write_str(message),
            Self::CiphertextTooShort => {
                f.write_str("ciphertext is shorter than the 16-byte authentication tag")
            }
            Self::AuthenticationFailed => f.write_str("authentication failed"),
        }
    }
}

impl std::error::Error for AeadError {}

/// An initialized cipher. Implementations must be safe for concurrent calls,
/// retain their key securely, and erase key material when dropped where the
/// backend supports it. The caller supplies every nonce; providers must not
/// generate one or change the requested algorithm.
pub trait AeadCipher: Send + Sync + 'static {
    /// Return ciphertext with its 16-byte authentication tag appended.
    fn encrypt(&self, nonce: &[u8], plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>, AeadError>;

    /// Authenticate before returning any plaintext. On authentication failure,
    /// return `AuthenticationFailed` without identifying which input was wrong.
    fn decrypt(&self, nonce: &[u8], ciphertext: &[u8], aad: &[u8]) -> Result<Vec<u8>, AeadError>;
}
