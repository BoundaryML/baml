use bex_vm_types::types::Value;

use super::{BamlNamespaceSys, PackageBamlImpl, copy};
use crate::vm::BexVm;

impl BamlNamespaceSys for PackageBamlImpl {
    fn heap_stats(vm: &mut BexVm) -> Value {
        // Native builtins run with the VM's active heap permit, excluding GC.
        // Sample before allocating the result so it does not count itself.
        let stats = vm.heap.stats();
        let count = |n| i64::try_from(n).expect("heap counters fit in addressable storage");
        copy::sys::HeapStats {
            total_objects: count(stats.total_objects),
            compile_time_objects: count(stats.compile_time_objects),
            runtime_objects: count(stats.runtime_objects),
            active_handles: count(stats.active_handles),
            tlab_chunks: count(stats.tlab_chunks),
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
