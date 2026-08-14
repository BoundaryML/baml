//! `baml.crypto` ciphers and hashes (`$rust_function`).
//!
//! The AES-GCM-SIV ciphers are backed by the `aes-gcm-siv` crate and `Sha256` by
//! the `sha2` crate. Every class here keeps its runtime state in an opaque
//! `$rust_type` field, which puts that state out of BAML's reach: no BAML
//! expression produces a `$rust_type`, so the class constructor is the only way
//! to build one.
//!
//! Crate types that share a name with the BAML class wrapping them are written
//! out in full (`aes_gcm_siv::Aes256GcmSiv`, `sha2::Sha256`) so they are never
//! ambiguous with the generated `view::crypto::` / `copy::crypto::` shells.
//!
//! # Ciphers
//!
//! A cipher instance holds the expanded cipher, so the AES key schedule is
//! derived once at `new` rather than per message, and the raw key never becomes
//! a BAML value. A wrong-length key cannot get past `new`.
//!
//! ## Error split
//!
//! `baml.errors.InvalidArgument` means the call was malformed: a wrong-length
//! nonce, or an input past an RFC 8452 size cap. That is a bug in the calling
//! program, independent of the ciphertext's contents.
//! `baml.crypto.DecryptionFailure` means this ciphertext did not authenticate.
//! The two never overlap, so a caller can retry the first and must not retry the
//! second.
//!
//! # Hashes
//!
//! A hasher instance holds its compression state behind a mutex. Feeding and
//! finishing both need `&mut` on the hasher, but a BAML value is shared as an
//! immutable `Object::RustData`, so mutation has to go through interior
//! mutability, and concurrent `update` calls on one hasher across `spawn` fibers
//! must be serialized rather than racing on the compression state.

use std::{
    any::Any,
    sync::{Arc, Mutex, PoisonError},
};

use aes_gcm_siv::aead::{
    Aead, AeadCore, KeyInit, Payload,
    array::typenum::Unsigned,
    {self},
};
use bex_heap::TlabHolder;
use bex_vm_types::types::Value;
use sha2::Digest;

use super::{
    BamlClassCryptoAes128GcmSiv, BamlClassCryptoAes256GcmSiv, BamlClassCryptoChaCha20Poly1305,
    BamlClassCryptoSha256, BamlClassCryptoXChaCha20Poly1305, BamlNamespaceCrypto, PackageBamlImpl,
    copy, view,
};
use crate::{
    BexVm,
    errors::{VmBamlError, VmInternalError, VmRustFnError},
};

/// Fully-qualified name of the class thrown when a ciphertext is rejected.
const DECRYPTION_FAILURE_FQN: &str = "baml.crypto.DecryptionFailure";

/// The per-algorithm facts the shared seal and open paths report on.
///
/// Key, nonce, and tag lengths are not here: they come from the cipher type's
/// own associated types, so they cannot drift from the implementation. Only the
/// display name and the standard's length caps have to be stated.
struct Algorithm {
    /// Name reported in `InvalidArgument` messages and `DecryptionFailure.algorithm`.
    name: &'static str,
    /// Longest plaintext the standard permits, in bytes.
    max_plaintext: u64,
    /// Longest associated data the standard permits, in bytes.
    max_aad: u64,
}

/// RFC 8452 §6 caps both the plaintext and the associated data at 2^36 bytes.
const AES_128_GCM_SIV: Algorithm = Algorithm {
    name: "AES-128-GCM-SIV",
    max_plaintext: aes_gcm_siv::P_MAX,
    max_aad: aes_gcm_siv::A_MAX,
};

/// See [`AES_128_GCM_SIV`]; the caps do not depend on the key size.
const AES_256_GCM_SIV: Algorithm = Algorithm {
    name: "AES-256-GCM-SIV",
    max_plaintext: aes_gcm_siv::P_MAX,
    max_aad: aes_gcm_siv::A_MAX,
};

/// The first plaintext length at which the stream cipher's 32-bit block counter
/// would wrap: 2^32 blocks of 64 bytes. `chacha20poly1305` rejects at exactly
/// this bound, so the largest accepted plaintext is one byte below it.
const CHACHA_MAX_PLAINTEXT: u64 = (1 << 38) - 1;

/// RFC 8439 bounds the plaintext by that counter and puts no practical cap on
/// associated data, whose length only has to fit the `u64` the Poly1305 length
/// block encodes.
const CHACHA20_POLY1305: Algorithm = Algorithm {
    name: "ChaCha20-Poly1305",
    max_plaintext: CHACHA_MAX_PLAINTEXT,
    max_aad: u64::MAX,
};

/// See [`CHACHA20_POLY1305`]; the extended nonce changes how the subkey is
/// derived, not the message caps.
const XCHACHA20_POLY1305: Algorithm = Algorithm {
    name: "XChaCha20-Poly1305",
    max_plaintext: CHACHA_MAX_PLAINTEXT,
    max_aad: u64::MAX,
};

fn invalid_argument(message: String) -> VmRustFnError {
    VmRustFnError::BamlError(VmBamlError::InvalidArgument { message })
}

