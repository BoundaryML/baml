// Layer 1: Raw SSE primitive tests — the HTTP SSE layer (`baml.http.fetch_sse`,
// `SseStream.next()`, `SseStream.close()`) directly, without the LLM accumulator
// or SAP parsing.
//
// Each test hosts its own `baml.http.Server` (bound to "127.0.0.1:0") and points
// `fetch_sse` at `server.addr`. Each case is a top-level `function` (needed to
// work around the test-block local-boxing VM bug) plus a `test` block asserting
// on its result.
//
// `sse.next()` returns a JSON array string of `{"event","data","id"}` objects
// (or null when the stream is done).
//
// A non-2xx status makes `fetch_sse` throw `baml.errors.Io` with message
// "SSE request failed with status {status}: {body}".

// Shape of one serialized SSE event as emitted by `sse.next()`.
class SseJsonEvent {
    event string
    data string
    id string?
}

// ─── fetch + next + close basics ─────────────────────────────────────────────

function sse_fetch_and_next_returns_events_fn() -> bool {
    let sse_body = "event: message\ndata: hello\n\nevent: message\ndata: world\n\n";
    let server = baml.http.Server.bind("127.0.0.1:0");
    let task = spawn {
        server.serve((req: baml.http.Request) -> baml.http.Response {
            baml.http.Response.new(200, { "content-type": "text/event-stream" }, sse_body.to_utf8())
        })
    };
    let sse = baml.http.fetch_sse(baml.http.Request {
        method: "GET",
        url: "http://" + server.addr + "/sse",
        headers: { },
        body: "",
    });
    let events = sse.next();
    sse.close();
    task.cancel();
    if (events == null) { return false; }
    assert.contains(events, "\"data\":\"hello\"");
    assert.contains(events, "\"data\":\"world\"");
    true
}

test "sse_fetch_and_next_returns_events" {
    assert.is_true(sse_fetch_and_next_returns_events_fn())
}

function sse_next_returns_null_when_done_fn() -> bool {
    let sse_body = "event: message\ndata: only-one\n\n";
    let server = baml.http.Server.bind("127.0.0.1:0");
    let task = spawn {
        server.serve((req: baml.http.Request) -> baml.http.Response {
            baml.http.Response.new(200, { "content-type": "text/event-stream" }, sse_body.to_utf8())
        })
    };
    let sse = baml.http.fetch_sse(baml.http.Request {
        method: "GET",
        url: "http://" + server.addr + "/sse",
        headers: { },
        body: "",
    });
    let first = sse.next();
    let second = sse.next();
    sse.close();
    task.cancel();
    if (first == null) { return false; }
    // Once the stream is exhausted, next() returns null.
    assert.is_true(second == null);
    // The first batch contains the only event.
    assert.contains(first, "\"data\":\"only-one\"");
    true
}

test "sse_next_returns_null_when_done" {
    assert.is_true(sse_next_returns_null_when_done_fn())
}

function sse_close_does_not_hang_fn() -> bool {
    let sse_body = "event: message\ndata: hello\n\n";
    let server = baml.http.Server.bind("127.0.0.1:0");
    let task = spawn {
        server.serve((req: baml.http.Request) -> baml.http.Response {
            baml.http.Response.new(200, { "content-type": "text/event-stream" }, sse_body.to_utf8())
        })
    };
    let sse = baml.http.fetch_sse(baml.http.Request {
        method: "GET",
        url: "http://" + server.addr + "/sse",
        headers: { },
        body: "",
    });
    let events = sse.next();
    sse.close();
    task.cancel();
    events != null
}

test "sse_close_does_not_hang" {
    assert.is_true(sse_close_does_not_hang_fn())
}

// A 500 response makes fetch_sse throw a catchable Io whose message mentions
// the status code.
function sse_fetch_error_on_bad_status_fn() -> bool {
    let server = baml.http.Server.bind("127.0.0.1:0");
    let task = spawn {
        server.serve((req: baml.http.Request) -> baml.http.Response {
            baml.http.Response.new(500, { }, "Internal Server Error".to_utf8())
        })
    };
    let ok = {
        let sse = baml.http.fetch_sse(baml.http.Request {
            method: "GET",
            url: "http://" + server.addr + "/sse",
            headers: { },
            body: "",
        });
        baml.sys.panic("expected fetch_sse on a 500 response to throw")
    } catch (e) {
        let err: baml.errors.Io => {
            // Error should mention the status code.
            assert.contains(err.message, "500");
            true
        }
    };
    task.cancel();
    ok
}

