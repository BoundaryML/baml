function hello_world() -> string {
    "hello world"
}

function handle(req: baml.http.Request) -> baml.http.Response {
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
