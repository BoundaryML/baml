// BEP-034 demo: concurrent HTTP server.
//
// We listen on a port, accept connections one by one, and `spawn` a handler
// per connection. Each handler sleeps for 500 ms before responding — without
// concurrency you'd see 4 sequential curls take ~2 s; with spawn they all
// finish in ~0.5 s wall clock.
//
// Run:
//   baml run server.serve
//
// In another terminal:
//   time bash -c 'for i in 1 2 3 4; do curl -s -w "\n" http://127.0.0.1:8080/ & done; wait'

function build_response(body: string) -> string {
    "HTTP/1.1 200 OK\r\n"
        + "Content-Type: application/json\r\n"
        + "Content-Length: "
        + baml.unstable.string(body.byte_length())
        + "\r\n"
        + "Connection: close\r\n"
        + "\r\n"
        + body
}

// Pull "METHOD /path HTTP/1.1" off the first line for logging.
function summarize_request(req: string) -> string {
    let i = req.index_of("\r\n");
    if (i < 0) {
        return req.trim();
    };
    req.substring(0, i).trim()
}

function handle(sock: baml.net.TcpStream, conn_id: int) -> null {
    let label = "[conn " + baml.unstable.string(conn_id) + "]";
    baml.io.println(label + " accepted");
    let raw = sock.read() catch_all (e) {
        _ => {
            baml.io.println(label + " read failed");
            return null;
        },
    };
    baml.io.println(label + " req: " + summarize_request(raw.to_string()));

    // Simulated work — without spawn this would dominate wall-clock time.
    baml.sys.sleep(500);

    let body = "{\"hello\":\"world\",\"conn\":" + baml.unstable.string(conn_id) + "}";
    let _ = sock.write(build_response(body).to_utf8()) catch_all (e) {
        _ => { baml.io.println(label + " write failed"); null },
    };
    let _ = sock.close() catch_all (e) { _ => null };
    baml.io.println(label + " done");
    null
}

// BAML's `let` is function-scoped, not block-scoped, AND spawn captures
// closure variables by reference. So spawning inside a loop with a bare
// `let id = conn_id; spawn { handle(sock, id) }` would race — every spawn
// would see the loop's CURRENT id, not the one at spawn time. Wrapping the
// spawn in a per-call helper isolates the capture: each call gets its own
// `sock`/`id` function parameters.
function spawn_handler(
    sock: baml.net.TcpStream,
    id: int,
) -> baml.future.Future<null, baml.errors.InvalidArgument | baml.errors.Io> {
    spawn { handle(sock, id) }
}

/// Bind, accept, spawn per connection. Loops forever — Ctrl-C to exit.
function serve() -> null {
    let listener = baml.net.TcpListener.bind("127.0.0.1:8080");
    baml.io
        .println(
            "listening on http://127.0.0.1:8080 — fire concurrent curls and watch them all return in ~0.5s",
        );
    let conn_id = 0;
    while (true) {
        let sock = listener.accept();
        conn_id = conn_id + 1;
        let _ = spawn_handler(sock, conn_id);
    }
    null
}