/// Expand `key` into a cipher, rejecting a key that is not the algorithm's key
/// length.
///
/// This is the only path to a `baml.crypto` cipher instance, so every cipher
/// that exists was built from a correctly sized key.
fn build_cipher<C: KeyInit>(algorithm: &Algorithm, key: &[u8]) -> Result<C, VmRustFnError> {
    C::new_from_slice(key).map_err(|_| {
        invalid_argument(format!(
            "{}: key must be exactly {} bytes, got {}",
            algorithm.name,
            C::key_size(),
            key.len()
        ))
    })
}

/// Borrow `nonce` as the fixed-size nonce `C` takes, rejecting any other length.
///
/// The expected length comes from `C::NonceSize`, so the 12-byte and 24-byte
/// ciphers share this one check and neither can drift from its cipher.
fn nonce_array<'a, C: AeadCore>(
    algorithm: &Algorithm,
    nonce: &'a [u8],
) -> Result<&'a aead::Nonce<C>, VmRustFnError> {
    <&aead::Nonce<C>>::try_from(nonce).map_err(|_| {
        invalid_argument(format!(
            "{}: nonce must be exactly {} bytes, got {}",
            algorithm.name,
            C::NonceSize::USIZE,
            nonce.len()
        ))
    })
}

/// Encrypt `plaintext`, returning the ciphertext with its tag appended.
fn seal<C: Aead>(
    algorithm: &Algorithm,
    cipher: &C,
    nonce: &[u8],
    plaintext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, VmRustFnError> {
    let nonce = nonce_array::<C>(algorithm, nonce)?;
    cipher
        .encrypt(
            nonce,
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| {
            // An over-long plaintext or aad is the only encryption failure mode
            // either family has: both reject on their own caps before touching
            // the message, and the postfix-tag `encrypt_in_place` wrapper around
            // that cannot fail for the `Vec` buffer `Aead::encrypt` hands it.
            invalid_argument(format!(
                "{}: plaintext ({} bytes) must be at most {} bytes and aad ({} bytes) \
                 at most {} bytes",
                algorithm.name,
                plaintext.len(),
                algorithm.max_plaintext,
                aad.len(),
                algorithm.max_aad
            ))
        })
}

/// Why an authenticated-decryption attempt produced no plaintext.
///
/// Split out from `VmRustFnError` because a rejection has to be reported as a
/// `DecryptionFailure` instance, which cannot be allocated while the cipher's
/// heap instance is still borrowed. [`open`] classifies the failure and the
/// caller allocates once the borrow is released.
enum OpenError {
    /// The call was malformed independently of the ciphertext's contents.
    Invalid(VmRustFnError),
    /// The ciphertext was rejected, with a coarse reason.
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
    let nonce = nonce_array::<C>(algorithm, nonce).map_err(OpenError::Invalid)?;
    let tag_len = C::TagSize::USIZE;
    // Reported separately from a tag mismatch because it is a fact the caller
    // already holds. A ciphertext's length is public and attacker-chosen, so
    // naming it reveals nothing, while "authentication failed" would point a
    // caller at a key mismatch instead.
    if ciphertext.len() < tag_len {
        return Err(OpenError::Rejected(format!(
            "ciphertext is shorter than the {tag_len}-byte authentication tag"
        )));
    }
    let max_ciphertext = algorithm.max_plaintext.saturating_add(tag_len as u64);
    if ciphertext.len() as u64 > max_ciphertext || aad.len() as u64 > algorithm.max_aad {
        return Err(OpenError::Invalid(invalid_argument(format!(
            "{}: ciphertext ({} bytes) must be at most {max_ciphertext} bytes and \
             aad ({} bytes) at most {} bytes",
            algorithm.name,
            ciphertext.len(),
            aad.len(),
            algorithm.max_aad
        ))));
    }
    cipher
        .decrypt(
            nonce,
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        // Every remaining rejection is a tag-verification failure. Which of the
        // key, nonce, aad, or ciphertext was wrong is not reported: an
        // implementation that distinguished them would be a decryption oracle.
        .map_err(|_| OpenError::Rejected("authentication failed".to_string()))
}

/// Convert an [`OpenError`] into the error the BAML `decrypt` contract declares,
/// allocating the `DecryptionFailure` instance when the ciphertext was rejected.
fn open_error(vm: &mut BexVm, algorithm: &str, err: OpenError) -> VmRustFnError {
    let reason = match err {
        OpenError::Invalid(e) => return e,
        OpenError::Rejected(reason) => reason,
    };
    let Some(class_ptr) = vm.lookup_type_by_fqn(DECRYPTION_FAILURE_FQN) else {
        return VmRustFnError::InternalError(VmInternalError::MissingNativeFunction {
            name: DECRYPTION_FAILURE_FQN.to_string(),
        });
    };
    let algorithm = Value::object(vm.alloc_string(algorithm.to_string()));
    let reason = Value::object(vm.alloc_string(reason));
    VmRustFnError::Thrown(Value::object(
        vm.alloc_instance(class_ptr, vec![algorithm, reason]),
    ))
}

// =========================================================================
// AEAD ciphers
// =========================================================================

/// Implement one BAML AEAD class over its backing cipher type.
///
/// Every cipher's `new` / `encrypt` / `decrypt` bodies are identical; only the
/// generated trait, the class shell, the cipher type, and the [`Algorithm`]
/// descriptor differ. Codegen emits a separate trait per class, so a blanket
/// impl cannot express this and a macro is what keeps the four from drifting
/// apart the way four hand-written copies would.
macro_rules! impl_aead_class {
    ($trait_name:ident, $class:ident, $cipher:ty, $algorithm:expr) => {
        #[expect(
            clippy::used_underscore_items,
            reason = "the `_cipher` view accessor is generated from the private BAML field"
        )]
        impl $trait_name for PackageBamlImpl {
            fn new(vm: &mut BexVm, key: &[u8]) -> Result<Value, VmRustFnError> {
                let cipher: $cipher = build_cipher(&$algorithm, key)?;
                let state: Arc<dyn Any + Send + Sync> = Arc::new(cipher);
                Ok(copy::crypto::$class { _cipher: state }.to_value(vm))
            }

            fn encrypt(
                vm: &BexVm,
                cipher: &view::crypto::$class<'_>,
                nonce: &[u8],
                plaintext: &[u8],
                aad: &[u8],
            ) -> Result<Vec<u8>, VmRustFnError> {
                seal(
                    &$algorithm,
                    cipher._cipher::<$cipher>(vm),
                    nonce,
                    plaintext,
                    aad,
                )
            }

            fn decrypt(
                vm: &mut BexVm,
                cipher: &Value,
                nonce: &[u8],
                ciphertext: &[u8],
                aad: &[u8],
            ) -> Result<Vec<u8>, VmRustFnError> {
                // The view borrows `vm` shared-ly, so decryption runs in an
                // inner scope. `open_error` needs `&mut vm` to allocate the
                // `DecryptionFailure` instance, which cannot coexist with that
                // borrow.
                let opened = {
                    let view = view::crypto::$class {
                        instance: vm.as_instance(cipher)?,
                    };
                    open(
                        &$algorithm,
                        view._cipher::<$cipher>(vm),
                        nonce,
                        ciphertext,
                        aad,
                    )
                };
                opened.map_err(|e| open_error(vm, $algorithm.name, e))
            }
        }
    };
}

