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

    fn panic(message: &str) -> Result<(), VmRustFnError> {
        Err(VmRustFnError::Panic(crate::VmPanic::UserPanic {
            message: message.to_string(),
        }))
    }

    fn argv(vm: &mut BexVm) -> Vec<String> {
        vm.argv.iter().cloned().collect()
    }
}
