// openai.ResponsesClient — wire-level tests against the mock provider servers
// in ../ns_llm_mock (see its README for the authoring rules these follow):
//
//   * request shape — the EXACT typed body the client puts on the wire, the
//     passthrough merge (including null-deletes), headers/query params, tool
//     definitions, journal replay and image_generation tool injection;
//   * response parse — usage detail fields, generated image output;
//   * streaming — SSE decode, `response.failed` as a typed error, a non-2xx on
//     the SSE endpoint classified like the non-streaming path, and the
//     function-call argument assembly the decoder buffers.
//
// Everything here is offline. Real calls live in the `live` testset at the
// bottom, which the default profile excludes.
//
// Per the corpus convention every case body is a top-level `function` and the
// `test` block only asserts (test-block locals are boxed by the VM).

// ─── specs under test ────────────────────────────────────────────────────────

function OaiEcho(question: string) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `
        Reply with exactly one word.
        ${question}
    `
}

function OaiPicture(subject: string) -> image {
    client: "openai/gpt-4o-mini"
    prompt: `
        Draw ${subject}.
    `
}

function OaiLook(pic: image, clip: audio, doc: pdf) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `
        ${role("user")}
        Look: ${pic} ${clip} ${doc}
    `
}

function OaiSystemLook(pic: image) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `
        ${role("system")}
        Look: ${pic}
    `
}

// `role(...)` builds a metadata-less marker, so a per-message directive is
// authored by interpolating the typed marker class directly.
function OaiMetaEcho(question: string) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `${ai.Role { name: "system", metadata: { "cache_hint": "ephemeral" } }}Follow the rules.${role("user")}${question}`
}

// ─── canned provider bodies ──────────────────────────────────────────────────

// A completed text response with the full usage envelope: totals, cached input
// tokens and reasoning tokens.
function _oai_text_response() -> string {
    let body = `{"id":"resp_1","model":"gpt-4o-mini","status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"pong"}]}],"usage":{"input_tokens":3,"output_tokens":1,"total_tokens":4,"input_tokens_details":{"cached_tokens":2},"output_tokens_details":{"reasoning_tokens":7}}}`;
    body
}

// A completed response whose only output item is a generated image.
function _oai_image_response() -> string {
    let body = `{"id":"resp_2","model":"gpt-4o-mini","status":"completed","output":[{"type":"image_generation_call","id":"ig_1","status":"completed","result":"aGVsbG8=","output_format":"jpeg","revised_prompt":"a red apple, studio light"}],"usage":{"input_tokens":10,"output_tokens":20}}`;
    body
}

// A response whose `usage` omits every count — the optional-int hardening: a
// short envelope must not become ai.errors.ParseFailed.
function _oai_sparse_usage_response() -> string {
    let body = `{"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"pong"}]}],"usage":{}}`;
    body
}

// ─── helpers ─────────────────────────────────────────────────────────────────

function _oai_json(fields: map<string, unknown>) -> string {
    baml.json.stringify(baml.json.to_json(fields))
}

// One `SseStream.next()` batch: the JSON array of `{event,data,id}` objects the
// provider decoders receive.
function _oai_batch(payloads: string[]) -> string {
    let events: map<string, unknown>[] = [];
    for (let payload in payloads) {
        let event: map<string, unknown> = { };
        let _ = event.set("event", null);
        let _ = event.set("data", payload);
        let _ = event.set("id", null);
        events.push(event);
    }
    baml.json.stringify(baml.json.to_json(events))
}

function _oai_first_text(events: ai.stream.StreamEvent[]) -> string {
    for (let e in events) {
        if let delta: ai.stream.TextDelta = e {
            return delta.text;
        };
    }
    ""
}

// The classified failure, as a comparable string.
function _oai_error_name(error: unknown) -> string {
    match (error) {
        let invalid: ai.errors.InvalidRequest => `invalid_request:${invalid.detail}`,
        let limited: ai.errors.RateLimited => "rate_limited",
        let refused: ai.errors.Refused => `refused:${refused.reason}`,
        let network: ai.errors.NetworkFailure => `network:${network.detail}`,
        let parse: ai.errors.ParseFailed => "parse_failed",
        _ => `other:${error.to_string()}`,
    }
}

// The rendered request body, as a plain call so a `catch_all` at the call site
// is unambiguous.
function _oai_render_body(cl: openai.ResponsesClient, input: ai.ModelTurnInput) -> string {
    let req = openai.internal.openai_render(cl, input);
    req.body
}

// A ModelTurnInput built from a spec, for driving the client below the runner.
function _oai_input(
    spec: ai.FunctionSpec<string>,
    journal: ai.Journal,
    toolbox: ai.tools.Toolbox,
) -> ai.ModelTurnInput {
    ai.ModelTurnInput {
        prompt: spec.prompt_template,
        journal: journal,
        toolbox: toolbox,
        output_type: spec.output_type(),
    }
}

// ─── request shape ───────────────────────────────────────────────────────────

