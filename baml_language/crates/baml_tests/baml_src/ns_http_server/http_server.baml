// End-to-end `baml.http.Server` tests, converted from crates/baml_tests/tests/http.rs.
//
// Each case is a top-level `function` (to avoid the test-block local-boxing VM
// bug) plus a `test` block that asserts on its result. Every server binds
// "127.0.0.1:0"; the OS-assigned address is read directly from `server.addr`
// (`bind` resolves the port synchronously, so there's no busy-wait and no bind
// race). The socket queues connections from bind time, so
// a client may connect before `serve` begins accepting.

// Round trip: a capturing closure handler echoes a captured local + the request
// body. Exercises `bind`, `serve`, the per-request `spawn_with_callable`, request
// / response marshaling, and `Response.new`.
function server_roundtrip() -> string {
    let greeting = "you said: ";
    let server = baml.http.Server.bind("127.0.0.1:0");
    let task = spawn {
        server.serve((req: baml.http.Request) -> baml.http.Response {
            baml.http.Response.new(200, { "x-echo-method": req.method }, (greeting + req.body).to_utf8())
        })
    };
    let resp = baml.http.send(baml.http.Request {
        method: "POST",
        url: "http://" + server.addr + "/echo",
        headers: { },
        body: "ping",
    });
    let body = resp.text();
    task.cancel();
    body
}

test "server_roundtrip" {
    assert.equal(server_roundtrip(), "you said: ping")
}

// A non-2xx status set by the handler reaches the client verbatim.
function server_status_code() -> int {
    let server = baml.http.Server.bind("127.0.0.1:0");
    let task = spawn {
        server.serve((req: baml.http.Request) -> baml.http.Response {
            baml.http.Response.new(503, { }, "down".to_utf8())
        })
    };
    let resp = baml.http.fetch("http://" + server.addr + "/");
    task.cancel();
    resp.status_code
}

test "server_status_code" {
    assert.equal(server_status_code(), 503)
}

// Response headers set by the handler are sent on the wire and readable by the
// client (header names are lowercased by the HTTP layer).
function server_response_headers() -> string? {
    let server = baml.http.Server.bind("127.0.0.1:0");
    let task = spawn {
        server.serve((req: baml.http.Request) -> baml.http.Response {
            baml.http.Response.new(200, { "x-custom": "from-handler" }, "".to_utf8())
        })
    };
    let resp = baml.http.fetch("http://" + server.addr + "/");
    task.cancel();
    resp.headers.get("x-custom")
}

test "server_response_headers" {
    assert.equal(server_response_headers(), "from-handler")
}

// Framing / hop-by-hop headers a handler sets (here a bogus `Content-Length` far
// larger than the body, plus `Connection`) must be stripped so hyper frames the
// real body — otherwise the response is corrupted (and hyper debug-asserts on the
// length mismatch, dropping the connection so the client would hang).
function server_strips_framing_headers() -> string {
    let server = baml.http.Server.bind("127.0.0.1:0");
    let task = spawn {
        server.serve((req: baml.http.Request) -> baml.http.Response {
            baml.http.Response.new(
                200,
                { "content-length": "99999", "connection": "close" },
                "hello".to_utf8(),
            )
        })
    };
    let resp = baml.http.fetch("http://" + server.addr + "/");
    let body = resp.text();
    task.cancel();
    body
}

test "server_strips_framing_headers" {
    assert.equal(server_strips_framing_headers(), "hello")
}

// A request body larger than `max_body_size` is rejected with 413 before the
// handler runs, so the body can't be buffered unbounded.
function server_rejects_oversized_body() -> int {
    let server = baml.http.Server.bind("127.0.0.1:0");
    let task = spawn {
        server.serve(
            (req: baml.http.Request) -> baml.http.Response {
                baml.http.Response.new(200, { }, "ok".to_utf8())
            },
            max_body_size = 8,
        )
    };
    let resp = baml.http.send(baml.http.Request {
        method: "POST",
        url: "http://" + server.addr + "/",
        headers: { },
        body: "this body is much longer than eight bytes",
    });
    task.cancel();
    resp.status_code
}

test "server_rejects_oversized_body" {
    assert.equal(server_rejects_oversized_body(), 413)
}

// A handler that panics on one request fails only that request (client gets a
// 500); the server stays up and serves the next request normally.
function server_isolates_handler_panic() -> bool {
    let server = baml.http.Server.bind("127.0.0.1:0");
    let task = spawn {
        server.serve((req: baml.http.Request) -> baml.http.Response {
            if (req.url == "/boom") {
                baml.sys.panic("handler exploded")
            } else {
                baml.http.Response.new(200, { }, "ok".to_utf8())
            }
        })
    };
    let boom = baml.http.fetch("http://" + server.addr + "/boom");
    let good = baml.http.fetch("http://" + server.addr + "/ok");
    task.cancel();
    boom.status_code == 500 && good.status_code == 200
}

