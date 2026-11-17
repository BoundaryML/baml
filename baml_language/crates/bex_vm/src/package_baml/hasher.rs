use std::{collections::hash_map::DefaultHasher, hash::Hasher as _, sync::Arc};

use bex_vm_types::{AllocDebt, MeteredGuard, MeteredMutex, RetainedFootprint, Value};

use super::{
    BamlClassHashDefaultHasher, BamlClassHashHasher_for_DefaultHasher, BamlNamespaceHash,
    PackageBamlImpl, copy, view,
};
use crate::BexVm;

/// A hasher in progress as `$rust_type` data. A map-key hasher grows with
/// what it is written, so its buffer is charged and measured.
pub(super) struct HasherState(MeteredMutex<Algorithm>);

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

impl RetainedFootprint for Algorithm {
    fn retained_bytes(&self) -> usize {
        match self {
            Self::Default(_) => 0,
            Self::MapKey(stream) => stream.capacity(),
        }
    }
}

impl bex_vm_types::BexRustData for HasherState {
    fn measure(&self, meter: &mut bex_vm_types::Meter) {
        meter.bytes(self.0.retained_bytes());
    }
}

pub(super) fn finish_state(state: &Arc<HasherState>, vm: &BexVm) -> u64 {
    finish_charged(state, vm.tlab.alloc_debt())
}

/// Finish, charging `debt` what the state grew since its last charge: the
/// last write's growth, which no later lock would charge.
fn finish_charged(state: &HasherState, debt: &AllocDebt) -> u64 {
    state.0.lock(debt).finish()
}

#[expect(
    clippy::used_underscore_items,
    reason = "the `_state` view accessor is generated from the private BAML field"
)]
fn state<'v>(hasher: &view::hash::DefaultHasher<'_>, vm: &'v BexVm) -> MeteredGuard<'v, Algorithm> {
    // Charges the VM's allocation account what the state grew since the last lock.
    hasher
        ._state::<HasherState>(vm)
        .0
        .lock(vm.tlab.alloc_debt())
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
    let state = Arc::new(HasherState(MeteredMutex::new(algorithm)));
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

#[cfg(test)]
mod tests {
    use bex_vm_types::{AllocDebt, BexRustData, Meter, MeteredMutex};

    use super::{Algorithm, HasherState, finish_charged};

    /// A map-key hasher keeps what it is written until it finishes, so its
    /// buffer is reported to a census and, by the time the map has its hash,
    /// charged to the VM's account, even for a key whose `Hash` wrote once.
    #[test]
    fn a_map_key_hasher_reports_and_charges_its_buffer() {
        let state = HasherState(MeteredMutex::new(Algorithm::MapKey(Vec::new())));
        let debt = AllocDebt::new();
        state.0.lock(&debt).write(&[7; 4096]);

        let mut meter = Meter::census();
        state.measure(&mut meter);
        assert!(meter.total() >= 4096, "measured {}", meter.total());

        finish_charged(&state, &debt);
        assert!(debt.balance() >= 4096, "charged {}", debt.balance());
    }

    /// `baml.hash.DefaultHasher.new()` streams, so it keeps nothing.
    #[test]
    fn a_default_hasher_keeps_nothing() {
        let state = HasherState(MeteredMutex::new(Algorithm::Default(
            std::collections::hash_map::DefaultHasher::new(),
        )));
        state.0.lock(&AllocDebt::new()).write(&[7; 4096]);
        let mut meter = Meter::census();
        state.measure(&mut meter);
        assert_eq!(meter.total(), 0);
    }
}
