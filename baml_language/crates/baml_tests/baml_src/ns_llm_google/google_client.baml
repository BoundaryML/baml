// Wire-level tests for the two Google clients: `google.GoogleClient` (the
// Gemini Developer API) and `google.VertexClient` (Gemini on Vertex AI).
//
// Everything here is offline: request shapes are asserted against the captured
// request of a `root.llm_mock` server, SSE decoding replays canned events, and
// the Vertex URL builders are called directly (they are pure string work once
// project and location are given explicitly). Real endpoint coverage is the
// `live` testset at the bottom, registered only when its key is present.
//
// Per the corpus convention every case body is a top-level `function` and the
// `test` block only asserts (test-block local-boxing VM bug); see
// ../ns_llm_mock/README.md.

// ─── specs under test ────────────────────────────────────────────────────────

// Two system messages on purpose: Google MERGES them into one
// `systemInstruction` (PLAN §5) rather than demoting the second to a user turn.
function GoogleEcho(question: string) -> string {
    client: "google/gemini-2.5-flash"
    prompt: `${role("system")}Be brief.${role("system")}Answer in one word.${role("user")}${question}`
}

/// Report the weather for a city.
function weather_lookup(city: string) -> string {
    let answer = `sunny in ${city}`;
    answer
}

function GoogleToolEcho(question: string) -> string {
    client: "google/gemini-2.5-flash"
    tools: [weather_lookup]
    prompt: `${role("user")}${question}`
}

// One completed non-streaming Gemini turn.
function _canned_turn() -> string {
    let body = `{"candidates":[{"content":{"role":"model","parts":[{"text":"pong"}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":3,"candidatesTokenCount":1,"totalTokenCount":4},"modelVersion":"gemini-2.5-flash"}`;
    body
}

// ─── Gemini: request shape ───────────────────────────────────────────────────

// The full option surface on the wire: URL, credential header, user headers and
// query params, merged systemInstruction, typed generationConfig, safety
// settings.
function google_request_shape() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_canned_turn())],
        (mock: root.llm_mock.MockJsonServer) -> {
            let extra_headers: map<string, string> = { "X-Custom": "yes" };
            let extra_query: map<string, string> = { "trace": "abc" };
            let cl = google.GoogleClient.new(
                model = "gemini-2.5-flash",
                api_key = "test-key",
                base_url = mock.base_url(),
                headers = extra_headers,
                query_params = extra_query,
                temperature = 0.25,
                max_output_tokens = 64,
                top_p = 0.9,
                top_k = 32,
                stop_sequences = ["STOP!"],
                safety_settings = [
                    google.SafetySetting.new("HARM_CATEGORY_HARASSMENT", "BLOCK_ONLY_HIGH"),
                ],
            );
            let result = ai.Agent.new(client = cl).run(GoogleEcho@spec("Say pong."));
            let sent = mock.last_request();
            let body = sent.json();
            [
                result.value,
                sent.method,
                sent.path(),
                sent.query() ?? "",
                sent.header("x-goog-api-key") ?? "",
                sent.header("content-type") ?? "",
                sent.header("x-custom") ?? "",
                baml.json.path<string>(body, ".systemInstruction.parts[0].text"),
                baml.json.path<string>(body, ".systemInstruction.parts[1].text"),
                baml.json.path<string>(body, ".contents[0].role"),
                baml.json.path<string>(body, ".contents[0].parts[0].text"),
                baml.json.path<float>(body, ".generationConfig.temperature").to_string(),
                baml.json.path<int>(body, ".generationConfig.maxOutputTokens").to_string(),
                baml.json.path<float>(body, ".generationConfig.topP").to_string(),
                baml.json.path<int>(body, ".generationConfig.topK").to_string(),
                baml.json.path<string>(body, ".generationConfig.stopSequences[0]"),
                baml.json.path<string>(body, ".safetySettings[0].category"),
                baml.json.path<string>(body, ".safetySettings[0].threshold"),
            ].join("|")
        },
    )
}

test "google_request_shape" {
    let got = google_request_shape();
    assert.contains(got, "pong|POST|/models/gemini-2.5-flash:generateContent|trace=abc|test-key|");
    assert.contains(got, "application/json|yes|Be brief.|Answer in one word.|user|");
    assert.contains(got, "Say pong.");
    assert.contains(got, "|0.25|64|0.9|32|STOP!|HARM_CATEGORY_HARASSMENT|BLOCK_ONLY_HIGH")
}

// A client with no options set sends `contents` and nothing else: every unset
// field is ABSENT from the body, never present as null. Gemini's `Part` is a
// union, so `{"text": "hi", "inlineData": null}` is a different request.
function google_request_omits_unset_fields() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_canned_turn())],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = google.GoogleClient.new(
                model = "gemini-2.5-flash",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            let _ = ai.Agent.new(client = cl).run(GoogleEcho@spec("Say pong."));
            mock.last_request().body
        },
    )
}

test "google_request_omits_unset_fields" {
    let body = google_request_omits_unset_fields();
    assert.contains(body, "\"systemInstruction\"");
    assert.is_true(!body.includes("null"));
    assert.is_true(!body.includes("\"tools\""));
    assert.is_true(!body.includes("\"generationConfig\""));
    assert.is_true(!body.includes("\"safetySettings\""))
}

