use bex_events::ids::{BexCallId, BexThreadId, BoundaryId, CallRef, DecodeError, RuntimeId};

use super::{BamlNamespaceId, PackageBamlImpl};
use crate::{
    VmPanic,
    errors::{VmBamlError, VmRustFnError},
    vm::BexVm,
};

impl BamlNamespaceId for PackageBamlImpl {
    fn current(vm: &BexVm) -> bex_str::BexStr {
        current_runtime_id(vm).map_or_else(
            || bex_str::BexStr::from(""),
            |id| bex_str::BexStr::from(id.as_str()),
        )
    }

    fn new() -> Result<bex_str::BexStr, VmRustFnError> {
        let mut id = [0u8; 16];
        getrandom::getrandom(&mut id).map_err(|e| VmPanic::HostUnavailable {
            resource: "entropy".to_string(),
            message: format!("getrandom failed in baml.id.new: {e}"),
        })?;
        Ok(bex_str::BexStr::from(
            RuntimeId::Boundary(BoundaryId::from_bytes(id))
                .encode()
                .as_str(),
        ))
    }

    fn set(vm: &mut BexVm, id: &bex_str::BexStr) -> Result<bex_str::BexStr, VmRustFnError> {
        let id = id.to_string();
        let runtime_id = RuntimeId::decode(&id).map_err(|e| invalid_id_error(&id, &e))?;
        let RuntimeId::Boundary(_) = runtime_id else {
            return Err(VmBamlError::InvalidArgument {
                message: "baml.id.set expects a boundary ID created by baml.id.new()".to_string(),
            }
            .into());
        };

        let call_id = vm.current_call_id();
        if call_id == 0 || vm.bex_ref_seed.is_none() {
            return Err(VmBamlError::InvalidArgument {
                message: "baml.id.set is only available while a BEX function is running"
                    .to_string(),
            }
            .into());
        }

        if let Some(top) = vm.id_overrides.last_mut()
            && top.0 == call_id
        {
            top.1.clone_from(&id);
        } else {
            vm.id_overrides.push((call_id, id.clone()));
        }

        Ok(bex_str::BexStr::from(id.as_str()))
    }
}

/// The current call's runtime id: the override if one was set for this call,
/// otherwise the call's `CallRef`, encoded on demand. `None` outside a call
/// (or before the engine attached identity).
pub(crate) fn current_runtime_id(vm: &BexVm) -> Option<String> {
    let call_id = vm.current_call_id();
    if call_id == 0 {
        return None;
    }
    if let Some((override_call, encoded)) = vm.id_overrides.last()
        && *override_call == call_id
    {
        return Some(encoded.clone());
    }
    let (process_euid, engine_id) = vm.bex_ref_seed?;
    Some(
        CallRef {
            process_euid,
            engine_id,
            thread_id: BexThreadId(vm.thread_id),
            call_id: BexCallId(call_id),
        }
        .encode(),
    )
}

fn invalid_id_error(id: &str, source: &DecodeError) -> VmRustFnError {
    VmBamlError::InvalidArgument {
        message: format!("invalid BEX runtime ID `{id}`: {source}"),
    }
    .into()
}
