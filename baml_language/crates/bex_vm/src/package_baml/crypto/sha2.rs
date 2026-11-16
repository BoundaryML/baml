//! `baml.crypto.Sha256`, backed by the `sha2` crate.

use std::sync::{Arc, Mutex, PoisonError};

use bex_vm_types::types::Value;
use sha2::Digest;

use super::{super::BamlClassCryptoSha256, BexVm, PackageBamlImpl, copy, view};

/// A digest in progress as `$rust_type` data: fixed size.
struct Sha256State(Mutex<sha2::Sha256>);

impl bex_vm_types::BexRustData for Sha256State {
    fn measure(&self, _: &mut bex_vm_types::Meter) {}
}

impl std::ops::Deref for Sha256State {
    type Target = Mutex<sha2::Sha256>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

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
        ._state::<Sha256State>(vm)
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

impl BamlClassCryptoSha256 for PackageBamlImpl {
    fn new(vm: &mut BexVm) -> Value {
        let state: Arc<dyn bex_vm_types::BexRustData> =
            Arc::new(Sha256State(Mutex::new(sha2::Sha256::new())));
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
