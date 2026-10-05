use std::{
    any::Any,
    collections::hash_map::DefaultHasher,
    hash::Hasher as _,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use bex_vm_types::Value;

use super::{
    BamlClassHashDefaultHasher, BamlClassHashHasher_for_DefaultHasher, BamlNamespaceHash,
    PackageBamlImpl, copy, view,
};
use crate::BexVm;

pub(super) fn finish_state(state: &Arc<Mutex<DefaultHasher>>) -> u64 {
    state
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .finish()
}

#[expect(
    clippy::used_underscore_items,
    reason = "the `_state` view accessor is generated from the private BAML field"
)]
fn state<'v>(
    hasher: &view::hash::DefaultHasher<'_>,
    vm: &'v BexVm,
) -> MutexGuard<'v, DefaultHasher> {
    hasher
        ._state::<Mutex<DefaultHasher>>(vm)
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

impl BamlClassHashDefaultHasher for PackageBamlImpl {
    fn new(vm: &mut BexVm) -> Value {
        new_state(vm).0
    }
}

pub(super) fn new_state(vm: &mut BexVm) -> (Value, Arc<Mutex<DefaultHasher>>) {
    let state = Arc::new(Mutex::new(DefaultHasher::new()));
    let opaque: Arc<dyn Any + Send + Sync> = state.clone();
    let value = copy::hash::DefaultHasher { _state: opaque }.to_value(vm);
    (value, state)
}

impl BamlClassHashHasher_for_DefaultHasher for PackageBamlImpl {
    fn write(vm: &BexVm, hasher: &view::hash::DefaultHasher<'_>, bytes: &[u8]) {
        state(hasher, vm).write(bytes);
    }

    fn finish(vm: &BexVm, hasher: &view::hash::DefaultHasher<'_>) -> i64 {
        // Arithmetic shift maps all u64 hashes into BAML's signed 63-bit range.
        state(hasher, vm).finish().cast_signed() >> 1
    }
}

impl BamlNamespaceHash for PackageBamlImpl {}
