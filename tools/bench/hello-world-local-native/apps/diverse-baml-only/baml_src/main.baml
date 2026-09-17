function hello_world() -> string {
    "Hello World"
}

function nested_payload(workload: string, payload_size: int) -> baml.json.json {
    {
        "workload": workload,
        "nested": { "level1": { "level2": { "payload": "x".repeat(payload_size) } } },
    }
}

function json_64k() -> baml.json.json {
    // The serialized envelope is 69 bytes, making the response exactly 64 KiB.
    nested_payload("json-64k", 65467)
}

function json_1m() -> baml.json.json {
    // The serialized envelope is 68 bytes, making the response exactly 1 MiB.
    nested_payload("json-1m", 1048508)
}

function image_base64_from_file() -> string {
    baml.fs.open("benchmark-image.png", "r").bytes().to_base64() catch_all (e) {
        _ => baml.sys.panic(`failed to read benchmark image: ${e.to_string()}`),
    }
}

function response(body: string, content_type: string) -> baml.http.Response {
    baml.http.Response.new(
        200,
        { "content-type": content_type, "cache-control": "no-store" },
        body.to_utf8(),
    )
}

function collect_garbage_response() -> baml.http.Response {
    baml.sys.collect_garbage();
    response("ok", "text/plain; charset=utf-8")
}

function handle(req: baml.http.Request) -> baml.http.Response {
    if (req.method == "GET" && req.url == "/gc") {
        collect_garbage_response()
    } else if (req.url == "/hello") {
        response(hello_world(), "text/plain; charset=utf-8")
    } else if (req.url == "/json-64k") {
        response(baml.json.stringify(json_64k()), "application/json")
    } else if (req.url == "/json-1m") {
        response(baml.json.stringify(json_1m()), "application/json")
    } else if (req.url == "/image") {
        response(image_base64_from_file(), "text/plain; charset=utf-8")
    } else {
        baml.http.Response.new(
            404,
            { "content-type": "text/plain; charset=utf-8" },
            "not found".to_utf8(),
        )
    }
}

function main() -> null {
    let server = baml.http.Server.bind("127.0.0.1:8505");
    server.serve(handle, allow_http1 = true, allow_http2 = false);
}
