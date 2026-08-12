use super::{BamlNamespaceSys, PackageBamlImpl};
use crate::{errors::VmRustFnError, vm::BexVm};

impl BamlNamespaceSys for PackageBamlImpl {
    fn panic(message: &bex_str::BexStr) -> Result<(), VmRustFnError> {
        Err(VmRustFnError::Panic(crate::VmPanic::UserPanic {
            message: message.to_string(),
        }))
    }

    fn exit(code: i64) -> Result<(), VmRustFnError> {
        // `baml.sys.exit(code)` is modeled as a catchable panic
        // (`baml.panics.Exit { code }`), so user code can intercept it
        // for cleanup / testing. If nothing catches it, the engine
        // surfaces it as `EngineError::Exit` and the host terminates.
        Err(VmRustFnError::Panic(crate::VmPanic::Exit { code }))
    }

    fn argv(vm: &mut BexVm) -> Vec<bex_str::BexStr> {
        vm.argv
            .iter()
            .map(|s| bex_str::BexStr::from(s.as_str()))
            .collect()
    }
}
