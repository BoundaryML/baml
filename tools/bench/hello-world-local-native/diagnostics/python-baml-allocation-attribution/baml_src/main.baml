function hello_world() -> string {
    let _ = baml.http.Response.new(200, {}, "".to_utf8());
    "hello world"
}

function heap_stats(prefix: string, stats: baml.sys.HeapStats) -> string {
    prefix
        + "_total_objects="
        + string.from(stats.total_objects)
        + "\n"
        + prefix
        + "_compile_time_objects="
        + string.from(stats.compile_time_objects)
        + "\n"
        + prefix
        + "_runtime_objects="
        + string.from(stats.runtime_objects)
        + "\n"
        + prefix
        + "_active_handles="
        + string.from(stats.active_handles)
        + "\n"
        + prefix
        + "_tlab_chunks="
        + string.from(stats.tlab_chunks)
        + "\n"
        + prefix
        + "_reserved_slots="
        + string.from(stats.reserved_slots)
        + "\n"
        + prefix
        + "_gen0_slots="
        + string.from(stats.gen0_slots)
        + "\n"
        + prefix
        + "_gen1_slots="
        + string.from(stats.gen1_slots)
        + "\n"
        + prefix
        + "_gen2_slots="
        + string.from(stats.gen2_slots)
        + "\n"
        + prefix
        + "_gen0_capacity_slots="
        + string.from(stats.gen0_capacity_slots)
        + "\n"
        + prefix
        + "_gen1_capacity_slots="
        + string.from(stats.gen1_capacity_slots)
        + "\n"
        + prefix
        + "_gen2_capacity_slots="
        + string.from(stats.gen2_capacity_slots)
        + "\n"
        + prefix
        + "_scratch_capacity_slots="
        + string.from(stats.scratch_capacity_slots)
        + "\n"
        + prefix
        + "_allocations_since_gc="
        + string.from(stats.allocations_since_gc)
        + "\n"
        + prefix
        + "_object_slot_bytes="
        + string.from(stats.object_slot_bytes)
        + "\n"
        + prefix
        + "_tracked_slot_capacity_bytes="
        + string.from(stats.tracked_slot_capacity_bytes)
        + "\n"
        + prefix
        + "_permit_holder_slots="
        + string.from(stats.permit_holder_slots)
        + "\n"
        + prefix
        + "_allocator_blocks_in_use="
        + string.from(stats.allocator_blocks_in_use)
        + "\n"
        + prefix
        + "_allocator_bytes_in_use="
        + string.from(stats.allocator_bytes_in_use)
        + "\n"
        + prefix
        + "_allocator_bytes_high_water="
        + string.from(stats.allocator_bytes_high_water)
        + "\n"
        + prefix
        + "_allocator_bytes_reserved="
        + string.from(stats.allocator_bytes_reserved)
        + "\n"
}

function sample_heap_stats() -> string {
    heap_stats("sample", baml.sys.heap_stats())
}

function collect_and_report() -> string {
    let before = baml.sys.heap_stats();
    baml.sys.collect_garbage();
    let after = baml.sys.heap_stats();
    heap_stats("before", before) + heap_stats("after", after)
}

function collect_only() -> string {
    baml.sys.collect_garbage();
    ""
}
