//! AES-GCM-SIV and ChaCha20-Poly1305: `baml.crypto.Aead` implementations.
//!
//! A cipher instance holds the expanded cipher, so the AES key schedule is
//! derived once at `new` rather than per message, and the raw key never becomes
//! a BAML value. A wrong-length key cannot get past `new`.
//!
//! # Error split
//!
//! `baml.errors.InvalidArgument` means the call was malformed: a wrong-length
//! nonce, or an input past an RFC 8452 size cap. That is a bug in the calling
//! program, independent of the ciphertext's contents.
//! `baml.crypto.DecryptionFailure` means this ciphertext did not authenticate.
//! The two never overlap, so a caller can retry the first and must not retry the
//! second.

use std::sync::Arc;

use aes_gcm_siv::aead::{
    Aead, AeadCore, KeyInit, Payload,
    array::typenum::Unsigned,
    {self},
};
use bex_heap::TlabHolder;
use bex_vm_types::types::Value;

use super::{
    super::{
        BamlClassCryptoAead_for_Aes128GcmSiv, BamlClassCryptoAead_for_Aes256GcmSiv,
        BamlClassCryptoAead_for_ChaCha20Poly1305, BamlClassCryptoAead_for_XChaCha20Poly1305,
        BamlClassCryptoAes128GcmSiv, BamlClassCryptoAes256GcmSiv, BamlClassCryptoChaCha20Poly1305,
        BamlClassCryptoXChaCha20Poly1305,
    },
    BexVm, PackageBamlImpl, VmRustFnError, copy, invalid_argument, view,
};

/// Fully-qualified name of the class thrown when a ciphertext is rejected.
const DECRYPTION_FAILURE: baml_base::LangItem = crate::lang_items::BAML_CRYPTO_DECRYPTION_FAILURE;

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
    let class_ptr = vm.lang_object(DECRYPTION_FAILURE);
    let algorithm = Value::object(vm.alloc_string(algorithm.to_string()));
    let reason = Value::object(vm.alloc_string(reason));
    VmRustFnError::thrown_fresh(Value::object(
        vm.alloc_instance(class_ptr, vec![algorithm, reason]),
    ))
}

// =========================================================================
// AEAD ciphers
// =========================================================================

/// Implement one BAML AEAD class over its backing cipher type.
///
/// Every cipher's `new` / `encrypt` / `decrypt` bodies are identical; only the
/// generated traits, the class shell, the cipher type, and the [`Algorithm`]
/// descriptor differ. Codegen emits a class trait (`new`) plus a per-impl
/// `Aead_for_*` trait (`encrypt` / `decrypt` — the `implements crypto.Aead`
/// block's methods), so a blanket impl cannot express this and a macro is
/// what keeps the four from drifting apart the way four hand-written copies
/// would.
/// A cipher as `$rust_type` data: a key schedule of fixed size.
struct AeadCipher<C>(C);

impl<C: Send + Sync + 'static> bex_vm_types::BexRustData for AeadCipher<C> {
    fn measure(&self, _: &mut bex_vm_types::Meter) {}
}

macro_rules! impl_aead_class {
    ($class_trait:ident, $aead_trait:ident, $class:ident, $cipher:ty, $algorithm:expr) => {
        impl $class_trait for PackageBamlImpl {
            fn new(vm: &mut BexVm, key: &[u8]) -> Result<Value, VmRustFnError> {
                let cipher: $cipher = build_cipher(&$algorithm, key)?;
                let state: Arc<dyn bex_vm_types::BexRustData> = Arc::new(AeadCipher(cipher));
                Ok(copy::crypto::$class { _cipher: state }.to_value(vm))
            }
        }

        #[expect(
            clippy::used_underscore_items,
            reason = "the `_cipher` view accessor is generated from the private BAML field"
        )]
        impl $aead_trait for PackageBamlImpl {
            fn encrypt(
                vm: &BexVm,
                cipher: &view::crypto::$class<'_>,
                nonce: &[u8],
                plaintext: &[u8],
                aad: &[u8],
            ) -> Result<Vec<u8>, VmRustFnError> {
                seal(
                    &$algorithm,
                    &cipher._cipher::<AeadCipher<$cipher>>(vm).0,
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
                        &view._cipher::<AeadCipher<$cipher>>(vm).0,
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
    BamlClassCryptoAead_for_Aes128GcmSiv,
    Aes128GcmSiv,
    aes_gcm_siv::Aes128GcmSiv,
    AES_128_GCM_SIV
);
impl_aead_class!(
    BamlClassCryptoAes256GcmSiv,
    BamlClassCryptoAead_for_Aes256GcmSiv,
    Aes256GcmSiv,
    aes_gcm_siv::Aes256GcmSiv,
    AES_256_GCM_SIV
);
impl_aead_class!(
    BamlClassCryptoChaCha20Poly1305,
    BamlClassCryptoAead_for_ChaCha20Poly1305,
    ChaCha20Poly1305,
    chacha20poly1305::ChaCha20Poly1305,
    CHACHA20_POLY1305
);
impl_aead_class!(
    BamlClassCryptoXChaCha20Poly1305,
    BamlClassCryptoAead_for_XChaCha20Poly1305,
    XChaCha20Poly1305,
    chacha20poly1305::XChaCha20Poly1305,
    XCHACHA20_POLY1305
);