impl_aead_class!(
    BamlClassCryptoAes128GcmSiv,
    Aes128GcmSiv,
    aes_gcm_siv::Aes128GcmSiv,
    AES_128_GCM_SIV
);
impl_aead_class!(
    BamlClassCryptoAes256GcmSiv,
    Aes256GcmSiv,
    aes_gcm_siv::Aes256GcmSiv,
    AES_256_GCM_SIV
);
impl_aead_class!(
    BamlClassCryptoChaCha20Poly1305,
    ChaCha20Poly1305,
    chacha20poly1305::ChaCha20Poly1305,
    CHACHA20_POLY1305
);
impl_aead_class!(
    BamlClassCryptoXChaCha20Poly1305,
    XChaCha20Poly1305,
    chacha20poly1305::XChaCha20Poly1305,
    XCHACHA20_POLY1305
);

// =========================================================================
// Sha256
// =========================================================================

/// Lock a hasher's state, recovering from a poisoned mutex.
///
/// A panic can only reach the mutex through `sha2`'s own compression, which does
/// not panic, so poisoning means the process is already unwinding. Recovering
/// keeps a panicking fiber from turning every other holder of the same hasher
/// into a second panic; the state is a partial digest either way, and a partial
/// digest is exactly what an interrupted `update` sequence should leave behind.
fn lock_hasher<'v>(
    hasher: &view::crypto::Sha256<'_>,
    vm: &'v BexVm,
) -> std::sync::MutexGuard<'v, sha2::Sha256> {
    #[expect(
        clippy::used_underscore_items,
        reason = "the `_state` view accessor is generated from the private BAML field"
    )]
    hasher
        ._state::<Mutex<sha2::Sha256>>(vm)
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

impl BamlClassCryptoSha256 for PackageBamlImpl {
    fn new(vm: &mut BexVm) -> Value {
        let state: Arc<dyn Any + Send + Sync> = Arc::new(Mutex::new(sha2::Sha256::new()));
        copy::crypto::Sha256 { _state: state }.to_value(vm)
    }

    fn update(vm: &BexVm, hasher: &view::crypto::Sha256<'_>, data: &[u8]) {
        lock_hasher(hasher, vm).update(data);
    }

    fn finish(vm: &BexVm, hasher: &view::crypto::Sha256<'_>) -> Vec<u8> {
        // `finalize_reset` rather than `finalize` because the BAML contract says
        // `finish` leaves the hasher ready for a new message. `finalize` would
        // need to consume the hasher, which the shared `Object::RustData` behind
        // `_state` cannot give up.
        lock_hasher(hasher, vm).finalize_reset().to_vec()
    }
}

impl BamlNamespaceCrypto for PackageBamlImpl {}
