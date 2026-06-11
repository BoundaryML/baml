use bex_events::{
    DiskEventV1,
    ids::{DecodeError, RuntimeId},
};

use super::{BamlNamespaceId, PackageBamlImpl};
use crate::{
    VmPanic,
    errors::{VmBamlError, VmRustFnError},
    vm::BexVm,
};

impl BamlNamespaceId for PackageBamlImpl {
    fn current(vm: &BexVm) -> bex_str::BexStr {
        vm.current_bex_identity
            .as_ref()
            .map(|identity| bex_str::BexStr::from(identity.runtime_id.as_str()))
            .unwrap_or_else(|| bex_str::BexStr::from(""))
    }

    fn new() -> Result<bex_str::BexStr, VmRustFnError> {
        let mut id = [0u8; 16];
        getrandom::getrandom(&mut id).map_err(|e| VmPanic::HostUnavailable {
            resource: "entropy".to_string(),
            message: format!("getrandom failed in baml.id.new: {e}"),
        })?;
        Ok(bex_str::BexStr::from(
            RuntimeId::OverrideUuid(id).encode().as_str(),
        ))
    }

    fn set(vm: &mut BexVm, id: &bex_str::BexStr) -> Result<bex_str::BexStr, VmRustFnError> {
        let id = id.to_string();
        let runtime_id = RuntimeId::decode(&id).map_err(|e| invalid_id_error(&id, &e))?;
        let RuntimeId::OverrideUuid(uuid) = runtime_id else {
            return Err(VmBamlError::InvalidArgument {
                message: "baml.id.set expects an override ID created by baml.id.new()".to_string(),
            }
            .into());
        };

        let identity =
            vm.current_bex_identity
                .as_mut()
                .ok_or_else(|| VmBamlError::InvalidArgument {
                    message: "baml.id.set is only available while a BEX function is running"
                        .to_string(),
                })?;
        identity.runtime_id.clone_from(&id);
        vm.pending_disk_events.push(DiskEventV1::SetId {
            thread_id: identity.thread_id,
            call_id: identity.call_id,
            id: uuid,
            timestamp_ns: bex_events::now_ns(),
        });

        Ok(bex_str::BexStr::from(id.as_str()))
    }
}

fn invalid_id_error(id: &str, source: &DecodeError) -> VmRustFnError {
    VmBamlError::InvalidArgument {
        message: format!("invalid BEX runtime ID `{id}`: {source}"),
    }
    .into()
}
