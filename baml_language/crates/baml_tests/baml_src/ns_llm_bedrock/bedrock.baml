// `aws.BedrockClient` — the offline corpus for the native Bedrock Converse
// client, plus one env-gated `::live::` leaf.
//
// Coverage is split deliberately in two:
//
//   * REQUEST SHAPE goes through `aws.internal.build_request`, the unsigned
//     seam. That is the complete wire request (typed body -> JSON -> null
//     pruning -> `request_body` merge, plus user headers and query params);
//     only the SigV4 headers are added after it. Asserting there needs no
//     server and no credentials, so these cases are fully deterministic and
//     they are where the ported sys_llm `#[cfg(test)]` assertions live
//     (`crates/sys_llm/src/build_request/bedrock.rs:519-971` and
//     `forks/aws-bedrock/src/lib.rs:300-480`).
//   * THE FULL PATH (sign -> send -> parse -> `ai.errors`) goes through
//     `cl.invoke(...)` against `root.llm_mock.mock_json_serve` with
//     `endpoint_url` pointed at the mock and explicit static credentials, so
//     signing runs offline (explicit keys short-circuit the AWS provider
//     chain). Those cases assert that a signature was produced, not what it
//     is — the signature bytes are the Rust seam's own contract.
//
// There are no SSE tests: Bedrock's ConverseStream frames its response as
// binary `application/vnd.amazon.eventstream`, which `baml.http.fetch_sse`
// cannot consume, so `BedrockClient` deliberately does not implement
// `ai.stream.StreamingClient` (PLAN §6). The `::live::` leaf and the error
// cases take that budget instead.
//
// Corpus rules (ns_llm_mock/README.md §4): every case body is a top-level
// function, `test` blocks only assert, cross-namespace references are
// `root.`-absolute.

// ─── fixtures ────────────────────────────────────────────────────────────────

// The model id used everywhere below. A `:` in the id is the interesting part:
// it must be percent-encoded into the URL path, never sent in the body.
function _model() -> string {
    let id = "anthropic.claude-3-5-haiku-20241022-v1:0";
    id
}

function BedrockEcho(question: string) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `
        ${role("system")}Be terse.
        ${role("user")}${question}
    `
}

// A leading literal keeps the media part at a known index: the rendered parts
// are ["Look at ", <media>].
function BedrockVision(picture: image) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `
        ${role("user")}Look at ${picture}
    `
}

function BedrockClip(clip: video) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `
        ${role("user")}Look at ${clip}
    `
}

function BedrockListen(clip: audio) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `
        ${role("user")}Listen to ${clip}
    `
}

// Assistant-first with NO system content: Converse rejects a message list that
// does not start with a user message, so the client must synthesize one.
function BedrockAssistantFirst(hint: string) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `
        ${role("assistant")}${hint}
    `
}

// Assistant-first WITH system content: the system blocks are promoted into the
// leading user message instead of a synthesized placeholder.
function BedrockSystemThenAssistant(hint: string) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `
        ${role("system")}Be terse.
        ${role("assistant")}${hint}
    `
}

// A user-only prompt: no system message, so the system/assistant-first
// fallback never fires.
function BedrockUserOnly(question: string) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `
        ${role("user")}${question}
    `
}

function _empty_journal() -> ai.Journal {
    let log: ai.events.Event[] = [];
    ai.Journal { log: log }
}

function _no_tools() -> ai.tools.Toolbox {
    let none: ai.tools.Tool[] = [];
    ai.tools.Toolbox.new(none)
}

// One toolbox with a hand-written JSON Schema, so the `inputSchema.json`
// passthrough can be asserted verbatim.
function _weather_tools() -> ai.tools.Toolbox {
    let properties: map<string, unknown> = { "city": { "type": "string" } };
    let schema: map<string, unknown> = {
        "type": "object",
        "properties": properties,
        "required": ["city"],
    };
    ai.tools.Toolbox.new(
        [
            ai.tools.raw_tool(
                "get_weather",
                "Look up the weather.",
                schema,
                (args: map<string, unknown>) -> { "sunny" },
            ),
        ],
    )
}