// `request_body` is merged LAST and merges into maps key-by-key: an override
// reaches a field the typed options do not model without erasing the ones they
// do, and a null override DELETES a key the client injected.
function google_request_body_passthrough() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_canned_turn())],
        (mock: root.llm_mock.MockJsonServer) -> {
            let overrides = baml.json.parse(
                `{"generationConfig":{"thinkingConfig":{"thinkingBudget":0},"topK":null},"cachedContent":"projects/p/cachedContents/1"}`,
            );
            let cl = google.GoogleClient.new(
                model = "gemini-2.5-flash",
                api_key = "test-key",
                base_url = mock.base_url(),
                request_body = overrides,
                temperature = 0.5,
                top_k = 40,
            );
            let _ = ai.Agent.new(client = cl).run(GoogleEcho@spec("Say pong."));
            let body = mock.last_request().json();
            [
                baml.json.path<float>(body, ".generationConfig.temperature").to_string(),
                baml.json.path<int>(body, ".generationConfig.thinkingConfig.thinkingBudget").to_string(),
                baml.json.path_or<int>(body, ".generationConfig.topK", -1).to_string(),
                baml.json.path<string>(body, ".cachedContent"),
            ].join("|")
        },
    )
}

test "google_request_body_passthrough" {
    assert.equal(
        google_request_body_passthrough(),
        "0.5|0|-1|projects/p/cachedContents/1",
    )
}

// A toolbox lowers to `tools[0].functionDeclarations` with the JSON Schema
// under `parametersJsonSchema` (the unconstrained field, not the OpenAPI
// subset), plus `toolConfig.functionCallingConfig.mode`.
function google_tool_declarations() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_canned_turn())],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = google.GoogleClient.new(
                model = "gemini-2.5-flash",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            let _ = ai.Agent.new(client = cl).run(GoogleToolEcho@spec("Weather in NYC?"));
            let body = mock.last_request().json();
            [
                baml.json.path<string>(body, ".tools[0].functionDeclarations[0].name"),
                baml.json.path<string>(
                    body,
                    ".tools[0].functionDeclarations[0].parametersJsonSchema.properties.city.type",
                ),
                baml.json.path<string>(body, ".toolConfig.functionCallingConfig.mode"),
            ].join("|")
        },
    )
}

test "google_tool_declarations" {
    assert.equal(google_tool_declarations(), "weather_lookup|string|AUTO")
}

// ─── Gemini: response parsing ────────────────────────────────────────────────

// Thought parts become Reasoning (never the answer), and the complete
// usageMetadata lands in ai.events.Usage — including cachedContentTokenCount
// and thoughtsTokenCount, which the Rust builders dropped.
function google_parse_thoughts_and_usage() -> string {
    let raw = `{"candidates":[{"content":{"parts":[{"text":"weighing it up","thought":true},{"text":"answer"}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":10,"candidatesTokenCount":4,"cachedContentTokenCount":6,"thoughtsTokenCount":7,"totalTokenCount":27}}`;
    let turn = google.internal.google_parse(
        baml.json.from_string<google.internal.GmResponse>(raw),
        0,
    );
    [
        turn.terminal_text() ?? "",
        turn.content.length().to_string(),
        turn.stop_reason.to_string(),
        (turn.usage?.input_tokens ?? -1).to_string(),
        (turn.usage?.output_tokens ?? -1).to_string(),
        (turn.usage?.cached_input_tokens ?? -1).to_string(),
        (turn.usage?.reasoning_tokens ?? -1).to_string(),
    ].join("|")
}

test "google_parse_thoughts_and_usage" {
    assert.equal(google_parse_thoughts_and_usage(), "answer|2|Complete|10|4|6|7")
}

// Function calls carry no id on the wire, so one is synthesized from the
// journal position; the turn stops for tool use.
function google_parse_function_call() -> string {
    let raw = `{"candidates":[{"content":{"parts":[{"functionCall":{"name":"weather_lookup","args":{"city":"NYC"}}}]},"finishReason":"STOP"}]}`;
    let turn = google.internal.google_parse(
        baml.json.from_string<google.internal.GmResponse>(raw),
        7,
    );
    [
        turn.stop_reason.to_string(),
        turn.tool_uses().at(0)?.id ?? "",
        turn.tool_uses().at(0)?.name ?? "",
    ].join("|")
}

test "google_parse_function_call" {
    assert.equal(google_parse_function_call(), "ToolUse|call_7_0|weather_lookup")
}

// The image-out models answer with base64 `inlineData` parts, which become
// ai.content.Media blocks (so a `-> image` function has something to return)
// while the text parts stay text.
function google_parse_inline_media() -> string {
    let raw = `{"candidates":[{"content":{"parts":[{"inlineData":{"mimeType":"image/png","data":"aGk="}},{"text":"here you go"}]},"finishReason":"STOP"}]}`;
    let turn = google.internal.google_parse(
        baml.json.from_string<google.internal.GmResponse>(raw),
        0,
    );
    let kinds: string[] = [];
    for (let m in turn.media()) {
        match (m) {
            let img: baml.media.Image => {
                kinds.push(`image:${img.mime_type() ?? ""}`);
                null
            },
            _ => {
                kinds.push("other");
                null
            },
        };
    }
    [turn.terminal_text() ?? "", turn.content.length().to_string(), kinds.join(",")].join("|")
}

test "google_parse_inline_media" {
    assert.equal(google_parse_inline_media(), "here you go|2|image:image/png")
}

// A blocked prompt and a safety-stopped candidate both mean Refused; an
// unknown finish reason must NOT be mistaken for one.
function _stop_reason_of(raw: string) -> string {
    let resp = baml.json.from_string<google.internal.GmResponse>(raw);
    google.internal.google_parse(resp, 0).stop_reason.to_string()
}

function google_parse_stop_reasons() -> string {
    let blocked = `{"promptFeedback":{"blockReason":"SAFETY"}}`;
    let safety = `{"candidates":[{"content":{"parts":[{"text":""}]},"finishReason":"SAFETY"}]}`;
    let truncated = `{"candidates":[{"content":{"parts":[{"text":"half"}]},"finishReason":"MAX_TOKENS"}]}`;
    let unknown = `{"candidates":[{"content":{"parts":[{"text":"ok"}]},"finishReason":"SOMETHING_NEW"}]}`;
    [
        _stop_reason_of(blocked),
        _stop_reason_of(safety),
        _stop_reason_of(truncated),
        _stop_reason_of(unknown),
    ].join("|")
}

