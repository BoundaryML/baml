//! `baml.crypto`: SHA-256 and the AEAD ciphers, by the crates the
//! interpreter uses for them (`sha2`, `aes-gcm-siv`, `chacha20poly1305`).
//!
//! Every class keeps its state as owned Rust state: a digest in progress
//! behind a mutex, or a cipher, whose key schedule is derived once when it
//! is made. A cipher is given its inputs as copies, taken before it runs.
//!
//! What fails does so as it does in the interpreter: a key, a nonce or an
//! input of the wrong size with `baml.errors.InvalidArgument`, and a
//! ciphertext that does not authenticate with
//! `baml.crypto.DecryptionFailure`.

use std::sync::{Arc, Mutex, PoisonError};

use aes_gcm_siv::{
    KeySizeUser as _,
    aead::{Aead, AeadCore, KeyInit, Nonce, Payload, array::typenum::Unsigned},
};
use sha2::Digest as _;

use super::{
    Bytes, BytesRef, NativeBamlCryptoAeadForAes128GcmSiv, NativeBamlCryptoAeadForAes256GcmSiv,
    NativeBamlCryptoAeadForChaCha20Poly1305, NativeBamlCryptoAeadForXChaCha20Poly1305,
    NativeBamlCryptoAes128GcmSiv, NativeBamlCryptoAes256GcmSiv, NativeBamlCryptoChaCha20Poly1305,
    NativeBamlCryptoHasherForSha256, NativeBamlCryptoSha256, NativeBamlCryptoXChaCha20Poly1305,
    Natives, Obj, ObjRef, Opaque, Root, Scope, Thrown,
};
use crate::layout::{
    NAMED_AES_128_GCM_SIV, NAMED_AES_256_GCM_SIV, NAMED_CHACHA20_POLY1305,
    NAMED_DECRYPTION_FAILURE, NAMED_SHA256, NAMED_XCHACHA20_POLY1305,
};

/// What the messages of one cipher say of it. The sizes of its key, nonce
/// and tag are the cipher type's own.
struct Algorithm {
    name: &'static str,
    /// The longest plaintext its standard permits, in bytes.
    max_plaintext: u64,
    /// The longest associated data its standard permits, in bytes.
    max_aad: u64,
}

const AES_128_GCM_SIV: Algorithm = Algorithm {
    name: "AES-128-GCM-SIV",
    max_plaintext: aes_gcm_siv::P_MAX,
    max_aad: aes_gcm_siv::A_MAX,
};

const AES_256_GCM_SIV: Algorithm = Algorithm {
    name: "AES-256-GCM-SIV",
    max_plaintext: aes_gcm_siv::P_MAX,
    max_aad: aes_gcm_siv::A_MAX,
};

/// The first plaintext length at which the 32-bit block counter would
/// wrap, less one: 2^32 blocks of 64 bytes.
const CHACHA_MAX_PLAINTEXT: u64 = (1 << 38) - 1;

const CHACHA20_POLY1305: Algorithm = Algorithm {
    name: "ChaCha20-Poly1305",
    max_plaintext: CHACHA_MAX_PLAINTEXT,
    max_aad: u64::MAX,
};

const XCHACHA20_POLY1305: Algorithm = Algorithm {
    name: "XChaCha20-Poly1305",
    max_plaintext: CHACHA_MAX_PLAINTEXT,
    max_aad: u64::MAX,
};

/// `nonce` as the nonce `C` takes, or the message of one of another
/// length.
fn nonce_of<'a, C: AeadCore>(
    algorithm: &Algorithm,
    nonce: &'a [u8],
) -> Result<&'a Nonce<C>, String> {
    <&Nonce<C>>::try_from(nonce).map_err(|_| {
        format!(
            "{}: nonce must be exactly {} bytes, got {}",
            algorithm.name,
            C::NonceSize::USIZE,
            nonce.len()
        )
    })
}

/// Why a ciphertext gave no plaintext.
enum OpenError {
    /// The call was malformed, whatever the ciphertext holds.
    Invalid(String),
    /// The ciphertext was rejected, for this reason.
    Rejected(String),
}