function _turn_input(
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

// A client aimed at `endpoint_url`, with the AWS documentation's example
// credentials so signing never touches the environment.
function _client(endpoint_url: string?) -> aws.BedrockClient {
    aws.BedrockClient.new(
        model = _model(),
        region = "us-east-1",
        access_key_id = "AKIAIOSFODNN7EXAMPLE",
        secret_access_key = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
        endpoint_url = endpoint_url,
    )
}

// The body `build_request` would put on the wire, as parsed JSON.
function _body(client: aws.BedrockClient, input: ai.ModelTurnInput) -> json {
    baml.json.parse(aws.internal.build_request(client, input).body)
}

// Whether the serialized body mentions `key` at all.
//
// `baml.json.path_or` cannot express this: it returns the default both when a
// selector misses AND when the leaf does not coerce, so `path_or<string>(body,
// ".inferenceConfig", "ABSENT")` says "ABSENT" for a present object too. Every
// key asserted with this helper is distinctive enough that a substring match on
// the compact JSON is exact.
function _mentions(body: json, key: string) -> bool {
    baml.json.stringify(body).includes(`"${key}":`)
}

// The length of the array at `selector`, or -1 when it is not an array.
function _array_len(body: json, selector: string) -> int {
    match (baml.json.path<json>(body, selector)) {
        let items: baml.json.json[] => items.length(),
        _ => -1,
    }
}

// A turn's stop reason as text, so a success and a classified failure can share
// one string-valued expression under `catch_all`.
function _stop_label(turn: ai.ModelTurn) -> string {
    let label = `${turn.stop_reason}`;
    label
}

// The built request's URL, or the identity of the local rejection that stopped
// it. Every pre-send rejection in the client is one typed identity.
function _build_label(client: aws.BedrockClient, input: ai.ModelTurnInput) -> string {
    aws.internal.build_request(client, input).url catch_all (e) {
        _ => _failure_label(e),
    }
}

// A classified failure's identity, for the error-taxonomy cases.
function _failure_label(error: unknown) -> string {
    match (error) {
        let e: ai.errors.RateLimited => "RateLimited",
        let e: ai.errors.InvalidRequest => `InvalidRequest/${e.status_code ?? -1}`,
        let e: ai.errors.NetworkFailure => "NetworkFailure",
        let e: ai.errors.ParseFailed => "ParseFailed",
        _ => `other/${error.to_string()}`,
    }
}

// ─── 1. model-id path encoding ───────────────────────────────────────────────
//
// Ported from `forks/aws-bedrock/src/lib.rs:300-317` and
// `build_request/bedrock.rs:649-669`. `:` -> %3A and `/` -> %2F keep an ARN in
// a single path segment; `-`, `.`, `_` and `~` are never escaped.

function bedrock_model_path_encoding() -> string {
    let arn = "arn:aws:bedrock:us-west-2:123456789012:foundation-model/anthropic.claude-3-sonnet";
    [
        aws.internal.converse_model_path(_model()),
        aws.internal.converse_model_path(arn),
        aws.internal.converse_model_path("us.anthropic.claude-haiku-4-5-20251001-v1:0"),
        aws.internal.encode_path_label("a b~c_d.e-f"),
    ].join("\n")
}

test "bedrock_model_path_encoding" {
    let got = bedrock_model_path_encoding();
    assert.contains(got, "/model/anthropic.claude-3-5-haiku-20241022-v1%3A0/converse");
    assert.contains(
        got,
        "/model/arn%3Aaws%3Abedrock%3Aus-west-2%3A123456789012%3Afoundation-model%2Fanthropic.claude-3-sonnet/converse",
    );
    assert.contains(got, "/model/us.anthropic.claude-haiku-4-5-20251001-v1%3A0/converse");
    assert.contains(got, "a%20b~c_d.e-f")
}

// ─── 2. request shape: system + messages ─────────────────────────────────────
//
// `bedrock_system_and_user_text` + `bedrock_no_model_or_creds_in_body`
// (`build_request/bedrock.rs:519-535`, `:671-677`).

function bedrock_body_system_and_user() -> string {
    let input = _turn_input(BedrockEcho@spec("Hello"), _empty_journal(), _no_tools());
    let body = _body(_client(null), input);
    [
        baml.json.path<string>(body, ".system[0].text").trim(),
        baml.json.path<string>(body, ".messages[0].role"),
        baml.json.path<string>(body, ".messages[0].content[0].text"),
        _array_len(body, ".messages").to_string(),
        // The model id and the credentials are NOT body fields.
        _mentions(body, "model").to_string(),
        _mentions(body, "modelId").to_string(),
        // Unset optionals are pruned, not sent as nulls.
        _mentions(body, "inferenceConfig").to_string(),
        _mentions(body, "toolConfig").to_string(),
        _mentions(body, "additionalModelRequestFields").to_string(),
    ].join("|")
}

test "bedrock_body_system_and_user" {
    assert.equal(
        bedrock_body_system_and_user(),
        "Be terse.|user|Hello|1|false|false|false|false|false",
    )
}

// The URL carries the model id and the region; the method and the two static
// headers match `build_request/bedrock.rs:53-58,614-627`.
function bedrock_request_url_and_headers() -> string {
    let input = _turn_input(BedrockEcho@spec("Hello"), _empty_journal(), _no_tools());
    let request = aws.internal.build_request(_client(null), input);
    [
        request.method,
        request.url,
        request.headers.get("content-type") ?? "",
        request.headers.get("accept") ?? "",
        request.headers.get("authorization") ?? "UNSIGNED",
    ].join("|")
}

test "bedrock_request_url_and_headers" {
    assert.equal(
        bedrock_request_url_and_headers(),
        "POST|https://bedrock-runtime.us-east-1.amazonaws.com/model/anthropic.claude-3-5-haiku-20241022-v1%3A0/converse|application/json|application/json|UNSIGNED",
    )
}

// `endpoint_url` replaces the regional host, with or without a trailing slash
// (`build_request/bedrock.rs:629-647`, `:900-918`).
function bedrock_endpoint_url_override() -> string {
    let input = _turn_input(BedrockEcho@spec("hi"), _empty_journal(), _no_tools());
    let plain = aws.internal.build_request(_client("http://localhost:4566"), input).url;
    let slashed = aws.internal.build_request(_client("http://localhost:4566/"), input).url;
    // No region anywhere: the override must not need one
    // (`bedrock_endpoint_url_does_not_require_region`, :920-935).
    let regionless = aws.BedrockClient.new(model = _model(), endpoint_url = "http://localhost:4566");
    let third = aws.internal.build_request(regionless, input).url;
    [plain, slashed, third].join("|")
}

test "bedrock_endpoint_url_override" {
    let expected = "http://localhost:4566/model/anthropic.claude-3-5-haiku-20241022-v1%3A0/converse";
    assert.equal(bedrock_endpoint_url_override(), [expected, expected, expected].join("|"))
}

// Converse requires a message list that starts with a `user` message
// ("A conversation must start with a user message"), which two prompt shapes
// miss. With system content the system blocks are PROMOTED into the leading
// user message; without it a `(continue)` user turn is SYNTHESIZED — the same
// two fallbacks the native Anthropic client uses.
function bedrock_assistant_first_no_system() -> string {
    let input = _turn_input(BedrockAssistantFirst@spec("carry on"), _empty_journal(), _no_tools());
    let body = _body(_client(null), input);
    [
        baml.json.path<string>(body, ".messages[0].role"),
        baml.json.path<string>(body, ".messages[0].content[0].text"),
        baml.json.path<string>(body, ".messages[1].role"),
        baml.json.path<string>(body, ".messages[1].content[0].text").trim(),
        _array_len(body, ".messages").to_string(),
        _mentions(body, "system").to_string(),
    ].join("|")
}

test "bedrock_assistant_first_no_system" {
    assert.equal(
        bedrock_assistant_first_no_system(),
        "user|(continue)|assistant|carry on|2|false",
    )
}

function bedrock_assistant_first_with_system() -> string {
    let input = _turn_input(
        BedrockSystemThenAssistant@spec("carry on"),
        _empty_journal(),
        _no_tools(),
    );
    let body = _body(_client(null), input);
    [
        baml.json.path<string>(body, ".messages[0].role"),
        baml.json.path<string>(body, ".messages[0].content[0].text").trim(),
        baml.json.path<string>(body, ".messages[1].role"),
        _array_len(body, ".messages").to_string(),
        // the promoted blocks are NOT also sent as a top-level system array
        _mentions(body, "system").to_string(),
    ].join("|")
}

test "bedrock_assistant_first_with_system" {
    assert.equal(bedrock_assistant_first_with_system(), "user|Be terse.|assistant|2|false")
}

// ─── 3. inferenceConfig ──────────────────────────────────────────────────────
//
// `bedrock_inference_config` (`build_request/bedrock.rs:572-611`) plus the
// stop-sequence rule (only emitted when non-empty, `:381-385`).

function bedrock_inference_config() -> string {
    let cl = aws.BedrockClient.new(
        model = _model(),
        region = "us-east-1",
        max_tokens = 500,
        temperature = 0.5,
        top_p = 0.75,
        stop_sequences = ["STOP"],
    );
    let body = _body(cl, _turn_input(BedrockUserOnly@spec("Hi"), _empty_journal(), _no_tools()));
    [
        baml.json.path<int>(body, ".inferenceConfig.maxTokens").to_string(),
        baml.json.path<float>(body, ".inferenceConfig.temperature").to_string(),
        baml.json.path<float>(body, ".inferenceConfig.topP").to_string(),
        baml.json.path<string>(body, ".inferenceConfig.stopSequences[0]"),
    ].join("|")
}

test "bedrock_inference_config" {
    assert.equal(bedrock_inference_config(), "500|0.5|0.75|STOP")
}

// An empty stop-sequence list leaves the key out, and a config with only one
// member still emits just that member.
function bedrock_inference_config_partial() -> string {
    let cl = aws.BedrockClient.new(
        model = _model(),
        region = "us-east-1",
        temperature = 0.25,
        stop_sequences = [],
    );
    let body = _body(cl, _turn_input(BedrockUserOnly@spec("Hi"), _empty_journal(), _no_tools()));
    [
        baml.json.path<float>(body, ".inferenceConfig.temperature").to_string(),
        _mentions(body, "stopSequences").to_string(),
        _mentions(body, "maxTokens").to_string(),
        _mentions(body, "topP").to_string(),
    ].join("|")
}

test "bedrock_inference_config_partial" {
    assert.equal(bedrock_inference_config_partial(), "0.25|false|false|false")
}

// `max_tokens` is an i32 on the wire; a wider value is rejected before the
// request is built (`bedrock_max_tokens_overflow_rejected`, :937-971).
function bedrock_max_tokens_overflow_rejected() -> string {
    let cl = aws.BedrockClient.new(model = _model(), region = "us-east-1", max_tokens = 2147483648);
    let input = _turn_input(BedrockUserOnly@spec("Hi"), _empty_journal(), _no_tools());
    let label = aws.internal.build_request(cl, input).url catch_all (e) {
        _ => e.to_string(),
    };
    label
}

test "bedrock_max_tokens_overflow_rejected" {
    assert.contains(bedrock_max_tokens_overflow_rejected(), "out of the supported range")
}

// ─── 4. multi-turn conversation ──────────────────────────────────────────────
//
// `bedrock_multi_turn` (`build_request/bedrock.rs:550-570`): the journal
// replays as alternating user/assistant messages after the prompt.

function bedrock_multi_turn() -> string {
    let log: ai.events.Event[] = [
        ai.events.AssistantMessage {
            content: [ai.content.Text { text: "4" }],
            client_id: `bedrock/${_model()}`,
        },
        ai.events.UserMessage { content: "And 3+3?" },
    ];
    let journal = ai.Journal { log: log };
    let body = _body(_client(null), _turn_input(BedrockEcho@spec("What is 2+2?"), journal, _no_tools()));
    [
        baml.json.path<string>(body, ".messages[0].role"),
        baml.json.path<string>(body, ".messages[0].content[0].text"),
        baml.json.path<string>(body, ".messages[1].role"),
        baml.json.path<string>(body, ".messages[1].content[0].text"),
        baml.json.path<string>(body, ".messages[2].role"),
        baml.json.path<string>(body, ".messages[2].content[0].text"),
    ].join("|")
}

test "bedrock_multi_turn" {
    assert.equal(bedrock_multi_turn(), "user|What is 2+2?|assistant|4|user|And 3+3?")
}

// ─── 5. toolConfig (NET-NEW — neither sys_llm nor the engine sent one) ────────

function bedrock_tool_config() -> string {
    let input = _turn_input(BedrockUserOnly@spec("weather?"), _empty_journal(), _weather_tools());
    let body = _body(_client(null), input);
    [
        baml.json.path<string>(body, ".toolConfig.tools[0].toolSpec.name"),
        baml.json.path<string>(body, ".toolConfig.tools[0].toolSpec.description"),
        baml.json.path<string>(body, ".toolConfig.tools[0].toolSpec.inputSchema.json.type"),
        baml.json.path<string>(body, ".toolConfig.tools[0].toolSpec.inputSchema.json.properties.city.type"),
        baml.json.path<string>(body, ".toolConfig.tools[0].toolSpec.inputSchema.json.required[0]"),
        // toolChoice is omitted unless asked for: it is optional in the API and
        // several Bedrock model families reject the field outright.
        _mentions(body, "toolChoice").to_string(),
    ].join("|")
}

test "bedrock_tool_config" {
    assert.equal(
        bedrock_tool_config(),
        "get_weather|Look up the weather.|object|string|city|false",
    )
}

// The three `toolChoice` shapes. `auto` and `any` are empty objects on the
// wire, so these assert on the serialized bytes.
function bedrock_tool_choice_modes() -> string {
    let auto = aws.BedrockClient.new(model = _model(), region = "us-east-1", tool_choice = "auto");
    let any = aws.BedrockClient.new(model = _model(), region = "us-east-1", tool_choice = "any");
    let named = aws.BedrockClient.new(
        model = _model(),
        region = "us-east-1",
        tool_choice = "get_weather",
    );
    let input = _turn_input(BedrockUserOnly@spec("weather?"), _empty_journal(), _weather_tools());
    [
        baml.json.stringify(_body(auto, input)),
        baml.json.stringify(_body(any, input)),
        baml.json.path<string>(_body(named, input), ".toolConfig.toolChoice.tool.name"),
    ].join("\n")
}

test "bedrock_tool_choice_modes" {
    let got = bedrock_tool_choice_modes();
    assert.contains(got, "\"toolChoice\":{\"auto\":{}}");
    assert.contains(got, "\"toolChoice\":{\"any\":{}}");
    assert.contains(got, "get_weather")
}

// A toolbox is only sent when it is non-empty.
function bedrock_no_tool_config_without_tools() -> bool {
    let input = _turn_input(BedrockUserOnly@spec("hi"), _empty_journal(), _no_tools());
    _mentions(_body(_client(null), input), "toolConfig")
}

test "bedrock_no_tool_config_without_tools" {
    assert.is_true(!bedrock_no_tool_config_without_tools())
}

// ─── 6. tool replay through the journal (NET-NEW) ────────────────────────────
//
// Converse wants `toolResult` blocks in a user message that follows the
// assistant message holding the matching `toolUse`, and consecutive results in
// ONE user message.

function bedrock_tool_replay() -> string {
    let args: map<string, unknown> = { "city": "SF" };
    let log: ai.events.Event[] = [
        ai.events.UserMessage { content: "weather in SF and NY?" },
        ai.events.AssistantMessage {
            content: [
                ai.content.ToolUse { id: "tu_1", name: "get_weather", args: args },
                ai.content.ToolUse { id: "tu_2", name: "get_weather", args: args },
            ],
            client_id: `bedrock/${_model()}`,
        },
        ai.events.ToolCompleted { id: "tu_1", output: "sunny" },
        ai.events.ToolFailed { id: "tu_2", message: "upstream down" },
    ];
    let journal = ai.Journal { log: log };
    let body = _body(_client(null), _turn_input(BedrockUserOnly@spec("go"), journal, _weather_tools()));
    [
        baml.json.path<string>(body, ".messages[2].content[0].toolUse.toolUseId"),
        baml.json.path<string>(body, ".messages[2].content[0].toolUse.name"),
        baml.json.path<string>(body, ".messages[2].content[0].toolUse.input.city"),
        // both results land in ONE user message, in order
        baml.json.path<string>(body, ".messages[3].role"),
        baml.json.path<string>(body, ".messages[3].content[0].toolResult.toolUseId"),
        baml.json.path<string>(body, ".messages[3].content[0].toolResult.content[0].text"),
        baml.json.path<string>(body, ".messages[3].content[0].toolResult.status"),
        baml.json.path<string>(body, ".messages[3].content[1].toolResult.toolUseId"),
        baml.json.path<string>(body, ".messages[3].content[1].toolResult.status"),
        // prompt user + journal user + assistant + ONE grouped tool-result user
        _array_len(body, ".messages").to_string(),
        _array_len(body, ".messages[3].content").to_string(),
    ].join("|")
}

test "bedrock_tool_replay" {
    assert.equal(
        bedrock_tool_replay(),
        "tu_1|get_weather|SF|user|tu_1|sunny|success|tu_2|error|4|2",
    )
}

// The MODEL authored the tool arguments, so a `null` in them is DATA, not an
// unset wire member. Request-level null pruning must copy `toolUse.input`
// verbatim — otherwise a tool schema with nullable parameters sees its explicit
// nulls vanish from the replayed history on every turn.
function bedrock_tool_replay_null_arg() -> string {
    let args: map<string, unknown> = { "city": "SF", "filter": null };
    let log: ai.events.Event[] = [
        ai.events.AssistantMessage {
            content: [ai.content.ToolUse { id: "tu_1", name: "get_weather", args: args }],
            client_id: `bedrock/${_model()}`,
        },
        ai.events.ToolCompleted { id: "tu_1", output: "sunny" },
    ];
    let journal = ai.Journal { log: log };
    let body = _body(_client(null), _turn_input(BedrockUserOnly@spec("go"), journal, _weather_tools()));
    baml.json.stringify(baml.json.path<json>(body, ".messages[1].content[0].toolUse.input"))
}

test "bedrock_tool_replay_null_arg" {
    let got = bedrock_tool_replay_null_arg();
    assert.contains(got, "\"filter\":null");
    assert.contains(got, "\"city\":\"SF\"")
}

// `ToolResultBlock.status` "is only supported by Amazon Nova and Anthropic
// Claude 3 and 4 models", so it is gated on the model id rather than sent to
// every family. An unrecognized model omits it; the result text still carries
// the error message.
function _replay_status(model: string) -> string {
    let cl = aws.BedrockClient.new(model = model, region = "us-east-1");
    let no_args: map<string, unknown> = { };
    let log: ai.events.Event[] = [
        ai.events.AssistantMessage {
            content: [ai.content.ToolUse { id: "tu_1", name: "get_weather", args: no_args }],
            client_id: cl.id(),
        },
        ai.events.ToolFailed { id: "tu_1", message: "upstream down" },
    ];
    let journal = ai.Journal { log: log };
    let body = _body(cl, _turn_input(BedrockUserOnly@spec("go"), journal, _weather_tools()));
    baml.json.path_or<string>(body, ".messages[2].content[0].toolResult.status", "ABSENT")
}

function bedrock_tool_result_status_gating() -> string {
    [
        _replay_status(_model()),
        _replay_status("us.amazon.nova-lite-v1:0"),
        _replay_status(
            "arn:aws:bedrock:us-west-2:123456789012:foundation-model/anthropic.claude-sonnet-4-5",
        ),
        _replay_status("mistral.mistral-large-2407-v1:0"),
        _replay_status("meta.llama3-1-70b-instruct-v1:0"),
    ].join("|")
}

test "bedrock_tool_result_status_gating" {
    assert.equal(
        bedrock_tool_result_status_gating(),
        "error|error|error|ABSENT|ABSENT",
    )
}

// ─── 7. cache points ─────────────────────────────────────────────────────────
//
// The engine emits `{"cachePoint":{"type":"default"}}`; sys_llm dropped the
// feature entirely (research G4). Restored here as two client options, because
// nothing populates `ai.PromptMessage.metadata` yet.

function bedrock_cache_points() -> string {
    let cl = aws.BedrockClient.new(
        model = _model(),
        region = "us-east-1",
        cache_system_prompt = true,
        cache_tools = true,
    );
    let body = _body(cl, _turn_input(BedrockEcho@spec("hi"), _empty_journal(), _weather_tools()));
    [
        baml.json.path<string>(body, ".system[1].cachePoint.type"),
        baml.json.path<string>(body, ".toolConfig.tools[1].cachePoint.type"),
        // the cachePoint entry is a bare union member: no toolSpec beside it
        baml.json.path_or<string>(body, ".toolConfig.tools[1].toolSpec.name", "ABSENT"),
        _array_len(body, ".toolConfig.tools").to_string(),
        _array_len(body, ".system").to_string(),
    ].join("|")
}

test "bedrock_cache_points" {
    assert.equal(bedrock_cache_points(), "default|default|ABSENT|2|2")
}

// ─── 8. passthrough: additionalModelRequestFields + request_body ─────────────

function bedrock_additional_model_request_fields() -> string {
    let cl = aws.BedrockClient.new(
        model = _model(),
        region = "us-east-1",
        additional_model_request_fields = baml.json.parse(`{"top_k":50,"nulled":null}`),
    );
    let body = _body(cl, _turn_input(BedrockUserOnly@spec("hi"), _empty_journal(), _no_tools()));
    [
        baml.json.path<int>(body, ".additionalModelRequestFields.top_k").to_string(),
        // The blob is user-authored JSON, so a null inside it is DATA and must
        // survive the request's null pruning.
        _mentions(body, "nulled").to_string(),
    ].join("|")
}

test "bedrock_additional_model_request_fields" {
    assert.equal(bedrock_additional_model_request_fields(), "50|true")
}

// `request_body` deep-merges over the serialized request as the last step:
// maps merge, `null` deletes, everything else replaces.
function bedrock_request_body_merge() -> string {
    let cl = aws.BedrockClient.new(
        model = _model(),
        region = "us-east-1",
        max_tokens = 100,
        temperature = 0.3,
        request_body = baml.json.parse(
            `{"inferenceConfig":{"temperature":null,"topP":0.9},"guardrailConfig":{"guardrailIdentifier":"g1"}}`,
        ),
    );
    let body = _body(cl, _turn_input(BedrockUserOnly@spec("hi"), _empty_journal(), _no_tools()));
    [
        baml.json.path<int>(body, ".inferenceConfig.maxTokens").to_string(),
        // a null override DELETES the key the client injected
        _mentions(body, "temperature").to_string(),
        baml.json.path<float>(body, ".inferenceConfig.topP").to_string(),
        baml.json.path<string>(body, ".guardrailConfig.guardrailIdentifier"),
    ].join("|")
}

test "bedrock_request_body_merge" {
    assert.equal(bedrock_request_body_merge(), "100|false|0.9|g1")
}

// ─── 9. user headers and query params ────────────────────────────────────────
//
// Both are applied BEFORE signing, so the signature covers them
// (`build_request/mod.rs:112-125,145`).

function bedrock_headers_and_query() -> string {
    let extra_headers: map<string, string> = {
        "X-Amzn-Bedrock-Trace": "ENABLED",
        "Content-Type": "application/json; charset=utf-8",
    };
    let query: map<string, string> = { "traceId": "abc/def" };
    let cl = aws.BedrockClient.new(
        model = _model(),
        region = "us-east-1",
        headers = extra_headers,
        query_params = query,
    );
    let request = aws.internal.build_request(
        cl,
        _turn_input(BedrockUserOnly@spec("hi"), _empty_journal(), _no_tools()),
    );
    [
        request.headers.get("x-amzn-bedrock-trace") ?? "",
        // A user header replaces the client's own, lowercased — never duplicated.
        request.headers.get("content-type") ?? "",
        request.url,
    ].join("|")
}

test "bedrock_headers_and_query" {
    let got = bedrock_headers_and_query();
    assert.contains(got, "ENABLED|application/json; charset=utf-8|");
    assert.contains(got, "/converse?traceId=abc%2Fdef")
}

// ─── 10. media ───────────────────────────────────────────────────────────────

function bedrock_image_base64() -> string {
    let picture = baml.media.Image.from_base64("SGVsbG8=", "image/png");
    let input = _turn_input(BedrockVision@spec(picture), _empty_journal(), _no_tools());
    let body = _body(_client(null), input);
    [
        baml.json.path<string>(body, ".messages[0].content[0].text"),
        baml.json.path<string>(body, ".messages[0].content[1].image.format"),
        baml.json.path<string>(body, ".messages[0].content[1].image.source.bytes"),
        _mentions(body, "s3Location").to_string(),
    ].join("|")
}

test "bedrock_image_base64" {
    assert.equal(bedrock_image_base64(), "Look at |png|SGVsbG8=|false")
}

function bedrock_video_s3() -> string {
    let clip = baml.media.Video.from_url("s3://bucket/clip.mp4", "video/mp4");
    let input = _turn_input(BedrockClip@spec(clip), _empty_journal(), _no_tools());
    let body = _body(_client(null), input);
    [
        baml.json.path<string>(body, ".messages[0].content[1].video.format"),
        baml.json.path<string>(body, ".messages[0].content[1].video.source.s3Location.uri"),
        _mentions(body, "bytes").to_string(),
    ].join("|")
}

test "bedrock_video_s3" {
    assert.equal(bedrock_video_s3(), "mp4|s3://bucket/clip.mp4|false")
}

// A video URL that is not `s3://` is rejected with the URL in the message
// (`bedrock_non_s3_url_rejected`, `build_request/bedrock.rs:851-870`). Videos
// are never inlined, so no network call happens here.
function bedrock_non_s3_video_rejected() -> string {
    let clip = baml.media.Video.from_url("https://example.com/clip.mp4", "video/mp4");
    let input = _turn_input(BedrockClip@spec(clip), _empty_journal(), _no_tools());
    let label = aws.internal.build_request(_client(null), input).url catch_all (e) {
        _ => e.to_string(),
    };
    label
}

test "bedrock_non_s3_video_rejected" {
    let got = bedrock_non_s3_video_rejected();
    assert.contains(got, "Bedrock requires s3:// URIs for video URLs");
    assert.contains(got, "https://example.com/clip.mp4")
}

// An unsupported container is named rather than silently sent.
function bedrock_unsupported_image_format_rejected() -> string {
    let picture = baml.media.Image.from_base64("SGVsbG8=", "image/tiff");
    let input = _turn_input(BedrockVision@spec(picture), _empty_journal(), _no_tools());
    let label = aws.internal.build_request(_client(null), input).url catch_all (e) {
        _ => e.to_string(),
    };
    label
}

test "bedrock_unsupported_image_format_rejected" {
    assert.contains(bedrock_unsupported_image_format_rejected(), "unsupported image format for Bedrock")
}

// `AudioSource` is a `bytes | s3Location` union exactly like the other three
// media sources, so `s3://` audio lowers to an `s3Location` instead of being
// hard-rejected. `s3://` is never fetched (it is not an HTTP scheme), so this
// makes no network call.
function bedrock_audio_s3() -> string {
    let clip = baml.media.Audio.from_url("s3://bucket/take.mp3", "audio/mpeg");
    let input = _turn_input(BedrockListen@spec(clip), _empty_journal(), _no_tools());
    let body = _body(_client(null), input);
    [
        baml.json.path<string>(body, ".messages[0].content[1].audio.format"),
        baml.json.path<string>(body, ".messages[0].content[1].audio.source.s3Location.uri"),
        _mentions(body, "bytes").to_string(),
    ].join("|")
}

test "bedrock_audio_s3" {
    assert.equal(bedrock_audio_s3(), "mp3|s3://bucket/take.mp3|false")
}

// Inline audio still takes the bytes member, and the format map covers the
// current 15-value `AudioBlock.format` enum rather than sys_llm's five.
function bedrock_audio_base64() -> string {
    let clip = baml.media.Audio.from_base64("SGVsbG8=", "audio/opus");
    let input = _turn_input(BedrockListen@spec(clip), _empty_journal(), _no_tools());
    let body = _body(_client(null), input);
    [
        baml.json.path<string>(body, ".messages[0].content[1].audio.format"),
        baml.json.path<string>(body, ".messages[0].content[1].audio.source.bytes"),
        _mentions(body, "s3Location").to_string(),
    ].join("|")
}

test "bedrock_audio_base64" {
    assert.equal(bedrock_audio_base64(), "opus|SGVsbG8=|false")
}

// Every LOCAL, pre-send rejection carries ONE typed identity —
// `ai.errors.InvalidRequest` with no status code — the same class an
// unresolvable region or a signing failure throws. `baml.errors.InvalidArgument`
// would be flattened into an untyped `UnknownError` by `invoke`'s `normalize`.
function bedrock_local_rejection_identity() -> string {
    let tiff = baml.media.Image.from_base64("SGVsbG8=", "image/tiff");
    let ogg = baml.media.Audio.from_base64("SGVsbG8=", "audio/basic");
    let over = aws.BedrockClient.new(
        model = _model(),
        region = "us-east-1",
        max_tokens = 2147483648,
    );
    let plain = _turn_input(BedrockUserOnly@spec("hi"), _empty_journal(), _no_tools());
    [
        _build_label(_client(null), _turn_input(BedrockVision@spec(tiff), _empty_journal(), _no_tools())),
        _build_label(_client(null), _turn_input(BedrockListen@spec(ogg), _empty_journal(), _no_tools())),
        _build_label(over, plain),
    ].join("|")
}

test "bedrock_local_rejection_identity" {
    assert.equal(
        bedrock_local_rejection_identity(),
        "InvalidRequest/-1|InvalidRequest/-1|InvalidRequest/-1",
    )
}

// ─── 11. response parse ──────────────────────────────────────────────────────
//
// Ported from `parse_response/bedrock.rs:142-322`, minus the two behaviors that
// were bugs: the hardcoded `cached_input_tokens: None` (G3) and the
// "no text blocks" error, which made a tool-only turn unrepresentable.

function _parse(raw: string) -> ai.ModelTurn {
    aws.internal.parse_response(baml.json.from_string<aws.internal.ConverseResponse>(raw))
}

function bedrock_parse_text_and_usage() -> string {
    let raw = `{"output":{"message":{"role":"assistant","content":[{"text":"First part. "},{"text":"Second part."}]}},"stopReason":"end_turn","usage":{"inputTokens":10,"outputTokens":15,"totalTokens":25,"cacheReadInputTokens":7,"cacheWriteInputTokens":3},"metrics":{"latencyMs":42}}`;
    let turn = _parse(raw);
    [
        turn.terminal_text() ?? "",
        _stop_label(turn),
        (turn.usage?.input_tokens ?? -1).to_string(),
        (turn.usage?.output_tokens ?? -1).to_string(),
        (turn.usage?.cached_input_tokens ?? -1).to_string(),
        turn.content.length().to_string(),
    ].join("|")
}

test "bedrock_parse_text_and_usage" {
    assert.equal(bedrock_parse_text_and_usage(), "Second part.|Complete|10|15|7|2")
}

// A turn whose stop reason means the generation was cut short or is unusable
// must NEVER read as `Complete`: an agent loop would take the truncated text as
// a clean final answer. Only `end_turn`, `stop_sequence` and genuinely unknown
// future values fall through to `Complete`.
function bedrock_parse_stop_reasons() -> string {
    let shell = `{"output":{"message":{"content":[{"text":"x"}]}},"stopReason":`;
    [
        _stop_label(_parse(shell + "\"end_turn\"}")),
        _stop_label(_parse(shell + "\"stop_sequence\"}")),
        _stop_label(_parse(shell + "\"max_tokens\"}")),
        // prompt + output overflowed the context window: the twin of max_tokens
        _stop_label(_parse(shell + "\"model_context_window_exceeded\"}")),
        _stop_label(_parse(shell + "\"tool_use\"}")),
        _stop_label(_parse(shell + "\"content_filtered\"}")),
        _stop_label(_parse(shell + "\"guardrail_intervened\"}")),
        _stop_label(_parse(shell + "\"something_new\"}")),
        // stopReason absent entirely
        _stop_label(_parse(`{"output":{"message":{"content":[{"text":"x"}]}}}`)),
    ].join("|")
}

test "bedrock_parse_stop_reasons" {
    assert.equal(
        bedrock_parse_stop_reasons(),
        "Complete|Complete|MaxTokens|MaxTokens|ToolUse|Refused|Refused|Complete|Complete",
    )
}

// `malformed_model_output` / `malformed_tool_use` say the service could not
// assemble the generation into a message at all. There is no answer to stop on,
// so `parse_response` throws `ai.errors.ParseFailed` (retry-safe, carries the
// raw response) rather than returning a turn; the pure mapper keeps them
// non-Complete too, for any direct caller.
function _stop_or_failure(raw: string) -> string {
    let body = `{"output":{"message":{"content":[{"text":"x"}]}},"stopReason":"${raw}"}`;
    _stop_label(_parse(body)) catch_all (e) {
        _ => _failure_label(e),
    }
}

function bedrock_parse_malformed_stop() -> string {
    [
        _stop_or_failure("malformed_model_output"),
        _stop_or_failure("malformed_tool_use"),
        `${aws.internal.stop_reason("malformed_model_output")}`,
        `${aws.internal.stop_reason("malformed_tool_use")}`,
    ].join("|")
}

test "bedrock_parse_malformed_stop" {
    assert.equal(
        bedrock_parse_malformed_stop(),
        "ParseFailed|ParseFailed|Refused|Refused",
    )
}

// A tool-only turn parses cleanly — sys_llm errored on it
// (`parse_response/bedrock.rs:89-104`), which would have made tool calling
// impossible.
function bedrock_parse_tool_use() -> string {
    let raw = `{"output":{"message":{"role":"assistant","content":[{"text":"Checking."},{"toolUse":{"toolUseId":"tu_1","name":"get_weather","input":{"city":"SF"}}}]}},"stopReason":"tool_use"}`;
    let turn = _parse(raw);
    let uses = turn.tool_uses();
    [
        _stop_label(turn),
        uses.length().to_string(),
        uses.at(0)?.id ?? "",
        uses.at(0)?.name ?? "",
        baml.json.stringify(baml.json.to_json(uses.at(0)?.args)),
    ].join("|")
}

test "bedrock_parse_tool_use" {
    assert.equal(bedrock_parse_tool_use(), "ToolUse|1|tu_1|get_weather|{\"city\":\"SF\"}")
}

function bedrock_parse_reasoning() -> string {
    let raw = `{"output":{"message":{"content":[{"reasoningContent":{"reasoningText":{"text":"thinking…","signature":"sig"}}},{"text":"answer"}]}},"stopReason":"end_turn"}`;
    let turn = _parse(raw);
    [turn.content.length().to_string(), turn.terminal_text() ?? ""].join("|")
}

test "bedrock_parse_reasoning" {
    assert.equal(bedrock_parse_reasoning(), "2|answer")
}

// Empty content is not an error (an empty turn is a legitimate, if unhelpful,
// answer); a missing `output.message` is, because that response is malformed.
function bedrock_parse_empty_and_malformed() -> string {
    let empty = _parse(`{"output":{"message":{"role":"assistant","content":[]}},"stopReason":"end_turn"}`);
    let malformed = _stop_label(_parse(`{"output":{},"stopReason":"end_turn"}`)) catch_all (e) {
        _ => _failure_label(e),
    };
    [empty.content.length().to_string(), malformed].join("|")
}

test "bedrock_parse_empty_and_malformed" {
    assert.equal(bedrock_parse_empty_and_malformed(), "0|ParseFailed")
}

// Usage is optional on the wire; its absence is not a parse failure
// (`test_parse_no_usage`, :306-322).
function bedrock_parse_no_usage() -> string {
    let turn = _parse(`{"output":{"message":{"content":[{"text":"hi"}]}},"stopReason":"end_turn"}`);
    match (turn.usage) {
        null => "no-usage",
        _ => "usage",
    }
}

test "bedrock_parse_no_usage" {
    assert.equal(bedrock_parse_no_usage(), "no-usage")
}

// ─── 12. the full path against the mock (sign -> send -> parse) ──────────────

function bedrock_invoke_against_mock() -> string {
    let canned = `{"output":{"message":{"role":"assistant","content":[{"text":"pong"}]}},"stopReason":"end_turn","usage":{"inputTokens":10,"outputTokens":2,"totalTokens":12,"cacheReadInputTokens":4}}`;
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(canned)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = _client(mock.base_url());
            let turn = cl.invoke(
                _turn_input(BedrockEcho@spec("Say pong."), _empty_journal(), _no_tools()),
            );
            let sent = mock.last_request();
            [
                turn.terminal_text() ?? "",
                (turn.usage?.cached_input_tokens ?? -1).to_string(),
                sent.method,
                sent.path(),
                baml.json.path<string>(sent.json(), ".messages[0].content[0].text"),
            ].join("|")
        },
    )
}

