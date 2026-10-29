use std::{collections::hash_map::DefaultHasher, hash::Hasher as _};

use bex_vm_types::{OwnedState, Value};

use super::{
    BamlClassHashDefaultHasher, BamlClassHashHasher_for_DefaultHasher, BamlNamespaceHash,
    PackageBamlImpl, copy, view,
};
use crate::{BexVm, errors::VmInternalError};

/// The hash of everything written so far to `hasher`, a
/// `baml.hash.DefaultHasher`, as the VM's own structural hashing reads it.
pub(super) fn finish_state(vm: &BexVm, hasher: Value) -> Result<u64, VmInternalError> {
    let instance = vm.as_rust_instance(&hasher)?;
    Ok(view::hash::DefaultHasher { instance }
        .payload()
        .lock()
        .finish())
}

/// A fresh `baml.hash.DefaultHasher`, with nothing written to it.
pub(super) fn new_state(vm: &mut BexVm) -> Value {
    copy::hash::DefaultHasher {
        payload: Box::new(OwnedState::new(DefaultHasher::new())),
    }
    .to_value(vm)
}

impl BamlClassHashDefaultHasher for PackageBamlImpl {
    fn new(vm: &mut BexVm) -> Value {
        new_state(vm)
    }
}

impl BamlClassHashHasher_for_DefaultHasher for PackageBamlImpl {
    fn write(_vm: &BexVm, hasher: &view::hash::DefaultHasher<'_>, bytes: &[u8]) {
        hasher.payload().lock().write(bytes);
    }

    fn finish(_vm: &BexVm, hasher: &view::hash::DefaultHasher<'_>) -> i64 {
        // Arithmetic shift maps all u64 hashes into BAML's signed 63-bit range.
        hasher.payload().lock().finish().cast_signed() >> 1
    }
}

impl BamlNamespaceHash for PackageBamlImpl {}
