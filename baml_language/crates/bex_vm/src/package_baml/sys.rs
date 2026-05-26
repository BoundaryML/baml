use bex_vm_types::BexStr;

use super::{BamlNamespaceSys, PackageBamlImpl};
use crate::{errors::VmRustFnError, vm::BexVm};

impl BamlNamespaceSys for PackageBamlImpl {
    #[allow(clippy::cast_possible_truncation)]
    fn now_ms() -> i64 {
        web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .expect("system time before UNIX epoch")
            .as_millis() as i64
    }

    fn panic(message: &BexStr) -> Result<(), VmRustFnError> {
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

    fn argv(vm: &mut BexVm) -> Vec<BexStr> {
        vm.argv.iter().map(|s| BexStr::from(s.as_str())).collect()
    }
}
