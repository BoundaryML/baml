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
pub(super) struct HasherState(Mutex<Algorithm>);

/// What a `baml.hash.DefaultHasher` computes.
pub(super) enum Algorithm {
    /// `baml.hash.DefaultHasher.new()`: the same result in every process.
    Default(DefaultHasher),
    /// A map's key hash: the stream is buffered and hashed once by
    /// [`bex_vm_types::map_key_hash`] (keyed per process), so a key whose
    /// `Hash` runs BAML code lands in the same bucket as a native key that
    /// writes the same bytes.
    MapKey(Vec<u8>),
}

impl Algorithm {
    pub(super) fn write(&mut self, bytes: &[u8]) {
        match self {
            Self::Default(hasher) => hasher.write(bytes),
            Self::MapKey(stream) => stream.extend_from_slice(bytes),
        }
    }

    pub(super) fn finish(&self) -> u64 {
        match self {
            Self::Default(hasher) => hasher.finish(),
            Self::MapKey(stream) => bex_vm_types::map_key_hash(stream),
        }
    }
}

impl bex_vm_types::BexRustData for HasherState {
    fn measure(&self, _: &mut bex_vm_types::Meter) {}
}

impl std::ops::Deref for HasherState {
    type Target = Mutex<Algorithm>;
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
fn state<'v>(hasher: &view::hash::DefaultHasher<'_>, vm: &'v BexVm) -> MutexGuard<'v, Algorithm> {
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

/// A fresh `baml.hash.DefaultHasher`.
pub(super) fn new_state(vm: &mut BexVm) -> (Value, Arc<HasherState>) {
    with_algorithm(vm, Algorithm::Default(DefaultHasher::new()))
}

/// A `baml.hash.DefaultHasher` that computes a map's key hash, for a key
/// whose `Hash` runs BAML code.
pub(super) fn new_map_key_state(vm: &mut BexVm) -> (Value, Arc<HasherState>) {
    with_algorithm(vm, Algorithm::MapKey(Vec::new()))
}

fn with_algorithm(vm: &mut BexVm, algorithm: Algorithm) -> (Value, Arc<HasherState>) {
    let state = Arc::new(HasherState(Mutex::new(algorithm)));
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
