use std::sync::{Arc, Mutex};

use bex_events::ids::{BoundaryId, RuntimeId};
use bex_vm_types::types::Value;

use super::{BamlClassLocalId, BamlNamespaceId, BamlPackageBoundary, PackageBoundaryImpl};
use crate::{
    BexVm, VmPanic,
    errors::{VmBamlError, VmRustFnError},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LocalIdCaptureOverrides {
    pub inputs: Option<bool>,
    pub output: Option<bool>,
    pub error: Option<bool>,
}

#[derive(Debug)]
pub(crate) struct LocalIdState {
    pub boundary_id: BoundaryId,
    pub encoded: String,
    pub capture: LocalIdCaptureOverrides,
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
            boundary_id: self.boundary_id,
            encoded: self.encoded.clone(),
            capture: self.capture,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConsumedLocalId {
    pub boundary_id: BoundaryId,
    pub encoded: String,
    pub capture: LocalIdCaptureOverrides,
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
            boundary_id,
            encoded,
            capture: LocalIdCaptureOverrides::default(),
            consumed: false,
        };
        Ok(alloc_local_id(vm, state))
    }
}

impl BamlClassLocalId for PackageBoundaryImpl {
    fn capture(
        vm: &mut BexVm,
        localid: &Value,
        inputs: Option<bool>,
        output: Option<bool>,
        error: Option<bool>,
    ) -> Result<Value, VmRustFnError> {
        let state = local_id_state(vm, *localid)?;
        let mut guard = state.lock().map_err(|_| VmBamlError::InvalidArgument {
            message: "boundary.LocalId state is unavailable".to_string(),
        })?;
        if guard.consumed {
            return Err(VmBamlError::InvalidArgument {
                message: "cannot change capture policy after a boundary.LocalId has been consumed"
                    .to_string(),
            }
            .into());
        }
        if let Some(inputs) = inputs {
            guard.capture.inputs = Some(inputs);
        }
        if let Some(output) = output {
            guard.capture.output = Some(output);
        }
        if let Some(error) = error {
            guard.capture.error = Some(error);
        }
        Ok(*localid)
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
