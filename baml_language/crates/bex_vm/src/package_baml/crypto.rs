//! BAML crypto values and error translation. AEAD implementations come from
//! `baml_crypto_provider`; the VM has no fallback ciphers. Native SHA-256 uses
//! the selected TLS provider, while browser builds retain the portable hasher.

use std::{
    any::Any,
    sync::{Arc, Mutex, PoisonError},
};

use baml_crypto_types::{AeadAlgorithm, AeadCipher, AeadError};
use bex_heap::TlabHolder;
use bex_vm_types::types::Value;

use super::{
    BamlClassCryptoAead_for_Aes128GcmSiv, BamlClassCryptoAead_for_Aes256GcmSiv,
    BamlClassCryptoAead_for_ChaCha20Poly1305, BamlClassCryptoAead_for_XChaCha20Poly1305,
    BamlClassCryptoAes128GcmSiv, BamlClassCryptoAes256GcmSiv, BamlClassCryptoChaCha20Poly1305,
    BamlClassCryptoHasher_for_Sha256, BamlClassCryptoSha256, BamlClassCryptoXChaCha20Poly1305,
    BamlNamespaceCrypto, PackageBamlImpl, copy, view,
};
use crate::{
    BexVm,
    errors::{VmBamlError, VmInternalError, VmRustFnError},
};

/// Fully-qualified name of the class thrown when a ciphertext is rejected.
const DECRYPTION_FAILURE_FQN: &str = "baml.crypto.DecryptionFailure";

fn operation_error(error: AeadError) -> VmRustFnError {
    match error {
        AeadError::InvalidArgument(message) => {
            VmRustFnError::BamlError(VmBamlError::InvalidArgument { message })
        }
        AeadError::Unsupported(_) => VmRustFnError::BamlError(VmBamlError::Unsupported {
            message: error.to_string(),
        }),
        _ => VmRustFnError::BamlError(VmBamlError::Io {
            message: error.to_string(),
        }),
    }
}

fn seal(
    algorithm: AeadAlgorithm,
    cipher: &dyn AeadCipher,
    nonce: &[u8],
    plaintext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, VmRustFnError> {
    algorithm
        .validate_encrypt(nonce, plaintext, aad)
        .map_err(operation_error)?;
    cipher
        .encrypt(nonce, plaintext, aad)
        .map_err(operation_error)
}

fn open(
    algorithm: AeadAlgorithm,
    cipher: &dyn AeadCipher,
    nonce: &[u8],
    ciphertext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, AeadError> {
    algorithm.validate_decrypt(nonce, ciphertext, aad)?;
    cipher.decrypt(nonce, ciphertext, aad)
}

/// Allocate the language-level exception only after releasing the cipher's VM borrow.
fn open_error(vm: &mut BexVm, algorithm: &str, error: AeadError) -> VmRustFnError {
    let reason = match error {
        AeadError::AuthenticationFailed | AeadError::CiphertextTooShort => error.to_string(),
        _ => return operation_error(error),
    };
    let Some(class_ptr) = vm.lookup_type_by_fqn(DECRYPTION_FAILURE_FQN) else {
        return VmRustFnError::InternalError(VmInternalError::MissingNativeFunction {
            name: DECRYPTION_FAILURE_FQN.to_string(),
        });
    };
    let algorithm = Value::object(vm.alloc_string(algorithm.to_string()));
    let reason = Value::object(vm.alloc_string(reason));
    VmRustFnError::thrown_fresh(Value::object(
        vm.alloc_instance(class_ptr, vec![algorithm, reason]),
    ))
}

// =========================================================================
// AEAD ciphers
// =========================================================================

/// Implement the generated class and AEAD traits using the provider's cipher.
/// Only the generated names and requested algorithm differ between classes.
macro_rules! impl_aead_class {
    ($class_trait:ident, $aead_trait:ident, $class:ident, $algorithm:expr) => {
        impl $class_trait for PackageBamlImpl {
            fn new(vm: &mut BexVm, key: &[u8]) -> Result<Value, VmRustFnError> {
                $algorithm.validate_key(key).map_err(operation_error)?;
                let cipher = baml_crypto::aead($algorithm, key).map_err(operation_error)?;
                let state: Arc<dyn Any + Send + Sync> = Arc::new(cipher);
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
                    $algorithm,
                    cipher._cipher::<Arc<dyn AeadCipher>>(vm).as_ref(),
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
                        $algorithm,
                        view._cipher::<Arc<dyn AeadCipher>>(vm).as_ref(),
                        nonce,
                        ciphertext,
                        aad,
                    )
                };
                opened.map_err(|e| open_error(vm, $algorithm.name(), e))
            }
        }
    };
}