// Every first-class typed parameter lands in the body, unset ones are absent
// (never an explicit null), and the endpoint/auth headers are the provider's.
function oai_request_shape() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_oai_text_response())],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ResponsesClient.new(
                model = "gpt-4o-mini",
                api_key = "test-key",
                base_url = mock.base_url(),
                temperature = 0.25,
                top_p = 0.5,
                max_output_tokens = 128,
                reasoning_effort = "low",
                reasoning_summary = "auto",
                verbosity = "low",
                parallel_tool_calls = true,
            );
            let result = ai.Agent<string>.new(client = cl).run(OaiEcho@spec("Say pong."));
            let sent = mock.last_request();
            [
                result.value,
                sent.method,
                sent.path(),
                sent.header("authorization") ?? "",
                sent.header("content-type") ?? "",
                sent.body,
            ].join("~~")
        },
    )
}

test "oai_request_shape" {
    let got = oai_request_shape();
    assert.contains(got, "pong~~POST~~/responses~~Bearer test-key~~application/json~~");
    // typed request fields, in the request class's declaration order
    assert.contains(got, `"model":"gpt-4o-mini"`);
    assert.contains(got, `"store":false`);
    assert.contains(got, `"input":[{"role":"system","content":[{"type":"input_text","text":`);
    assert.contains(got, "Say pong.");
    assert.contains(got, `"temperature":0.25`);
    assert.contains(got, `"top_p":0.5`);
    assert.contains(got, `"max_output_tokens":128`);
    assert.contains(got, `"reasoning":{"effort":"low","summary":"auto"}`);
    assert.contains(got, `"text":{"verbosity":"low"}`);
    assert.contains(got, `"parallel_tool_calls":true`);
    // an unset optional is ABSENT, not null; no tools and no stream flag here
    assert.is_true(!got.includes("null"));
    assert.is_true(!got.includes(`"tools"`));
    assert.is_true(!got.includes(`"tool_choice"`));
    assert.is_true(!got.includes(`"stream"`))
}

function oai_request_timeout_ms() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_oai_text_response())],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ResponsesClient.new(
                model = "gpt-4o-mini",
                api_key = "test-key",
                base_url = mock.base_url(),
                request_timeout_ms = 20,
            );
            let outcome = `turn:${cl.invoke(_oai_input(OaiEcho@spec("Say pong."), ai.Journal { log: [] }, ai.tools.Toolbox.new([]))).terminal_text() ?? ""}` catch_all (e) {
                let timeout: baml.errors.Timeout => "Timeout",
                _ => `other:${e.to_string()}`,
            };
            `${cl.request_timeout_ms ?? -1}|${outcome}`
        },
        response_delay_ms = 200,
    )
}

test "oai_request_timeout_ms" {
    assert.equal(oai_request_timeout_ms(), "20|Timeout")
}

// request_body deep-merges over the serialized typed request: a scalar
// replaces, a map merges key by key, and a null DELETES the key the client
// injected. Headers and query params are the user's, authorization is not.
function oai_passthrough() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_oai_text_response())],
        (mock: root.llm_mock.MockJsonServer) -> {
            let overrides = baml.json.parse(
                `{"temperature":0.75,"metadata":{"tag":"unit"},"store":null,"reasoning":{"effort":"high"}}`,
            );
            let cl = openai.ResponsesClient.new(
                model = "gpt-4o-mini",
                api_key = "test-key",
                base_url = mock.base_url(),
                temperature = 0.25,
                reasoning_effort = "low",
                reasoning_summary = "auto",
                request_body = overrides,
                headers = { "X-Trace": "abc", "authorization": "Bearer overridden" },
                query_params = { "api-version": "2024 05" },
            );
            let result = ai.Agent<string>.new(client = cl).run(OaiEcho@spec("Say pong."));
            let sent = mock.last_request();
            [
                result.value,
                sent.query() ?? "",
                sent.header("x-trace") ?? "",
                sent.header("authorization") ?? "",
                sent.body,
            ].join("~~")
        },
    )
}

test "oai_passthrough" {
    let got = oai_passthrough();
    // query params are percent-encoded onto the endpoint
    assert.contains(got, "~~api-version=2024%2005~~abc~~Bearer test-key~~");
    // scalar override replaces the typed parameter
    assert.contains(got, `"temperature":0.75`);
    assert.is_true(!got.includes(`"temperature":0.25`));
    // map override merges: effort replaced, summary survives
    assert.contains(got, `"reasoning":{"effort":"high","summary":"auto"}`);
    // a new key is added
    assert.contains(got, `"metadata":{"tag":"unit"}`);
    // a null override deletes the client's own default
    assert.is_true(!got.includes(`"store"`))
}

