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

/// A hasher in progress, the state of a `baml.hash.DefaultHasher`: fixed size.
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

fn state<'v>(hasher: &view::hash::DefaultHasher<'v>) -> MutexGuard<'v, DefaultHasher> {
    hasher
        .payload::<HasherState>()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

impl BamlClassHashDefaultHasher for PackageBamlImpl {
    fn new(vm: &mut BexVm) -> Value {
        new_state(vm).0
    }
}

pub(super) fn new_state(vm: &mut BexVm) -> (Value, Arc<HasherState>) {
    let state = Arc::new(HasherState(Mutex::new(DefaultHasher::new())));
    let payload: Arc<dyn bex_vm_types::BexRustData> = state.clone();
    let value = copy::hash::DefaultHasher { payload }.to_value(vm);
    (value, state)
}

impl BamlClassHashHasher_for_DefaultHasher for PackageBamlImpl {
    fn write(_vm: &BexVm, hasher: &view::hash::DefaultHasher<'_>, bytes: &[u8]) {
        state(hasher).write(bytes);
    }

    fn finish(_vm: &BexVm, hasher: &view::hash::DefaultHasher<'_>) -> i64 {
        // Arithmetic shift maps all u64 hashes into BAML's signed 63-bit range.
        state(hasher).finish().cast_signed() >> 1
    }
}

impl BamlNamespaceHash for PackageBamlImpl {}