test "google_parse_stop_reasons" {
    assert.equal(google_parse_stop_reasons(), "Refused|Refused|MaxTokens|Complete")
}

// A response that carries no turn at all — `{}` from a broken proxy, an empty
// candidate list, an `{"error": …}` document served with a 200 — is a FAILURE,
// not an empty answer. A finishReason with empty content IS the provider
// explaining itself and still parses.
function _parse_label(raw: string) -> string {
    let resp = baml.json.from_string<google.internal.GmResponse>(raw);
    google.internal.google_parse(resp, 0).stop_reason.to_string() catch_all (e) {
        _ => {
            match (e) {
                let failed: ai.errors.ParseFailed => "ParseFailed",
                let limited: ai.errors.RateLimited => "RateLimited",
                let bad: ai.errors.InvalidRequest => "InvalidRequest",
                _ => "other",
            }
        },
    }
}

function google_parse_empty_response() -> string {
    [
        _parse_label(`{}`),
        _parse_label(`{"candidates":[]}`),
        _parse_label(`{"candidates":[{}]}`),
        _parse_label(`{"candidates":[{"content":{"parts":[]},"finishReason":"MAX_TOKENS"}]}`),
        _parse_label(`{"error":{"code":429,"message":"quota"}}`),
        _parse_label(`{"promptFeedback":{"blockReason":"SAFETY"}}`),
    ].join("|")
}

test "google_parse_empty_response" {
    assert.equal(
        google_parse_empty_response(),
        "ParseFailed|ParseFailed|ParseFailed|MaxTokens|RateLimited|Refused",
    )
}

// End to end: the same `{}` body must not make a `-> string` function return
// the empty string as a success.
function google_empty_response_e2e() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(`{}`)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = google.GoogleClient.new(
                model = "gemini-2.5-flash",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            ai.Agent.new(client = cl).run(GoogleEcho@spec("Say pong.")).value
                catch_all (e) {
                _ => {
                    match (e) {
                        let failed: ai.errors.ParseFailed => "ParseFailed",
                        _ => "other",
                    }
                },
            }
        },
    )
}

test "google_empty_response_e2e" {
    assert.equal(google_empty_response_e2e(), "ParseFailed")
}

// ─── Gemini 3: thought signatures ────────────────────────────────────────────

// Gemini 3 issues a `thoughtSignature` with every functionCall part and
// REJECTS a replay that does not carry it back (HTTP 400, "Function call is
// missing a thought_signature"), which breaks every multi-turn tool loop.
// `ai.content.ToolUse` has no metadata slot, so the signature rides the
// synthesized call id (`call_<seq>_<i>~gts~<sig>`) through the journal and is
// re-attached to the wire part on replay.
function google_parse_thought_signature() -> string {
    let raw = `{"candidates":[{"content":{"parts":[{"functionCall":{"name":"weather_lookup","args":{"city":"NYC"}},"thoughtSignature":"CtwBAcu98"}]},"finishReason":"STOP"}]}`;
    let turn = google.internal.google_parse(
        baml.json.from_string<google.internal.GmResponse>(raw),
        7,
    );
    turn.tool_uses().at(0)?.id ?? ""
}

test "google_parse_thought_signature" {
    assert.equal(google_parse_thought_signature(), "call_7_0~gts~CtwBAcu98")
}

// The full loop: turn 1 answers with a signed functionCall, the tool runs, and
// turn 2's replayed `contents` carry the signature back on the same part.
function google_tool_loop_replays_signature(signature_json: string) -> string {
    root.llm_mock.mock_json_serve(
        [
            root.llm_mock.MockResponse.ok(
                `{"candidates":[{"content":{"role":"model","parts":[{"functionCall":{"name":"weather_lookup","args":{"city":"NYC"}}${signature_json}}]},"finishReason":"STOP"}]}`,
            ),
            root.llm_mock.MockResponse.ok(
                `{"candidates":[{"content":{"role":"model","parts":[{"text":"sunny in NYC"}]},"finishReason":"STOP"}]}`,
            ),
        ],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = google.GoogleClient.new(
                model = "gemini-3-flash",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            let result = ai.Agent.new(client = cl).run(
                GoogleToolEcho@spec("Weather in NYC?"),
            );
            let replay = mock.last_request().json();
            [
                result.value,
                mock.request_count().to_string(),
                baml.json.path<string>(replay, ".contents[1].role"),
                baml.json.path<string>(replay, ".contents[1].parts[0].functionCall.name"),
                baml.json.path_or<string>(replay, ".contents[1].parts[0].thoughtSignature", "absent"),
                baml.json.path<string>(replay, ".contents[2].parts[0].functionResponse.name"),
            ].join("|")
        },
    )
}

test "google_tool_loop_replays_signature" {
    assert.equal(
        google_tool_loop_replays_signature(`,"thoughtSignature":"CtwBAcu98"`),
        "sunny in NYC|2|model|weather_lookup|CtwBAcu98|weather_lookup",
    )
}

// A model that issues no signature (2.x) must not grow a null/empty
// `thoughtSignature` key on the replayed part.
test "google_tool_loop_without_signature" {
    assert.equal(
        google_tool_loop_replays_signature(""),
        "sunny in NYC|2|model|weather_lookup|absent|weather_lookup",
    )
}

// ─── media lowering ──────────────────────────────────────────────────────────

function GoogleVision(img: image) -> string {
    client: "google/gemini-2.5-flash"
    prompt: `${role("user")}Describe this.${img}`
}

function GoogleWatch(clip: video) -> string {
    client: "google/gemini-2.5-flash"
    prompt: `${role("user")}Summarize this.${clip}`
}

