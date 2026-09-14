function hello_world() -> string {
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
}

function collect_and_report() -> string {
    let before = baml.sys.heap_stats();
    baml.sys.collect_garbage();
    let after = baml.sys.heap_stats();
    heap_stats("before", before) + heap_stats("after", after)
}
