//! `baml.crypto` authenticated ciphers (`$rust_function`).
//!
//! Both AES-GCM-SIV key sizes are backed by the `aes-gcm-siv` crate. A BAML
//! cipher instance stores the expanded cipher in its opaque `_cipher` field, so
//! the AES key schedule is derived once at `new` rather than per message, and
//! the raw key never becomes a BAML value. No BAML expression produces a
//! `$rust_type`, so `Aes256GcmSiv.new` is the only way to build a cipher, and a
//! wrong-length key cannot get past it.
//!
//! Unqualified `Aes128GcmSiv` / `Aes256GcmSiv` below are the `aes-gcm-siv`
//! types. The BAML shells around them are always written out as
//! `view::crypto::...` / `copy::crypto::...`.
//!
//! # Error split
//!
//! `baml.errors.InvalidArgument` means the call was malformed: a wrong-length
//! nonce, or an input past an RFC 8452 size cap. That is a bug in the calling
//! program, independent of the ciphertext's contents.
//! `baml.crypto.DecryptionFailure` means this ciphertext did not authenticate.
//! The two never overlap, so a caller can retry the first and must not retry the
//! second.

use std::{any::Any, sync::Arc};

use aes_gcm_siv::{
    A_MAX, C_MAX, KeyInit, Nonce, P_MAX,
    aead::{Aead, AeadCore, Payload, consts::U12},
};
use bex_heap::TlabHolder;
use bex_vm_types::types::Value;

use super::{
    BamlClassCryptoAes128GcmSiv, BamlClassCryptoAes256GcmSiv, BamlNamespaceCrypto, PackageBamlImpl,
    copy, view,
};
use crate::{
    BexVm,
    errors::{VmBamlError, VmInternalError, VmRustFnError},
};

/// The 96-bit nonce length RFC 8452 fixes for every AES-GCM-SIV key size.
const NONCE_LEN: usize = 12;

/// The 128-bit authentication tag appended to every ciphertext.
const TAG_LEN: usize = 16;

/// Algorithm name reported in `InvalidArgument` messages and in
/// `DecryptionFailure.algorithm`.
const ALG_128: &str = "AES-128-GCM-SIV";
/// Algorithm name reported in `InvalidArgument` messages and in
/// `DecryptionFailure.algorithm`.
const ALG_256: &str = "AES-256-GCM-SIV";

/// Fully-qualified name of the class thrown when a ciphertext is rejected.
const DECRYPTION_FAILURE_FQN: &str = "baml.crypto.DecryptionFailure";

fn invalid_argument(message: String) -> VmRustFnError {
    VmRustFnError::BamlError(VmBamlError::InvalidArgument { message })
}

/// Expand `key` into a cipher, rejecting a key that is not the algorithm's key
/// length.
///
/// This is the only path to a `baml.crypto` cipher instance, so every cipher
/// that exists was built from a correctly sized key.
fn build_cipher<C: KeyInit>(algorithm: &str, key: &[u8]) -> Result<C, VmRustFnError> {
    C::new_from_slice(key).map_err(|_| {
        invalid_argument(format!(
            "{algorithm}: key must be exactly {} bytes, got {}",
            C::key_size(),
            key.len()
        ))
    })
}

/// Borrow `nonce` as the fixed-size nonce both ciphers take.
fn nonce_array<'a>(algorithm: &str, nonce: &'a [u8]) -> Result<&'a Nonce, VmRustFnError> {
    <&Nonce>::try_from(nonce).map_err(|_| {
        invalid_argument(format!(
            "{algorithm}: nonce must be exactly {NONCE_LEN} bytes, got {}",
            nonce.len()
        ))
    })
}