// Vertex resolves media URLs server-side, so an http(s) URL rides through as
// `fileData.fileUri` (sys_llm's `send_url` default for this backend) instead of
// being fetched and base64-inlined — which also keeps large media out of the
// inline request-size cap.
function vertex_media_url_passthrough() -> string {
    let img = baml.media.Image.from_url("https://example.test/cat.png", null);
    let input = ai.ModelTurnInput {
        prompt: GoogleVision@spec(img).prompt_template,
        journal: ai.Journal { log: [] },
        toolbox: ai.tools.Toolbox.new([]),
        output_type: reflect.Type.of<string>(),
    };
    let cl = google.VertexClient.new(
        model = "gemini-2.5-flash",
        project_id = "my-project",
        location = "us-central1",
        api_key = "express-key",
    );
    let body = baml.json.parse(google.internal.vertex_render(cl, input).body);
    [
        baml.json.path<string>(body, ".contents[0].parts[1].fileData.fileUri"),
        baml.json.path<string>(body, ".contents[0].parts[1].fileData.mimeType"),
        baml.json.path_or<string>(body, ".contents[0].parts[1].inlineData.data", "absent"),
    ].join("|")
}

test "vertex_media_url_passthrough" {
    assert.equal(
        vertex_media_url_passthrough(),
        "https://example.test/cat.png|image/png|absent",
    )
}

// The Developer API has no server-side URL fetch, so an ordinary http(s) URL
// IS downloaded and inlined there.
function google_media_url_inlined() -> string {
    let png_headers: map<string, string> = { "content-type": "image/png" };
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.with_headers(200, "PNGDATA", png_headers)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let img = baml.media.Image.from_url(`${mock.base_url()}/cat.png`, null);
            let input = ai.ModelTurnInput {
        prompt: GoogleVision@spec(img).prompt_template,
        journal: ai.Journal { log: [] },
        toolbox: ai.tools.Toolbox.new([]),
        output_type: reflect.Type.of<string>(),
    };
            let cl = google.GoogleClient.new(
                model = "gemini-2.5-flash",
                api_key = "test-key",
                base_url = "https://example.test",
            );
            let body = baml.json.parse(google.internal.google_render(cl, input).body);
            [
                baml.json.path<string>(body, ".contents[0].parts[1].inlineData.mimeType"),
                baml.json.path<string>(body, ".contents[0].parts[1].inlineData.data"),
                baml.json.path_or<string>(body, ".contents[0].parts[1].fileData.fileUri", "absent"),
            ].join("|")
        },
    )
}

test "google_media_url_inlined" {
    assert.equal(google_media_url_inlined(), "image/png|UE5HREFUQQ==|absent")
}

// A YouTube watch URL is a documented `fileData` input on BOTH backends and
// must never be fetched: downloading it inlines the watch PAGE's HTML as
// base64 "video". No mimeType is guessable from such a URL, so none is sent.
function google_media_youtube_passthrough() -> string {
    let clip = baml.media.Video.from_url("https://www.youtube.com/watch?v=dQw4w9WgXcQ", null);
    let input = ai.ModelTurnInput {
        prompt: GoogleWatch@spec(clip).prompt_template,
        journal: ai.Journal { log: [] },
        toolbox: ai.tools.Toolbox.new([]),
        output_type: reflect.Type.of<string>(),
    };
    let cl = google.GoogleClient.new(
        model = "gemini-2.5-flash",
        api_key = "test-key",
        base_url = "https://example.test",
    );
    let body = baml.json.parse(google.internal.google_render(cl, input).body);
    [
        baml.json.path<string>(body, ".contents[0].parts[1].fileData.fileUri"),
        baml.json.path_or<string>(body, ".contents[0].parts[1].fileData.mimeType", "absent"),
        baml.json.path_or<string>(body, ".contents[0].parts[1].inlineData.data", "absent"),
    ].join("|")
}

test "google_media_youtube_passthrough" {
    assert.equal(
        google_media_youtube_passthrough(),
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ|absent|absent",
    )
}

// ─── Gemini: streaming ───────────────────────────────────────────────────────

