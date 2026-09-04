// Native Anthropic Messages client — offline wire tests.
//
// Everything here runs against the shared mock providers in ../ns_llm_mock/
// (see its README for the authoring rules); the only network-touching cases are
// the `::live::` testset at the bottom, which the default `offline` profile
// excludes and which registers its leaves only when ANTHROPIC_API_KEY is set.
//
// The assertions are ported from sys_llm's `build_request/anthropic.rs` and
// `parse_response/anthropic.rs` unit tests — system extraction, cache_control
// placement, media block shapes, max_tokens default, stop-reason mapping,
// cached-token usage — plus the six native-client bugs the migration fixed.
//
// Corpus convention: every case body is a top-level `function`, `test` blocks
// only assert (test-block locals are boxed by the VM). Function bodies start
// with a statement so the LLM-function token pre-scan short-circuits.

// ─── functions under test ────────────────────────────────────────────────────

function AnthMockAsk(question: string) -> string {
    client: "anthropic/claude-haiku-4-5"
    prompt: `${role("system")}Follow the rules.${role("user")}${question}`
}

// The SYSTEM message carries an Anthropic cache breakpoint (that is where the
// marker below puts the `cache_control` metadata, and what
// `anthropic_cache_control_placement` asserts: `system_cc=ephemeral`,
// `user_cc=-`). `role(...)` builds a metadata-less marker, so the directive is
// authored by interpolating the typed marker class directly.
function AnthMockCached(question: string) -> string {
    client: "anthropic/claude-haiku-4-5"
    prompt: `${ai.Role { name: "system", metadata: { "cache_control": { "type": "ephemeral" } } }}Follow the rules.${role("user")}${question}`
}

// An assistant-first prompt with NO system content: the shape that used to
// produce an API-rejected request whose first message was not `user`.
function AnthAssistantFirst() -> string {
    client: "anthropic/claude-haiku-4-5"
    prompt: `${role("assistant")}I already said hello.`
}

// An ordinary single-user-message prompt, for journal-only assertions.
function AnthMockPlain() -> string {
    client: "anthropic/claude-haiku-4-5"
    prompt: `${role("user")}Hello.`
}

function anth_weather(city: string) -> string {
    let where = city;
    "sunny in " + where
}

function AnthMockTools(question: string) -> string {
    client: "anthropic/claude-haiku-4-5"
    tools: [anth_weather]
    prompt: `${role("user")}${question}`
}

// A leading literal keeps the media part at a known index: the rendered parts
// are ["Look at", <media>].
function AnthMockVision(picture: image) -> string {
    client: "anthropic/claude-haiku-4-5"
    prompt: `${role("user")}Look at ${picture}`
}

function AnthMockListen(clip: audio) -> string {
    client: "anthropic/claude-haiku-4-5"
    prompt: `${role("user")}Listen to ${clip}`
}

// ─── shared helpers ──────────────────────────────────────────────────────────

// A complete non-streaming Messages response with one text block.
function anth_canned_text(text: string, stop_reason: string) -> string {
    let head = `{"id":"msg_1","type":"message","role":"assistant","model":"claude-haiku-4-5","content":[{"type":"text","text":"${text}"}]`;
    `${head},"stop_reason":"${stop_reason}","usage":{"input_tokens":9,"output_tokens":2,"cache_read_input_tokens":4,"cache_creation_input_tokens":6}}`
}

// The classified-failure label a test asserts on. Keeping the taxonomy visible
// is the point: a 4xx must never come back as a retry-safe NetworkFailure.
function anth_failure_label(e: unknown) -> string {
    match (e) {
        let r: ai.errors.RateLimited => "RateLimited",
        let n: ai.errors.NetworkFailure => `NetworkFailure:${n.detail}`,
        let i: ai.errors.InvalidRequest => `InvalidRequest:${i.detail}`,
        let p: ai.errors.ParseFailed => `ParseFailed:${p.raw_output}`,
        let f: ai.errors.FinishReasonError => {
            `FinishReasonError:${f.finish_reason.to_string()}:${f.raw_output}`
        },
        let f: ai.errors.Refused => "Refused",
        _ => `other:${e.to_string()}`,
    }
}

// One SseStream batch string (a JSON array of {event,data,id}) from raw
// `data:` payloads — the input `decode_batch` consumes.
function anth_batch(payloads: string[]) -> string {
    let items: map<string, unknown>[] = [];
    for (let payload in payloads) {
        let item: map<string, unknown> = {};
        let _ = item.set("event", null);
        let _ = item.set("data", payload);
        let _ = item.set("id", null);
        items.push(item);
    }
    baml.json.stringify(baml.json.to_json(items))
}

// Render one request without any server: the client below the runner.
function anth_render_body(
    cl: anthropic.Client,
    spec: ai.FunctionSpec<string>,
    journal: ai.Journal,
) -> json {
    let req = anthropic.internal.render(
        cl,
        ai.ModelTurnInput {
            prompt: spec.prompt_template,
            journal: journal,
            toolbox: spec.tools(),
            output_type: spec.output_type(),
        },
    );
    baml.json.parse(req.body)
}

// The same ModelTurnInput `anth_render_body` builds, for calls that go through
// the ai.Client interface instead of the internal render function.
function anth_turn_input(spec: ai.FunctionSpec<string>) -> ai.ModelTurnInput {
    let s = spec;
    ai.ModelTurnInput {
        prompt: s.prompt_template,
        journal: ai.Journal { log: [] },
        toolbox: s.tools(),
        output_type: s.output_type(),
    }
}

// `invoke` through the interface, as a string, so a caller can catch_all it.
function anth_invoke_label(cl: anthropic.Client, input: ai.ModelTurnInput) -> string {
    let turn = cl.invoke(input);
    `blocks=${turn.content.length()}`
}

