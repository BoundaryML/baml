// Shared mock-provider helpers for the native LLM clients (openai / anthropic /
// google / …). Every per-provider test namespace drives its client against one
// of these instead of a real endpoint, so the whole suite stays offline.
//
// Two servers, both pure BAML on top of `baml.http.Server` (the pattern is
// proven in ns_http_server/http_server.baml and in
// sdk_tests/fixtures/llm_functions/baml_src/ns_replay/replay_server.baml):
//
//   * `mock_json_serve(responses, body)` — answers every request with a canned
//     JSON body and records what the client actually sent, so a test can assert
//     on the wire request (`mock.last_request().json()` + `baml.json.path<T>`).
//   * `mock_sse_serve(events, body)` — replays a canned SSE event list, one
//     event per socket write with a real inter-event delay, so a streaming
//     client sees genuine partials. A single buffered write is delivered in one
//     socket read and masks host-driven streaming bugs — see the long comment at
//     ns_replay/replay_server.baml:21-28.
//
// Both are path- and method-agnostic: one server answers `/responses`,
// `/v1/messages`, `/v1beta/models/x:generateContent`, … so the same helper works
// for every provider.
//
// Cross-namespace use is `root.`-absolute:
//
//   root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(body)], (mock) -> { … })
//
// Authoring rule for consumers: test-block locals are boxed by the VM, so each
// case must be a top-level `function` with a thin `test` block asserting on its
// return value (same rule as ns_http_server/http_server.baml:3-5). See
// ns_llm_mock/README.md for the full recipe.

// ─── captured request ────────────────────────────────────────────────────────

// One request exactly as the mock server received it. `url` is origin-form
// ("/v1/messages?key=abc" — the authority lives in the `host` header) and header
// names are lowercased by the HTTP layer.
class MockRequest {
    method: string,
    url: string,
    headers: map<string, string>,
    body: string,

    // The request body parsed as JSON, for `baml.json.path<T>(req.json(), ".model")`.
    // Throws `baml.json.ParseError` if the client did not send JSON.
    function json(self) -> json {
        baml.json.parse(self.body)
    }

    // A request header by name (case-insensitive), or null.
    function header(self, name: string) -> string? {
        self.headers.get(name.to_lower_case())
    }

    // The path portion of `url`, without the query string.
    function path(self) -> string {
        self.url.split("?").at(0) ?? self.url
    }

    // The query string, without the leading "?", or null when there was none.
    function query(self) -> string? {
        self.url.split("?").at(1)
    }
}

// ─── canned response ─────────────────────────────────────────────────────────

// A canned HTTP response for the JSON mock. `headers` is merged over the default
// `content-type: application/json`.
class MockResponse {
    status: int,
    body: string,
    headers: map<string, string>,

    // 200 + JSON body — the common case.
    function ok(body: string) -> MockResponse {
        MockResponse { status: 200, body: body, headers: { } }
    }

    // A provider error response (4xx/5xx) with a JSON body — for error-taxonomy
    // and retry tests.
    function error(status: int, body: string) -> MockResponse {
        MockResponse { status: status, body: body, headers: { } }
    }

    // Same as `ok`, plus extra response headers (rate-limit headers, request ids).
    function with_headers(status: int, body: string, headers: map<string, string>) -> MockResponse {
        MockResponse { status: status, body: body, headers: headers }
    }
}

// `content-type` first, then the caller's headers (which may override it).
function _merged_headers(content_type: string, extra: map<string, string>) -> map<string, string> {
    let out: map<string, string> = { "content-type": content_type };
    for (let k in extra.keys()) {
        let _ = out.set(k, extra.get(k) ?? "");
    }
    out
}

// ─── JSON mock ───────────────────────────────────────────────────────────────

// A non-streaming fake provider. `responses` are answered in order; once they
// run out the last one repeats, so a single canned response covers any number of
// requests (an agent loop can therefore be scripted turn-by-turn).
class MockJsonServer {
    server: baml.http.Server,
    responses: MockResponse[],
    response_delay_ms: int,
    // Every request the client sent, in order. Appended by `handle`; readable
    // from the test after the call.
    requests: MockRequest[],

    // Hand this to the client under test: `.new(base_url = mock.base_url())`.
    function base_url(self) -> string {
        "http://" + self.server.addr
    }

    function request_count(self) -> int {
        self.requests.length()
    }

    // The most recent captured request. Panics when the client never called —
    // which is the assertion you wanted anyway.
    function last_request(self) -> MockRequest {
        self.requests.at(self.requests.length() - 1)
            ?? baml.sys.panic("mock_json_serve: no request was captured")
    }