// Toolbox tools become typed function tools with tool_choice "auto", the
// journal replays as function_call items, and ai.wire.sanitize_for_client
// closes the tool call that never got a result.
function oai_tools_and_journal() -> string {
    let spec = OaiEcho@spec("Say pong.");
    let properties: map<string, unknown> = { };
    let required: string[] = [];
    let schema: map<string, unknown> = {
        "type": "object",
        "properties": properties,
        "required": required,
    };
    let toolbox = ai.tools.Toolbox.new(
        [
            ai.tools.raw_tool(
                "lookup",
                "Look something up",
                schema,
                (args: map<string, unknown>) -> {
                    let out = "ok";
                    out
                },
            ),
        ],
    );
    let args: map<string, unknown> = { };
    let journal = ai.Journal {
        log: [
            ai.events.UserMessage { content: "earlier question" },
            ai.events.AssistantMessage {
                content: [
                    ai.content.Text { text: "on it" },
                    ai.content.ToolUse { id: "call_1", name: "lookup", args: args },
                ],
                client_id: "openai/gpt-4o-mini",
            },
        ],
    };
    let cl = openai.ResponsesClient.new(model = "gpt-4o-mini", api_key = "test-key");
    let req = openai.internal.openai_render(cl, _oai_input(spec, journal, toolbox));
    req.body
}

test "oai_tools_and_journal" {
    let got = oai_tools_and_journal();
    assert.contains(
        got,
        `"tools":[{"type":"function","name":"lookup","description":"Look something up","parameters":{`,
    );
    assert.contains(got, `"strict":false}]`);
    assert.contains(got, `"tool_choice":"auto"`);
    // journal replay, in order
    assert.contains(got, `{"role":"user","content":[{"type":"input_text","text":"earlier question"}]}`);
    assert.contains(got, `{"role":"assistant","content":[{"type":"output_text","text":"on it"}]}`);
    assert.contains(got, `{"type":"function_call","call_id":"call_1","name":"lookup","arguments":"{}"}`);
    // the orphaned call is closed by the shared sanitizer
    assert.contains(got, `{"type":"function_call_output","call_id":"call_1","output":`);
    assert.contains(got, "No result provided")
}

// Prompt media becomes typed Responses content parts: an image data URL with
// `detail`, audio NESTED under `input_audio` (the Responses API's shape — the
// flat `data`/`format` sys_llm sent was wrong), and a PDF URL as `file_url`
// with the basename as filename.
function oai_media_input() -> string {
    let spec = OaiLook@spec(
        baml.media.Image.from_base64("aW1n", "image/png"),
        baml.media.Audio.from_base64("YXVk", "audio/wav"),
        baml.media.Pdf.from_url("https://example.test/files/report.pdf?token=1", "application/pdf"),
    );
    let cl = openai.ResponsesClient.new(model = "gpt-4o-mini", api_key = "test-key");
    let empty = ai.tools.Toolbox.new([]);
    let req = openai.internal.openai_render(cl, _oai_input(spec, ai.Journal.new(spec), empty));
    req.body
}

test "oai_media_input" {
    let got = oai_media_input();
    assert.contains(got, `{"type":"input_image","image_url":"data:image/png;base64,aW1n","detail":"auto"}`);
    assert.contains(got, `{"type":"input_audio","input_audio":{"data":"YXVk","format":"wav"}}`);
    assert.contains(
        got,
        `{"type":"input_file","filename":"report.pdf","file_url":"https://example.test/files/report.pdf?token=1"}`,
    )
}

// Responses compatibility permits image input in a system message. This is
// important for role-less prompts, which the compiler lowers to system for
// legacy parity, and for explicitly authored system prompts alike.
function oai_system_image_input() -> string {
    let spec = OaiSystemLook@spec(baml.media.Image.from_base64("aW1n", "image/png"));
    let cl = openai.ResponsesClient.new(model = "gpt-4o-mini", api_key = "test-key");
    let empty = ai.tools.Toolbox.new([]);
    _oai_render_body(cl, _oai_input(spec, ai.Journal.new(spec), empty))
}

test "oai_system_image_input" {
    let got = oai_system_image_input();
    assert.contains(
        got,
        `{"role":"system","content":[{"type":"input_text","text":"\\nLook: "},{"type":"input_image","image_url":"data:image/png;base64,aW1n","detail":"auto"}]}`,
    )
}

// An `-> image` return type enables the provider-side image_generation tool and
// pins tool_choice to it (sys_llm's image_generation_mode), and the
// image_generation_call output item comes back as the run's value.
function oai_image_generation() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_oai_image_response())],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ResponsesClient.new(
                model = "gpt-4o-mini",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            let result = ai.Agent<image>.new(client = cl).run(OaiPicture@spec("a red apple"));
            let sent = mock.last_request();
            [
                result.value.base64(),
                result.value.mime_type() ?? "",
                sent.body,
            ].join("~~")
        },
    )
}

test "oai_image_generation" {
    let got = oai_image_generation();
    assert.contains(got, "aGVsbG8=~~image/jpeg~~");
    assert.contains(got, `"tools":[{"type":"image_generation"}]`);
    assert.contains(got, `"tool_choice":{"type":"image_generation"}`)
}