function anth_stream_value(cl: anthropic.Client) -> string {
    let stream = ai.stream.from_spec<string | null, string>(
        AnthMockAsk@spec("Say pong."),
        client = cl,
    );
    stream.final()
}

// ─── request shape ───────────────────────────────────────────────────────────

// The default request: POST /v1/messages, pinned anthropic-version, system
// content hoisted out of `messages`, max_tokens 8192 (PLAN §5), no `stream`,
// no `tools`/`tool_choice` for a tool-less spec, and cached tokens read out of
// `usage.cache_read_input_tokens` instead of the hardcoded null.
function anthropic_request_shape() -> string {
    let canned = anth_canned_text("pong", "end_turn");
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(canned)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = anthropic.Client.new(
                model = "claude-haiku-4-5",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            let result = ai.Agent.new(client = cl).run(AnthMockAsk@spec("Say pong."));
            let sent = mock.last_request();
            let body = sent.json();
            [
                `value=${result.value}`,
                `method=${sent.method}`,
                `path=${sent.path()}`,
                `key=${sent.header("x-api-key") ?? "-"}`,
                `version=${sent.header("anthropic-version") ?? "-"}`,
                `model=${baml.json.path_or<string>(body, ".model", "-")}`,
                `max_tokens=${baml.json.path_or<int>(body, ".max_tokens", -1)}`,
                `system_type=${baml.json.path_or<string>(body, ".system[0].type", "-")}`,
                `system_text=${baml.json.path_or<string>(body, ".system[0].text", "-")}`,
                `role0=${baml.json.path_or<string>(body, ".messages[0].role", "-")}`,
                `text0=${baml.json.path_or<string>(body, ".messages[0].content[0].text", "-")}`,
                `has_stream=${sent.body.includes("\"stream\"")}`,
                `has_tools=${sent.body.includes("\"tools\"")}`,
                `has_null=${sent.body.includes("null")}`,
                `cached=${result.usage.cached_input_tokens ?? -1}`,
                `input=${result.usage.input_tokens}`,
            ].join("|")
        },
    )
}

test "anthropic_request_shape" {
    let got = anthropic_request_shape();
    assert.contains(got, "value=pong|");
    assert.contains(got, "method=POST|");
    assert.contains(got, "path=/v1/messages|");
    assert.contains(got, "key=test-key|");
    assert.contains(got, "version=2023-06-01|");
    assert.contains(got, "model=claude-haiku-4-5|");
    // PLAN §5: 8192, not the engine's 4096.
    assert.contains(got, "max_tokens=8192|");
    assert.contains(got, "system_type=text|");
    assert.contains(got, "system_text=Follow the rules.|");
    assert.contains(got, "role0=user|");
    assert.contains(got, "text0=Say pong.|");
    // unset optionals are dropped, never sent as explicit nulls
    assert.contains(got, "has_stream=false|");
    assert.contains(got, "has_tools=false|");
    assert.contains(got, "has_null=false|");
    // usage.cache_read_input_tokens is no longer hardcoded null
    assert.contains(got, "cached=4|");
    assert.contains(got, "input=9")
}

function anthropic_request_timeout_ms() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(anth_canned_text("pong", "end_turn"))],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = anthropic.Client.new(
                model = "claude-haiku-4-5",
                api_key = "test-key",
                base_url = mock.base_url(),
                request_timeout_ms = 20,
            );
            let outcome = anth_invoke_label(cl, anth_turn_input(AnthMockPlain@spec())) catch_all (e) {
                let timeout: baml.errors.Timeout => "Timeout",
                _ => `other:${e.to_string()}`,
            };
            `${cl.request_timeout_ms ?? -1}|${outcome}`
        },
        response_delay_ms = 200,
    )
}

test "anthropic_request_timeout_ms" {
    assert.equal(anthropic_request_timeout_ms(), "20|Timeout")
}

// Typed first-class parameters lower into the typed request; `headers` are
// applied BEFORE the auth header (so anthropic-beta is reachable and
// anthropic-version is overridable, while x-api-key still wins);
// `query_params` reach the URL; and `request_body` deep-merges LAST, with a
// null override deleting the client's own default.
function anthropic_params_and_passthrough() -> string {
    let canned = anth_canned_text("ok", "end_turn");
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(canned)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let overrides: baml.json.json = {
                "max_tokens": null,
                "metadata": { "user_id": "u1" },
            };
            let cl = anthropic.Client.new(
                model = "claude-haiku-4-5",
                api_key = "test-key",
                base_url = mock.base_url(),
                max_tokens = 2048,
                temperature = 0.25,
                top_p = 0.9,
                top_k = 40,
                stop_sequences = ["STOP"],
                thinking = anthropic.ThinkingConfig.enabled(1024),
                request_body = overrides,
                headers = {
                    "anthropic-beta": "context-1m-2025-08-07",
                    "anthropic-version": "2024-01-01",
                    "x-api-key": "hijacked",
                },
                query_params = { "beta": "true" },
            );
            let result = ai.Agent.new(client = cl).run(AnthMockAsk@spec("hi"));
            let sent = mock.last_request();
            let body = sent.json();
            [
                `value=${result.value}`,
                `query=${sent.query() ?? "-"}`,
                `beta=${sent.header("anthropic-beta") ?? "-"}`,
                `version=${sent.header("anthropic-version") ?? "-"}`,
                `key=${sent.header("x-api-key") ?? "-"}`,
                `temperature=${baml.json.path_or<float>(body, ".temperature", -1.0)}`,
                `top_p=${baml.json.path_or<float>(body, ".top_p", -1.0)}`,
                `top_k=${baml.json.path_or<int>(body, ".top_k", -1)}`,
                `stop0=${baml.json.path_or<string>(body, ".stop_sequences[0]", "-")}`,
                `thinking=${baml.json.path_or<string>(body, ".thinking.type", "-")}`,
                `budget=${baml.json.path_or<int>(body, ".thinking.budget_tokens", -1)}`,
                `user_id=${baml.json.path_or<string>(body, ".metadata.user_id", "-")}`,
                `has_max_tokens=${sent.body.includes("max_tokens")}`,
            ].join("|")
        },
    )
}