test "bedrock_invoke_against_mock" {
    assert.equal(
        bedrock_invoke_against_mock(),
        "pong|4|POST|/model/anthropic.claude-3-5-haiku-20241022-v1%3A0/converse|Say pong.",
    )
}

// Signing is the last mutation and it really happens: the request that reaches
// the server carries the SigV4 headers, scoped to `bedrock` in the client's
// region. Asserting the shape rather than the bytes keeps this test about the
// wiring, not about the signer's own contract.
function bedrock_request_is_signed() -> string {
    let canned = `{"output":{"message":{"content":[{"text":"ok"}]}},"stopReason":"end_turn"}`;
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(canned)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = _client(mock.base_url());
            let _ = cl.invoke(
                _turn_input(BedrockEcho@spec("hi"), _empty_journal(), _no_tools()),
            );
            let sent = mock.last_request();
            [
                sent.header("authorization") ?? "MISSING",
                if ((sent.header("x-amz-date") ?? "").length() > 0) {
                    "dated"
                } else {
                    "undated"
                },
                // Static credentials carry no session token.
                sent.header("x-amz-security-token") ?? "no-token",
            ].join("|")
        },
    )
}

test "bedrock_request_is_signed" {
    let got = bedrock_request_is_signed();
    assert.contains(got, "AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/");
    assert.contains(got, "/us-east-1/bedrock/aws4_request");
    assert.contains(got, "Signature=");
    assert.contains(got, "|dated|no-token")
}