impl_aead_class!(
    BamlClassCryptoAes128GcmSiv,
    BamlClassCryptoAead_for_Aes128GcmSiv,
    Aes128GcmSiv,
    AeadAlgorithm::Aes128GcmSiv
);
impl_aead_class!(
    BamlClassCryptoAes256GcmSiv,
    BamlClassCryptoAead_for_Aes256GcmSiv,
    Aes256GcmSiv,
    AeadAlgorithm::Aes256GcmSiv
);
impl_aead_class!(
    BamlClassCryptoChaCha20Poly1305,
    BamlClassCryptoAead_for_ChaCha20Poly1305,
    ChaCha20Poly1305,
    AeadAlgorithm::ChaCha20Poly1305
);
impl_aead_class!(
    BamlClassCryptoXChaCha20Poly1305,
    BamlClassCryptoAead_for_XChaCha20Poly1305,
    XChaCha20Poly1305,
    AeadAlgorithm::XChaCha20Poly1305
);

// =========================================================================
// Sha256
// =========================================================================

/// Native `baml.crypto.Sha256` always uses the selected crypto provider.
#[cfg(not(target_arch = "wasm32"))]
mod sha256_impl {
    use crate::errors::{VmBamlError, VmRustFnError};

    pub(super) type State = baml_crypto::Sha256;

    pub(super) fn new() -> Result<State, VmRustFnError> {
        baml_crypto::Sha256::new().map_err(|e| {
            VmRustFnError::BamlError(VmBamlError::Unsupported {
                message: format!("Sha256: {e}"),
            })
        })
    }

    pub(super) fn update(state: &mut State, data: &[u8]) {
        state.update(data);
    }

    pub(super) fn finish(state: &mut State) -> Vec<u8> {
        state.finish().to_vec()
    }
}

#[cfg(target_arch = "wasm32")]
mod sha256_impl {
    use sha2::Digest;

    use crate::errors::VmRustFnError;

    pub(super) type State = sha2::Sha256;

    #[allow(
        clippy::unnecessary_wraps,
        reason = "matches the provider-backed hasher, which can fail"
    )]
    pub(super) fn new() -> Result<State, VmRustFnError> {
        Ok(sha2::Sha256::new())
    }

    pub(super) fn update(state: &mut State, data: &[u8]) {
        state.update(data);
    }

    // `finalize_reset` rather than `finalize` because the BAML contract says
    // `finish` leaves the hasher ready for a new message. `finalize` would
    // need to consume the hasher, which the shared `Object::RustData` behind
    // `_state` cannot give up.
    pub(super) fn finish(state: &mut State) -> Vec<u8> {
        state.finalize_reset().to_vec()
    }
}

/// Lock a hasher's state, recovering from a poisoned mutex.
///
/// A panic can only reach the mutex through the hash's own compression, which
/// does not panic, so poisoning means the process is already unwinding. Recovering
/// keeps a panicking fiber from turning every other holder of the same hasher
/// into a second panic; the state is a partial digest either way, and a partial
/// digest is exactly what an interrupted `update` sequence should leave behind.
fn lock_hasher<'v>(
    hasher: &view::crypto::Sha256<'_>,
    vm: &'v BexVm,
) -> std::sync::MutexGuard<'v, sha256_impl::State> {
    #[expect(
        clippy::used_underscore_items,
        reason = "the `_state` view accessor is generated from the private BAML field"
    )]
    hasher
        ._state::<Mutex<sha256_impl::State>>(vm)
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

impl BamlClassCryptoSha256 for PackageBamlImpl {
    fn new(vm: &mut BexVm) -> Result<Value, VmRustFnError> {
        let state: Arc<dyn Any + Send + Sync> = Arc::new(Mutex::new(sha256_impl::new()?));
        Ok(copy::crypto::Sha256 { _state: state }.to_value(vm))
    }
}

impl BamlClassCryptoHasher_for_Sha256 for PackageBamlImpl {
    fn update(vm: &BexVm, hasher: &view::crypto::Sha256<'_>, data: &[u8]) {
        sha256_impl::update(&mut lock_hasher(hasher, vm), data);
    }

    fn finish(vm: &BexVm, hasher: &view::crypto::Sha256<'_>) -> Vec<u8> {
        sha256_impl::finish(&mut lock_hasher(hasher, vm))
    }
}

impl BamlNamespaceCrypto for PackageBamlImpl {}