// A tool's `parameters` is the CALLER's JSON Schema, where `"default": null`
// and `"const": null` are legitimate members. The null-prune that drops the
// typed request classes' unset optionals used to run over them too, so the
// schema on the wire was not the schema the caller declared.
function oai_tool_schema_keeps_nulls() -> string {
    let spec = OaiEcho@spec("Say pong.");
    let city: map<string, unknown> = { "type": "string", "default": null };
    let properties: map<string, unknown> = { "city": city };
    let required: string[] = [];
    let schema: map<string, unknown> = {
        "type": "object",
        "properties": properties,
        "required": required,
        "const": null,
    };
    let toolbox = ai.tools.Toolbox.new(
        [
            ai.tools.raw_tool(
                "lookup",
                "Look something up",
                schema,
                (args: map<string, unknown>) -> {
                    let out = "ok";
                    out
                },
            ),
        ],
    );
    let cl = openai.ResponsesClient.new(model = "gpt-4o-mini", api_key = "test-key");
    let req = openai.internal.openai_render(cl, _oai_input(spec, ai.Journal.new(spec), toolbox));
    req.body
}

test "oai_tool_schema_keeps_nulls" {
    let got = oai_tool_schema_keeps_nulls();
    assert.contains(got, `"default":null`);
    assert.contains(got, `"const":null`);
    // the TYPED layer is still pruned: no unset optional reaches the wire
    assert.is_true(!got.includes(`"temperature":null`));
    assert.is_true(!got.includes(`"reasoning":null`));
    assert.contains(got, `"tool_choice":"auto"`)
}

// ai.PromptMessage.metadata is forwarded verbatim onto the LAST content item of
// the message it was authored on (the engine's content-part placement); a
// message with no directives stays clean.
function oai_message_metadata_on_content_part() -> string {
    let spec = OaiMetaEcho@spec("Say pong.");
    let cl = openai.ResponsesClient.new(model = "gpt-4o-mini", api_key = "test-key");
    let empty = ai.tools.Toolbox.new([]);
    let req = openai.internal.openai_render(cl, _oai_input(spec, ai.Journal.new(spec), empty));
    let body = baml.json.parse(req.body);
    [
        `system_role=${baml.json.path_or<string>(body, ".input[0].role", "-")}`,
        `system_hint=${baml.json.path_or<string>(body, ".input[0].content[0].cache_hint", "-")}`,
        `user_hint=${baml.json.path_or<string>(body, ".input[1].content[0].cache_hint", "-")}`,
    ].join("|")
}

test "oai_message_metadata_on_content_part" {
    assert.equal(
        oai_message_metadata_on_content_part(),
        "system_role=system|system_hint=ephemeral|user_hint=-",
    )
}

// ─── response parsing ────────────────────────────────────────────────────────

// total/cached/reasoning token details reach ai.events.Usage.
function oai_usage_details() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_oai_text_response())],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ResponsesClient.new(
                model = "gpt-4o-mini",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            let result = ai.Agent<string>.new(client = cl).run(OaiEcho@spec("Say pong."));
            [
                result.usage.input_tokens.to_string(),
                result.usage.output_tokens.to_string(),
                (result.usage.cached_input_tokens ?? -1).to_string(),
                (result.usage.reasoning_tokens ?? -1).to_string(),
            ].join("|")
        },
    )
}

test "oai_usage_details" {
    assert.equal(oai_usage_details(), "3|1|2|7")
}

// A `usage` object with no counts at all still parses (the fields are optional
// on purpose) and contributes zeroes.
function oai_sparse_usage() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_oai_sparse_usage_response())],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ResponsesClient.new(
                model = "gpt-4o-mini",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            let result = ai.Agent<string>.new(client = cl).run(OaiEcho@spec("Say pong."));
            `${result.value}|${result.usage.input_tokens}|${result.usage.output_tokens}`
        },
    )
}

test "oai_sparse_usage" {
    assert.equal(oai_sparse_usage(), "pong|0|0")
}

// Malformed `arguments` on a function_call item is PROVIDER-malformed output:
// it must arrive as ai.errors.ParseFailed, not as the raw baml JsonParseError
// escaping through the client's catch_all as an untyped UnknownError.
function _oai_invoke_text(cl: openai.ResponsesClient, input: ai.ModelTurnInput) -> string {
    cl.invoke(input).terminal_text() ?? ""
}

function oai_bad_tool_arguments() -> string {
    let body = `{"status":"completed","output":[{"type":"function_call","id":"fc_1","call_id":"call_1","name":"lookup","arguments":"not json"}]}`;
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(body)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ResponsesClient.new(
                model = "gpt-4o-mini",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            let spec = OaiEcho@spec("Say pong.");
            let empty = ai.tools.Toolbox.new([]);
            _oai_invoke_text(cl, _oai_input(spec, ai.Journal.new(spec), empty)) catch_all (e) {
                _ => _oai_error_name(e),
            }
        },
    )
}

test "oai_bad_tool_arguments" {
    assert.equal(oai_bad_tool_arguments(), "parse_failed")
}

// A provider error status is classified, never reported as a transport failure.
function _oai_run(cl: openai.ResponsesClient) -> string {
    let result = ai.Agent<string>.new(client = cl).run(OaiEcho@spec("Say pong."));
    result.value
}