// ─── 13. error taxonomy ──────────────────────────────────────────────────────
//
// The discipline PLAN §7 demands: a 4xx must NOT become a retry-safe
// `NetworkFailure`, or a Retry policy would replay a request the service
// already rejected.

function _invoke_error(status: int, body: string) -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.error(status, body)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = _client(mock.base_url());
            let input = _turn_input(BedrockEcho@spec("hi"), _empty_journal(), _no_tools());
            let label = _stop_label(cl.invoke(input)) catch_all (e) {
                _ => _failure_label(e),
            };
            label
        },
    )
}

function bedrock_error_taxonomy() -> string {
    [
        _invoke_error(400, `{"message":"ValidationException: bad input"}`),
        _invoke_error(403, `{"message":"AccessDeniedException"}`),
        _invoke_error(429, `{"message":"ThrottlingException"}`),
        _invoke_error(500, `{"message":"InternalServerException"}`),
    ].join("|")
}

test "bedrock_error_taxonomy" {
    assert.equal(
        bedrock_error_taxonomy(),
        "InvalidRequest/400|InvalidRequest/403|RateLimited|NetworkFailure",
    )
}

// A 200 whose body is not a Converse response is a ParseFailed, never a
// silently empty turn.
function bedrock_garbage_body() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok("not json at all")],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = _client(mock.base_url());
            let input = _turn_input(BedrockEcho@spec("hi"), _empty_journal(), _no_tools());
            let label = _stop_label(cl.invoke(input)) catch_all (e) {
                _ => _failure_label(e),
            };
            label
        },
    )
}