/// Authenticate and decrypt `ciphertext`.
fn open<C: Aead>(
    algorithm: &Algorithm,
    cipher: &C,
    nonce: &[u8],
    ciphertext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, OpenError> {
    let nonce = nonce_of::<C>(algorithm, nonce).map_err(OpenError::Invalid)?;
    let tag = C::TagSize::USIZE;
    // Said apart from a tag that does not match: the length is public.
    if ciphertext.len() < tag {
        return Err(OpenError::Rejected(format!(
            "ciphertext is shorter than the {tag}-byte authentication tag"
        )));
    }
    let max_ciphertext = algorithm.max_plaintext.saturating_add(tag as u64);
    if ciphertext.len() as u64 > max_ciphertext || aad.len() as u64 > algorithm.max_aad {
        return Err(OpenError::Invalid(format!(
            "{}: ciphertext ({} bytes) must be at most {max_ciphertext} bytes and aad ({} bytes) \
             at most {} bytes",
            algorithm.name,
            ciphertext.len(),
            aad.len(),
            algorithm.max_aad
        )));
    }
    // Which of the key, the nonce, the data and the ciphertext was wrong
    // is not said: telling them apart would be a decryption oracle.
    cipher
        .decrypt(
            nonce,
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| OpenError::Rejected("authentication failed".to_owned()))
}

/// The natives of one AEAD class over its cipher type. The four differ
/// only in the generated traits, the cipher and what its messages say.
macro_rules! aead_class {
    ($class_trait:ident, $aead_trait:ident, $receiver:ident, $cipher:ty, $algorithm:expr, $named:expr) => {
        impl $class_trait for Natives {
            fn new<'s>(
                scope: &mut Scope<'s>,
                key: Root<'s, Bytes>,
            ) -> Result<Root<'s, Obj>, Thrown<'s>> {
                let key = key.get(scope).to_vec();
                let Ok(cipher) = <$cipher>::new_from_slice(&key) else {
                    return Err(scope.invalid_argument(&format!(
                        "{}: key must be exactly {} bytes, got {}",
                        $algorithm.name,
                        <$cipher>::key_size(),
                        key.len()
                    )));
                };
                let state = scope.alloc_owned(Arc::new(cipher));
                let class = scope.named_class($named);
                let mut built = scope.build(class);
                built.set(scope, 0, state);
                Ok(built.finish())
            }
        }

        impl $aead_trait for Natives {
            fn encrypt<'s>(
                scope: &mut Scope<'s>,
                $receiver: Root<'s, Obj>,
                nonce: Root<'s, Bytes>,
                plaintext: Root<'s, Bytes>,
                aad: Root<'s, Bytes>,
            ) -> Result<Root<'s, Bytes>, Thrown<'s>> {
                let (nonce, plaintext, aad) = (
                    nonce.get(scope).to_vec(),
                    plaintext.get(scope).to_vec(),
                    aad.get(scope).to_vec(),
                );
                // SAFETY: the class has one field, its cipher.
                let cipher = unsafe { $receiver.get(scope).field::<Opaque>(0) }.owned::<$cipher>();
                let sealed = nonce_of::<$cipher>(&$algorithm, &nonce).and_then(|nonce| {
                    cipher
                        .encrypt(
                            nonce,
                            Payload {
                                msg: &plaintext,
                                aad: &aad,
                            },
                        )
                        // An input past the standard's cap is the one way
                        // encryption fails.
                        .map_err(|_| {
                            format!(
                                "{}: plaintext ({} bytes) must be at most {} bytes and aad ({} \
                                 bytes) at most {} bytes",
                                $algorithm.name,
                                plaintext.len(),
                                $algorithm.max_plaintext,
                                aad.len(),
                                $algorithm.max_aad
                            )
                        })
                });
                match sealed {
                    Ok(ciphertext) => Ok(scope.alloc_bytes(&ciphertext)),
                    Err(message) => Err(scope.invalid_argument(&message)),
                }
            }

            fn decrypt<'s>(
                scope: &mut Scope<'s>,
                $receiver: Root<'s, Obj>,
                nonce: Root<'s, Bytes>,
                ciphertext: Root<'s, Bytes>,
                aad: Root<'s, Bytes>,
            ) -> Result<Root<'s, Bytes>, Thrown<'s>> {
                let (nonce, ciphertext, aad) = (
                    nonce.get(scope).to_vec(),
                    ciphertext.get(scope).to_vec(),
                    aad.get(scope).to_vec(),
                );
                // SAFETY: the class has one field, its cipher.
                let cipher = unsafe { $receiver.get(scope).field::<Opaque>(0) }.owned::<$cipher>();
                let reason = match open(&$algorithm, cipher, &nonce, &ciphertext, &aad) {
                    Ok(plaintext) => return Ok(scope.alloc_bytes(&plaintext)),
                    Err(OpenError::Invalid(message)) => {
                        return Err(scope.invalid_argument(&message));
                    }
                    Err(OpenError::Rejected(reason)) => reason,
                };
                let class = scope.named_class(NAMED_DECRYPTION_FAILURE);
                let algorithm = scope.alloc_str($algorithm.name);
                let reason = scope.alloc_str(&reason);
                let mut failure = scope.build(class);
                failure.set(scope, 0, algorithm);
                failure.set(scope, 1, reason);
                let failure = failure.finish();
                Err(scope.throw(failure))
            }
        }
    };
}

