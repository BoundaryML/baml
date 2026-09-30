use bex_vm_types::types::Value;

use super::{BamlClassErrorsUnknownError, PackageBamlImpl};
use crate::BexVm;

impl BamlClassErrorsUnknownError for PackageBamlImpl {
    fn _preserve_context(vm: &mut BexVm, source: &Value, target: &Value) {
        vm.transfer_context(*source, *target);
    }
}