test "server_isolates_handler_panic" {
    assert.is_true(server_isolates_handler_panic())
}

// Binding ":0" resolves to a concrete OS-assigned address, exposed as `.addr`.
function server_bind_resolves_ephemeral_port() -> bool {
    let server = baml.http.Server.bind("127.0.0.1:0");
    // The OS picks a real port, so `.addr` is neither empty nor the ":0" we asked for.
    server.addr != "" && server.addr != "127.0.0.1:0"
}

test "server_bind_resolves_ephemeral_port" {
    assert.is_true(server_bind_resolves_ephemeral_port())
}

// Cancelling a serve frees the handler but keeps the port bound (the listener
// lives in the `Server`), so the SAME server can be served again — here with a
// different handler. The short sleep lets the cancelled serve settle (releasing
// the "serving" flag) before re-serving. Distinct bodies ("first"/"second")
// ensure the second serve really took over.
function server_reserve_after_cancel() -> string {
    let server = baml.http.Server.bind("127.0.0.1:0");
    let firstTask = spawn {
        server.serve((req: baml.http.Request) -> baml.http.Response {
            baml.http.Response.new(200, { }, "first".to_utf8())
        })
    };
    let served = baml.http.fetch("http://" + server.addr + "/").text();

    firstTask.cancel();
    baml.sys.sleep(baml.time.Duration.from_milliseconds(100n));  // let the cancelled serve release the "serving" flag

    let secondTask = spawn {
        server.serve((req: baml.http.Request) -> baml.http.Response {
            baml.http.Response.new(200, { }, "second".to_utf8())
        })
    };
    let served_again = baml.http.fetch("http://" + server.addr + "/").text();
    secondTask.cancel();

    served + "/" + served_again
}

test "server_reserve_after_cancel" {
    assert.equal(server_reserve_after_cancel(), "first/second")
}

// A bound method used as the handler: `server.serve(greeter.handle)`. Proves the
// callable path injects the receiver as `self` (so `self.prefix` is visible).
class Greeter {
    prefix: string
    function handle(self, req: baml.http.Request) -> baml.http.Response {
        baml.http.Response.new(200, { }, (self.prefix + req.body).to_utf8())
    }
}

function server_bound_method_handler() -> string {
    let greeter = Greeter { prefix: "hi " };
    let server = baml.http.Server.bind("127.0.0.1:0");
    let task = spawn { server.serve(greeter.handle) };
    let resp = baml.http.send(baml.http.Request {
        method: "POST",
        url: "http://" + server.addr + "/",
        headers: { },
        body: "there",
    });
    task.cancel();
    resp.text()
}

test "server_bound_method_handler" {
    assert.equal(server_bound_method_handler(), "hi there")
}

// `TlsConfig.new` omitting `allow_tls1_2` (its documented default) must surface
// bad PEM as a catchable `Io`, NOT abort on the omitted default-arg sentinel
// (it once panicked the host). The wildcard catch only succeeds if a catchable
// error was thrown — a host panic would propagate past it.
function tls_config_new_bad_pem() -> bool {
    baml.http.TlsConfig.new("not a cert".to_utf8(), "not a key".to_utf8());
    false
}

function tls_config_new_default_arg() -> bool {
    tls_config_new_bad_pem() catch (e) {
        _ => true
    }
}

test "tls_config_new_default_arg" {
    assert.is_true(tls_config_new_default_arg())
}

// The new `serve` knobs (`max_connections`, `header_read_timeout`) are accepted
// and don't disturb normal serving.
function server_custom_config() -> int {
    let server = baml.http.Server.bind("127.0.0.1:0");
    let task = spawn {
        server.serve(
            (req: baml.http.Request) -> baml.http.Response {
                baml.http.Response.new(200, { }, "ok".to_utf8())
            },
            max_connections = 4,
            header_read_timeout = baml.time.Duration.from_seconds(5n),
        )
    };
    let resp = baml.http.send(baml.http.Request {
        method: "GET",
        url: "http://" + server.addr + "/",
        headers: { },
        body: "",
    });
    task.cancel();
    resp.status_code
}

test "server_custom_config" {
    assert.equal(server_custom_config(), 200)
}

// `TlsConfig.new` accepts an explicit `handshake_timeout`; bad PEM still surfaces
// as a catchable `Io` (the named `Duration` arg flows through to cert parsing).
function tls_config_handshake_timeout_bad_pem() -> bool {
    baml.http.TlsConfig.new(
        "not a cert".to_utf8(),
        "not a key".to_utf8(),
        handshake_timeout = baml.time.Duration.from_seconds(3n),
    );
    false
}

function tls_config_handshake_timeout_arg() -> bool {
    tls_config_handshake_timeout_bad_pem() catch (e) {
        _ => true
    }
}

test "tls_config_handshake_timeout_arg" {
    assert.is_true(tls_config_handshake_timeout_arg())
}