test "anthropic_params_and_passthrough" {
    let got = anthropic_params_and_passthrough();
    assert.contains(got, "query=beta=true|");
    assert.contains(got, "beta=context-1m-2025-08-07|");
    // user headers land before the defaults are frozen …
    assert.contains(got, "version=2024-01-01|");
    // … but auth is applied last, so it cannot be clobbered
    assert.contains(got, "key=test-key|");
    assert.contains(got, "temperature=0.25|");
    assert.contains(got, "top_p=0.9|");
    assert.contains(got, "top_k=40|");
    assert.contains(got, "stop0=STOP|");
    assert.contains(got, "thinking=enabled|");
    assert.contains(got, "budget=1024|");
    assert.contains(got, "user_id=u1|");
    // `{"max_tokens": null}` deletes the client's default outright
    assert.contains(got, "has_max_tokens=false")
}

// A `cache_control` key in a message's metadata merges onto that message's
// LAST content block (the engine's placement), and messages without metadata
// stay clean.
function anthropic_cache_control_placement() -> string {
    let canned = anth_canned_text("ok", "end_turn");
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(canned)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = anthropic.Client.new(
                model = "claude-haiku-4-5",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            let _ = ai.Agent.new(client = cl).run(AnthMockCached@spec("hi"));
            let body = mock.last_request().json();
            [
                `system_cc=${baml.json.path_or<string>(body, ".system[0].cache_control.type", "-")}`,
                `user_cc=${baml.json.path_or<string>(body, ".messages[0].content[0].cache_control.type", "-")}`,
            ].join("|")
        },
    )
}

test "anthropic_cache_control_placement" {
    let got = anthropic_cache_control_placement();
    assert.contains(got, "system_cc=ephemeral|");
    assert.contains(got, "user_cc=-")
}

// BUG 7.1: an assistant-first prompt with no system content used to send
// `messages[0].role == "assistant"`, which the API rejects outright.
function anthropic_assistant_first_gets_leading_user() -> string {
    let cl = anthropic.Client.new(model = "claude-haiku-4-5", api_key = "test-key");
    let body = anth_render_body(cl, AnthAssistantFirst@spec(), ai.Journal { log: [] });
    [
        `role0=${baml.json.path_or<string>(body, ".messages[0].role", "-")}`,
        `text0=${baml.json.path_or<string>(body, ".messages[0].content[0].text", "-")}`,
        `role1=${baml.json.path_or<string>(body, ".messages[1].role", "-")}`,
    ].join("|")
}

test "anthropic_assistant_first_gets_leading_user" {
    let got = anthropic_assistant_first_gets_leading_user();
    assert.contains(got, "role0=user|");
    assert.contains(got, "text0=(continue)|");
    assert.contains(got, "role1=assistant")
}

// BUG 7.2: an assistant turn carrying only Reasoning used to lower to
// `{"role":"assistant","content":[]}`, which the API rejects. Reasoning from a
// FOREIGN client is removed by ai.wire.sanitize_for_client; reasoning from THIS
// client survives sanitization and is dropped by the client's own lowering —
// either way no empty message reaches the wire.
function anthropic_reasoning_only_turn_is_dropped() -> string {
    let cl = anthropic.Client.new(model = "claude-haiku-4-5", api_key = "test-key");
    let journal = ai.Journal {
        log: [
            ai.events.AssistantMessage {
                content: [ai.content.Reasoning { summary: "quietly thinking" }],
                client_id: "anthropic/claude-haiku-4-5",
            },
        ],
    };
    let body = anth_render_body(cl, AnthMockPlain@spec(), journal);
    [
        `role0=${baml.json.path_or<string>(body, ".messages[0].role", "-")}`,
        `role1=${baml.json.path_or<string>(body, ".messages[1].role", "-")}`,
        `empty=${baml.json.stringify(body).includes("\"content\":[]")}`,
    ].join("|")
}

test "anthropic_reasoning_only_turn_is_dropped" {
    let got = anthropic_reasoning_only_turn_is_dropped();
    // the prompt's user message is the ONLY message left
    assert.contains(got, "role0=user|");
    assert.contains(got, "role1=-|");
    assert.contains(got, "empty=false")
}

// Dropping that turn MID-conversation left the user turns that bracketed it
// adjacent, and the leading-role repair in `messages_request` only
// fixes an assistant-FIRST list. `_render_messages` merges same-role
// neighbours, so the roles alternate and both user payloads survive in order
// inside one message.
function anthropic_reasoning_only_turn_mid_conversation() -> string {
    let cl = anthropic.Client.new(model = "claude-haiku-4-5", api_key = "test-key");
    let journal = ai.Journal {
        log: [
            ai.events.AssistantMessage {
                content: [ai.content.Text { text: "4" }],
                client_id: "anthropic/claude-haiku-4-5",
            },
            ai.events.UserMessage { content: "And 3+3?" },
            ai.events.AssistantMessage {
                content: [ai.content.Reasoning { summary: "quietly thinking" }],
                client_id: "anthropic/claude-haiku-4-5",
            },
            ai.events.UserMessage { content: "And 4+4?" },
        ],
    };
    let body = anth_render_body(cl, AnthMockPlain@spec(), journal);
    [
        `count=${baml.json.path_or<int>(body, ".messages.length", -1)}`,
        `role0=${baml.json.path_or<string>(body, ".messages[0].role", "-")}`,
        `role1=${baml.json.path_or<string>(body, ".messages[1].role", "-")}`,
        `role2=${baml.json.path_or<string>(body, ".messages[2].role", "-")}`,
        `role3=${baml.json.path_or<string>(body, ".messages[3].role", "-")}`,
        `text0=${baml.json.path_or<string>(body, ".messages[2].content[0].text", "-")}`,
        `text1=${baml.json.path_or<string>(body, ".messages[2].content[1].text", "-")}`,
    ].join("|")
}

