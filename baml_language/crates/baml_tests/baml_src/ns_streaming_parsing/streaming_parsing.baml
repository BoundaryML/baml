// Layer 3: Full streaming parse tests — SSE → StreamAccumulator → SAP
// partial/final parse → typed values.
//
// TODO(stream-migration): the request/stream companions this file was built
// on (`$build_request_stream`, `$parse_stream`) were removed with the legacy
// LLM path. The streaming test bodies below are parked until the new ai-world
// `$stream` machinery lands; the non-streaming scaffolding (client decl,
// types, SSE-body builder) is kept compiling so the tests can be restored
// in place.
//
// Parked tests (see git history for the original bodies):
//   - parse_stream_companion_class_in_namespace
//   - stream_done_signal_required
//
// Not in corpus (require exercising `$stream` companion's internal fetch):
//   - stream_string_final_value          (also pins inferred `<TStream, TFinal>`
//     type-args on a bare `stream_llm_function` call reaching `__make_stream`)
//   - stream_class_in_namespace_final_value  (tests the `$stream` companion's
//     internal fetch + class-typed stream in a namespace)
//   - stream_server_error_propagates     (500 propagating out of the client's
//     *internal* fetch; the raw `fetch_sse`-throws-on-500 behavior is already
//     covered by ns_streaming_sse_primitives "sse_fetch_error_on_bad_status")

// Never contacted: the URL points at a dead localhost port and the parked
// streaming tests swapped it for a live in-test server before fetching. The
// declaration only supplies provider/model/key metadata for prompt rendering.
client StreamParseClient = openai.OpenAiClient.new(
    model = "gpt-4o",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

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
// event and the [DONE] sentinel. Kept for the parked streaming tests above.
function openai_sse_body(chunks: string[], finish_reason: string) -> string {
    let body: string = "";
    for (let chunk in chunks) {
        body = body + "data: {\"choices\":[{\"delta\":{\"content\":\"" + chunk + "\"}}]}\n\n";
    }
    body = body + "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"" + finish_reason + "\"}]}\n\n";
    body = body + "data: [DONE]\n\n";
    body
}

// Rebuild a request against the live in-test server. The OpenAI client
// appends `/chat/completions` to base_url (no `/v1` prefix when base_url is
// already a full host URI). Kept for the parked streaming tests above.
function repoint_request(req: baml.http.Request, addr: string) -> baml.http.Request {
    baml.http.Request {
        method: req.method,
        url: "http://" + addr + "/chat/completions",
        headers: req.headers,
        body: req.body,
    }
}