test "sse_fetch_error_on_bad_status" {
    assert.is_true(sse_fetch_error_on_bad_status_fn())
}

// ─── HTTP behavior ───────────────────────────────────────────────────────────

// The handler only serves the SSE body when the request arrives as a POST with
// the exact body; anything else gets a 404, which makes fetch_sse throw.
function sse_post_with_body_fn() -> bool {
    let server = baml.http.Server.bind("127.0.0.1:0");
    let task = spawn {
        server.serve((req: baml.http.Request) -> baml.http.Response {
            if (req.method == "POST" && req.body == "my-request-body") {
                baml.http.Response.new(200, { "content-type": "text/event-stream" }, "data: ok\n\n".to_utf8())
            } else {
                baml.http.Response.new(404, { }, "".to_utf8())
            }
        })
    };
    let sse = baml.http.fetch_sse(baml.http.Request {
        method: "POST",
        url: "http://" + server.addr + "/sse",
        headers: { },
        body: "my-request-body",
    });
    let events = sse.next();
    sse.close();
    task.cancel();
    if (events == null) { return false; }
    assert.contains(events, "\"data\":\"ok\"");
    true
}

test "sse_post_with_body" {
    assert.is_true(sse_post_with_body_fn())
}

// Custom request headers reach the wire; the handler 404s unless the
// authorization header is present.
function sse_custom_headers_fn() -> bool {
    let server = baml.http.Server.bind("127.0.0.1:0");
    let task = spawn {
        server.serve((req: baml.http.Request) -> baml.http.Response {
            if (req.headers.get("authorization") == "Bearer test-token-123") {
                baml.http.Response.new(200, { "content-type": "text/event-stream" }, "data: authed\n\n".to_utf8())
            } else {
                baml.http.Response.new(404, { }, "".to_utf8())
            }
        })
    };
    let headers: map<string, string> = { };
    headers["authorization"] = "Bearer test-token-123";
    let sse = baml.http.fetch_sse(baml.http.Request {
        method: "GET",
        url: "http://" + server.addr + "/sse",
        headers: headers,
        body: "",
    });
    let events = sse.next();
    sse.close();
    task.cancel();
    if (events == null) { return false; }
    assert.contains(events, "\"data\":\"authed\"");
    true
}

test "sse_custom_headers" {
    assert.is_true(sse_custom_headers_fn())
}

// The very first next() on an empty stream is null.
function sse_empty_stream_returns_null_fn() -> string {
    let server = baml.http.Server.bind("127.0.0.1:0");
    let task = spawn {
        server.serve((req: baml.http.Request) -> baml.http.Response {
            baml.http.Response.new(200, { "content-type": "text/event-stream" }, "".to_utf8())
        })
    };
    let sse = baml.http.fetch_sse(baml.http.Request {
        method: "GET",
        url: "http://" + server.addr + "/sse",
        headers: { },
        body: "",
    });
    let first = sse.next();
    sse.close();
    task.cancel();
    if (first == null) { return "got-null"; }
    "unexpected-data"
}

test "sse_empty_stream_returns_null" {
    assert.equal(sse_empty_stream_returns_null_fn(), "got-null")
}

// close() drops the resource handle, so subsequent next() returns null.
function sse_next_after_close_returns_null_fn() -> string {
    let server = baml.http.Server.bind("127.0.0.1:0");
    let task = spawn {
        server.serve((req: baml.http.Request) -> baml.http.Response {
            baml.http.Response.new(200, { "content-type": "text/event-stream" }, "data: hello\n\n".to_utf8())
        })
    };
    let sse = baml.http.fetch_sse(baml.http.Request {
        method: "GET",
        url: "http://" + server.addr + "/sse",
        headers: { },
        body: "",
    });
    let first = sse.next();
    sse.close();
    let after_close = sse.next();
    task.cancel();
    if (after_close == null) { return "null-after-close"; }
    "unexpected-data"
}