test "anthropic_reasoning_only_turn_mid_conversation" {
    let got = anthropic_reasoning_only_turn_mid_conversation();
    assert.contains(got, "role0=user|");
    assert.contains(got, "role1=assistant|");
    assert.contains(got, "role2=user|");
    // exactly three messages: the fourth slot is empty
    assert.contains(got, "role3=-|");
    // both orphaned user payloads ride in the merged message, in order
    assert.contains(got, "text0=And 3+3?|");
    assert.contains(got, "text1=And 4+4?")
}

// An orphaned tool call — one with no result before the next user turn — is
// closed by ai.wire.sanitize_for_client rather than sent unanswered.
function anthropic_orphaned_tool_call_is_closed() -> string {
    let cl = anthropic.Client.new(model = "claude-haiku-4-5", api_key = "test-key");
    let journal = ai.Journal {
        log: [
            ai.events.AssistantMessage {
                content: [
                    ai.content.ToolUse { id: "t1", name: "anth_weather", args: { "city": "SF" } },
                ],
                client_id: "anthropic/claude-haiku-4-5",
            },
            ai.events.UserMessage { content: "never mind" },
        ],
    };
    let body = anth_render_body(cl, AnthMockPlain@spec(), journal);
    [
        `type=${baml.json.path_or<string>(body, ".messages[2].content[0].type", "-")}`,
        `id=${baml.json.path_or<string>(body, ".messages[2].content[0].tool_use_id", "-")}`,
        `is_error=${baml.json.path_or<bool>(body, ".messages[2].content[0].is_error", false)}`,
    ].join("|")
}

test "anthropic_orphaned_tool_call_is_closed" {
    let got = anthropic_orphaned_tool_call_is_closed();
    assert.contains(got, "type=tool_result|");
    assert.contains(got, "id=t1|");
    assert.contains(got, "is_error=true")
}

// A full tool round trip: native tool definitions go out, the assistant's
// tool_use blocks are replayed, and consecutive tool results coalesce into ONE
// user message (the API requires exactly that).
function anthropic_tool_round_trip() -> string {
    let first = `{"id":"msg_1","type":"message","role":"assistant","model":"claude-haiku-4-5","content":[{"type":"tool_use","id":"toolu_a","name":"anth_weather","input":{"city":"SF"}},{"type":"tool_use","id":"toolu_b","name":"anth_weather","input":{"city":"NY"}}],"stop_reason":"tool_use","usage":{"input_tokens":10,"output_tokens":5}}`;
    root.llm_mock.mock_json_serve(
        [
            root.llm_mock.MockResponse.ok(first),
            root.llm_mock.MockResponse.ok(anth_canned_text("done", "end_turn")),
        ],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = anthropic.Client.new(
                model = "claude-haiku-4-5",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            let result = ai.Agent.new(client = cl).run(AnthMockTools@spec("weather?"));
            let body = mock.last_request().json();
            [
                `value=${result.value}`,
                `requests=${mock.request_count()}`,
                `tool_name=${baml.json.path_or<string>(body, ".tools[0].name", "-")}`,
                `schema=${baml.json.path_or<string>(body, ".tools[0].input_schema.properties.city.type", "-")}`,
                `choice=${baml.json.path_or<string>(body, ".tool_choice.type", "-")}`,
                `role1=${baml.json.path_or<string>(body, ".messages[1].role", "-")}`,
                `use0=${baml.json.path_or<string>(body, ".messages[1].content[0].type", "-")}`,
                `city0=${baml.json.path_or<string>(body, ".messages[1].content[0].input.city", "-")}`,
                `role2=${baml.json.path_or<string>(body, ".messages[2].role", "-")}`,
                `res0=${baml.json.path_or<string>(body, ".messages[2].content[0].type", "-")}`,
                `res1=${baml.json.path_or<string>(body, ".messages[2].content[1].type", "-")}`,
                `role3=${baml.json.path_or<string>(body, ".messages[3].role", "-")}`,
            ].join("|")
        },
    )
}

test "anthropic_tool_round_trip" {
    let got = anthropic_tool_round_trip();
    assert.contains(got, "value=done|");
    assert.contains(got, "requests=2|");
    assert.contains(got, "tool_name=anth_weather|");
    assert.contains(got, "schema=string|");
    assert.contains(got, "choice=auto|");
    assert.contains(got, "role1=assistant|");
    assert.contains(got, "use0=tool_use|");
    assert.contains(got, "city0=SF|");
    // both results grouped into ONE user message …
    assert.contains(got, "role2=user|");
    assert.contains(got, "res0=tool_result|");
    assert.contains(got, "res1=tool_result|");
    // … so there is no fourth message
    assert.contains(got, "role3=-")
}

// ─── response parsing ────────────────────────────────────────────────────────

