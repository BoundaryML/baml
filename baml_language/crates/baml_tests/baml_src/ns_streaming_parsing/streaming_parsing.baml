// Layer 3: Full streaming parse tests — SSE → StreamAccumulator → SAP
// partial/final parse → typed values.
//
// Each test hosts a `baml.http.Server` (bound to "127.0.0.1:0") and repoints
// the request built by the `$build_request_stream` companion to `server.addr`
// before calling `baml.http.fetch_sse` — `client<llm>` option values are
// compile-time config (literals / env refs), so runtime URL injection only
// works on the explicit request-companion path.
//
// Request validation (method("POST") + path("/chat/completions")) is explicit
// in the BAML handler; a non-matching request gets 404, making `fetch_sse`
// throw and fail the test.
//
// Each case is a top-level `function` (avoiding the test-block local-boxing
// VM bug) plus a `test` block asserting on its result.
//
// Not in corpus (require exercising `$stream` companion's internal fetch):
//   - stream_string_final_value          (also pins inferred `<TStream, TFinal>`
//     type-args on a bare `stream_llm_function` call reaching `__make_stream`)
//   - stream_class_in_namespace_final_value  (tests the `$stream` companion's
//     internal fetch + class-typed stream in a namespace)
//   - stream_server_error_propagates     (500 propagating out of the client's
//     *internal* fetch; the raw `fetch_sse`-throws-on-500 behavior is already
//     covered by ns_streaming_sse_primitives "sse_fetch_error_on_bad_status")

// Never contacted: the built request's URL is swapped to the live in-test
// server before fetching. The block only supplies provider/model/key metadata
// for prompt rendering and request building.
client<llm> StreamParseClient {
    provider openai
    options {
        model "gpt-4o"
        api_key "test-key"
        base_url "http://localhost:1234"
    }
}

class Doc {
    title string
    body string?
    word_count int
}

function ExtractDoc(input: string) -> Doc {
    client StreamParseClient
    prompt `Say hello to ${input}`
}

function SayHello(input: string) -> string {
    client StreamParseClient
    prompt `Say hello to ${input}`
}

// Builds an OpenAI-format SSE body from content chunks plus a finish_reason
// event and the [DONE] sentinel.
function openai_sse_body(chunks: string[], finish_reason: string) -> string {
    let body: string = "";
    for (let chunk in chunks) {
        body = body + "data: {\"choices\":[{\"delta\":{\"content\":\"" + chunk + "\"}}]}\n\n";
    }
    body = body + "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"" + finish_reason + "\"}]}\n\n";
    body = body + "data: [DONE]\n\n";
    body
}

// Rebuild a companion-built request against the live in-test server. The
// OpenAI client appends `/chat/completions` to base_url (no `/v1` prefix when
// base_url is already a full host URI).
function repoint_request(req: baml.http.Request, addr: string) -> baml.http.Request {
    baml.http.Request {
        method: req.method,
        url: "http://" + addr + "/chat/completions",
        headers: req.headers,
        body: req.body,
    }
}

// ─── parse_stream_companion_class_in_namespace ───────────────────────────────
//
// Pin for the `$parse_stream` companion's type-arg threading: its body is
// synthesized by PPIR as `CLIENT.__make_stream<STREAM_EXPANDED, ORIGINAL>(sse)`
// (see `synthesize_llm_make_stream_call`), so `StreamCache.new` gets its types
// from the frame via `reflect.type_of` — no function-name string is involved.
// Class-typed and namespaced to exercise resolution of the PPIR-synthetic
// `Doc$stream` in the explicit type args. The two chunks split mid-string so
// the partial parser sees an incomplete object before the final one.

function parse_stream_companion_class_in_namespace_fn() -> string {
    let sse_body = openai_sse_body([
        "{\\\"title\\\": \\\"Hello\\\", \\\"body\\\": \\\"Wor",
        "ld\\\", \\\"word_count\\\": 1}",
    ], "stop");
    let server = baml.http.Server.bind("127.0.0.1:0");
    let task = spawn {
        server.serve((req: baml.http.Request) -> baml.http.Response {
            if (req.method == "POST" && req.url == "/chat/completions") {
                baml.http.Response.new(200, { "content-type": "text/event-stream" }, sse_body.to_utf8())
            } else {
                baml.http.Response.new(404, { }, "".to_utf8())
            }
        })
    };
    let req = ExtractDoc$build_request_stream("world");
    let sse = baml.http.fetch_sse(repoint_request(req, server.addr));
    let s = ExtractDoc$parse_stream(sse);
    let d = s.final();
    task.cancel();
    d.title
}

test "parse_stream_companion_class_in_namespace" {
    assert.equal(parse_stream_companion_class_in_namespace_fn(), "Hello")
}

// ─── stream_done_signal_required ─────────────────────────────────────────────
//
// An SSE body with content but no finish_reason and no [DONE] — the stream
// just ends. `Stream.final()` must throw ("Streaming finished without
// finish_reason" — a baml.errors.LlmClient) rather than return the partial
// content. The done-signal check lives in `Stream.final()` itself; the explicit
// companion path (`$build_request_stream` → `fetch_sse` → `$parse_stream`)
// exercises the identical accumulator/final logic.

function stream_done_signal_required_fn() -> bool {
    let sse_body = "data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n";
    let server = baml.http.Server.bind("127.0.0.1:0");
    let task = spawn {
        server.serve((req: baml.http.Request) -> baml.http.Response {
            if (req.method == "POST" && req.url == "/chat/completions") {
                baml.http.Response.new(200, { "content-type": "text/event-stream" }, sse_body.to_utf8())
            } else {
                baml.http.Response.new(404, { }, "".to_utf8())
            }
        })
    };
    let req = SayHello$build_request_stream("world");
    let sse = baml.http.fetch_sse(repoint_request(req, server.addr));
    let s = SayHello$parse_stream(sse);
    let ok = {
        s.final();
        baml.sys.panic("expected final() to throw when the stream ends without a completion signal")
    } catch (e) {
        baml.errors.LlmClient => true
    };
    task.cancel();
    ok
}

test "stream_done_signal_required" {
    assert.is_true(stream_done_signal_required_fn())
}