test "bedrock_garbage_body" {
    assert.equal(bedrock_garbage_body(), "ParseFailed")
}

// ─── 14. live ────────────────────────────────────────────────────────────────
//
// Real Bedrock. Gated twice: the `live` test profile selects the testset, and
// the leaf is registered only when `AWS_PROFILE` is set — Infisical carries no
// AWS keys for this account, so the only verification path is a local SSO
// session (`aws sso login --profile boundaryml-dev`). Without one the testset
// registers nothing and the run stays green; `ns_llm_live_smoke` keeps the
// overall `--profile live` selection non-empty.
//
// `amazon.nova-micro-v1:0` is the cheapest on-demand Bedrock model and is what
// the signer's own systest harness targets
// (`forks/aws-config-systest/src/main.rs:12-15`), so it is known-enabled on
// this account. There is no `bedrock/` client shorthand yet, so the client is
// constructed and handed to the agent explicitly.

function LiveBedrockEcho(question: string) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `
        ${role("user")}Reply with exactly one word. ${question}
    `
}

function live_bedrock_answers() -> bool {
    let cl = aws.BedrockClient.new(
        model = "amazon.nova-micro-v1:0",
        region = baml.env.get("AWS_REGION") ?? "us-east-1",
        profile = baml.env.get("AWS_PROFILE"),
        max_tokens = 64,
    );
    let result = ai.Agent<string>.new(client = cl).run(LiveBedrockEcho@spec("Say pong."));
    result.value.trim().length() > 0
}

testset "live" with testing.Sequential() {
    if (baml.env.get("AWS_PROFILE") != null) {
        test "bedrock" {
            assert.is_true(live_bedrock_answers())
        }
    } else {
        log.warn("live::bedrock not registered: AWS_PROFILE is unset (local SSO session required)")
    }
}