// BUG 7.5: `pause_turn` / `model_context_window_exceeded` / anything unknown
// used to map to Complete, so a truncated candidate was reported as a success.
// They are truncations now, exactly like max_tokens.
function anthropic_stop_reason_outcome(stop_reason: string) -> string {
    let canned = anth_canned_text("pong", stop_reason);
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(canned)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = anthropic.Client.new(
                model = "claude-haiku-4-5",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            let outcome = ai.Agent.new(client = cl).run(AnthMockAsk@spec("hi")).value catch_all (e) {
                _ => anth_failure_label(e),
            };
            outcome
        },
    )
}

test "anthropic_stop_reason_end_turn" {
    assert.equal(anthropic_stop_reason_outcome("end_turn"), "pong")
}

test "anthropic_stop_reason_stop_sequence" {
    assert.equal(anthropic_stop_reason_outcome("stop_sequence"), "pong")
}

test "anthropic_stop_reason_max_tokens" {
    assert.equal(anthropic_stop_reason_outcome("max_tokens"), "FinishReasonError:MaxTokens:pong")
}

test "anthropic_stop_reason_pause_turn" {
    assert.equal(anthropic_stop_reason_outcome("pause_turn"), "FinishReasonError:MaxTokens:pong")
}

test "anthropic_stop_reason_unknown" {
    assert.equal(
        anthropic_stop_reason_outcome("model_context_window_exceeded"),
        "FinishReasonError:MaxTokens:pong",
    )
}

test "anthropic_stop_reason_refusal" {
    assert.equal(anthropic_stop_reason_outcome("refusal"), "Refused")
}

// A non-2xx status is classified by ai.wire.send_as; a 401 must not be a
// retry-safe NetworkFailure.
function anthropic_http_error(status: int) -> string {
    let canned = `{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}`;
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.error(status, canned)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = anthropic.Client.new(
                model = "claude-haiku-4-5",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            let outcome = ai.Agent.new(client = cl).run(AnthMockAsk@spec("hi")).value catch_all (e) {
                _ => anth_failure_label(e),
            };
            outcome
        },
    )
}

test "anthropic_http_error_401" {
    assert.equal(anthropic_http_error(401), "InvalidRequest:http 401")
}

test "anthropic_http_error_429" {
    assert.equal(anthropic_http_error(429), "RateLimited")
}

test "anthropic_http_error_503" {
    assert.equal(anthropic_http_error(503), "NetworkFailure:http 503")
}

// ─── streaming ───────────────────────────────────────────────────────────────

