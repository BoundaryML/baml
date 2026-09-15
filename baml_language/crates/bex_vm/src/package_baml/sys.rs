use bex_vm_types::types::Value;

use super::{BamlNamespaceSys, PackageBamlImpl, copy};
use crate::vm::BexVm;

#[cfg(target_os = "macos")]
#[allow(unsafe_code, reason = "calls the macOS malloc zone statistics API")]
fn allocator_stats() -> [Value; 4] {
    #[repr(C)]
    #[derive(Default)]
    struct MallocStatistics {
        blocks_in_use: std::os::raw::c_uint,
        size_in_use: usize,
        max_size_in_use: usize,
        size_allocated: usize,
    }

    unsafe extern "C" {
        fn malloc_zone_statistics(zone: *mut std::ffi::c_void, stats: *mut MallocStatistics);
    }

    let mut stats = MallocStatistics::default();
    // SAFETY: Apple's malloc API accepts a null zone to aggregate every zone
    // in the current process and initializes the caller-owned output struct.
    unsafe { malloc_zone_statistics(std::ptr::null_mut(), &mut stats) };
    let count = |n| {
        i64::try_from(n)
            .ok()
            .and_then(Value::try_int)
            .unwrap_or(Value::NULL)
    };
    [
        count(stats.blocks_in_use as usize),
        count(stats.size_in_use),
        count(stats.max_size_in_use),
        count(stats.size_allocated),
    ]
}

#[cfg(not(target_os = "macos"))]
fn allocator_stats() -> [Value; 4] {
    [Value::NULL; 4]
}

impl BamlNamespaceSys for PackageBamlImpl {
    fn heap_stats(vm: &mut BexVm) -> Value {
        // Native builtins run with the VM's active heap permit, excluding GC.
        // Sample before allocating the result so it does not count itself.
        let stats = vm.heap.stats();
        let count = |n| i64::try_from(n).expect("heap counters fit in addressable storage");
        let allocator = allocator_stats();
        copy::sys::HeapStats {
            total_objects: count(stats.total_objects),
            compile_time_objects: count(stats.compile_time_objects),
            runtime_objects: count(stats.runtime_objects),
            active_handles: count(stats.active_handles),
            tlab_chunks: count(stats.tlab_chunks),
            reserved_slots: count(stats.reserved_slots),
            gen0_slots: count(stats.generation_slots[0]),
            gen1_slots: count(stats.generation_slots[1]),
            gen2_slots: count(stats.generation_slots[2]),
            gen0_capacity_slots: count(stats.capacity_slots[0]),
            gen1_capacity_slots: count(stats.capacity_slots[1]),
            gen2_capacity_slots: count(stats.capacity_slots[2]),
            scratch_capacity_slots: count(stats.capacity_slots[3]),
            allocations_since_gc: count(stats.allocations_since_gc),
            object_slot_bytes: count(stats.object_slot_bytes),
            tracked_slot_capacity_bytes: count(stats.tracked_slot_capacity_bytes),
            permit_holder_slots: count(stats.permit_holder_slots),
            allocator_blocks_in_use: allocator[0],
            allocator_bytes_in_use: allocator[1],
            allocator_bytes_high_water: allocator[2],
            allocator_bytes_reserved: allocator[3],
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