// `:streamGenerateContent?alt=sse` fragments are GmResponse documents: text
// deltas accumulate, thought parts are skipped, and the last fragment carries
// finishReason + cumulative usage.
function google_stream_decode() -> string {
    root.llm_mock.mock_sse_serve(
        [
            root.llm_mock.sse_data(`{"candidates":[{"content":{"parts":[{"text":"po"}]}}]}`),
            root.llm_mock.sse_data(
                `{"candidates":[{"content":{"parts":[{"text":"(thinking)","thought":true}]}}]}`,
            ),
            root.llm_mock.sse_data(
                `{"candidates":[{"content":{"role":"model","parts":[{"text":"ng"}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":3,"candidatesTokenCount":1}}`,
            ),
        ],
        (mock: root.llm_mock.MockSseServer) -> {
            let cl = google.GoogleClient.new(
                model = "gemini-2.5-flash",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            let stream = ai.stream.from_spec<string | null, string>(
                GoogleEcho@spec("Say pong."),
                client = cl,
            );
            let final = stream.final();
            let sent = mock.last_request();
            [final, sent.path(), sent.query() ?? ""].join("|")
        },
    )
}

test "google_stream_decode" {
    assert.equal(
        google_stream_decode(),
        "pong|/models/gemini-2.5-flash:streamGenerateContent|alt=sse",
    )
}

// Gemini's SSE has no terminal sentinel: the `finishReason` fragment IS the
// end of the turn. Both stream call sites therefore pass `require_terminal`, so
// a connection cut mid-answer raises instead of handing the caller a silently
// truncated answer labelled Complete.
function _stream_failure_label(events: string[]) -> string {
    root.llm_mock.mock_sse_serve(
        events,
        (mock: root.llm_mock.MockSseServer) -> {
            let cl = google.GoogleClient.new(
                model = "gemini-2.5-flash",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            let stream = ai.stream.from_spec<string | null, string>(
                GoogleEcho@spec("Say pong."),
                client = cl,
            );
            stream.final() catch_all (e) {
                _ => {
                    match (e) {
                        let net: ai.errors.NetworkFailure => "NetworkFailure",
                        let failed: ai.errors.ParseFailed => "ParseFailed",
                        _ => "other",
                    }
                },
            }
        },
    )
}

function google_stream_truncated() -> string {
    _stream_failure_label(
        [root.llm_mock.sse_data(`{"candidates":[{"content":{"parts":[{"text":"po"}]}}]}`)],
    )
}

test "google_stream_truncated" {
    assert.equal(google_stream_truncated(), "NetworkFailure")
}

// A `data:` payload that does not decode is a failure too: dropping it (what
// the decoder used to do) turns a cut or a proxy error page into missing text
// no caller can see.
function google_stream_malformed_payload() -> string {
    _stream_failure_label(
        [
            root.llm_mock.sse_data(`{"candidates":[{"content":{"parts":[{"text":"po"}]}}]}`),
            root.llm_mock.sse_data(`{"candidates":[{"content":{"parts":[{"text":"ng`),
        ],
    )
}

test "google_stream_malformed_payload" {
    assert.equal(google_stream_malformed_payload(), "ParseFailed")
}

// ─── Vertex: URL construction ────────────────────────────────────────────────

// The regional endpoint: host prefixed with the location, project and location
// in the path, `publishers/google/models`.
function vertex_url_regional() -> string {
    let cl = google.VertexClient.new(
        model = "gemini-2.5-flash",
        project_id = "my-project",
        location = "europe-west4",
    );
    google.internal.vertex_url(cl, false)
}

test "vertex_url_regional" {
    assert.equal(
        vertex_url_regional(),
        "https://europe-west4-aiplatform.googleapis.com/v1/projects/my-project/locations/europe-west4/publishers/google/models/gemini-2.5-flash:generateContent",
    )
}

// The `global` location uses the region-less host, and the path keeps
// `locations/global`.
function vertex_url_global() -> string {
    let cl = google.VertexClient.new(
        model = "gemini-2.5-flash",
        project_id = "my-project",
        location = "global",
    );
    google.internal.vertex_url(cl, false)
}

test "vertex_url_global" {
    assert.equal(
        vertex_url_global(),
        "https://aiplatform.googleapis.com/v1/projects/my-project/locations/global/publishers/google/models/gemini-2.5-flash:generateContent",
    )
}

// Streaming appends `?alt=sse`; express mode adds the key as a SECOND query
// parameter, after the caller's own.
function vertex_url_streaming_express() -> string {
    let extra_query: map<string, string> = { "trace": "abc" };
    let cl = google.VertexClient.new(
        model = "gemini-2.5-flash",
        project_id = "my-project",
        location = "us-central1",
        api_key = "express-key",
        query_params = extra_query,
    );
    google.internal.vertex_url(cl, true)
}

test "vertex_url_streaming_express" {
    assert.equal(
        vertex_url_streaming_express(),
        "https://us-central1-aiplatform.googleapis.com/v1/projects/my-project/locations/us-central1/publishers/google/models/gemini-2.5-flash:streamGenerateContent?alt=sse&trace=abc&key=express-key",
    )
}

// An explicit base_url replaces the whole publisher prefix — the caller owns
// the path from there on (this is also how the mock is reached).
function vertex_url_base_url_override() -> string {
    let cl = google.VertexClient.new(
        model = "gemini-2.5-flash",
        base_url = "https://example.test/v1/projects/p/locations/l/publishers/google/models",
        project_id = "ignored",
    );
    google.internal.vertex_url(cl, false)
}

test "vertex_url_base_url_override" {
    assert.equal(
        vertex_url_base_url_override(),
        "https://example.test/v1/projects/p/locations/l/publishers/google/models/gemini-2.5-flash:generateContent",
    )
}

// ─── Vertex: request shape ───────────────────────────────────────────────────

// Express mode is the keyed path: the credential rides the query string, no
// bearer token is minted, and the body is byte-for-byte the Gemini body (Vertex
// takes the model from the URL, so there is no `model` field).
function vertex_request_shape() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_canned_turn())],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = google.VertexClient.new(
                model = "gemini-2.5-flash",
                api_key = "express-key",
                base_url = mock.base_url(),
                temperature = 0.25,
            );
            let result = ai.Agent.new(client = cl).run(GoogleEcho@spec("Say pong."));
            let sent = mock.last_request();
            let body = sent.json();
            [
                result.value,
                sent.method,
                sent.path(),
                sent.query() ?? "",
                sent.header("authorization") ?? "none",
                sent.header("content-type") ?? "",
                baml.json.path<string>(body, ".systemInstruction.parts[0].text"),
                baml.json.path<string>(body, ".contents[0].role"),
                baml.json.path<float>(body, ".generationConfig.temperature").to_string(),
                baml.json.path_or<string>(body, ".model", "absent"),
            ].join("|")
        },
    )
}

test "vertex_request_shape" {
    assert.equal(
        vertex_request_shape(),
        "pong|POST|/gemini-2.5-flash:generateContent|key=express-key|none|application/json|Be brief.|user|0.25|absent",
    )
}

function vertex_request_timeout_ms() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_canned_turn())],
        (mock: root.llm_mock.MockJsonServer) -> {
            let spec = GoogleEcho@spec("Say pong.");
            let input = ai.ModelTurnInput {
                prompt: spec.prompt_template,
                journal: ai.Journal { log: [] },
                toolbox: spec.tools(),
                output_type: spec.output_type(),
            };
            let cl = google.VertexClient.new(
                model = "gemini-2.5-flash",
                api_key = "express-key",
                base_url = mock.base_url(),
                request_timeout_ms = 20,
            );
            let outcome = `turn:${cl.invoke(input).terminal_text() ?? ""}` catch_all (e) {
                let timeout: baml.errors.Timeout => "Timeout",
                _ => `other:${e.to_string()}`,
            };
            `${cl.request_timeout_ms ?? -1}|${outcome}`
        },
        response_delay_ms = 200,
    )
}