// Text deltas accumulate; thinking deltas deliberately do NOT (they are not
// part of the answer the parser sees), and `stream: true` reaches the wire.
function anthropic_stream_text() -> string {
    let events = [
        root.llm_mock.sse_event(
            "message_start",
            `{"type":"message_start","message":{"id":"m","model":"claude-haiku-4-5","usage":{"input_tokens":5,"cache_read_input_tokens":2}}}`,
        ),
        root.llm_mock.sse_event(
            "content_block_start",
            `{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}`,
        ),
        root.llm_mock.sse_event(
            "content_block_delta",
            `{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"po"}}`,
        ),
        root.llm_mock.sse_event(
            "content_block_delta",
            `{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"SECRET"}}`,
        ),
        root.llm_mock.sse_event(
            "content_block_delta",
            `{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"ng"}}`,
        ),
        root.llm_mock.sse_event("ping", `{"type":"ping"}`),
        root.llm_mock.sse_event(
            "message_delta",
            `{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":7}}`,
        ),
        root.llm_mock.sse_event("message_stop", `{"type":"message_stop"}`),
    ];
    root.llm_mock.mock_sse_serve(
        events,
        (mock: root.llm_mock.MockSseServer) -> {
            let cl = anthropic.Client.new(
                model = "claude-haiku-4-5",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            let value = anth_stream_value(cl);
            let sent = mock.last_request();
            [`value=${value}`, `stream=${sent.body.includes("\"stream\":true")}`].join("|")
        },
    )
}

test "anthropic_stream_text" {
    let got = anthropic_stream_text();
    assert.contains(got, "value=pong|");
    assert.contains(got, "stream=true")
}

function anthropic_stream_time_to_first_token_timeout_ms() -> string {
    root.llm_mock.mock_sse_serve(
        [
            root.llm_mock.sse_event(
                "message_start",
                `{"type":"message_start","message":{"id":"m","model":"claude-haiku-4-5","usage":{"input_tokens":5}}}`,
            ),
        ],
        (mock: root.llm_mock.MockSseServer) -> {
            let cl = anthropic.Client.new(
                model = "claude-haiku-4-5",
                api_key = "test-key",
                base_url = mock.base_url(),
                request_timeout_ms = 1000,
                time_to_first_token_timeout_ms = 20,
            );
            let outcome = anth_stream_value(cl) catch_all (e) {
                let timeout: baml.errors.Timeout => "Timeout",
                _ => `other:${e.to_string()}`,
            };
            `${cl.time_to_first_token_timeout_ms ?? -1}|${outcome}`
        },
        first_event_delay_ms = 200,
    )
}

test "anthropic_stream_time_to_first_token_timeout_ms" {
    assert.equal(anthropic_stream_time_to_first_token_timeout_ms(), "20|Timeout")
}

// A connection that ends BEFORE `message_stop` is a truncated turn, not a
// short successful one: the mock serves a message_start and one text delta and
// then closes. Without the decoder's `require_terminal` opt-in this returned
// `par` as a completed answer — silent data corruption reported as success,
// with nothing for ai.clients.Retry to act on.
function anthropic_stream_truncated() -> string {
    let events = [
        root.llm_mock.sse_event(
            "message_start",
            `{"type":"message_start","message":{"id":"m","model":"claude-haiku-4-5","usage":{"input_tokens":5}}}`,
        ),
        root.llm_mock.sse_event(
            "content_block_delta",
            `{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"par"}}`,
        ),
    ];
    root.llm_mock.mock_sse_serve(
        events,
        (mock: root.llm_mock.MockSseServer) -> {
            let cl = anthropic.Client.new(
                model = "claude-haiku-4-5",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            anth_stream_value(cl) catch_all (e) {
                _ => anth_failure_label(e),
            }
        },
    )
}

test "anthropic_stream_truncated" {
    let got = anthropic_stream_truncated();
    // retry-safe transport failure, not `par` as a Complete turn
    assert.contains(got, "NetworkFailure:stream ended before the provider's terminal event")
}

// BUG 7.3: a mid-stream `error` event used to be dropped, ending the turn as a
// short but SUCCESSFUL response. It is now the precise typed failure, and its
// retry safety follows the provider's error type.
function anthropic_stream_error_event(payload: string) -> string {
    let events = [
        root.llm_mock.sse_event(
            "message_start",
            `{"type":"message_start","message":{"id":"m","usage":{"input_tokens":5}}}`,
        ),
        root.llm_mock.sse_event(
            "content_block_delta",
            `{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"par"}}`,
        ),
        root.llm_mock.sse_event("error", payload),
    ];
    root.llm_mock.mock_sse_serve(
        events,
        (mock: root.llm_mock.MockSseServer) -> {
            let cl = anthropic.Client.new(
                model = "claude-haiku-4-5",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            anth_stream_value(cl) catch_all (e) {
                _ => anth_failure_label(e),
            }
        },
    )
}

test "anthropic_stream_error_overloaded" {
    assert.equal(
        anthropic_stream_error_event(
            `{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}`,
        ),
        "NetworkFailure:overloaded_error: Overloaded",
    )
}

test "anthropic_stream_error_rate_limit" {
    assert.equal(
        anthropic_stream_error_event(
            `{"type":"error","error":{"type":"rate_limit_error","message":"slow down"}}`,
        ),
        "RateLimited",
    )
}

test "anthropic_stream_error_auth" {
    assert.equal(
        anthropic_stream_error_event(
            `{"type":"error","error":{"type":"authentication_error","message":"bad key"}}`,
        ),
        "InvalidRequest:authentication_error: bad key",
    )
}

// BUG 7.4: every streaming transport error used to become a retry-SAFE
// NetworkFailure, so ai.clients.Retry happily replayed a 400. The status
// `fetch_sse` reports is now routed through the same classifier the
// non-streaming path uses.
function anthropic_stream_http_error(status: int) -> string {
    let canned = `{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}`;
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.error(status, canned)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = anthropic.Client.new(
                model = "claude-haiku-4-5",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            anth_stream_value(cl) catch_all (e) {
                _ => anth_failure_label(e),
            }
        },
    )
}

test "anthropic_stream_http_error_401" {
    assert.equal(anthropic_stream_http_error(401), "InvalidRequest:http 401")
}

test "anthropic_stream_http_error_429" {
    assert.equal(anthropic_stream_http_error(429), "RateLimited")
}

test "anthropic_stream_http_error_500" {
    assert.equal(anthropic_stream_http_error(500), "NetworkFailure:http 500")
}

// Typed SSE decode below the TurnStream: content_block_start names the tool,
// input_json_delta assembles its arguments, thinking/signature deltas are kept
// out of the answer text, and cache tokens survive to the end of the turn.
// ai.stream.Event cannot carry any of this yet (PLAN §6), so the decoder
// folds it into AnthStreamState — which is exactly what this asserts.
function anthropic_decode_tool_stream() -> string {
    let state = anthropic.internal.AnthStreamState.new();
    let batch = anth_batch(
        [
            `{"type":"message_start","message":{"id":"m1","model":"claude-haiku-4-5","usage":{"input_tokens":11,"cache_read_input_tokens":7,"cache_creation_input_tokens":3}}}`,
            `{"type":"content_block_start","index":0,"content_block":{"type":"text","text":"Hel"}}`,
            `{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"lo"}}`,
            `{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_1","name":"anth_weather"}}`,
            `{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\\\"city\\\":"}}`,
            `{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"\\\"SF\\\"}"}}`,
            `{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"hmm"}}`,
            `{"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"sig1"}}`,
            `{"type":"ping"}`,
            `{"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":42,"cache_read_input_tokens":9,"output_tokens_details":{"thinking_tokens":13}}}`,
            `{"type":"message_stop"}`,
        ],
    );
    let seen: string[] = [];
    for (let ev in anthropic.internal.decode_batch(batch, state)) {
        match (ev) {
            let d: ai.stream.TextDelta => {
                seen.push(`text:${d.text}`);
                null
            },
            let m: ai.stream.TurnMeta => {
                seen.push(`meta:${m.stop_reason ?? "-"}/${m.input_tokens ?? -1}/${m.output_tokens ?? -1}`);
                null
            },
            let d: ai.stream.TurnDone => {
                seen.push("done");
                null
            },
        };
    }
    let calls: string[] = [];
    for (let u in state.tool_uses()) {
        calls.push(`${u.id}/${u.name}/${baml.json.stringify(baml.json.to_json(u.args))}`);
    }
    [
        `events=${seen.join(",")}`,
        `thinking=${state.thinking}`,
        `signature=${state.signature}`,
        `calls=${calls.join(";")}`,
        `cache_read=${state.cache_read_input_tokens ?? -1}`,
        `cache_create=${state.cache_creation_input_tokens ?? -1}`,
        `reasoning=${state.reasoning_tokens ?? -1}`,
        `raw_stop=${state.stop_reason_raw ?? "-"}`,
        `model=${state.model ?? "-"}`,
    ].join("|")
}

test "anthropic_decode_tool_stream" {
    let got = anthropic_decode_tool_stream();
    assert.contains(got, "events=meta:-/11/-1,text:Hel,text:lo,meta:ToolUse/-1/42,done|");
    // thinking never becomes answer text
    assert.contains(got, "thinking=hmm|");
    assert.contains(got, "signature=sig1|");
    assert.contains(got, "calls=toolu_1/anth_weather/{\"city\":\"SF\"}|");
    assert.contains(got, "cache_read=9|");
    assert.contains(got, "cache_create=3|");
    // extended-thinking token count folds alongside the cache counters
    assert.contains(got, "reasoning=13|");
    assert.contains(got, "raw_stop=tool_use|");
    assert.contains(got, "model=claude-haiku-4-5")
}

// A malformed `data:` payload is skipped, not fatal.
function anthropic_decode_skips_garbage() -> string {
    let state = anthropic.internal.AnthStreamState.new();
    let batch = anth_batch(
        [
            "not json at all",
            `{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"ok"}}`,
        ],
    );
    let seen: string[] = [];
    for (let ev in anthropic.internal.decode_batch(batch, state)) {
        match (ev) {
            let d: ai.stream.TextDelta => {
                seen.push(d.text);
                null
            },
            _ => null,
        };
    }
    seen.join(",")
}

test "anthropic_decode_skips_garbage" {
    assert.equal(anthropic_decode_skips_garbage(), "ok")
}

// ─── media limits ────────────────────────────────────────────────────────────

// The Messages API has NO audio input content block — audio is not an input
// modality for it at all. This used to lower to an invented
// `{"type":"input_audio"}` block, so every audio-bearing request was a
// guaranteed provider 400; it is named client-side now, like video.
function anthropic_audio_input_rejected() -> string {
    let clip = baml.media.Audio.from_base64("QUJD", "audio/mp3");
    let cl = anthropic.Client.new(model = "claude-haiku-4-5", api_key = "test-key");
    let label = baml.json.stringify(
        anth_render_body(cl, AnthMockListen@spec(clip), ai.Journal { log: [] }),
    ) catch_all (e) {
        _ => e.to_string(),
    };
    label
}

test "anthropic_audio_input_rejected" {
    let got = anthropic_audio_input_rejected();
    assert.contains(got, "Anthropic does not support audio prompt input");
    // the invented block never reaches a body
    assert.is_true(!got.includes("input_audio"))
}

// A base64 image whose media_type is outside the API's allowed set
// (jpeg/png/gif/webp) is a client-side error naming the type, not an opaque
// provider 400 — `.svg` sources reach `ai.internal._media_mime_type` as image/svg+xml.
function anthropic_svg_image_rejected() -> string {
    let picture = baml.media.Image.from_base64("PHN2Zy8+", "image/svg+xml");
    let cl = anthropic.Client.new(model = "claude-haiku-4-5", api_key = "test-key");
    let label = baml.json.stringify(
        anth_render_body(cl, AnthMockVision@spec(picture), ai.Journal { log: [] }),
    ) catch_all (e) {
        _ => e.to_string(),
    };
    label
}

test "anthropic_svg_image_rejected" {
    let got = anthropic_svg_image_rejected();
    assert.contains(got, "image/svg+xml");
    assert.contains(got, "image/jpeg, image/png, image/gif and image/webp")
}

// A supported image still lowers to the documented base64 source shape.
function anthropic_png_image_block() -> string {
    let picture = baml.media.Image.from_base64("SGVsbG8=", "image/png");
    let cl = anthropic.Client.new(model = "claude-haiku-4-5", api_key = "test-key");
    let body = anth_render_body(cl, AnthMockVision@spec(picture), ai.Journal { log: [] });
    [
        `type=${baml.json.path_or<string>(body, ".messages[0].content[1].type", "-")}`,
        `src=${baml.json.path_or<string>(body, ".messages[0].content[1].source.type", "-")}`,
        `mime=${baml.json.path_or<string>(body, ".messages[0].content[1].source.media_type", "-")}`,
        `data=${baml.json.path_or<string>(body, ".messages[0].content[1].source.data", "-")}`,
    ].join("|")
}

test "anthropic_png_image_block" {
    assert.equal(anthropic_png_image_block(), "type=image|src=base64|mime=image/png|data=SGVsbG8=")
}

// ─── credentials ─────────────────────────────────────────────────────────────

// A missing credential must be a CATCHABLE typed error: `get_or_panic` raised
// an uncatchable panic that escaped catch_all, so it bypassed the client's own
// `invoke` normalize wrapper entirely.
//
// The environment decides which branch is exercised: CI has no key, so the
// error fires; a live/Infisical shell has ANTHROPIC_API_KEY, and the canonical
// fallback resolves it (base_url points at a closed port, so nothing escapes to
// the network either way). Both branches assert the same thing — no panic.
function anth_missing_key_client() -> anthropic.Client {
    // `env.NAME` naming a variable that exists nowhere: the ref resolves to
    // null at REQUEST time and the client falls through to ANTHROPIC_API_KEY.
    anthropic.Client.new(
        model = "claude-haiku-4-5",
        api_key = env.BAML_ANTHROPIC_KEY_THAT_IS_NEVER_SET,
        base_url = "http://127.0.0.1:1",
    )
}

function anthropic_missing_key_resolve() -> string {
    let cl = anth_missing_key_client();
    let outcome = cl.resolved_api_key() catch_all (e) {
        _ => `caught:${e.to_string()}`,
    };
    if (baml.env.get("ANTHROPIC_API_KEY") != null) {
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

test "anthropic_missing_key_resolve" {
    let got = anthropic_missing_key_resolve();
    assert.is_true(got == "resolved-from-env" || got.includes("ANTHROPIC_API_KEY"))
}

// The same misconfiguration through `invoke`: catchable at the client boundary,
// which is what ai.clients.Fallback/Retry composition needs.
function anthropic_missing_key_invoke() -> string {
    let cl = anth_missing_key_client();
    let outcome = anth_invoke_label(cl, anth_turn_input(AnthMockPlain@spec())) catch_all (e) {
        _ => `caught:${e.to_string()}`,
    };
    if (baml.env.get("ANTHROPIC_API_KEY") != null) {
        // a key resolved: the request fails on transport against the closed
        // port instead, which is still a caught, typed failure.
        let labeled = `key-present:${outcome}`;
        labeled
    } else {
        outcome
    }
}

test "anthropic_missing_key_invoke" {
    let got = anthropic_missing_key_invoke();
    // never a completed turn, never a panic past catch_all
    assert.contains(got, "caught:");
    assert.is_true(got.starts_with("key-present:") || got.includes("ANTHROPIC_API_KEY"))
}

// ─── replayed tool arguments ─────────────────────────────────────────────────

// `_anth_drop_nulls` prunes UNSET REQUEST OPTIONS, and must not touch the two
// opaque JSON subtrees: a replayed tool call's `input` and a tool's
// `input_schema`. A null argument is the schema-conformant answer for an
// optional parameter (the generated schema types it `["string","null"]` and
// marks it required), so dropping it rewrote conversation history.
function anthropic_replayed_tool_args_keep_nulls() -> string {
    let cl = anthropic.Client.new(model = "claude-haiku-4-5", api_key = "test-key");
    let args: map<string, unknown> = { "city": "SF", "unit": null };
    let journal = ai.Journal {
        log: [
            ai.events.AssistantMessage {
                content: [ai.content.ToolUse { id: "t1", name: "anth_weather", args: args }],
                client_id: "anthropic/claude-haiku-4-5",
            },
            ai.events.ToolCompleted { id: "t1", output: "sunny in SF" },
        ],
    };
    let body = anth_render_body(cl, AnthMockTools@spec("weather?"), journal);
    [
        `city=${baml.json.path_or<string>(body, ".messages[1].content[0].input.city", "-")}`,
        `unit_present=${baml.json.stringify(body).includes("\"unit\":null")}`,
        // the request's OWN unset optionals are still pruned
        `has_top_null=${baml.json.stringify(body).includes("\"temperature\":null")}`,
    ].join("|")
}

test "anthropic_replayed_tool_args_keep_nulls" {
    let got = anthropic_replayed_tool_args_keep_nulls();
    assert.contains(got, "city=SF|");
    assert.contains(got, "unit_present=true|");
    assert.contains(got, "has_top_null=false")
}

// ─── extended-thinking usage ─────────────────────────────────────────────────

// `usage.output_tokens_details.thinking_tokens` is the reasoning share of
// output_tokens under extended thinking; it used to be dropped on the floor
// with `reasoning_tokens: null` hardcoded.
function anthropic_reasoning_tokens() -> string {
    let canned = `{"id":"msg_1","type":"message","role":"assistant","model":"claude-haiku-4-5","content":[{"type":"text","text":"pong"}],"stop_reason":"end_turn","usage":{"input_tokens":9,"output_tokens":30,"output_tokens_details":{"thinking_tokens":21}}}`;
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(canned)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = anthropic.Client.new(
                model = "claude-haiku-4-5",
                api_key = "test-key",
                base_url = mock.base_url(),
                thinking = anthropic.ThinkingConfig.enabled(1024),
            );
            let result = ai.Agent.new(client = cl).run(AnthMockAsk@spec("hi"));
            [
                `value=${result.value}`,
                `output=${result.usage.output_tokens}`,
                `reasoning=${result.usage.reasoning_tokens ?? -1}`,
            ].join("|")
        },
    )
}

test "anthropic_reasoning_tokens" {
    let got = anthropic_reasoning_tokens();
    assert.contains(got, "value=pong|");
    assert.contains(got, "output=30|");
    assert.contains(got, "reasoning=21")
}

// ─── live ────────────────────────────────────────────────────────────────────
//
// Excluded by the default `offline` profile; selected by `--profile live`, and
// registered only when the key is present. Model id live-verified 2026-08-13.

function AnthLiveAsk(question: string) -> string {
    client: "anthropic/claude-haiku-4-5"
    prompt: `${role("user")}Reply with exactly one word. ${question}`
}

function anth_live_temperature(city: string) -> string {
    let where = city;
    "The temperature in " + where + " is 21 degrees celsius."
}

function AnthLiveTools(question: string) -> string {
    client: "anthropic/claude-haiku-4-5"
    tools: [anth_live_temperature]
    prompt: `${role("user")}${question} Call the tool, then answer with the number.`
}

function anth_live_answers() -> bool {
    AnthLiveAsk("Say pong.").trim().length() > 0
}

function anth_live_streams() -> bool {
    let stream = ai.stream.from_spec<string | null, string>(AnthLiveAsk@spec("Say pong."));
    stream.final().trim().length() > 0
}

function anth_live_tool_round_trip() -> bool {
    AnthLiveTools("What is the temperature in Paris?").includes("21")
}

testset "live" with testing.Sequential() {
    // Always registered: guarantees a non-empty selection for `--profile live`.
    test "preconditions" {
        assert.is_true(true)
    }

    if (baml.env.get("ANTHROPIC_API_KEY") != null) {
        test "anthropic_text" {
            assert.is_true(anth_live_answers())
        }

        test "anthropic_stream" {
            assert.is_true(anth_live_streams())
        }

        test "anthropic_tool_round_trip" {
            assert.is_true(anth_live_tool_round_trip())
        }
    } else {
        log.warn("live::anthropic not registered: ANTHROPIC_API_KEY is unset")
    }
}
