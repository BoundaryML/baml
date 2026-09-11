function gauge(name: string, help: string, value: int) -> string {
    "# HELP "
        + name
        + " "
        + help
        + "\n# TYPE "
        + name
        + " gauge\n"
        + name
        + " "
        + string.from(value)
        + "\n"
}

function handle_metrics(req: baml.http.Request) -> baml.http.Response {
    if (req.method != "GET" || req.url != "/metrics") {
        return baml.http.Response.new(404, {}, "".to_utf8());
    };
    let heap = baml.sys.heap_stats();
    let body = gauge(
        "baml_heap_total_object_slots",
        "BAML compile-time plus runtime object slots, not heap bytes or live objects.",
        heap.total_objects,
    )
        + gauge(
            "baml_heap_compile_time_object_slots",
            "Permanent BAML compile-time object slots.",
            heap.compile_time_objects,
        )
        + gauge(
            "baml_heap_runtime_object_slots",
            "BAML runtime object slots including unused reservations and uncollected objects.",
            heap.runtime_objects,
        )
        + gauge(
            "baml_heap_active_handles",
            "Registered BAML heap handles acting as GC roots.",
            heap.active_handles,
        )
        + gauge(
            "baml_heap_tlab_chunks",
            "Current BAML nursery allocation reservations; may decrease after GC.",
            heap.tlab_chunks,
        );
    baml.http.Response.new(
        200,
        { "content-type": "text/plain; version=0.0.4; charset=utf-8", "cache-control": "no-store" },
        body.to_utf8(),
    )
}