test "vertex_request_timeout_ms" {
    assert.equal(vertex_request_timeout_ms(), "20|Timeout")
}

// Claude on Vertex goes to the anthropic publisher's `:rawPredict` with the
// Anthropic Messages body (built by `anthropic.internal`), plus the required
// `anthropic_version` field — and WITHOUT `model`, which on this platform is
// named by the endpoint URL and not passed in the body.
function vertex_claude_raw_predict() -> string {
    let spec = GoogleEcho@spec("Say pong.");
    let input = ai.ModelTurnInput {
        prompt: spec.prompt_template,
        journal: ai.Journal { log: [] },
        toolbox: ai.tools.Toolbox.new([]),
        output_type: reflect.Type.of<string>(),
    };
    let cl = google.VertexClient.new(
        model = "claude-sonnet-4-5",
        project_id = "my-project",
        location = "us-east5",
        api_key = "express-key",
    );
    let req = google.internal.vertex_render(cl, input);
    let body = baml.json.parse(req.body);
    [
        req.url,
        baml.json.path<string>(body, ".anthropic_version"),
        // NOT in the body: the endpoint URL names the model
        // (platform.claude.com/docs/en/api/claude-on-vertex-ai).
        baml.json.path_or<string>(body, ".model", "absent"),
        baml.json.path<int>(body, ".max_tokens").to_string(),
        baml.json.path<string>(body, ".messages[0].role"),
    ].join("|")
}

test "vertex_claude_raw_predict" {
    let got = vertex_claude_raw_predict();
    assert.contains(got, "us-east5-aiplatform.googleapis.com");
    assert.contains(got, "/publishers/anthropic/models/claude-sonnet-4-5:rawPredict");
    assert.contains(got, "|vertex-2023-10-16|absent|8192|user")
}

// …and a caller who wants it back puts it in `request_body`, which merges
// after the Vertex constants.
function vertex_claude_model_override() -> string {
    let spec = GoogleEcho@spec("Say pong.");
    let input = ai.ModelTurnInput {
        prompt: spec.prompt_template,
        journal: ai.Journal { log: [] },
        toolbox: ai.tools.Toolbox.new([]),
        output_type: reflect.Type.of<string>(),
    };
    let cl = google.VertexClient.new(
        model = "claude-sonnet-4-5",
        project_id = "my-project",
        location = "us-east5",
        api_key = "express-key",
        request_body = baml.json.parse(`{"model":"claude-opus-4-1"}`),
    );
    let body = baml.json.parse(google.internal.vertex_render(cl, input).body);
    baml.json.path_or<string>(body, ".model", "absent")
}

test "vertex_claude_model_override" {
    assert.equal(vertex_claude_model_override(), "claude-opus-4-1")
}

// The data-residency multi-region endpoints have their own host shape:
// `aiplatform.{us|eu}.rep.googleapis.com`, not `{loc}-aiplatform`.
function vertex_url_multi_region() -> string {
    let us = google.VertexClient.new(
        model = "gemini-2.5-flash",
        project_id = "my-project",
        location = "us",
    );
    let eu = google.VertexClient.new(
        model = "gemini-2.5-flash",
        project_id = "my-project",
        location = "eu",
    );
    [google.internal.vertex_url(us, false), google.internal.vertex_url(eu, false)].join("|")
}

test "vertex_url_multi_region" {
    assert.equal(
        vertex_url_multi_region(),
        "https://aiplatform.us.rep.googleapis.com/v1/projects/my-project/locations/us/publishers/google/models/gemini-2.5-flash:generateContent|https://aiplatform.eu.rep.googleapis.com/v1/projects/my-project/locations/eu/publishers/google/models/gemini-2.5-flash:generateContent",
    )
}

// An express-mode client resolves NO project: the key is issued for the
// project-less endpoint, and an ambient GOOGLE_CLOUD_PROJECT (set by every GCP
// runtime) must not re-point it at a project-scoped URL the key cannot
// authorize. An explicit project_id option still wins — that IS configuration.
function vertex_express_ignores_ambient_project() -> string {
    let express = google.VertexClient.new(model = "gemini-2.5-flash", api_key = "express-key");
    let configured = google.VertexClient.new(
        model = "gemini-2.5-flash",
        api_key = "express-key",
        project_id = "explicit-project",
    );
    [
        (google.internal.vertex_project_id(express) ?? "none"),
        google.internal.vertex_url(express, false),
        (google.internal.vertex_project_id(configured) ?? "none"),
    ].join("|")
}

test "vertex_express_ignores_ambient_project" {
    assert.equal(
        vertex_express_ignores_ambient_project(),
        "none|https://aiplatform.googleapis.com/v1/publishers/google/models/gemini-2.5-flash:generateContent?key=express-key|explicit-project",
    )
}

// Streaming over Vertex decodes the same fragments as the Developer API.
function vertex_stream_decode() -> string {
    root.llm_mock.mock_sse_serve(
        [
            root.llm_mock.sse_data(`{"candidates":[{"content":{"parts":[{"text":"po"}]}}]}`),
            root.llm_mock.sse_data(
                `{"candidates":[{"content":{"parts":[{"text":"ng"}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":3,"candidatesTokenCount":1}}`,
            ),
        ],
        (mock: root.llm_mock.MockSseServer) -> {
            let cl = google.VertexClient.new(
                model = "gemini-2.5-flash",
                api_key = "express-key",
                base_url = mock.base_url(),
            );
            let stream = ai.stream.from_spec<string | null, string>(
                GoogleEcho@spec("Say pong."),
                client = cl,
            );
            let final = stream.final();
            let sent = mock.last_request();
            [final, sent.path(), sent.query() ?? ""].join("|")
        },
    )
}

