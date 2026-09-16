function hello_world() -> string {
    "hello world"
}

function handle(req: baml.http.Request) -> baml.http.Response {
    if (req.method == "GET" && req.url == "/gc") {
        baml.sys.collect_garbage();
        return baml.http.Response.new(
            200,
            {
                "content-type": "text/plain; charset=utf-8",
                "content-length": "2",
                "cache-control": "no-store",
            },
            "ok".to_utf8(),
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
    let server = baml.http.Server.bind("0.0.0.0:8080");
    server.serve(handle, allow_http1 = true, allow_http2 = false);
}
