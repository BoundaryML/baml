use std::sync::{Arc, Mutex};

use bex_events::{
    ids::{BoundaryId, RuntimeId},
    prof::backend::{ScopeContextPatch, ScopeValue},
};
use bex_vm_types::types::{Object, Value};

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
    pub context: ScopeContextPatch,
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
            context: self.context.clone(),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ConsumedLocalId {
    pub boundary_id: BoundaryId,
    pub encoded: String,
    pub capture: LocalIdCaptureOverrides,
    pub context: ScopeContextPatch,
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
            context: ScopeContextPatch::default(),
            consumed: false,
        };
        Ok(alloc_local_id(vm, state))
    }
}

impl BamlClassLocalId for PackageBoundaryImpl {
    fn context(
        vm: &mut BexVm,
        localid: &Value,
        metadata: &indexmap::IndexMap<bex_str::BexStr, Value>,
        distinct_id: Option<&bex_str::BexStr>,
    ) -> Result<Value, VmRustFnError> {
        let mut patch = ScopeContextPatch {
            distinct_id: distinct_id.map(ToString::to_string),
            ..ScopeContextPatch::default()
        };
        for (key, value) in metadata {
            let value = if value.is_null() {
                None
            } else if let Some(value) = value.as_int() {
                Some(ScopeValue::Int(value))
            } else if let Some(value) = value.as_bool() {
                Some(ScopeValue::Bool(value))
            } else {
                match value.as_object_ptr().map(|ptr| vm.get_object(ptr)) {
                    Some(Object::String(value)) => Some(ScopeValue::String(value.to_string())),
                    Some(Object::Float(value)) => Some(ScopeValue::Float(*value)),
                    _ => {
                        return Err(VmBamlError::InvalidArgument {
                            message:
                                "scope metadata values must be string, int, float, bool, or null"
                                    .to_string(),
                        }
                        .into());
                    }
                }
            };
            patch.metadata.insert(key.to_string(), value);
        }
        let mut guard =
            local_id_state(vm, *localid)?
                .lock()
                .map_err(|_| VmBamlError::InvalidArgument {
                    message: "boundary.LocalId state is unavailable".to_string(),
                })?;
        if guard.consumed {
            return Err(VmBamlError::InvalidArgument {
                message: "cannot change context after a boundary.LocalId has been consumed"
                    .to_string(),
            }
            .into());
        }
        guard.context.compose(patch);
        Ok(*localid)
    }

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

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn native_context_validation_is_atomic_and_preserves_integer_range() {
        let program = baml_db::testing::compile_source("function main() -> int { 0 }");
        let mut vm =
            BexVm::from_program(program, Arc::new(std::sync::atomic::AtomicBool::new(false)))
                .unwrap();
        let id = PackageBoundaryImpl::id(&mut vm).unwrap();
        let values = indexmap::IndexMap::from([
            ("min".into(), Value::int(Value::INT_MIN)),
            ("max".into(), Value::int(Value::INT_MAX)),
        ]);
        PackageBoundaryImpl::context(&mut vm, &id, &values, None).unwrap();
        let before = local_id_state(&vm, id)
            .unwrap()
            .lock()
            .unwrap()
            .context
            .clone();
        assert_eq!(
            before.metadata["min"],
            Some(ScopeValue::Int(Value::INT_MIN))
        );
        assert_eq!(
            before.metadata["max"],
            Some(ScopeValue::Int(Value::INT_MAX))
        );
        let invalid =
            indexmap::IndexMap::from([("min".into(), Value::int(0)), ("invalid".into(), id)]);
        assert!(PackageBoundaryImpl::context(&mut vm, &id, &invalid, None).is_err());
        assert_eq!(
            local_id_state(&vm, id).unwrap().lock().unwrap().context,
            before
        );
        let consumed = consume_local_id(&vm, id).unwrap();
        assert_eq!(consumed.context, before);
    }
}