test "vertex_stream_decode" {
    assert.equal(
        vertex_stream_decode(),
        "pong|/gemini-2.5-flash:streamGenerateContent|alt=sse&key=express-key",
    )
}

function vertex_stream_time_to_first_token_timeout_ms() -> string {
    root.llm_mock.mock_sse_serve(
        [root.llm_mock.sse_data(`{"candidates":[{"content":{"parts":[{"text":"pong"}]},"finishReason":"STOP"}]}`)],
        (mock: root.llm_mock.MockSseServer) -> {
            // The corpus runs every leaf concurrently, so leave transport
            // headroom while keeping the delayed first event past the TTFT limit.
            let cl = google.VertexClient.new(
                model = "gemini-2.5-flash",
                api_key = "express-key",
                base_url = mock.base_url(),
                request_timeout_ms = 15000,
                time_to_first_token_timeout_ms = 5000,
            );
            let stream = ai.stream.from_spec<string | null, string>(
                GoogleEcho@spec("Say pong."),
                client = cl,
            );
            let outcome = stream.final() catch_all (e) {
                let timeout: baml.errors.Timeout => "Timeout",
                _ => `other:${e.to_string()}`,
            };
            `${cl.time_to_first_token_timeout_ms ?? -1}|${outcome}`
        },
        first_event_delay_ms = 10000,
    )
}

test "vertex_stream_time_to_first_token_timeout_ms" {
    assert.equal(vertex_stream_time_to_first_token_timeout_ms(), "5000|Timeout")
}

// ─── error taxonomy ──────────────────────────────────────────────────────────

// A 400 must classify as InvalidRequest, NOT as a retry-safe NetworkFailure:
// the Retry policy would otherwise replay an invalid request forever. 429 is
// RateLimited and 503 is a genuine NetworkFailure.
function google_error_taxonomy() -> string {
    let outcomes: string[] = [];
    let statuses = [400, 429, 503];
    for (let status in statuses) {
        let label = root.llm_mock.mock_json_serve(
            [root.llm_mock.MockResponse.error(status, `{"error":{"code":${status},"message":"nope"}}`)],
            (mock: root.llm_mock.MockJsonServer) -> {
                let cl = google.GoogleClient.new(
                    model = "gemini-2.5-flash",
                    api_key = "test-key",
                    base_url = mock.base_url(),
                );
                ai.Agent.new(client = cl).run(GoogleEcho@spec("Say pong.")).value
                    catch_all (e) {
                    _ => {
                        match (e) {
                            let bad: ai.errors.InvalidRequest => "InvalidRequest",
                            let limited: ai.errors.RateLimited => "RateLimited",
                            let down: ai.errors.NetworkFailure => "NetworkFailure",
                            _ => "other",
                        }
                    },
                }
            },
        );
        outcomes.push(label);
    }
    outcomes.join("|")
}

test "google_error_taxonomy" {
    assert.equal(google_error_taxonomy(), "InvalidRequest|RateLimited|NetworkFailure")
}

// ─── missing credential ──────────────────────────────────────────────────────
//
// A missing Google key must be a CATCHABLE `ai.errors.Failure`, not a panic.
// This client used to end `resolved_api_key` with
// `baml.env.get_or_panic("GOOGLE_API_KEY")`, and a panic unwinds straight past
// `catch_all` — so an unconfigured deployment took the process down instead of
// reaching the client's own `invoke` normalize wrapper, and no `ai.Fallback` /
// `ai.Retry` composition could route around it.
//
// The environment decides which branch runs: CI has no key and the typed error
// fires; a live shell has GOOGLE_API_KEY and the canonical fallback resolves
// it. `base_url` points at a closed port, so neither branch reaches the
// network. Both assert the same thing — no panic. (Mirrors
// `anthropic_missing_key_resolve` / `_invoke`.)
function _google_missing_key_client() -> google.GoogleClient {
    // `env.NAME` naming a variable that exists nowhere: the ref resolves to
    // null at REQUEST time and the client falls through to GOOGLE_API_KEY.
    google.GoogleClient.new(
        model = "gemini-2.5-flash",
        api_key = env.BAML_GOOGLE_KEY_THAT_IS_NEVER_SET,
        base_url = "http://127.0.0.1:1",
    )
}