aead_class!(
    NativeBamlCryptoAes128GcmSiv,
    NativeBamlCryptoAeadForAes128GcmSiv,
    aes128gcmsiv,
    aes_gcm_siv::Aes128GcmSiv,
    AES_128_GCM_SIV,
    NAMED_AES_128_GCM_SIV
);
aead_class!(
    NativeBamlCryptoAes256GcmSiv,
    NativeBamlCryptoAeadForAes256GcmSiv,
    aes256gcmsiv,
    aes_gcm_siv::Aes256GcmSiv,
    AES_256_GCM_SIV,
    NAMED_AES_256_GCM_SIV
);
aead_class!(
    NativeBamlCryptoChaCha20Poly1305,
    NativeBamlCryptoAeadForChaCha20Poly1305,
    chacha20poly1305,
    chacha20poly1305::ChaCha20Poly1305,
    CHACHA20_POLY1305,
    NAMED_CHACHA20_POLY1305
);
aead_class!(
    NativeBamlCryptoXChaCha20Poly1305,
    NativeBamlCryptoAeadForXChaCha20Poly1305,
    xchacha20poly1305,
    chacha20poly1305::XChaCha20Poly1305,
    XCHACHA20_POLY1305,
    NAMED_XCHACHA20_POLY1305
);

/// A digest in progress. `update` and `finish` both change it, and the
/// object that owns it is shared.
type Sha256State = Mutex<sha2::Sha256>;

impl NativeBamlCryptoSha256 for Natives {
    fn new<'s>(scope: &mut Scope<'s>) -> Root<'s, Obj> {
        let state = scope.alloc_owned(Arc::new(Sha256State::new(sha2::Sha256::new())));
        let class = scope.named_class(NAMED_SHA256);
        let mut hasher = scope.build(class);
        hasher.set(scope, 0, state);
        hasher.finish()
    }
}

impl NativeBamlCryptoHasherForSha256 for Natives {
    fn update(sha256: ObjRef<'_>, data: BytesRef<'_>) {
        // SAFETY: a `Sha256` has one field, its state.
        let state = unsafe { sha256.field::<Opaque>(0) }.owned::<Sha256State>();
        // The bytes are read once, front to back, into a state of a few
        // words: a copy of them would be made only to be thrown away.
        state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .update(data.as_slice());
    }

    fn finish<'s>(scope: &mut Scope<'s>, sha256: Root<'s, Obj>) -> Root<'s, Bytes> {
        // SAFETY: a `Sha256` has one field, its state.
        let state = unsafe { sha256.get(scope).field::<Opaque>(0) }.owned::<Sha256State>();
        // The hasher is left ready for a new message.
        let digest = state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .finalize_reset();
        scope.alloc_bytes(&digest)
    }
}
