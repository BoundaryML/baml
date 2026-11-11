//! `baml.random` seedable pseudo-random generators (`$rust_function`).
//!
//! The cryptographic [`SystemRandom`] generator is host-backed and lives in the
//! sys-ops path (`sys_native` / `bridge_wasm`); the seedable PRNGs implemented
//! here run inline in the VM.
//!
//! Each generator stores its state behind an `Arc<Mutex<_>>` in the instance's
//! opaque `_state` field. The mutex is what makes drawing sound: a generator
//! value is shared as an immutable `Object::RustData`, so advancing its state
//! requires interior mutability, and concurrent draws from the same generator
//! across `spawn` fibers must be serialized rather than racing on the backing
//! `RngCore` state.

use std::sync::{Arc, Mutex, PoisonError};

use bex_vm_types::types::Value;
use rand_chacha::ChaCha20Rng;
use rand_core::{RngCore, SeedableRng};
use rand_xoshiro::Xoshiro256PlusPlus as XoshiroRng;

use super::{
    BamlClassRandomChaCha20, BamlClassRandomRng_for_ChaCha20,
    BamlClassRandomRng_for_Xoshiro256PlusPlus, BamlClassRandomXoshiro256PlusPlus,
    BamlNamespaceRandom, PackageBamlImpl, copy, view,
};
use crate::{BexVm, errors::VmRustFnError};

use bex_vm_types::random::seed_array;

fn fill<R: RngCore>(mutex: &Mutex<R>, bytes: i64) -> Result<Vec<u8>, VmRustFnError> {
    bex_vm_types::random::fill(
        &mut *mutex.lock().unwrap_or_else(PoisonError::into_inner),
        bytes,
    )
}
fn next_i63<R: RngCore>(mutex: &Mutex<R>) -> i64 {
    bex_vm_types::random::next_i63(&mut *mutex.lock().unwrap_or_else(PoisonError::into_inner))
}

// =========================================================================
// Xoshiro256PlusPlus
// =========================================================================

/// A generator as `$rust_type` data: fixed size.
struct RngState<R>(Mutex<R>);

impl<R: Send + 'static> bex_vm_types::BexRustData for RngState<R> {
    fn measure(&self, _: &mut bex_vm_types::Meter) {}
}

impl BamlClassRandomXoshiro256PlusPlus for PackageBamlImpl {
    fn _new(vm: &mut BexVm, seed: &[u8]) -> Result<Value, VmRustFnError> {
        let rng = XoshiroRng::from_seed(seed_array(seed)?);
        let state: Arc<dyn bex_vm_types::BexRustData> = Arc::new(RngState(Mutex::new(rng)));
        Ok(copy::random::Xoshiro256PlusPlus { _state: state }.to_value(vm))
    }
}

// The `implements random.Rng` block's methods — impl-block methods, so
// codegen emits a per-impl trait.
#[expect(
    clippy::used_underscore_items,
    reason = "the `_state` view accessor is generated from the private BAML field"
)]
impl BamlClassRandomRng_for_Xoshiro256PlusPlus for PackageBamlImpl {
    fn random(
        vm: &BexVm,
        rng: &view::random::Xoshiro256PlusPlus<'_>,
        bytes: i64,
    ) -> Result<Vec<u8>, VmRustFnError> {
        fill(&rng._state::<RngState<XoshiroRng>>(vm).0, bytes)
    }

    fn random_int(vm: &BexVm, rng: &view::random::Xoshiro256PlusPlus<'_>) -> i64 {
        next_i63(&rng._state::<RngState<XoshiroRng>>(vm).0)
    }
}

// =========================================================================
// ChaCha20
// =========================================================================

impl BamlClassRandomChaCha20 for PackageBamlImpl {
    fn _new(vm: &mut BexVm, seed: &[u8]) -> Result<Value, VmRustFnError> {
        let rng = ChaCha20Rng::from_seed(seed_array(seed)?);
        let state: Arc<dyn bex_vm_types::BexRustData> = Arc::new(RngState(Mutex::new(rng)));
        Ok(copy::random::ChaCha20 { _state: state }.to_value(vm))
    }
}

// The `implements random.Rng` block's methods — impl-block methods, so
// codegen emits a per-impl trait.
#[expect(
    clippy::used_underscore_items,
    reason = "the `_state` view accessor is generated from the private BAML field"
)]
impl BamlClassRandomRng_for_ChaCha20 for PackageBamlImpl {
    fn random(
        vm: &BexVm,
        rng: &view::random::ChaCha20<'_>,
        bytes: i64,
    ) -> Result<Vec<u8>, VmRustFnError> {
        fill(&rng._state::<RngState<ChaCha20Rng>>(vm).0, bytes)
    }

    fn random_int(vm: &BexVm, rng: &view::random::ChaCha20<'_>) -> i64 {
        next_i63(&rng._state::<RngState<ChaCha20Rng>>(vm).0)
    }
}

impl BamlNamespaceRandom for PackageBamlImpl {}