function google_missing_key_resolve() -> string {
    let outcome = _google_missing_key_client().resolved_api_key() catch_all (e) {
        _ => `caught:${e.to_string()}`,
    };
    if (baml.env.get("GOOGLE_API_KEY") != null) {
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

test "google_missing_key_resolve" {
    assert.is_true(
        google_missing_key_resolve() == "resolved-from-env"
            || google_missing_key_resolve().includes("GOOGLE_API_KEY"),
    )
}

// The same misconfiguration through `invoke`: caught at the client boundary and
// classified, which is what wrapper-client composition needs.
function google_missing_key_invoke() -> string {
    let cl = _google_missing_key_client();
    let input = ai.ModelTurnInput {
        prompt: GoogleEcho@spec("Say pong.").prompt_template,
        journal: ai.Journal { log: [] },
        toolbox: ai.tools.Toolbox.new([]),
        output_type: reflect.Type.of<string>(),
    };
    // The concrete classes, not the `ai.errors.Failure` interface: a
    // configuration error and a transport error are both typed, and matching
    // them by class is what proves the failure was CLASSIFIED rather than
    // escaping as a raw panic or an unknown.
    let outcome = `turn:${cl.invoke(input).terminal_text() ?? ""}` catch_all (e) {
        let f: ai.errors.InvalidRequest => `caught:InvalidRequest|${f.detail}`,
        let f: ai.errors.NetworkFailure => `caught:NetworkFailure|${f.detail}`,
        _ => `caught-unclassified:${e.to_string()}`,
    };
    if (baml.env.get("GOOGLE_API_KEY") != null) {
        // a key resolved: the request fails on transport against the closed
        // port instead, which is still a caught, typed failure.
        let labeled = `key-present:${outcome}`;
        labeled
    } else {
        outcome
    }
}

test "google_missing_key_invoke" {
    // never a completed turn, never a panic past catch_all, never untyped
    assert.contains(google_missing_key_invoke(), "caught:");
    assert.is_true(
        google_missing_key_invoke().starts_with("key-present:")
            || google_missing_key_invoke().includes("GOOGLE_API_KEY"),
    )
}

// ─── live ────────────────────────────────────────────────────────────────────

function LiveGeminiEcho(question: string) -> string {
    client: "google/gemini-2.5-flash"
    prompt: `
        Reply with exactly one word.
        ${question}
    `
}

function live_gemini_answers() -> bool {
    LiveGeminiEcho("Say pong.").trim().length() > 0
}

// Vertex needs a constructed client (credentials cannot ride a
// "vertex/model" shorthand), so the spec is run through an explicit Agent.
// project_id is deliberately left unset: it resolves from GOOGLE_CLOUD_PROJECT
// or from the credential document itself.
function live_vertex_answers() -> bool {
    let cl = google.VertexClient.new(
        model = "gemini-2.5-flash",
        credentials_json = baml.env.get("INTEG_TESTS_GOOGLE_APPLICATION_CREDENTIALS_CONTENT"),
        location = baml.env.get("GOOGLE_CLOUD_LOCATION") ?? "us-central1",
    );
    let result = ai.Agent.new(client = cl).run(LiveGeminiEcho@spec("Say pong."));
    result.value.trim().length() > 0
}

testset "live" with testing.Sequential() {
    if (baml.env.get("GOOGLE_API_KEY") != null) {
        test "gemini" {
            assert.is_true(live_gemini_answers())
        }
    } else {
        log.warn("live::gemini not registered: GOOGLE_API_KEY is unset")
    }

    if (baml.env.get("INTEG_TESTS_GOOGLE_APPLICATION_CREDENTIALS_CONTENT") != null) {
        test "vertex" {
            assert.is_true(live_vertex_answers())
        }
    } else {
        log.warn(
            "live::vertex not registered: INTEG_TESTS_GOOGLE_APPLICATION_CREDENTIALS_CONTENT is unset",
        )
    }
}

// ─── journal UserContent: media after a tool result ──────────────────────────
//
// A runner that screenshots after every tool call appends an
// `ai.events.UserContent` carrying the image AFTER the tool result. It lowers
// to a `user` content at that position — after the flushed functionResponse —
// with its text and inlineData parts in order, on both backends.

function _google_journal_with_screenshot() -> ai.Journal {
    let args: map<string, unknown> = { "city": "NYC" };
    ai.Journal {
        log: [
            ai.events.AssistantMessage {
                content: [ai.content.ToolUse { id: "call_1", name: "weather_lookup", args: args }],
                client_id: "google/gemini-2.5-flash",
            },
            ai.events.ToolCompleted { id: "call_1", output: "sunny in NYC" },
            ai.events.UserContent.new(
                [
                    "Screenshot after the tool ran:",
                    baml.media.Image.from_base64("SGVsbG8=", "image/png"),
                ],
            ),
        ],
    }
}

function _google_user_content_shape(body: json) -> string {
    [
        baml.json.path<string>(body, ".contents[1].role"),
        baml.json.path<string>(body, ".contents[1].parts[0].functionCall.name"),
        baml.json.path<string>(body, ".contents[2].role"),
        baml.json.path<string>(body, ".contents[2].parts[0].functionResponse.name"),
        baml.json.path<string>(body, ".contents[3].role"),
        baml.json.path<string>(body, ".contents[3].parts[0].text"),
        baml.json.path<string>(body, ".contents[3].parts[1].inlineData.mimeType"),
        baml.json.path<string>(body, ".contents[3].parts[1].inlineData.data"),
        baml.json.path_or<string>(body, ".contents[4].role", "absent"),
    ].join("|")
}

function google_user_content_after_tool_result() -> string {
    let spec = GoogleToolEcho@spec("Weather in NYC?");
    let input = ai.ModelTurnInput {
        prompt: spec.prompt_template,
        journal: _google_journal_with_screenshot(),
        toolbox: spec.tools(),
        output_type: spec.output_type(),
    };
    let cl = google.GoogleClient.new(
        model = "gemini-2.5-flash",
        api_key = "test-key",
        base_url = "https://example.test",
    );
    _google_user_content_shape(baml.json.parse(google.internal.google_render(cl, input).body))
}

test "google_user_content_after_tool_result" {
    assert.equal(
        google_user_content_after_tool_result(),
        "model|weather_lookup|user|weather_lookup|user|Screenshot after the tool ran:|image/png|SGVsbG8=|absent",
    )
}

function vertex_user_content_after_tool_result() -> string {
    let spec = GoogleToolEcho@spec("Weather in NYC?");
    let input = ai.ModelTurnInput {
        prompt: spec.prompt_template,
        journal: _google_journal_with_screenshot(),
        toolbox: spec.tools(),
        output_type: spec.output_type(),
    };
    let cl = google.VertexClient.new(
        model = "gemini-2.5-flash",
        project_id = "my-project",
        location = "us-central1",
        api_key = "express-key",
    );
    _google_user_content_shape(baml.json.parse(google.internal.vertex_render(cl, input).body))
}

test "vertex_user_content_after_tool_result" {
    assert.equal(
        vertex_user_content_after_tool_result(),
        "model|weather_lookup|user|weather_lookup|user|Screenshot after the tool ran:|image/png|SGVsbG8=|absent",
    )
}