    function handle(self, req: baml.http.Request) -> baml.http.Response {
        let _ = self.requests.push(MockRequest {
            method: req.method,
            url: req.url,
            headers: req.headers,
            body: req.body,
        });
        // 0-based index of this request, clamped to the last canned response.
        let taken = self.requests.length() - 1;
        let idx = if (taken < self.responses.length()) {
            taken
        } else {
            self.responses.length() - 1
        };
        if let canned: MockResponse = self.responses.at(idx) {
            let headers = root.llm_mock._merged_headers("application/json", canned.headers);
            if (self.response_delay_ms > 0) {
                let response = baml.http.Response.new_streaming(canned.status, headers);
                let delay = baml.time.Duration.from_milliseconds(self.response_delay_ms);
                let body = canned.body;
                spawn {
                    baml.sys.sleep(delay);
                    response.write(body.to_utf8());
                    response.end();
                    null
                };
                response
            } else {
                baml.http.Response.new(canned.status, headers, canned.body.to_utf8())
            }
        } else {
            // `responses` was empty. Answer with a 500 whose body names the bug
            // rather than throwing out of a handler that must not throw.
            baml.http.Response.new(
                500,
                { "content-type": "application/json" },
                `{"error":{"message":"mock_json_serve: no canned response configured"}}`.to_utf8(),
            )
        }
    }
}

// Runs `body` against a JSON mock server and tears the server down on the way
// out (the `defer` also fires when `body` throws, so a failing assertion never
// leaks a serve task). The closure receives the live server, so it can read
// `base_url()` before the call and `requests` after it.
function mock_json_serve<T, E>(
    responses: MockResponse[],
    body: (mock: MockJsonServer) -> T throws E,
    response_delay_ms: int = 0,
) -> T throws E | baml.errors.Io {
    let mock = MockJsonServer {
        server: baml.http.Server.bind("127.0.0.1:0"),
        responses: responses,
        response_delay_ms: response_delay_ms,
        requests: [],
    };
    let task = spawn { mock.server.serve(mock.handle) };
    defer { task.cancel(); }
    body(mock)
}

// ─── SSE mock ────────────────────────────────────────────────────────────────

// A streaming fake provider. `events` are SSE event texts WITHOUT the trailing
// blank line, e.g. "event: message_start\ndata: {…}" or just "data: {…}"; the
// blank-line delimiter is appended per event.
class MockSseServer {
    server: baml.http.Server,
    events: string[],
    first_event_delay: baml.time.Duration,
    // Gap between event writes. `write` backpressures until the previous chunk
    // is on the wire, and the sleep keeps events in separate socket reads.
    chunk_delay: baml.time.Duration,
    requests: MockRequest[],

    function base_url(self) -> string {
        "http://" + self.server.addr
    }

    function request_count(self) -> int {
        self.requests.length()
    }

    function last_request(self) -> MockRequest {
        self.requests.at(self.requests.length() - 1)
            ?? baml.sys.panic("mock_sse_serve: no request was captured")
    }

    function handle(self, req: baml.http.Request) -> baml.http.Response {
        let _ = self.requests.push(MockRequest {
            method: req.method,
            url: req.url,
            headers: req.headers,
            body: req.body,
        });
        let resp = baml.http.Response.new_streaming(200, { "content-type": "text/event-stream" });
        let events = self.events;
        let delay = self.chunk_delay;
        let first_event_delay = self.first_event_delay;
        // The writes run in a `spawn` because hyper only reads the response body
        // after the handler returns (see `Response.new_streaming`).
        spawn {
            baml.sys.sleep(first_event_delay);
            for (let event in events) {
                if (event.length() > 0) {
                    resp.write((event + "\n\n").to_utf8()) catch (e) {
                        // A request-timeout test intentionally drops the client
                        // while this producer is still replaying later events.
                        baml.errors.Io => {
                            if (e.message == "streaming response could not be written: the client has hung up") {
                                break;
                            }
                            throw e
                        },
                    };
                    baml.sys.sleep(delay);
                }
            }
            resp.end();
            null
        };
        resp
    }
}

// Runs `body` against an SSE mock server and tears it down afterwards.
// `chunk_delay_ms` is the inter-event gap; keep it non-zero so partials really
// cross the FFI boundary.
function mock_sse_serve<T, E>(
    events: string[],
    body: (mock: MockSseServer) -> T throws E,
    chunk_delay_ms: int = 10,
    first_event_delay_ms: int = 0,
) -> T throws E | baml.errors.Io {
    let mock = MockSseServer {
        server: baml.http.Server.bind("127.0.0.1:0"),
        events: events,
        first_event_delay: baml.time.Duration.from_milliseconds(first_event_delay_ms),
        chunk_delay: baml.time.Duration.from_milliseconds(chunk_delay_ms),
        requests: [],
    };
    let task = spawn { mock.server.serve(mock.handle) };
    defer { task.cancel(); }
    body(mock)
}

