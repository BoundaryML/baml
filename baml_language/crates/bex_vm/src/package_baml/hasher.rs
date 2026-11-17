use std::{
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

/// A hasher in progress as `$rust_type` data: fixed size.
pub(super) struct HasherState(Mutex<DefaultHasher>);

impl bex_vm_types::BexRustData for HasherState {
    fn measure(&self, _: &mut bex_vm_types::Meter) {}
}

impl std::ops::Deref for HasherState {
    type Target = Mutex<DefaultHasher>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub(super) fn finish_state(state: &Arc<HasherState>) -> u64 {
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
        ._state::<HasherState>(vm)
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

impl BamlClassHashDefaultHasher for PackageBamlImpl {
    fn new(vm: &mut BexVm) -> Value {
        new_state(vm).0
    }
}

/// A fresh `baml.hash.DefaultHasher`: the same results in every process.
pub(super) fn new_state(vm: &mut BexVm) -> (Value, Arc<HasherState>) {
    with_hasher(vm, DefaultHasher::new())
}

/// A `baml.hash.DefaultHasher` that computes a map's key hash, for a key
/// whose `Hash` runs BAML code: keyed per process like a native key's hash
/// ([`bex_vm_types::map_key_hasher`]), so both land in the same bucket.
pub(super) fn new_map_key_state(vm: &mut BexVm) -> (Value, Arc<HasherState>) {
    with_hasher(vm, bex_vm_types::map_key_hasher())
}

fn with_hasher(vm: &mut BexVm, hasher: DefaultHasher) -> (Value, Arc<HasherState>) {
    let state = Arc::new(HasherState(Mutex::new(hasher)));
    let opaque: Arc<dyn bex_vm_types::BexRustData> = state.clone();
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
