use super::{BamlNamespaceSys, PackageBamlImpl};
use crate::vm::BexVm;

impl BamlNamespaceSys for PackageBamlImpl {
    fn argv(vm: &mut BexVm) -> Vec<String> {
        vm.argv.to_vec()
    }
}
