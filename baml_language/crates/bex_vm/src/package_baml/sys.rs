use bex_vm_types::types::Value;

use super::{BamlNamespaceSys, PackageBamlImpl, copy};
use crate::vm::BexVm;

impl BamlNamespaceSys for PackageBamlImpl {
    fn heap_stats(vm: &mut BexVm) -> Value {
        // Native builtins run with the VM's active heap permit, excluding GC.
        // Sample before allocating the result so it does not count itself.
        let stats = vm.heap.stats();
        copy::sys::HeapStats {
            total_objects: stats.total_objects as i64,
            compile_time_objects: stats.compile_time_objects as i64,
            runtime_objects: stats.runtime_objects as i64,
            active_handles: stats.active_handles as i64,
            tlab_chunks: stats.tlab_chunks as i64,
        }
        .to_value(vm)
    }

    fn argv(vm: &mut BexVm) -> Vec<bex_str::BexStr> {
        vm.argv
            .iter()
            .map(|s| bex_str::BexStr::from(s.as_str()))
            .collect()
    }
}