test "sse_next_after_close_returns_null" {
    assert.equal(sse_next_after_close_returns_null_fn(), "null-after-close")
}

// ─── JSON structure of the event batch ───────────────────────────────────────

// next() yields a JSON array of objects with event, data, id fields.
function sse_event_fields_in_json_fn() -> bool {
    let server = baml.http.Server.bind("127.0.0.1:0");
    let task = spawn {
        server.serve((req: baml.http.Request) -> baml.http.Response {
            baml.http.Response.new(200, { "content-type": "text/event-stream" }, "id: 42\nevent: custom\ndata: payload\n\n".to_utf8())
        })
    };
    let sse = baml.http.fetch_sse(baml.http.Request {
        method: "GET",
        url: "http://" + server.addr + "/sse",
        headers: { },
        body: "",
    });
    let events = sse.next();
    sse.close();
    task.cancel();
    if (events == null) { return false; }
    let parsed: SseJsonEvent[] = baml.json.from_string<SseJsonEvent[]>(events);
    assert.equal(parsed.length(), 1);
    assert.equal(parsed[0].event, "custom");
    assert.equal(parsed[0].data, "payload");
    assert.equal(parsed[0].id, "42");
    true
}

test "sse_event_fields_in_json" {
    assert.is_true(sse_event_fields_in_json_fn())
}

function sse_event_id_null_when_not_set_fn() -> bool {
    let server = baml.http.Server.bind("127.0.0.1:0");
    let task = spawn {
        server.serve((req: baml.http.Request) -> baml.http.Response {
            baml.http.Response.new(200, { "content-type": "text/event-stream" }, "data: no-id\n\n".to_utf8())
        })
    };
    let sse = baml.http.fetch_sse(baml.http.Request {
        method: "GET",
        url: "http://" + server.addr + "/sse",
        headers: { },
        body: "",
    });
    let events = sse.next();
    sse.close();
    task.cancel();
    if (events == null) { return false; }
    let parsed: SseJsonEvent[] = baml.json.from_string<SseJsonEvent[]>(events);
    assert.equal(parsed.length(), 1);
    // id should be null when not set.
    assert.is_true(parsed[0].id == null);
    true
}

test "sse_event_id_null_when_not_set" {
    assert.is_true(sse_event_id_null_when_not_set_fn())
}

// ─── 4xx error ───────────────────────────────────────────────────────────────

function sse_http_4xx_error_fn() -> bool {
    let server = baml.http.Server.bind("127.0.0.1:0");
    let task = spawn {
        server.serve((req: baml.http.Request) -> baml.http.Response {
            baml.http.Response.new(403, { }, "Forbidden".to_utf8())
        })
    };
    let ok = {
        let sse = baml.http.fetch_sse(baml.http.Request {
            method: "GET",
            url: "http://" + server.addr + "/sse",
            headers: { },
            body: "",
        });
        baml.sys.panic("expected fetch_sse on a 403 response to throw")
    } catch (e) {
        let err: baml.errors.Io => {
            // Error should mention the status code.
            assert.contains(err.message, "403");
            true
        }
    };
    task.cancel();
    ok
}

test "sse_http_4xx_error" {
    assert.is_true(sse_http_4xx_error_fn())
}

// ─── large payload ───────────────────────────────────────────────────────────

// A single 50KB data field round-trips intact.
function sse_large_event_payload_fn() -> bool {
    let large_data = "x".repeat(50000);
    let sse_body = "data: " + large_data + "\n\n";
    let server = baml.http.Server.bind("127.0.0.1:0");
    let task = spawn {
        server.serve((req: baml.http.Request) -> baml.http.Response {
            baml.http.Response.new(200, { "content-type": "text/event-stream" }, sse_body.to_utf8())
        })
    };
    let sse = baml.http.fetch_sse(baml.http.Request {
        method: "GET",
        url: "http://" + server.addr + "/sse",
        headers: { },
        body: "",
    });
    let events = sse.next();
    sse.close();
    task.cancel();
    if (events == null) { return false; }
    // The full 50KB payload is present.
    assert.contains(events, large_data);
    true
}

test "sse_large_event_payload" {
    assert.is_true(sse_large_event_payload_fn())
}
