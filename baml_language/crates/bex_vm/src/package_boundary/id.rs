use std::sync::{Arc, Mutex};

use bex_events::ids::{BoundaryId, RuntimeId};
use bex_vm_types::types::Value;

use super::{BamlClassLocalId, BamlNamespaceId, BamlPackageBoundary, PackageBoundaryImpl};
use crate::{
    BexVm, VmPanic,
    errors::{VmBamlError, VmRustFnError},
};

#[derive(Debug)]
pub(crate) struct LocalIdState {
    pub encoded: String,
    pub consumed: bool,
}

impl LocalIdState {
    pub(crate) fn consume(&mut self) -> Result<ConsumedLocalId, VmRustFnError> {
        if self.consumed {
            return Err(VmBamlError::InvalidArgument {
                message: "boundary.LocalId values are single-use and have already been consumed"
                    .to_string(),
            }
            .into());
        }
        self.consumed = true;
        Ok(ConsumedLocalId {
            encoded: self.encoded.clone(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConsumedLocalId {
    pub encoded: String,
}

impl BamlNamespaceId for PackageBoundaryImpl {
    fn current(vm: &BexVm) -> bex_str::BexStr {
        crate::package_baml::id::current_runtime_id(vm).map_or_else(
            || bex_str::BexStr::from(""),
            |id| bex_str::BexStr::from(id.as_str()),
        )
    }
}

impl BamlPackageBoundary for PackageBoundaryImpl {
    fn id(vm: &mut BexVm) -> Result<Value, VmRustFnError> {
        let mut id = [0u8; 16];
        getrandom::getrandom(&mut id).map_err(|e| VmPanic::HostUnavailable {
            resource: "entropy".to_string(),
            message: format!("getrandom failed in boundary.id: {e}"),
        })?;
        let boundary_id = BoundaryId::from_bytes(id);
        let encoded = RuntimeId::Boundary(boundary_id).encode();
        let state = LocalIdState {
            encoded,
            consumed: false,
        };
        Ok(alloc_local_id(vm, state))
    }
}

impl BamlClassLocalId for PackageBoundaryImpl {
    fn capture(
        _vm: &mut BexVm,
        _localid: &Value,
        _inputs: Option<bool>,
        _output: Option<bool>,
        _error: Option<bool>,
    ) -> Result<Value, VmRustFnError> {
        Err(VmBamlError::InvalidArgument {
            message: "profiling capture is unavailable: the old runtime tracing pipeline has been removed".to_string(),
        }.into())
    }
}

pub(crate) fn consume_local_id(vm: &BexVm, value: Value) -> Result<ConsumedLocalId, VmRustFnError> {
    let state = local_id_state(vm, value)?;
    let mut guard = state.lock().map_err(|_| VmBamlError::InvalidArgument {
        message: "boundary.LocalId state is unavailable".to_string(),
    })?;
    guard.consume()
}

fn alloc_local_id(vm: &mut BexVm, state: LocalIdState) -> Value {
    super::copy::LocalId {
        _handle: Arc::new(Mutex::new(state)),
    }
    .to_value(vm)
}

fn local_id_state(vm: &BexVm, value: Value) -> Result<&Mutex<LocalIdState>, VmRustFnError> {
    let instance = vm.as_instance(&value)?;
    let handle = instance.load_field(0);
    vm.as_rust_data::<Mutex<LocalIdState>>(&handle)
        .map_err(VmRustFnError::from)
}
