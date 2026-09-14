// Part 2 capture_wire regressions (spec tests 8 and 9), driven against the
// mock provider servers in ../ns_llm_mock:
//
//   * test 8 — with the flag off, `http_request`, `http_response.body` and
//     `sse_responses` are null while status/headers/timing are still real;
//     with it on, both bodies appear and credential-bearing query params in
//     the request URL are redacted (`key=abc` -> `key=REDACTED`);
//   * test 9 — the streaming settle record carries `ttft_ms` and the open
//     status/headers, and `sse_responses` only under capture_wire.
//
// Per the ns_llm_mock README every case body is a top-level `function` and
// the `test` block only asserts (test-block locals are boxed by the VM).

function WireEcho(question: string) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `
        Reply with exactly one word.
        ${question}
    `
}

// The listener drops the one LLMCall it sees here; a class instance so the
// lambda mutates shared state.
class WireCallBox {
    call: ai.events.LLMCall?,
}

// ─── test 8: capture_wire gating on the unary path ───────────────────────────

function _wire_unary_run(capture: bool) -> string {
    let canned = `{"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"pong"}]}],"usage":{"input_tokens":3,"output_tokens":1}}`;
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(canned)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let box = WireCallBox { call: null };
            let cb = (e: ai.events.Event) -> void {
                if let c: ai.events.LLMCall = e {
                    box.call = c;
                };
            };
            let cl = openai.ResponsesClient.new(
                model = "gpt-4o-mini",
                api_key = "test-key",
                base_url = mock.base_url(),
                query_params = { "key": "abc" },
                capture_wire = capture,
            );
            let value = WireEcho("Say pong.", client = cl, on_event = cb);
            let call = box.call ?? baml.sys.panic("no LLMCall was delivered");
            let resp = call.http_response ?? baml.sys.panic("no http_response on the record");
            let req_state = match (call.http_request) {
                let req: baml.http.Request => {
                    // Redaction scrubs the query-param value but keeps the rest
                    // of the URL; the request body rides along verbatim.
                    let redacted = req.url.includes("key=REDACTED") && !req.url.includes("abc");
                    let body_kept = req.body.includes("gpt-4o-mini");
                    `req=${if (redacted) { "redacted" } else { req.url }},${if (body_kept) { "body" } else { "no-body" }}`
                },
                null => "req=null",
            };
            [
                value,
                if (call.selected) { "sel" } else { "leg" },
                req_state,
                `status=${resp.status}`,
                match (resp.body) {
                    let text: string => if (text.includes("pong")) { "respbody=set" } else { "respbody=wrong" },
                    null => "respbody=null",
                },
                if (call.sse_responses == null) { "sse=null" } else { "sse=set" },
                if (resp.headers.get("content-type") == null) { "hdr=missing" } else { "hdr=set" },
                if (call.timing.duration_ms == null) { "dur=null" } else { "dur=set" },
            ].join("|")
        },
    )
}

test "wire_capture_off_unary" {
    assert.equal(
        _wire_unary_run(false),
        "pong|sel|req=null|status=200|respbody=null|sse=null|hdr=set|dur=set",
    )
}

test "wire_capture_on_unary" {
    assert.equal(
        _wire_unary_run(true),
        "pong|sel|req=redacted,body|status=200|respbody=set|sse=null|hdr=set|dur=set",
    )
}

// ─── test 9: the streaming settle record ─────────────────────────────────────

function _wire_stream_run(capture: bool) -> string {
    let completed = `{"type":"response.completed","response":{"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"pong"}]}],"usage":{"input_tokens":3,"output_tokens":1}}}`;
    root.llm_mock.mock_sse_serve(
        [
            root.llm_mock.sse_data(`{"type":"response.output_text.delta","item_id":"msg_1","delta":"po"}`),
            root.llm_mock.sse_data(`{"type":"response.output_text.delta","item_id":"msg_1","delta":"ng"}`),
            root.llm_mock.sse_data(completed),
        ],
        (mock: root.llm_mock.MockSseServer) -> {
            let box = WireCallBox { call: null };
            let cb = (e: ai.events.Event) -> void {
                if let c: ai.events.LLMCall = e {
                    box.call = c;
                };
            };
            let cl = openai.ResponsesClient.new(
                model = "gpt-4o-mini",
                api_key = "test-key",
                base_url = mock.base_url(),
                capture_wire = capture,
            );
            let stream = ai.stream.from_spec<string | null, string>(
                WireEcho@spec("Say pong."),
                client = cl,
                on_event = cb,
            );
            let value = stream.final();
            let call = box.call ?? baml.sys.panic("settle delivered no LLMCall");
            let resp = call.http_response ?? baml.sys.panic("no http_response on the stream record");
            let sse_state = match (call.sse_responses) {
                let records: ai.events.SseEventRecord[] => {
                    if (records.length() > 0) { "sse=set" } else { "sse=empty" }
                },
                null => "sse=null",
            };
            [
                value,
                if (call.selected) { "sel" } else { "leg" },
                if (call.timing.ttft_ms == null) { "ttft=null" } else { "ttft=set" },
                if (call.timing.duration_ms == null) { "dur=null" } else { "dur=set" },
                `status=${resp.status}`,
                `ct=${resp.headers.get("content-type") ?? "missing"}`,
                sse_state,
            ].join("|")
        },
    )
}

test "wire_capture_on_stream" {
    assert.equal(
        _wire_stream_run(true),
        "pong|sel|ttft=set|dur=set|status=200|ct=text/event-stream|sse=set",
    )
}

test "wire_capture_off_stream" {
    assert.equal(
        _wire_stream_run(false),
        "pong|sel|ttft=set|dur=set|status=200|ct=text/event-stream|sse=null",
    )
}
