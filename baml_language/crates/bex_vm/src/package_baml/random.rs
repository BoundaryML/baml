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
    BamlClassRandomChaCha20, BamlClassRandomXoshiro256PlusPlus, BamlNamespaceRandom,
    PackageBamlImpl, copy, view,
};
use crate::{
    BexVm,
    errors::{VmPanic, VmRustFnError},
};

/// Minimum seed length, in bytes, for the seedable PRNGs.
const SEED_LEN: usize = 32;

/// Materialize a 32-byte seed from a BAML `uint8array`, panicking if it is too
/// short. Bytes beyond the first 32 are ignored.
fn seed_array(seed: &[u8]) -> Result<[u8; SEED_LEN], VmRustFnError> {
    let head = seed.get(..SEED_LEN).ok_or_else(|| {
        VmRustFnError::Panic(VmPanic::UserPanic {
            message: format!(
                "Rng seed must be at least {SEED_LEN} bytes, got {}",
                seed.len()
            ),
        })
    })?;
    let mut arr = [0u8; SEED_LEN];
    arr.copy_from_slice(head);
    Ok(arr)
}

/// Convert a `bytes` count into a buffer length, panicking if it is negative
/// (per the `Rng.random` contract).
fn byte_count(bytes: i64) -> Result<usize, VmRustFnError> {
    usize::try_from(bytes).map_err(|_| {
        VmRustFnError::Panic(VmPanic::UserPanic {
            message: format!("Rng.random: byte count must be non-negative, got {bytes}"),
        })
    })
}

/// Draw `n` random bytes from a locked generator.
///
/// Allocates fallibly (`try_reserve`) so an unsatisfiable request surfaces as a
/// catchable [`VmPanic::AllocFailure`] rather than aborting the host process via
/// the global allocator's OOM handler.
fn fill<R: RngCore>(mutex: &Mutex<R>, n: usize) -> Result<Vec<u8>, VmRustFnError> {
    let mut buf = Vec::new();
    buf.try_reserve(n).map_err(|_| VmPanic::AllocFailure {
        message: format!("Rng.random: allocation of {n} bytes failed"),
    })?;
    buf.resize(n, 0u8);
    mutex
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .fill_bytes(&mut buf);
    Ok(buf)
}

/// Draw a uniformly random BAML `int` (i63) from a locked generator.
///
/// `next_u64` is uniform over all 64 bits; reinterpreting it as `i64` and
/// arithmetic-shifting right by one maps it uniformly onto `[INT_MIN, INT_MAX]`
/// (every i63 value has exactly two u64 preimages), which always fits in
/// `Value::int`.
fn next_i63<R: RngCore>(mutex: &Mutex<R>) -> i64 {
    let r = mutex
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .next_u64();
    r.cast_signed() >> 1
}

// =========================================================================
// Xoshiro256PlusPlus
// =========================================================================

#[expect(
    clippy::used_underscore_items,
    reason = "the `_state` view accessor is generated from the private BAML field"
)]
impl BamlClassRandomXoshiro256PlusPlus for PackageBamlImpl {
    fn _new(vm: &mut BexVm, seed: &[u8]) -> Result<Value, VmRustFnError> {
        let rng = XoshiroRng::from_seed(seed_array(seed)?);
        let state: Arc<dyn std::any::Any + Send + Sync> = Arc::new(Mutex::new(rng));
        Ok(copy::random::Xoshiro256PlusPlus { _state: state }.to_value(vm))
    }

    fn random(
        vm: &BexVm,
        rng: &view::random::Xoshiro256PlusPlus<'_>,
        bytes: i64,
    ) -> Result<Vec<u8>, VmRustFnError> {
        let n = byte_count(bytes)?;
        fill(rng._state::<Mutex<XoshiroRng>>(vm), n)
    }

    fn random_int(vm: &BexVm, rng: &view::random::Xoshiro256PlusPlus<'_>) -> i64 {
        next_i63(rng._state::<Mutex<XoshiroRng>>(vm))
    }
}

// =========================================================================
// ChaCha20
// =========================================================================

#[expect(
    clippy::used_underscore_items,
    reason = "the `_state` view accessor is generated from the private BAML field"
)]
impl BamlClassRandomChaCha20 for PackageBamlImpl {
    fn _new(vm: &mut BexVm, seed: &[u8]) -> Result<Value, VmRustFnError> {
        let rng = ChaCha20Rng::from_seed(seed_array(seed)?);
        let state: Arc<dyn std::any::Any + Send + Sync> = Arc::new(Mutex::new(rng));
        Ok(copy::random::ChaCha20 { _state: state }.to_value(vm))
    }

    fn random(
        vm: &BexVm,
        rng: &view::random::ChaCha20<'_>,
        bytes: i64,
    ) -> Result<Vec<u8>, VmRustFnError> {
        let n = byte_count(bytes)?;
        fill(rng._state::<Mutex<ChaCha20Rng>>(vm), n)
    }

    fn random_int(vm: &BexVm, rng: &view::random::ChaCha20<'_>) -> i64 {
        next_i63(rng._state::<Mutex<ChaCha20Rng>>(vm))
    }
}

impl BamlNamespaceRandom for PackageBamlImpl {}
