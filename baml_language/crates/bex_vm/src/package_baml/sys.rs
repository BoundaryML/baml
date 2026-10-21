use super::{BamlNamespaceSys, PackageBamlImpl};
use crate::vm::BexVm;

impl BamlNamespaceSys for PackageBamlImpl {
    fn argv(vm: &mut BexVm) -> Vec<bex_str::BexStr> {
        vm.argv
            .iter()
            .map(|s| bex_str::BexStr::from(s.as_str()))
            .collect()
    }
}