// ─── SSE event construction ──────────────────────────────────────────────────

// One `data:`-only event (OpenAI Responses / chat-completions style).
function sse_data(data: string) -> string {
    "data: " + data
}

// One named event (Anthropic style: an `event:` line plus a `data:` line).
function sse_event(name: string, data: string) -> string {
    "event: " + name + "\ndata: " + data
}

// Splits a *recorded* SSE body ("…\n\n…\n\n") into the event list the SSE mock
// expects, dropping the trailing empty element.
function sse_events(body: string) -> string[] {
    body.split("\n\n").filter((e) -> { e.trim().length() > 0 })
}

// ─── self-tests: the helpers themselves ──────────────────────────────────────

// The JSON mock answers with the canned body and captures method, path, query,
// headers and the JSON body of what the client sent.
function mock_json_captures_request() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(`{"ok":true}`)],
        (mock: MockJsonServer) -> {
            let resp = baml.http.send(baml.http.Request {
                method: "POST",
                url: mock.base_url() + "/v1/responses?key=abc",
                headers: { "authorization": "Bearer test-key" },
                body: `{"model":"gpt-4o-mini","stream":false}`,
            });
            let sent = mock.last_request();
            [
                resp.text(),
                sent.method,
                sent.path(),
                sent.query() ?? "",
                sent.header("Authorization") ?? "",
                baml.json.path<string>(sent.json(), ".model"),
                mock.request_count().to_string(),
            ].join("|")
        },
    )
}

test "mock_json_captures_request" {
    assert.equal(
        mock_json_captures_request(),
        "{\"ok\":true}|POST|/v1/responses|key=abc|Bearer test-key|gpt-4o-mini|1",
    )
}

// Canned responses are consumed in order and the last one repeats, so an agent
// loop can be scripted turn-by-turn.
function mock_json_replays_in_order() -> string {
    root.llm_mock.mock_json_serve(
        [
            root.llm_mock.MockResponse.ok(`{"n":1}`),
            root.llm_mock.MockResponse.ok(`{"n":2}`),
        ],
        (mock: MockJsonServer) -> {
            let first = baml.http.fetch(mock.base_url() + "/a").text();
            let second = baml.http.fetch(mock.base_url() + "/b").text();
            let third = baml.http.fetch(mock.base_url() + "/c").text();
            [first, second, third, mock.request_count().to_string()].join("|")
        },
    )
}

test "mock_json_replays_in_order" {
    assert.equal(
        mock_json_replays_in_order(),
        "{\"n\":1}|{\"n\":2}|{\"n\":2}|3",
    )
}

// A canned error status reaches the client verbatim — the entry point for
// error-taxonomy tests.
function mock_json_serves_error_status() -> int {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.error(429, `{"error":{"message":"slow down"}}`)],
        (mock: MockJsonServer) -> {
            baml.http.fetch(mock.base_url() + "/v1/messages").status_code
        },
    )
}

test "mock_json_serves_error_status" {
    assert.equal(mock_json_serves_error_status(), 429)
}

// The SSE mock streams the canned events; `SseStream.next()` returns a JSON
// array string of `{"event","data","id"}` objects per batch (see
// ns_streaming_sse_primitives/streaming_sse_primitives.baml:9-11), so the test
// concatenates the batches and asserts on the payloads.
function mock_sse_streams_events() -> string {
    root.llm_mock.mock_sse_serve(
        [
            root.llm_mock.sse_event("message_start", `{"type":"message_start"}`),
            root.llm_mock.sse_data(`{"type":"response.output_text.delta","delta":"hi"}`),
        ],
        (mock: MockSseServer) -> {
            let stream = baml.http.fetch_sse(baml.http.Request {
                method: "POST",
                url: mock.base_url() + "/v1/messages",
                headers: { },
                body: `{"stream":true}`,
            });
            let seen: string[] = [];
            while (true) {
                if let batch: string = stream.next() {
                    let _ = seen.push(batch);
                } else {
                    break;
                }
            }
            stream.close();
            seen.join("")
        },
    )
}

test "mock_sse_streams_events" {
    let got = mock_sse_streams_events();
    assert.contains(got, "\"event\":\"message_start\"");
    assert.contains(got, "response.output_text.delta")
}

// `sse_events` turns a recorded body back into the event list, so a checked-in
// recording round-trips through the mock.
function mock_sse_events_roundtrip() -> string {
    let recorded = "event: a\ndata: 1\n\nevent: b\ndata: 2\n\n";
    root.llm_mock.sse_events(recorded).join(" / ")
}

test "mock_sse_events_roundtrip" {
    assert.equal(mock_sse_events_roundtrip(), "event: a\ndata: 1 / event: b\ndata: 2")
}