/// Encrypt `plaintext`, returning the ciphertext with its tag appended.
fn seal<C>(
    algorithm: &str,
    cipher: &C,
    nonce: &[u8],
    plaintext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, VmRustFnError>
where
    C: Aead + AeadCore<NonceSize = U12>,
{
    let nonce = nonce_array(algorithm, nonce)?;
    cipher
        .encrypt(
            nonce,
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| {
            // The RFC 8452 §6 caps are the cipher's only encryption failure
            // mode. `encrypt_inout_detached` rejects an over-long plaintext or
            // aad, and the postfix-tag `encrypt_in_place` wrapper around it
            // cannot fail for the `Vec` buffer `Aead::encrypt` hands it.
            invalid_argument(format!(
                "{algorithm}: plaintext ({} bytes) and aad ({} bytes) must each be \
                 at most {P_MAX} bytes",
                plaintext.len(),
                aad.len()
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
    Rejected(&'static str),
}

/// Authenticate and decrypt `ciphertext`.
fn open<C>(
    algorithm: &str,
    cipher: &C,
    nonce: &[u8],
    ciphertext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, OpenError>
where
    C: Aead + AeadCore<NonceSize = U12>,
{
    let nonce = nonce_array(algorithm, nonce).map_err(OpenError::Invalid)?;
    // Reported separately from a tag mismatch because it is a fact the caller
    // already holds. A ciphertext's length is public and attacker-chosen, so
    // naming it reveals nothing, while "authentication failed" would point a
    // caller at a key mismatch instead.
    if ciphertext.len() < TAG_LEN {
        return Err(OpenError::Rejected(
            "ciphertext is shorter than the 16-byte authentication tag",
        ));
    }
    if ciphertext.len() as u64 > C_MAX || aad.len() as u64 > A_MAX {
        return Err(OpenError::Invalid(invalid_argument(format!(
            "{algorithm}: ciphertext ({} bytes) must be at most {C_MAX} bytes and \
             aad ({} bytes) at most {A_MAX} bytes",
            ciphertext.len(),
            aad.len()
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
        .map_err(|_| OpenError::Rejected("authentication failed"))
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
    let reason = Value::object(vm.alloc_string(reason.to_string()));
    VmRustFnError::Thrown(Value::object(
        vm.alloc_instance(class_ptr, vec![algorithm, reason]),
    ))
}

// =========================================================================
// Aes256GcmSiv
// =========================================================================

#[expect(
    clippy::used_underscore_items,
    reason = "the `_cipher` view accessor is generated from the private BAML field"
)]
impl BamlClassCryptoAes256GcmSiv for PackageBamlImpl {
    fn new(vm: &mut BexVm, key: &[u8]) -> Result<Value, VmRustFnError> {
        let cipher: aes_gcm_siv::Aes256GcmSiv = build_cipher(ALG_256, key)?;
        let state: Arc<dyn Any + Send + Sync> = Arc::new(cipher);
        Ok(copy::crypto::Aes256GcmSiv { _cipher: state }.to_value(vm))
    }

    fn encrypt(
        vm: &BexVm,
        cipher: &view::crypto::Aes256GcmSiv<'_>,
        nonce: &[u8],
        plaintext: &[u8],
        aad: &[u8],
    ) -> Result<Vec<u8>, VmRustFnError> {
        seal(
            ALG_256,
            cipher._cipher::<aes_gcm_siv::Aes256GcmSiv>(vm),
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
        // The view borrows `vm` shared-ly, so decryption runs in an inner scope.
        // `open_error` needs `&mut vm` to allocate the `DecryptionFailure`
        // instance, which cannot coexist with that borrow.
        let opened = {
            let view = view::crypto::Aes256GcmSiv {
                instance: vm.as_instance(cipher)?,
            };
            open(
                ALG_256,
                view._cipher::<aes_gcm_siv::Aes256GcmSiv>(vm),
                nonce,
                ciphertext,
                aad,
            )
        };
        opened.map_err(|e| open_error(vm, ALG_256, e))
    }
}

// =========================================================================
// Aes128GcmSiv
// =========================================================================

#[expect(
    clippy::used_underscore_items,
    reason = "the `_cipher` view accessor is generated from the private BAML field"
)]
impl BamlClassCryptoAes128GcmSiv for PackageBamlImpl {
    fn new(vm: &mut BexVm, key: &[u8]) -> Result<Value, VmRustFnError> {
        let cipher: aes_gcm_siv::Aes128GcmSiv = build_cipher(ALG_128, key)?;
        let state: Arc<dyn Any + Send + Sync> = Arc::new(cipher);
        Ok(copy::crypto::Aes128GcmSiv { _cipher: state }.to_value(vm))
    }

    fn encrypt(
        vm: &BexVm,
        cipher: &view::crypto::Aes128GcmSiv<'_>,
        nonce: &[u8],
        plaintext: &[u8],
        aad: &[u8],
    ) -> Result<Vec<u8>, VmRustFnError> {
        seal(
            ALG_128,
            cipher._cipher::<aes_gcm_siv::Aes128GcmSiv>(vm),
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
        // See `Aes256GcmSiv::decrypt` for why the borrow is scoped.
        let opened = {
            let view = view::crypto::Aes128GcmSiv {
                instance: vm.as_instance(cipher)?,
            };
            open(
                ALG_128,
                view._cipher::<aes_gcm_siv::Aes128GcmSiv>(vm),
                nonce,
                ciphertext,
                aad,
            )
        };
        opened.map_err(|e| open_error(vm, ALG_128, e))
    }
}

impl BamlNamespaceCrypto for PackageBamlImpl {}
