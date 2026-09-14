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

function handle(req: baml.http.Request) -> baml.http.Response {
    if (req.method == "GET" && req.url == "/gc") {
        let before = baml.sys.heap_stats();
        baml.sys.collect_garbage();
        let after = baml.sys.heap_stats();
        let body = heap_stats("before", before) + heap_stats("after", after);
        return baml.http.Response.new(
            200,
            { "content-type": "text/plain; charset=utf-8", "cache-control": "no-store" },
            body.to_utf8(),
        );
    };

    if (req.method != "GET" || req.url != "/") {
        return baml.http.Response.new(404, {}, "".to_utf8());
    };

    baml.http.Response.new(
        200,
        {
            "content-type": "text/plain; charset=utf-8",
            "content-length": "11",
            "cache-control": "no-store",
        },
        hello_world().to_utf8(),
    )
}

function main() -> null {
    let server = baml.http.Server.bind("127.0.0.1:8505");
    server.serve(handle, allow_http1 = true, allow_http2 = false);
}