function oai_http_error_taxonomy() -> string {
    root.llm_mock.mock_json_serve(
        [
            root.llm_mock.MockResponse.error(429, `{"error":{"message":"slow down"}}`),
            root.llm_mock.MockResponse.error(400, `{"error":{"message":"bad request"}}`),
        ],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ResponsesClient.new(
                model = "gpt-4o-mini",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            let first = _oai_run(cl) catch_all (e) {
                _ => _oai_error_name(e),
            };
            let second = _oai_run(cl) catch_all (e) {
                _ => _oai_error_name(e),
            };
            `${first}|${second}`
        },
    )
}

test "oai_http_error_taxonomy" {
    assert.equal(oai_http_error_taxonomy(), "rate_limited|invalid_request:http 400")
}

// ─── streaming ───────────────────────────────────────────────────────────────

// Text deltas accumulate into the final value; the terminal event carries the
// usage and stop reason.
function oai_stream_text() -> string {
    let completed = `{"type":"response.completed","response":{"status":"completed","model":"gpt-4o-mini","output":[{"type":"message","content":[{"type":"output_text","text":"pong"}]}],"usage":{"input_tokens":3,"output_tokens":1}}}`;
    root.llm_mock.mock_sse_serve(
        [
            root.llm_mock.sse_data(
                `{"type":"response.created","response":{"status":"in_progress","model":"gpt-4o-mini"}}`,
            ),
            root.llm_mock.sse_data(
                `{"type":"response.output_text.delta","item_id":"msg_1","delta":"po"}`,
            ),
            root.llm_mock.sse_data(
                `{"type":"response.output_text.delta","item_id":"msg_1","delta":"ng"}`,
            ),
            root.llm_mock.sse_data(
                `{"type":"response.output_text.done","item_id":"msg_1","text":"pong"}`,
            ),
            root.llm_mock.sse_data(completed),
        ],
        (mock: root.llm_mock.MockSseServer) -> {
            let cl = openai.ResponsesClient.new(
                model = "gpt-4o-mini",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            let stream = ai.stream.from_spec<string | null, string>(
                OaiEcho@spec("Say pong."),
                client = cl,
            );
            let value = stream.final();
            let sent = mock.last_request();
            [value, sent.body].join("~~")
        },
    )
}

test "oai_stream_text" {
    let got = oai_stream_text();
    // `output_text.done` must NOT duplicate the deltas that already arrived
    assert.is_true(got.starts_with("pong~~"));
    assert.contains(got, `"stream":true`)
}

// One streamed run to completion, as a plain call so a `catch_all` at the call
// site is unambiguous.
function _oai_stream_final(cl: openai.ResponsesClient) -> string {
    let stream = ai.stream.from_spec<string | null, string>(
        OaiEcho@spec("Say pong."),
        client = cl,
    );
    stream.final()
}

function oai_stream_time_to_first_token_timeout_ms() -> string {
    let completed = `{"type":"response.completed","response":{"status":"completed","output":[],"usage":{"input_tokens":3,"output_tokens":1}}}`;
    root.llm_mock.mock_sse_serve(
        [root.llm_mock.sse_data(completed)],
        (mock: root.llm_mock.MockSseServer) -> {
            let cl = openai.ResponsesClient.new(
                model = "gpt-4o-mini",
                api_key = "test-key",
                base_url = mock.base_url(),
                request_timeout_ms = 1000,
                time_to_first_token_timeout_ms = 20,
            );
            let outcome = _oai_stream_final(cl) catch_all (e) {
                let timeout: baml.errors.Timeout => "Timeout",
                _ => `other:${e.to_string()}`,
            };
            `${cl.time_to_first_token_timeout_ms ?? -1}|${outcome}`
        },
        first_event_delay_ms = 200,
    )
}

test "oai_stream_time_to_first_token_timeout_ms" {
    assert.equal(oai_stream_time_to_first_token_timeout_ms(), "20|Timeout")
}

// A stream whose text arrives ONLY as `output_text.done` still yields it.
function oai_stream_text_done_only() -> string {
    let completed = `{"type":"response.completed","response":{"status":"completed","output":[],"usage":{"input_tokens":3,"output_tokens":1}}}`;
    root.llm_mock.mock_sse_serve(
        [
            root.llm_mock.sse_data(
                `{"type":"response.output_text.done","item_id":"msg_1","text":"pong"}`,
            ),
            root.llm_mock.sse_data(completed),
        ],
        (mock: root.llm_mock.MockSseServer) -> {
            let cl = openai.ResponsesClient.new(
                model = "gpt-4o-mini",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            _oai_stream_final(cl)
        },
    )
}

test "oai_stream_text_done_only" {
    assert.equal(oai_stream_text_done_only(), "pong")
}

// `response.failed` is an ERROR, not a completed turn: the old decoder ran the
// ordinary parse and emitted TurnDone, so a failed run looked empty.
function oai_stream_failed_event() -> string {
    let failed = `{"type":"response.failed","response":{"status":"failed","error":{"code":"server_error","message":"the model failed"}}}`;
    root.llm_mock.mock_sse_serve(
        [
            root.llm_mock.sse_data(
                `{"type":"response.output_text.delta","item_id":"msg_1","delta":"po"}`,
            ),
            root.llm_mock.sse_data(failed),
        ],
        (mock: root.llm_mock.MockSseServer) -> {
            let cl = openai.ResponsesClient.new(
                model = "gpt-4o-mini",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            _oai_stream_final(cl) catch_all (e) {
                _ => _oai_error_name(e),
            }
        },
    )
}

test "oai_stream_failed_event" {
    // "server_error" is the docs' own `response.failed` example: a TRANSIENT
    // provider failure that happens to be delivered inside a 200 body. It
    // classifies exactly like the HTTP 500 form (NetworkFailure), so a Retry
    // policy treats the two envelopes of the same failure alike.
    assert.equal(oai_stream_failed_event(), "network:the model failed")
}

// An in-body failure whose code is NOT transient stays a permanent
// InvalidRequest — the classification is by code, not by envelope.
function oai_stream_failed_event_permanent() -> string {
    let failed = `{"type":"response.failed","response":{"status":"failed","error":{"code":"invalid_prompt","message":"bad prompt"}}}`;
    root.llm_mock.mock_sse_serve(
        [root.llm_mock.sse_data(failed)],
        (mock: root.llm_mock.MockSseServer) -> {
            let cl = openai.ResponsesClient.new(
                model = "gpt-4o-mini",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            _oai_stream_final(cl) catch_all (e) {
                _ => _oai_error_name(e),
            }
        },
    )
}

test "oai_stream_failed_event_permanent" {
    assert.equal(oai_stream_failed_event_permanent(), "invalid_request:bad prompt")
}

// A stream that dies mid-turn — one delta, then the socket closes with no
// terminal event — is a TRUNCATED turn. The Responses protocol always ends a
// live turn with response.completed/incomplete/failed, so the decoder opts into
// ai.stream's strict termination contract and the partial text is reported as
// the transport failure it is instead of being handed back as a good answer.
function oai_stream_truncated() -> string {
    root.llm_mock.mock_sse_serve(
        [
            root.llm_mock.sse_data(
                `{"type":"response.output_text.delta","item_id":"msg_1","content_index":0,"delta":"par"}`,
            ),
        ],
        (mock: root.llm_mock.MockSseServer) -> {
            let cl = openai.ResponsesClient.new(
                model = "gpt-4o-mini",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            _oai_stream_final(cl) catch_all (e) {
                _ => _oai_error_name(e),
            }
        },
    )
}

test "oai_stream_truncated" {
    assert.contains(
        oai_stream_truncated(),
        "network:stream ended before the provider's terminal event",
    )
}

// The stop reason the decoder computes on the terminal event reaches the
// completed turn through ai.stream.TurnMeta — a streamed refusal and a
// max-tokens truncation are NOT StopReason.Complete.
function _oai_stream_turn(cl: openai.ResponsesClient) -> string {
    let spec = OaiEcho@spec("Say pong.");
    let empty = ai.tools.Toolbox.new([]);
    let stream = cl.invoke_stream(_oai_input(spec, ai.Journal.new(spec), empty));
    let turn = stream.final_turn();
    `${turn.stop_reason}|${turn.terminal_text() ?? ""}`
}

function oai_stream_refusal() -> string {
    let completed = `{"type":"response.completed","response":{"status":"completed","output":[{"type":"message","content":[{"type":"refusal","refusal":"I cannot help with that."}]}]}}`;
    root.llm_mock.mock_sse_serve(
        [root.llm_mock.sse_data(completed)],
        (mock: root.llm_mock.MockSseServer) -> {
            let cl = openai.ResponsesClient.new(
                model = "gpt-4o-mini",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            _oai_stream_turn(cl)
        },
    )
}

test "oai_stream_refusal" {
    // no refusal DELTA event exists, so the turn's text is empty — what must
    // not happen is an empty turn reported as a complete answer.
    assert.equal(oai_stream_refusal(), "Refused|")
}

function oai_stream_incomplete() -> string {
    let incomplete = `{"type":"response.incomplete","response":{"status":"incomplete","incomplete_details":{"reason":"max_output_tokens"},"output":[{"type":"message","content":[{"type":"output_text","text":"par"}]}],"usage":{"input_tokens":3,"output_tokens":9}}}`;
    root.llm_mock.mock_sse_serve(
        [
            root.llm_mock.sse_data(
                `{"type":"response.output_text.delta","item_id":"msg_1","content_index":0,"delta":"par"}`,
            ),
            root.llm_mock.sse_data(incomplete),
        ],
        (mock: root.llm_mock.MockSseServer) -> {
            let cl = openai.ResponsesClient.new(
                model = "gpt-4o-mini",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            _oai_stream_turn(cl)
        },
    )
}

test "oai_stream_incomplete" {
    assert.equal(oai_stream_incomplete(), "MaxTokens|par")
}

// A bare `error` event mid-stream is classified from its own code/message.
function oai_stream_error_event() -> string {
    root.llm_mock.mock_sse_serve(
        [
            root.llm_mock.sse_data(
                `{"type":"error","code":"rate_limit_exceeded","message":"slow down"}`,
            ),
        ],
        (mock: root.llm_mock.MockSseServer) -> {
            let cl = openai.ResponsesClient.new(
                model = "gpt-4o-mini",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            _oai_stream_final(cl) catch_all (e) {
                _ => _oai_error_name(e),
            }
        },
    )
}

test "oai_stream_error_event" {
    assert.equal(oai_stream_error_event(), "rate_limited")
}

// Open a stream below the runner and drain it — the client interface directly,
// no prompt rendering or SAP involved.
function _oai_open_stream(cl: openai.ResponsesClient, input: ai.ModelTurnInput) -> string {
    let stream = cl.invoke_stream(input);
    let turn = stream.final_turn();
    turn.terminal_text() ?? ""
}

// A non-2xx opening the SSE connection is classified exactly like the
// non-streaming path — NOT a retry-safe NetworkFailure, which would make a
// retry policy replay an unauthorized request forever.
function oai_stream_status_error() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.error(401, `{"error":{"message":"bad key"}}`)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ResponsesClient.new(
                model = "gpt-4o-mini",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            let spec = OaiEcho@spec("Say pong.");
            let empty = ai.tools.Toolbox.new([]);
            let input = _oai_input(spec, ai.Journal.new(spec), empty);
            _oai_open_stream(cl, input) catch_all (e) {
                _ => _oai_error_name(e),
            }
        },
    )
}

test "oai_stream_status_error" {
    assert.equal(oai_stream_status_error(), "invalid_request:http 401")
}

// The decoder buffers streamed function-call arguments across batches (the
// stream event types cannot carry a tool call yet — PLAN §6), and a
// `output_text.done` for an item that never streamed a delta emits it once.
function oai_stream_decoder_buffers() -> string {
    let state = openai.internal.OaStreamState.new();
    let first: map<string, unknown> = {
        "type": "response.function_call_arguments.delta",
        "item_id": "fc_1",
        "delta": "{\"city\":",
    };
    let second: map<string, unknown> = {
        "type": "response.function_call_arguments.delta",
        "item_id": "fc_1",
        "delta": "\"paris\"}",
    };
    let done: map<string, unknown> = {
        "type": "response.output_text.done",
        "item_id": "msg_1",
        "text": "hello",
    };
    let batch_one = openai.internal._openai_decode_batch(
        _oai_batch([_oai_json(first), _oai_json(second)]),
        state,
    );
    let batch_two = openai.internal._openai_decode_batch(_oai_batch([_oai_json(done)]), state);
    [
        state.call_arguments.get("fc_1") ?? "",
        batch_one.length().to_string(),
        batch_two.length().to_string(),
        _oai_first_text(batch_two),
        state.model ?? "unset",
    ].join("|")
}

test "oai_stream_decoder_buffers" {
    assert.equal(oai_stream_decoder_buffers(), "{\"city\":\"paris\"}|0|1|hello|unset")
}

// The dedup key is the CONTENT PART, not the item: a message with two text
// parts whose second part arrives only as `output_text.done` must still emit
// it, even though part 0 already streamed a delta.
function oai_stream_content_index_parts() -> string {
    let state = openai.internal.OaStreamState.new();
    let delta: map<string, unknown> = {
        "type": "response.output_text.delta",
        "item_id": "msg_1",
        "content_index": 0,
        "delta": "one",
    };
    let done_zero: map<string, unknown> = {
        "type": "response.output_text.done",
        "item_id": "msg_1",
        "content_index": 0,
        "text": "one",
    };
    let done_one: map<string, unknown> = {
        "type": "response.output_text.done",
        "item_id": "msg_1",
        "content_index": 1,
        "text": "two",
    };
    let events = openai.internal._openai_decode_batch(
        _oai_batch([_oai_json(delta), _oai_json(done_zero), _oai_json(done_one)]),
        state,
    );
    let texts: string[] = [];
    for (let e in events) {
        if let d: ai.stream.TextDelta = e {
            let _ = texts.push(d.text);
        };
    }
    texts.join("+")
}

test "oai_stream_content_index_parts" {
    // part 0's `done` is suppressed (its delta already arrived), part 1's is not
    assert.equal(oai_stream_content_index_parts(), "one+two")
}

// ─── missing credential ──────────────────────────────────────────────────────
//
// A missing OpenAI key must be a CATCHABLE `ai.errors.Failure`, not a panic.
// `resolved_api_key` used to end in `baml.env.get_or_panic("OPENAI_API_KEY")`,
// and a panic unwinds straight past `catch_all` — so an unconfigured
// deployment took the process down instead of reaching this client's own
// `invoke` normalize wrapper, and no `ai.Fallback` / `ai.Retry` composition
// could route around it.
//
// The environment decides which branch runs: CI has no key and the typed error
// fires; a live shell has OPENAI_API_KEY and the canonical fallback resolves
// it. `base_url` points at a closed port, so neither branch reaches the
// network. Both assert the same thing — no panic.
function _oai_missing_key_client() -> openai.ResponsesClient {
    // An explicit env ref naming a variable that exists nowhere resolves to
    // null at REQUEST time and the client falls through to OPENAI_API_KEY.
    openai.ResponsesClient.new(
        model = "gpt-4o-mini",
        api_key = baml.env.ref("BAML_OPENAI_KEY_THAT_IS_NEVER_SET"),
        base_url = "http://127.0.0.1:1",
    )
}

function oai_missing_key_resolve() -> string {
    let outcome = _oai_missing_key_client().resolved_api_key() catch_all (e) {
        _ => `caught:${e.to_string()}`,
    };
    if (baml.env.get("OPENAI_API_KEY") != null) {
        // the canonical variable is present: the fallback resolves it
        let resolved = if (outcome.starts_with("caught:")) {
            outcome
        } else {
            "resolved-from-env"
        };
        resolved
    } else {
        outcome
    }
}

test "oai_missing_key_resolve" {
    assert.is_true(
        oai_missing_key_resolve() == "resolved-from-env"
            || oai_missing_key_resolve().includes("OPENAI_API_KEY"),
    )
}

// The same misconfiguration through `invoke`: caught at the client boundary and
// classified, which is what wrapper-client composition needs.
function oai_missing_key_invoke() -> string {
    let cl = _oai_missing_key_client();
    let input = _oai_input(OaiEcho@spec("Say pong."), ai.Journal { log: [] }, ai.tools.Toolbox.new([]));
    // The concrete classes, not the `ai.errors.Failure` interface: a
    // configuration error and a transport error are both typed, and matching
    // them by class is what proves the failure was CLASSIFIED rather than
    // escaping as a raw panic or an unknown.
    let outcome = `turn:${cl.invoke(input).terminal_text() ?? ""}` catch_all (e) {
        let f: ai.errors.InvalidRequest => `caught:InvalidRequest|${f.detail}`,
        let f: ai.errors.NetworkFailure => `caught:NetworkFailure|${f.detail}`,
        _ => `caught-unclassified:${e.to_string()}`,
    };
    if (baml.env.get("OPENAI_API_KEY") != null) {
        // a key resolved: the request fails on transport against the closed
        // port instead, which is still a caught, typed failure.
        let labeled = `key-present:${outcome}`;
        labeled
    } else {
        outcome
    }
}

test "oai_missing_key_invoke" {
    // never a completed turn, never a panic past catch_all, never untyped
    assert.contains(oai_missing_key_invoke(), "caught:");
    assert.is_true(
        oai_missing_key_invoke().starts_with("key-present:")
            || oai_missing_key_invoke().includes("OPENAI_API_KEY"),
    )
}

// ─── live ────────────────────────────────────────────────────────────────────
//
// Real OpenAI calls, excluded by the default profile and registered only when
// the key is present. Run with:
//
//   cd /Users/aaron/projects/baml
//   infisical run -- baml_language/target/debug/baml-cli test \
//       --from baml_language/crates/baml_tests/baml_src --profile live \
//       -i "::live::openai_responses"

function _oai_live_client() -> openai.ResponsesClient {
    openai.ResponsesClient.new(
        model = "gpt-4o-mini",
        temperature = 0.0,
        max_output_tokens = 32,
    )
}

// The whole wire path against the real endpoint: typed body, auth, parse.
function oai_live_answers() -> bool {
    let result = ai.Agent<string>.new(client = _oai_live_client())
        .run(OaiEcho@spec("Say pong."));
    result.value.trim().length() > 0 && result.usage.output_tokens > 0
}

function oai_live_streams() -> bool {
    let stream = ai.stream.from_spec<string | null, string>(
        OaiEcho@spec("Say pong."),
        client = _oai_live_client(),
    );
    stream.final().trim().length() > 0
}

// Image generation is billed per image, so it stays behind a second, explicit
// gate: OPENAI_LIVE_IMAGE=1.
function oai_live_generates_image() -> bool {
    let cl = openai.ResponsesClient.new(model = "gpt-4o-mini");
    let picture = ai.Agent<image>.new(client = cl).run(OaiPicture@spec("a single red apple"));
    picture.value.base64().length() > 0
}

testset "live" with testing.Sequential() {
    // Always registered: keeps a keyless `--profile live` selection non-empty.
    test "preconditions" {
        assert.is_true(true)
    }

    if (baml.env.get("OPENAI_API_KEY") != null) {
        test "openai_responses" {
            assert.is_true(oai_live_answers())
        }

        test "openai_responses_stream" {
            assert.is_true(oai_live_streams())
        }

        if (baml.env.get("OPENAI_LIVE_IMAGE") != null) {
            test "openai_responses_image" {
                assert.is_true(oai_live_generates_image())
            }
        } else {
            log.warn("live::openai_responses_image not registered: OPENAI_LIVE_IMAGE is unset")
        }
    } else {
        log.warn("live::openai_responses not registered: OPENAI_API_KEY is unset")
    }
}
