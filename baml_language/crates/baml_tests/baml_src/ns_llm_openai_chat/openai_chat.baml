// The chat-completions family (`openai.{ChatClient,GenericClient,AzureClient,
// OllamaClient,OpenRouterClient}`) driven against the shared mock provider in
// ../ns_llm_mock.
//
// What is asserted here is the WIRE: the exact JSON body, URL, query string and
// headers each client puts on the socket, and the exact `ai.ModelTurn` it
// builds back out of a canned response. The request-shape cases port the
// assertions from `crates/sys_llm/src/build_request/openai/chat_completions.rs`
// (`chat_single_message`, `chat_multiple_messages`, `chat_image_url`,
// `chat_image_base64`, `azure_defaults_max_tokens_4096`,
// `azure_max_completion_tokens_forwarded`, `openai_no_default_max_tokens`),
// which are the ground truth for the family.
//
// Corpus rules (see ../ns_llm_mock/README.md): every case body is a top-level
// `function` — test-block locals are boxed by the VM — and the `test` block only
// asserts. Cross-namespace references are `root.`-absolute.

// ─── fixtures ────────────────────────────────────────────────────────────────

function ChatEcho(question: string) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `
        ${role("system")}
        You are terse.
        ${role("user")}
        ${question}
    `
}

// No `${role(...)}` marker anywhere: the whole prompt is role-less content.
function ChatBare(question: string) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `Answer briefly: ${question}`
}

function ChatVision(photo: image) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `
        ${role("user")}
        Describe ${photo}
    `
}

/// Report the weather in a city.
function chat_get_weather(city: string) -> string {
    let answer = `sunny in ${city}`;
    answer
}

// A turn input below the runner: no SAP, no journal, no toolbox.
function _chat_input(question: string) -> ai.ModelTurnInput {
    let spec = ChatEcho@spec(question);
    ai.ModelTurnInput {
        prompt: spec.prompt_template,
        journal: ai.Journal { log: [] },
        toolbox: spec.tools(),
        output_type: spec.output_type(),
    }
}

// The same, with a prior tool round already on the record and a live toolbox.
function _chat_input_with_history() -> ai.ModelTurnInput {
    let spec = ChatEcho@spec("and tomorrow?");
    let args: map<string, unknown> = { "city": "SF" };
    let journal = ai.Journal {
        log: [
            ai.events.UserMessage { content: "weather in SF?" },
            ai.events.AssistantMessage {
                content: [
                    ai.content.Text { text: "checking" },
                    ai.content.ToolUse { id: "call_1", name: "chat_get_weather", args: args },
                ],
                client_id: "openai-chat/gpt-4o-mini",
            },
            ai.events.ToolCompleted { id: "call_1", output: "sunny in SF" },
        ],
    };
    ai.ModelTurnInput {
        prompt: spec.prompt_template,
        journal: journal,
        toolbox: ai.tools.Toolbox.new([ai.tools.tool(chat_get_weather)]),
        output_type: spec.output_type(),
    }
}

function _chat_vision_input(photo: image) -> ai.ModelTurnInput {
    let spec = ChatVision@spec(photo);
    ai.ModelTurnInput {
        prompt: spec.prompt_template,
        journal: ai.Journal { log: [] },
        toolbox: spec.tools(),
        output_type: spec.output_type(),
    }
}

// A complete, ordinary chat-completions success body.
function _chat_ok_body() -> string {
    let body = `{"id":"chatcmpl-1","object":"chat.completion","created":1,"model":"gpt-4o-mini","choices":[{"index":0,"message":{"role":"assistant","content":"pong","refusal":null},"logprobs":null,"finish_reason":"stop"}],"usage":{"prompt_tokens":9,"completion_tokens":2,"total_tokens":11,"prompt_tokens_details":{"cached_tokens":4},"completion_tokens_details":{"reasoning_tokens":1}}}`;
    body
}

// The body's top-level keys, in wire order — the assertion that an unset option
// is ABSENT rather than sent as null.
function _chat_top_keys(body: json) -> string {
    match (body) {
        let object: map<string, json> => object.keys().join(","),
        _ => "-",
    }
}

// The failure taxonomy of one thrown error, as a comparable label.
function _chat_failure_label(error: ai.errors.RaisedInClient) -> string {
    match (ai.errors.normalize(error)) {
        let f: ai.errors.RateLimited => `RateLimited/${f.retry_safety()}`,
        let f: ai.errors.InvalidRequest => `InvalidRequest/${f.status_code ?? -1}/${f.retry_safety()}`,
        let f: ai.errors.NetworkFailure => `NetworkFailure/${f.retry_safety()}`,
        let f: ai.errors.ParseFailed => "ParseFailed",
        _ => "other",
    }
}

// ─── request shape: openai.ChatClient ────────────────────────────────────────

// The plain OpenAI request: POST /chat/completions, bearer auth, one message
// per prompt role, content as an array of typed `text` parts, and nothing else
// on the body — no `max_tokens` default, no nulls for unset options
// (sys_llm `openai_no_default_max_tokens`, `chat_multiple_messages`).
function chat_openai_request_shape() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_chat_ok_body())],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ChatClient.new(
                model = "gpt-4o-mini",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            let turn = cl.invoke(_chat_input("Say pong."));
            let sent = mock.last_request();
            let body = sent.json();
            [
                sent.method,
                sent.path(),
                sent.header("authorization") ?? "-",
                sent.header("content-type") ?? "-",
                baml.json.path<string>(body, ".model"),
                baml.json.path<string>(body, ".messages[0].role"),
                baml.json.path<string>(body, ".messages[0].content[0].type"),
                baml.json.path<string>(body, ".messages[1].role"),
                _chat_top_keys(body),
                turn.terminal_text() ?? "-",
                `${turn.stop_reason}`,
                `${turn.usage?.input_tokens ?? -1}/${turn.usage?.output_tokens ?? -1}/${turn.usage?.cached_input_tokens ?? -1}/${turn.usage?.reasoning_tokens ?? -1}`,
            ].join("|")
        },
    )
}

test "chat_openai_request_shape" {
    assert.equal(
        chat_openai_request_shape(),
        "POST|/chat/completions|Bearer test-key|application/json|gpt-4o-mini|system|text|user|model,messages|pong|Complete|9/2/4/1",
    )
}

function chat_request_timeout_ms() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_chat_ok_body())],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ChatClient.new(
                api_key = "k",
                base_url = mock.base_url(),
                request_timeout_ms = 20,
            );
            let outcome = `turn:${cl.invoke(_chat_input("hi")).terminal_text() ?? ""}` catch_all (e) {
                let timeout: baml.errors.Timeout => "Timeout",
                _ => `other:${e.to_string()}`,
            };
            `${cl.request_timeout_ms ?? -1}|${outcome}`
        },
        response_delay_ms = 200,
    )
}

test "chat_request_timeout_ms" {
    assert.equal(chat_request_timeout_ms(), "20|Timeout")
}

// The prompt text itself survives role splitting.
function chat_openai_message_text() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_chat_ok_body())],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ChatClient.new(api_key = "k", base_url = mock.base_url());
            let _ = cl.invoke(_chat_input("Say pong."));
            let body = mock.last_request().json();
            baml.json.path<string>(body, ".messages[0].content[0].text").trim()
                + "//"
                + baml.json.path<string>(body, ".messages[1].content[0].text").trim()
        },
    )
}

test "chat_openai_message_text" {
    assert.equal(chat_openai_message_text(), "You are terse.//Say pong.")
}

// A prompt with no `${role(...)}` marker is legacy system content. The
// compiler-generated LLM spec applies this compatibility default while the
// standalone `prompt` tag remains role-less unless explicitly marked.
function chat_roleless_defaults_to_system() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_chat_ok_body())],
        (mock: root.llm_mock.MockJsonServer) -> {
            let spec = ChatBare@spec("Say pong.");
            let input = ai.ModelTurnInput {
                prompt: spec.prompt_template,
                journal: ai.Journal { log: [] },
                toolbox: spec.tools(),
                output_type: spec.output_type(),
            };
            let cl = openai.ChatClient.new(api_key = "k", base_url = mock.base_url());
            let _ = cl.invoke(input);
            let body = mock.last_request().json();
            baml.json.path<string>(body, ".messages[0].role")
                + "|"
                + `${baml.json.path<json[]>(body, ".messages").length()}`
        },
    )
}

test "chat_roleless_defaults_to_system" {
    assert.equal(chat_roleless_defaults_to_system(), "system|1")
}

// First-class typed parameters lower into their wire keys; the `request_body`
// passthrough merges LAST and a null value deletes a key the client set.
function chat_openai_passthrough() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_chat_ok_body())],
        (mock: root.llm_mock.MockJsonServer) -> {
            let overrides: baml.json.json = {
                "temperature": 0.9,
                "frequency_penalty": 0.5,
                "seed": null,
            };
            let cl = openai.ChatClient.new(
                model = "gpt-4o-mini",
                api_key = "test-key",
                base_url = mock.base_url(),
                temperature = 0.1,
                max_tokens = 256,
                top_p = 0.8,
                stop = ["END"],
                seed = 7,
                headers = { "x-trace": "abc" },
                query_params = { "beta": "a b" },
                request_body = overrides,
            );
            let _ = cl.invoke(_chat_input("hi"));
            let sent = mock.last_request();
            let body = sent.json();
            [
                sent.query() ?? "-",
                sent.header("x-trace") ?? "-",
                `${baml.json.path<float>(body, ".temperature")}`,
                `${baml.json.path<int>(body, ".max_tokens")}`,
                `${baml.json.path<float>(body, ".top_p")}`,
                baml.json.path<string>(body, ".stop[0]"),
                `${baml.json.path<float>(body, ".frequency_penalty")}`,
                `${baml.json.path_or<int>(body, ".seed", -1)}`,
            ].join("|")
        },
    )
}

test "chat_openai_passthrough" {
    assert.equal(
        chat_openai_passthrough(),
        "beta=a%20b|abc|0.9|256|0.8|END|0.5|-1",
    )
}

// A user-supplied `authorization` header cannot shadow the endpoint's own
// credential: user headers are applied first, auth last.
function chat_auth_beats_user_headers() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_chat_ok_body())],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ChatClient.new(
                api_key = "real-key",
                base_url = mock.base_url(),
                headers = { "authorization": "Bearer spoofed" },
            );
            let _ = cl.invoke(_chat_input("hi"));
            mock.last_request().header("authorization") ?? "-"
        },
    )
}

test "chat_auth_beats_user_headers" {
    assert.equal(chat_auth_beats_user_headers(), "Bearer real-key")
}

// ─── tools, journal replay, tool_calls parse ─────────────────────────────────

// A toolbox becomes `tools: [{"type":"function","function":{…}}]` +
// `tool_choice: "auto"`; a prior turn replays as an assistant message carrying
// `tool_calls`, followed by a `role: "tool"` message keyed by tool_call_id.
function chat_tools_and_journal_replay() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_chat_ok_body())],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ChatClient.new(api_key = "k", base_url = mock.base_url());
            let _ = cl.invoke(_chat_input_with_history());
            let body = mock.last_request().json();
            [
                baml.json.path<string>(body, ".tools[0].type"),
                baml.json.path<string>(body, ".tools[0].function.name"),
                baml.json.path<string>(body, ".tools[0].function.parameters.type"),
                baml.json.path<string>(body, ".tool_choice"),
                baml.json.path<string>(body, ".messages[2].role"),
                baml.json.path<string>(body, ".messages[2].content"),
                baml.json.path<string>(body, ".messages[3].role"),
                baml.json.path<string>(body, ".messages[3].content"),
                baml.json.path<string>(body, ".messages[3].tool_calls[0].id"),
                baml.json.path<string>(body, ".messages[3].tool_calls[0].type"),
                baml.json.path<string>(body, ".messages[3].tool_calls[0].function.name"),
                baml.json.path<string>(body, ".messages[3].tool_calls[0].function.arguments"),
                baml.json.path<string>(body, ".messages[4].role"),
                baml.json.path<string>(body, ".messages[4].tool_call_id"),
                baml.json.path<string>(body, ".messages[4].content"),
            ].join("|")
        },
    )
}

test "chat_tools_and_journal_replay" {
    assert.equal(
        chat_tools_and_journal_replay(),
        "function|chat_get_weather|object|auto|user|weather in SF?|assistant|checking|call_1|function|chat_get_weather|{\"city\":\"SF\"}|tool|call_1|sunny in SF",
    )
}

// `tool_calls` on the response become `ai.content.ToolUse` blocks with parsed
// arguments — net-new behaviour, neither Rust implementation parsed them.
function chat_parses_tool_calls() -> string {
    let canned = "{\"id\":\"chatcmpl-2\",\"model\":\"gpt-4o-mini\",\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":null,\"tool_calls\":[{\"id\":\"call_9\",\"type\":\"function\",\"function\":{\"name\":\"chat_get_weather\",\"arguments\":\"{\\\"city\\\":\\\"Paris\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}]}";
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(canned)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ChatClient.new(api_key = "k", base_url = mock.base_url());
            let turn = cl.invoke(_chat_input("weather?"));
            let uses = turn.tool_uses();
            let use = uses.at(0);
            [
                uses.length().to_string(),
                use?.id ?? "-",
                use?.name ?? "-",
                baml.json.stringify(baml.json.to_json(use?.args)),
                `${turn.stop_reason}`,
                turn.terminal_text() ?? "no-text",
            ].join("|")
        },
    )
}

test "chat_parses_tool_calls" {
    assert.equal(
        chat_parses_tool_calls(),
        "1|call_9|chat_get_weather|{\"city\":\"Paris\"}|ToolUse|no-text",
    )
}

// ─── media input ─────────────────────────────────────────────────────────────

// An image URL passes through untouched; base64 becomes a data URL. Both land
// as an `image_url` content part alongside the message's text
// (sys_llm `chat_image_url` / `chat_image_base64`).
function chat_media_parts() -> string {
    root.llm_mock.mock_json_serve(
        [
            root.llm_mock.MockResponse.ok(_chat_ok_body()),
            root.llm_mock.MockResponse.ok(_chat_ok_body()),
        ],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ChatClient.new(api_key = "k", base_url = mock.base_url());
            let _ = cl.invoke(
                _chat_vision_input(baml.media.Image.from_url("https://example.com/cat.png", null)),
            );
            let first = mock.last_request().json();
            let _ = cl.invoke(
                _chat_vision_input(baml.media.Image.from_base64("YWJj", "image/jpeg")),
            );
            let second = mock.last_request().json();
            [
                baml.json.path<string>(first, ".messages[0].content[0].type"),
                baml.json.path<string>(first, ".messages[0].content[1].type"),
                baml.json.path<string>(first, ".messages[0].content[1].image_url.url"),
                baml.json.path<string>(second, ".messages[0].content[1].image_url.url"),
            ].join("|")
        },
    )
}

test "chat_media_parts" {
    assert.equal(
        chat_media_parts(),
        "text|image_url|https://example.com/cat.png|data:image/jpeg;base64,YWJj",
    )
}

// ─── openai.GenericClient ────────────────────────────────────────────────────

// All-text messages collapse to a plain string (many OpenAI-compatible servers
// reject `content: [...]`), and no credential is attached when none is set.
function chat_generic_collapses_text() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_chat_ok_body())],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.GenericClient.new(model = "qwen3", base_url = mock.base_url());
            let _ = cl.invoke(_chat_input("hi"));
            let sent = mock.last_request();
            let body = sent.json();
            [
                sent.header("authorization") ?? "absent",
                baml.json.path<string>(body, ".model"),
                baml.json.path<string>(body, ".messages[0].content").trim(),
                cl.id(),
            ].join("|")
        },
    )
}

test "chat_generic_collapses_text" {
    assert.equal(
        chat_generic_collapses_text(),
        "absent|qwen3|You are terse.|openai-generic/qwen3",
    )
}

// Regression for #4589. Both provider paths receive the exact same answer;
// each result retains the reported omitted/present/explicit-null comparison.
class Issue4589Bbox {
    x float
    y float
    w float
    h float
}

class Issue4589Source {
    doc_id string
    page int
    bbox Issue4589Bbox?
}

class Issue4589Envelope {
    value string?
    source Issue4589Source?
    abstained bool
    note string?
}

class Issue4589FourCases {
    optional_omitted Issue4589Envelope?
    required_omitted Issue4589Envelope
    optional_present Issue4589Envelope?
    optional_explicit_null Issue4589Envelope?
}

function Issue4589Probe(t: string) -> Issue4589FourCases {
    client: "openai/gpt-4o-mini"
    prompt: `${role("user")}${t}\n${ctx.output_format()}`
}

function _issue_4589_answer() -> string {
    `{"optional_omitted":{"value":"optional-omitted","source":{"doc_id":"d","page":0},"abstained":false},"required_omitted":{"value":"required-omitted","source":{"doc_id":"d","page":0},"abstained":false},"optional_present":{"value":"optional-present","source":{"doc_id":"d","page":0,"bbox":{"x":0.1,"y":0.1,"w":0.1,"h":0.1}},"abstained":false},"optional_explicit_null":{"value":"optional-explicit-null","source":{"doc_id":"d","page":0,"bbox":null},"abstained":false}}`
}

function _issue_4589_chat_response() -> string {
    let answer = baml.json.stringify(baml.json.to_json(_issue_4589_answer()));
    `{"id":"chatcmpl-4589","model":"m","choices":[{"index":0,"message":{"role":"assistant","content":${answer}},"finish_reason":"stop"}]}`
}

function _issue_4589_responses_response() -> string {
    let answer = baml.json.stringify(baml.json.to_json(_issue_4589_answer()));
    `{"id":"resp-4589","model":"m","status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":${answer}}]}]}`
}

function _issue_4589_result_label(path: string, value: Issue4589FourCases) -> string {
    [
        path,
        value.optional_omitted?.value ?? "NULLED",
        value.required_omitted.value ?? "NULLED",
        value.optional_present?.value ?? "NULLED",
        value.optional_explicit_null?.value ?? "NULLED",
    ].join("|")
}

function issue_4589_chat_preserves_omitted_nested_optional_class() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_issue_4589_chat_response())],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.GenericClient.new(
                model = "m",
                base_url = mock.base_url() + "/v1",
            );
            let result = ai.Agent.new(client = cl).run(Issue4589Probe@spec("probe")).value;
            _issue_4589_result_label(mock.last_request().path(), result)
        },
    )
}

test "issue_4589_chat_preserves_omitted_nested_optional_class" {
    assert.equal(
        issue_4589_chat_preserves_omitted_nested_optional_class(),
        "/v1/chat/completions|optional-omitted|required-omitted|optional-present|optional-explicit-null",
    )
}

function issue_4589_responses_preserves_omitted_nested_optional_class() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_issue_4589_responses_response())],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ResponsesClient.new(
                model = "m",
                api_key = "test-key",
                base_url = mock.base_url() + "/v1",
            );
            let result = ai.Agent.new(client = cl).run(Issue4589Probe@spec("probe")).value;
            _issue_4589_result_label(mock.last_request().path(), result)
        },
    )
}

test "issue_4589_responses_preserves_omitted_nested_optional_class" {
    assert.equal(
        issue_4589_responses_preserves_omitted_nested_optional_class(),
        "/v1/responses|optional-omitted|required-omitted|optional-present|optional-explicit-null",
    )
}

function chat_generic_request_timeout_ms() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_chat_ok_body())],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.GenericClient.new(
                model = "qwen3",
                base_url = mock.base_url(),
                request_timeout_ms = 20,
            );
            let outcome = `turn:${cl.invoke(_chat_input("hi")).terminal_text() ?? ""}` catch_all (e) {
                let timeout: baml.errors.Timeout => "Timeout",
                _ => `other:${e.to_string()}`,
            };
            `${cl.request_timeout_ms ?? -1}|${outcome}`
        },
        response_delay_ms = 200,
    )
}

test "chat_generic_request_timeout_ms" {
    assert.equal(chat_generic_request_timeout_ms(), "20|Timeout")
}

// An unset base_url is a hard error, not a silent fallback to api.openai.com.
function chat_generic_requires_base_url() -> string {
    let cl = openai.GenericClient.new(model = "qwen3");
    let resolved = cl.resolved_base_url() catch_all (e) {
        _ => {
            return "rejected";
        },
    };
    resolved
}

test "chat_generic_requires_base_url" {
    assert.equal(chat_generic_requires_base_url(), "rejected")
}

// ─── openai.AzureClient ──────────────────────────────────────────────────────

// The deployment lives in the URL, the api version in the query string, the
// credential in an `api-key` header, and 4096 tokens are injected because the
// caller named no ceiling (sys_llm `azure_defaults_max_tokens_4096`).
function chat_azure_url_auth_and_max_tokens() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_chat_ok_body())],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.AzureClient.new(
                model = "gpt-4o",
                api_key = "azure-key",
                base_url = mock.base_url(),
            );
            let _ = cl.invoke(_chat_input("hi"));
            let sent = mock.last_request();
            [
                sent.path(),
                sent.query() ?? "-",
                sent.header("api-key") ?? "-",
                sent.header("authorization") ?? "absent",
                `${baml.json.path<int>(sent.json(), ".max_tokens")}`,
                cl.id(),
            ].join("|")
        },
    )
}

test "chat_azure_url_auth_and_max_tokens" {
    assert.equal(
        chat_azure_url_auth_and_max_tokens(),
        "/chat/completions|api-version=2024-10-21|azure-key|absent|4096|azure/gpt-4o",
    )
}

// resource_name + deployment_id build the canonical Azure endpoint.
function chat_azure_deployment_url() -> string {
    let cl = openai.AzureClient.new(
        model = "gpt-4o",
        resource_name = "my-resource",
        deployment_id = "gpt-4o-deploy",
        api_version = "2025-01-01-preview",
    );
    let compat = cl.compat();
    `${compat.base_url}?api-version=${compat.compat_query.get("api-version") ?? "-"}`
}

test "chat_azure_deployment_url" {
    assert.equal(
        chat_azure_deployment_url(),
        "https://my-resource.openai.azure.com/openai/deployments/gpt-4o-deploy?api-version=2025-01-01-preview",
    )
}

// An Azure deployment is almost always named after the model it serves, so an
// unset `deployment_id` falls back to `model`. Without that the `"azure/…"`
// shorthand — which can only carry a model — could never name a deployment.
function chat_azure_deployment_defaults_to_model() -> string {
    let cl = openai.AzureClient.new(model = "gpt-4o", resource_name = "my-resource");
    cl.compat().base_url
}

test "chat_azure_deployment_defaults_to_model" {
    assert.equal(
        chat_azure_deployment_defaults_to_model(),
        "https://my-resource.openai.azure.com/openai/deployments/gpt-4o",
    )
}

// With no base_url, no resource_name and no `AZURE_OPENAI_ENDPOINT`, the client
// cannot send — and the error has to name the endpoint variable, because that
// is the one the `"azure/<model>"` shorthand expects the deployment to come
// from. It is a TYPED `ai.errors.InvalidRequest`, so an `ai.Fallback` can skip
// this member rather than abort (see `chat_fallback_skips_misconfigured_member`).
//
// A corpus test cannot set environment variables, so the env rung itself is
// covered by the message naming it plus the URL composition asserted above.
function chat_azure_missing_endpoint_names_env() -> string {
    let cl = openai.AzureClient.new(model = "gpt-4o");
    cl.resolved_base_url() catch (e) {
        ai.errors.InvalidRequest => `${e.provider}|${e.detail}`,
    }
}

// Registered only when the variable is unset: the fallback chain ends at the
// real environment, so a machine that HAS an Azure endpoint configured would
// (correctly) resolve a URL instead of failing.
testset "azure_unconfigured" {
    if (baml.env.get("AZURE_OPENAI_ENDPOINT") == null) {
        test "missing_endpoint_names_env" {
            let got = chat_azure_missing_endpoint_names_env();
            assert.contains(got, "azure|");
            assert.contains(got, "AZURE_OPENAI_ENDPOINT");
            assert.contains(got, "base_url");
            assert.contains(got, "resource_name")
        }
    } else {
        log.warn(
            "azure_unconfigured::missing_endpoint_names_env not registered: AZURE_OPENAI_ENDPOINT is set",
        )
    }
}

// ─── typed configuration failures ────────────────────────────────────────────
//
// A missing key / bad config must reach the caller as an `ai.errors.Failure`,
// not a `baml.errors.InvalidArgument`. `ai.errors.normalize` classifies every
// raw infrastructure type into a typed Failure (config bugs land in
// `InvalidRequest`), which is exactly what `ai.Fallback` inspects to decide
// whether to try the next member.

function _config_failure_class(e: ai.errors.RaisedInClient) -> string {
    match (ai.errors.normalize(e)) {
        let f: ai.errors.InvalidRequest => `InvalidRequest/${f.provider}`,
        // Anything else means normalize could not keep an identity — which is
        // precisely what `ai.Fallback` cannot act on.
        _ => "untyped",
    }
}

// `GenericClient.base_url` has no canonical variable to fall through to, so
// this case is deterministic on any machine.
function chat_generic_config_error_survives_normalize() -> string {
    openai.GenericClient.new(model = "m").resolved_base_url() catch_all (e) {
        _ => _config_failure_class(e),
    }
}

test "chat_generic_config_error_survives_normalize" {
    assert.equal(chat_generic_config_error_survives_normalize(), "InvalidRequest/openai-generic")
}

function chat_azure_config_error_survives_normalize() -> string {
    openai.AzureClient.new(model = "gpt-4o").resolved_base_url() catch_all (e) {
        _ => _config_failure_class(e),
    }
}

function chat_anthropic_config_error_survives_normalize() -> string {
    anthropic.AnthropicClient.new(
        model = "claude-haiku-4-5",
        api_key = env.BAML_TEST_UNSET_ANTHROPIC_KEY,
    ).resolved_api_key() catch_all (e) { _ => _config_failure_class(e) }
}

// Both consult a canonical variable as the last rung, so they only fail on a
// machine that has not configured the provider.
testset "config_errors_are_typed" {
    if (baml.env.get("AZURE_OPENAI_ENDPOINT") == null) {
        test "azure" {
            assert.equal(chat_azure_config_error_survives_normalize(), "InvalidRequest/azure")
        }
    } else {
        log.warn("config_errors_are_typed::azure not registered: AZURE_OPENAI_ENDPOINT is set")
    }

    if (baml.env.get("ANTHROPIC_API_KEY") == null) {
        test "anthropic" {
            assert.equal(
                chat_anthropic_config_error_survives_normalize(),
                "InvalidRequest/anthropic",
            )
        }
    } else {
        log.warn("config_errors_are_typed::anthropic not registered: ANTHROPIC_API_KEY is set")
    }
}

// The contract those typed errors buy: a misconfigured member does not abort
// the chain, it is skipped. Before the unification the missing base_url
// normalized to `UnknownError` and `ai.Fallback` stopped there.
function chat_fallback_skips_misconfigured_member() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_chat_ok_body())],
        (mock: root.llm_mock.MockJsonServer) -> {
            let broken = openai.GenericClient.new(model = "broken");
            let working = openai.ChatClient.new(
                model = "gpt-4o-mini",
                api_key = "k",
                base_url = mock.base_url(),
            );
            let members: ai.Client[] = [broken, working];
            let chain = ai.clients.Fallback { members: members };
            chain.invoke(_chat_input("hi")).terminal_text() ?? "-"
        },
    )
}

test "chat_fallback_skips_misconfigured_member" {
    assert.equal(chat_fallback_skips_misconfigured_member(), "pong")
}

// The 4096 default is skipped when the caller set max_completion_tokens — the
// engine's rule; sys_llm sent both (sys_llm `azure_max_completion_tokens_forwarded`
// locked in the wrong body).
function chat_azure_skips_default_max_tokens() -> string {
    root.llm_mock.mock_json_serve(
        [
            root.llm_mock.MockResponse.ok(_chat_ok_body()),
            root.llm_mock.MockResponse.ok(_chat_ok_body()),
        ],
        (mock: root.llm_mock.MockJsonServer) -> {
            let typed = openai.AzureClient.new(
                model = "gpt-4o",
                api_key = "k",
                base_url = mock.base_url(),
                max_completion_tokens = 2048,
            );
            let _ = typed.invoke(_chat_input("hi"));
            let first = mock.last_request().json();
            let removed: baml.json.json = { "max_tokens": null };
            let passthrough = openai.AzureClient.new(
                model = "gpt-4o",
                api_key = "k",
                base_url = mock.base_url(),
                request_body = removed,
            );
            let _ = passthrough.invoke(_chat_input("hi"));
            let second = mock.last_request().json();
            [
                `${baml.json.path_or<int>(first, ".max_tokens", -1)}`,
                `${baml.json.path<int>(first, ".max_completion_tokens")}`,
                `${baml.json.path_or<int>(second, ".max_tokens", -1)}`,
            ].join("|")
        },
    )
}

test "chat_azure_skips_default_max_tokens" {
    assert.equal(chat_azure_skips_default_max_tokens(), "-1|2048|-1")
}

// ─── openai.OllamaClient / openai.OpenRouterClient ───────────────────────────

// The `/v1` suffix is load-bearing: Ollama serves the OpenAI-compatible API
// only under it, and sys_llm's `:11434` default 404s on every call.
function chat_ollama_defaults() -> string {
    let cl = openai.OllamaClient.new(model = "llama3.2");
    `${cl.resolved_base_url()}|${cl.id()}|${cl.resolved_api_key() ?? "none"}`
}

test "chat_ollama_defaults" {
    assert.equal(
        chat_ollama_defaults(),
        "http://localhost:11434/v1|ollama/llama3.2|none",
    )
}

// A `system` message reaches Ollama unchanged (sys_llm rejected the role
// outright), and content collapses to a string.
function chat_ollama_allows_system_role() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_chat_ok_body())],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.OllamaClient.new(model = "llama3.2", base_url = mock.base_url());
            let _ = cl.invoke(_chat_input("hi"));
            let body = mock.last_request().json();
            baml.json.path<string>(body, ".messages[0].role")
                + "|"
                + baml.json.path<string>(body, ".messages[0].content").trim()
        },
    )
}

test "chat_ollama_allows_system_role" {
    assert.equal(chat_ollama_allows_system_role(), "system|You are terse.")
}

// OpenRouter is bearer auth plus attribution headers through the ordinary
// `headers` option.
function chat_openrouter_request_shape() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_chat_ok_body())],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.OpenRouterClient.new(
                model = "openai/gpt-4o-mini",
                api_key = "or-key",
                base_url = mock.base_url(),
                headers = { "HTTP-Referer": "https://myapp.com", "X-Title": "My App" },
            );
            let _ = cl.invoke(_chat_input("hi"));
            let sent = mock.last_request();
            [
                sent.header("authorization") ?? "-",
                sent.header("http-referer") ?? "-",
                sent.header("x-title") ?? "-",
                baml.json.path<string>(sent.json(), ".model"),
                cl.id(),
            ].join("|")
        },
    )
}

test "chat_openrouter_request_shape" {
    assert.equal(
        chat_openrouter_request_shape(),
        "Bearer or-key|https://myapp.com|My App|openai/gpt-4o-mini|openrouter/openai/gpt-4o-mini",
    )
}

// Every wrapper's default base URL and identity, in one place.
function chat_client_identities() -> string {
    [
        openai.ChatClient.new(model = "m").id(),
        openai.GenericClient.new(model = "m").id(),
        openai.AzureClient.new(model = "m").id(),
        openai.OllamaClient.new(model = "m").id(),
        openai.OpenRouterClient.new(model = "m").id(),
    ].join(" ")
}

test "chat_client_identities" {
    assert.equal(
        chat_client_identities(),
        "openai-chat/m openai-generic/m azure/m ollama/m openrouter/m",
    )
}

// ─── error taxonomy ──────────────────────────────────────────────────────────

function _chat_invoke_label(cl: openai.ChatClient, input: ai.ModelTurnInput) -> string {
    let turn = cl.invoke(input) catch_all (e) {
        _ => {
            return _chat_failure_label(e);
        },
    };
    let _ = turn;
    "no-error"
}

// A 400 must never become a retry-safe NetworkFailure: an `ai.clients.Retry`
// wrapper would replay a request the provider already rejected.
function chat_classifies_http_errors() -> string {
    root.llm_mock.mock_json_serve(
        [
            root.llm_mock.MockResponse.error(400, `{"error":{"message":"bad model"}}`),
            root.llm_mock.MockResponse.error(429, `{"error":{"message":"slow down"}}`),
            root.llm_mock.MockResponse.error(503, `{"error":{"message":"overloaded"}}`),
            root.llm_mock.MockResponse.ok(`not json at all`),
        ],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ChatClient.new(api_key = "k", base_url = mock.base_url());
            [
                _chat_invoke_label(cl, _chat_input("hi")),
                _chat_invoke_label(cl, _chat_input("hi")),
                _chat_invoke_label(cl, _chat_input("hi")),
                _chat_invoke_label(cl, _chat_input("hi")),
            ].join("|")
        },
    )
}

test "chat_classifies_http_errors" {
    assert.equal(
        chat_classifies_http_errors(),
        "InvalidRequest/400/Safe|RateLimited/Safe|NetworkFailure/Safe|ParseFailed",
    )
}

// The same discipline on the streaming path: `fetch_sse` reports a non-2xx as an
// opaque Io error, and the client recovers the status from it rather than
// blanket-wrapping every stream failure as retry-safe.
function chat_stream_classifies_http_errors() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.error(400, `{"error":{"message":"bad model"}}`)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ChatClient.new(api_key = "k", base_url = mock.base_url());
            let stream = cl.invoke_stream(_chat_input("hi")) catch_all (e) {
                _ => {
                    return _chat_failure_label(e);
                },
            };
            let _ = stream;
            "no-error"
        },
    )
}

test "chat_stream_classifies_http_errors" {
    assert.equal(chat_stream_classifies_http_errors(), "InvalidRequest/400/Safe")
}

// ─── streaming ───────────────────────────────────────────────────────────────

// Text deltas, the finish-reason chunk, the usage chunk that
// `stream_options.include_usage` asks for, and the `[DONE]` sentinel.
function chat_sse_decodes_text_and_usage() -> string {
    root.llm_mock.mock_sse_serve(
        [
            root.llm_mock.sse_data(`{"id":"c","model":"gpt-4o-mini","choices":[{"index":0,"delta":{"role":"assistant","content":"po"}}]}`),
            root.llm_mock.sse_data(`{"id":"c","choices":[{"index":0,"delta":{"content":"ng"}}]}`),
            root.llm_mock.sse_data(`{"id":"c","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}`),
            root.llm_mock.sse_data(`{"id":"c","choices":[],"usage":{"prompt_tokens":7,"completion_tokens":2,"total_tokens":9}}`),
            root.llm_mock.sse_data(`[DONE]`),
        ],
        (mock: root.llm_mock.MockSseServer) -> {
            let cl = openai.ChatClient.new(api_key = "k", base_url = mock.base_url());
            let stream = cl.invoke_stream(_chat_input("Say pong."));
            let deltas: string[] = [];
            while (true) {
                match (stream.next()) {
                    let done: ai.stream.Done => {
                        break;
                    },
                    let batch: string[] => {
                        for (let piece in batch) {
                            deltas.push(piece);
                        }
                        null
                    },
                };
            }
            let turn = stream.final_turn();
            let sent = mock.last_request();
            [
                deltas.join(","),
                turn.terminal_text() ?? "-",
                `${turn.stop_reason}`,
                `${turn.usage?.input_tokens ?? -1}/${turn.usage?.output_tokens ?? -1}`,
                _chat_top_keys(sent.json()),
                `${baml.json.path_or<bool>(sent.json(), ".stream_options.include_usage", false)}`,
            ].join("|")
        },
    )
}

test "chat_sse_decodes_text_and_usage" {
    assert.equal(
        chat_sse_decodes_text_and_usage(),
        "po,ng|pong|Complete|7/2|model,messages,stream,stream_options|true",
    )
}

// The total request deadline remains active after visible deltas arrive and
// aborts a stalled streaming body with the same typed Timeout as invoke().
function chat_stream_total_timeout_after_delta() -> string {
    root.llm_mock.mock_sse_serve(
        [
            root.llm_mock.sse_data(`{"choices":[{"index":0,"delta":{"content":"po"}}]}`),
            root.llm_mock.sse_data(`{"choices":[{"index":0,"delta":{"content":"ng"}}]}`),
            root.llm_mock.sse_data(`[DONE]`),
        ],
        (mock: root.llm_mock.MockSseServer) -> {
            // The corpus runs every leaf concurrently, so leave connection
            // headroom while keeping the next event past the total deadline.
            let cl = openai.ChatClient.new(
                api_key = "k",
                base_url = mock.base_url(),
                request_timeout_ms = 30000,
            );
            let stream = cl.invoke_stream(_chat_input("hi"));
            let first = match (stream.next()) {
                let batch: string[] => batch.join(""),
                let done: ai.stream.Done => "Done",
            };
            let outcome = `${stream.final_turn().terminal_text() ?? ""}` catch_all (e) {
                let timeout: baml.errors.Timeout => "Timeout",
                _ => `other:${e.to_string()}`,
            };
            `${cl.request_timeout_ms ?? -1}|${first}|${outcome}`
        },
        chunk_delay_ms = 60000,
    )
}

test "chat_stream_total_timeout_after_delta" {
    assert.equal(chat_stream_total_timeout_after_delta(), "30000|po|Timeout")
}

// SSE comment/heartbeat bytes are transport keepalives, not tokens: they must
// not clear TTFT before the first parsed data event arrives.
function chat_stream_heartbeat_does_not_clear_ttft() -> string {
    root.llm_mock.mock_sse_serve(
        [
            ": heartbeat",
            root.llm_mock.sse_data(`{"choices":[{"index":0,"delta":{"content":"pong"}}]}`),
            root.llm_mock.sse_data(`[DONE]`),
        ],
        (mock: root.llm_mock.MockSseServer) -> {
            let cl = openai.ChatClient.new(
                api_key = "k",
                base_url = mock.base_url(),
                request_timeout_ms = 1000,
                time_to_first_token_timeout_ms = 20,
            );
            let outcome = `${cl.invoke_stream(_chat_input("hi")).final_turn().terminal_text() ?? ""}` catch_all (e) {
                let timeout: baml.errors.Timeout => "Timeout",
                _ => `other:${e.to_string()}`,
            };
            `${cl.time_to_first_token_timeout_ms ?? -1}|${outcome}`
        },
        chunk_delay_ms = 200,
    )
}

test "chat_stream_heartbeat_does_not_clear_ttft" {
    assert.equal(chat_stream_heartbeat_does_not_clear_ttft(), "20|Timeout")
}

// `stream_options` is first-party OpenAI only: a compatible server that does not
// know the field would 400 on it.
function chat_generic_omits_stream_options() -> string {
    root.llm_mock.mock_sse_serve(
        [
            root.llm_mock.sse_data(`{"choices":[{"index":0,"delta":{"content":"ok"}}]}`),
            root.llm_mock.sse_data(`[DONE]`),
        ],
        (mock: root.llm_mock.MockSseServer) -> {
            let cl = openai.GenericClient.new(model = "qwen3", base_url = mock.base_url());
            let text = cl.invoke_stream(_chat_input("hi")).final_turn().terminal_text() ?? "-";
            let body = mock.last_request().json();
            text + "|" + _chat_top_keys(body)
        },
    )
}

test "chat_generic_omits_stream_options" {
    assert.equal(chat_generic_omits_stream_options(), "ok|model,messages,stream")
}

function chat_generic_stream_time_to_first_token_timeout_ms() -> string {
    root.llm_mock.mock_sse_serve(
        [
            root.llm_mock.sse_data(`{"choices":[{"index":0,"delta":{"content":"ok"}}]}`),
            root.llm_mock.sse_data(`[DONE]`),
        ],
        (mock: root.llm_mock.MockSseServer) -> {
            let cl = openai.GenericClient.new(
                model = "qwen3",
                base_url = mock.base_url(),
                request_timeout_ms = 1000,
                time_to_first_token_timeout_ms = 20,
            );
            let outcome = `${cl.invoke_stream(_chat_input("hi")).final_turn().terminal_text() ?? ""}` catch_all (e) {
                let timeout: baml.errors.Timeout => "Timeout",
                _ => `other:${e.to_string()}`,
            };
            `${cl.time_to_first_token_timeout_ms ?? -1}|${outcome}`
        },
        first_event_delay_ms = 200,
    )
}

test "chat_generic_stream_time_to_first_token_timeout_ms" {
    assert.equal(chat_generic_stream_time_to_first_token_timeout_ms(), "20|Timeout")
}

// Streamed tool calls arrive as id/name once and argument fragments after.
// `ai.stream.StreamEvent` cannot carry them to the runner yet (PLAN §6), so they
// are buffered in the decoder and readable when the turn ends — this drives the
// decoder over a real SSE connection and asserts the assembled call.
function chat_sse_assembles_tool_calls() -> string {
    root.llm_mock.mock_sse_serve(
        [
            root.llm_mock.sse_data("{\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_7\",\"type\":\"function\",\"function\":{\"name\":\"chat_get_weather\",\"arguments\":\"\"}}]}}]}"),
            root.llm_mock.sse_data("{\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"city\\\":\"}}]}}]}"),
            root.llm_mock.sse_data("{\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"Paris\\\"}\"}}]}}]}"),
            root.llm_mock.sse_data(`{"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}`),
            root.llm_mock.sse_data(`[DONE]`),
        ],
        (mock: root.llm_mock.MockSseServer) -> {
            let decoder = openai.internal.ChatStreamDecoder.new(provider = "openai-chat");
            let request = baml.http.Request {
                method: "POST",
                url: mock.base_url() + "/chat/completions",
                headers: { "content-type": "application/json" },
                body: `{"stream":true}`,
            };
            let sse = baml.http.fetch_sse(request);
            let stops: string[] = [];
            while (true) {
                match (sse.next()) {
                    null => {
                        break;
                    },
                    let batch: string => {
                        for (let event in decoder.decode(batch)) {
                            match (event) {
                                let meta: ai.stream.TurnMeta => {
                                    if let reason: ai.content.StopReason = meta.stop_reason {
                                        stops.push(`${reason}`);
                                    }
                                    null
                                },
                                _ => null,
                            };
                        }
                        null
                    },
                };
            }
            sse.close();
            let uses = decoder.tool_uses();
            let use = uses.at(0);
            [
                uses.length().to_string(),
                use?.id ?? "-",
                use?.name ?? "-",
                baml.json.stringify(baml.json.to_json(use?.args)),
                stops.join(","),
            ].join("|")
        },
    )
}

test "chat_sse_assembles_tool_calls" {
    assert.equal(
        chat_sse_assembles_tool_calls(),
        "1|call_7|chat_get_weather|{\"city\":\"Paris\"}|ToolUse",
    )
}

// ─── through the runner ──────────────────────────────────────────────────────

// The whole path: spec -> prompt -> request -> canned response -> SAP -> value.
function chat_runs_through_the_agent() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_chat_ok_body())],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ChatClient.new(
                model = "gpt-4o-mini",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            let result = ai.Agent.new(client = cl).run(ChatEcho@spec("Say pong."));
            result.value
        },
    )
}

test "chat_runs_through_the_agent" {
    assert.equal(chat_runs_through_the_agent(), "pong")
}

// ─── Stage D verifier regressions ────────────────────────────────────────────
//
// One case per confirmed finding in
// `_plan/sys_llm_native/stage-d-findings/V1-chat-family.json`. Each asserts the
// wire or the typed error the finding said was missing.

function ChatDoc(doc: pdf) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `
        ${role("user")}
        Summarize ${doc}
    `
}

function _chat_doc_input(doc: pdf) -> ai.ModelTurnInput {
    let spec = ChatDoc@spec(doc);
    ai.ModelTurnInput {
        prompt: spec.prompt_template,
        journal: ai.Journal { log: [] },
        toolbox: spec.tools(),
        output_type: spec.output_type(),
    }
}

// Chat completions has NO document-by-URL part — the API's `file` object is
// `{filename, file_data, file_id}`, and `{"type":"input_file","file_url":…}` is
// the Responses API's shape, which this endpoint ignores or rejects. A remote
// PDF is therefore fetched and inlined, exactly like audio. The mock answers the
// media GET first, then the chat POST.
function chat_pdf_url_is_inlined() -> string {
    root.llm_mock.mock_json_serve(
        [
            root.llm_mock.MockResponse.with_headers(
                200,
                "%PDF-1.4",
                { "content-type": "application/pdf" },
            ),
            root.llm_mock.MockResponse.ok(_chat_ok_body()),
        ],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ChatClient.new(api_key = "k", base_url = mock.base_url());
            let doc = baml.media.Pdf.from_url(mock.base_url() + "/doc.pdf", null);
            let _ = cl.invoke(_chat_doc_input(doc));
            let body = mock.last_request().json();
            [
                baml.json.path<string>(body, ".messages[0].content[1].type"),
                baml.json.path<string>(body, ".messages[0].content[1].file.filename"),
                baml.json.path<string>(body, ".messages[0].content[1].file.file_data"),
                baml.json.path_or<string>(body, ".messages[0].content[1].file.file_url", "absent"),
                baml.json.path_or<string>(body, ".messages[0].content[1].file.type", "absent"),
            ].join("|")
        },
    )
}

// The filename is DERIVED from the source (`.../doc.pdf` -> `doc.pdf`), the
// same basename rule the Responses client uses, rather than a constant.
test "chat_pdf_url_is_inlined" {
    assert.equal(
        chat_pdf_url_is_inlined(),
        "file|doc.pdf|data:application/pdf;base64,JVBERi0xLjQ=|absent|absent",
    )
}

// An already-inline PDF takes the same path, and a scheme the client cannot
// fetch is a typed configuration error rather than a blank `file_data`.
function chat_pdf_base64_and_unfetchable() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_chat_ok_body())],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ChatClient.new(api_key = "k", base_url = mock.base_url());
            let inline = baml.media.Pdf.from_base64("JVBERi0xLjQ=", "application/pdf");
            let _ = cl.invoke(_chat_doc_input(inline));
            let body = mock.last_request().json();
            let remote = baml.media.Pdf.from_url("gs://bucket/doc.pdf", null);
            let rejected = cl.invoke(_chat_doc_input(remote)) catch_all (e) {
                _ => {
                    return baml.json.path<string>(
                            body,
                            ".messages[0].content[1].file.file_data",
                        )
                        + "|rejected";
                },
            };
            let _ = rejected;
            "gs-url-accepted"
        },
    )
}

test "chat_pdf_base64_and_unfetchable" {
    assert.equal(
        chat_pdf_base64_and_unfetchable(),
        "data:application/pdf;base64,JVBERi0xLjQ=|rejected",
    )
}

// A tool's `parameters` is the caller's verbatim JSON Schema: a `null` in it is
// DATA (`"default": null`, a nullable union member), not an unset option, and
// pruning it changes the schema the model is shown and breaks strict-schema
// servers. The null-pruning pass must not walk into it.
function chat_tool_schema_keeps_nulls() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_chat_ok_body())],
        (mock: root.llm_mock.MockJsonServer) -> {
            let schema = baml.json.from_string<map<string, unknown>>(
                `{"type":"object","properties":{"x":{"type":["string","null"],"default":null}},"required":[]}`,
            );
            let spec = ChatEcho@spec("hi");
            let input = ai.ModelTurnInput {
                prompt: spec.prompt_template,
                journal: ai.Journal { log: [] },
                toolbox: ai.tools.Toolbox.new(
                    [
                        ai.tools.raw_tool(
                            name = "nullable_tool",
                            description = "takes a nullable field",
                            input_schema = schema,
                            handler = (args) -> { "ok" },
                        ),
                    ],
                ),
                output_type: spec.output_type(),
            };
            let cl = openai.ChatClient.new(api_key = "k", base_url = mock.base_url());
            let _ = cl.invoke(input);
            let params = baml.json.path<json>(
                mock.last_request().json(),
                ".tools[0].function.parameters",
            );
            match (params) {
                let object: map<string, json> => {
                    let properties = baml.json.path<json>(params, ".properties.x");
                    match (properties) {
                        let field: map<string, json> => {
                            `${field.has("default")}|${baml.json.stringify(field.get("default"))}`
                        },
                        _ => "not-an-object",
                    }
                },
                _ => "not-an-object",
            }
        },
    )
}

test "chat_tool_schema_keeps_nulls" {
    assert.equal(chat_tool_schema_keeps_nulls(), "true|null")
}

// `data: [DONE]` is chat completions' explicit terminator. A connection that
// ends without it lost data, and reporting the partial text as a completed turn
// is silent truncation — `require_terminal` turns it into a typed transport
// failure instead.
function chat_truncated_stream_throws() -> string {
    root.llm_mock.mock_sse_serve(
        [
            root.llm_mock.sse_data(`{"id":"c","choices":[{"index":0,"delta":{"content":"par"}}]}`),
        ],
        (mock: root.llm_mock.MockSseServer) -> {
            let cl = openai.ChatClient.new(api_key = "k", base_url = mock.base_url());
            let stream = cl.invoke_stream(_chat_input("Say pong."));
            let text = stream.final_turn().terminal_text() catch_all (e) {
                _ => {
                    return _chat_failure_label(e);
                },
            };
            `no-error/${text ?? "-"}`
        },
    )
}

test "chat_truncated_stream_throws" {
    assert.equal(chat_truncated_stream_throws(), "NetworkFailure/Safe")
}

// A stream that DOES reach `[DONE]` still completes normally — the strict check
// must not fire on a healthy turn (guard against over-correcting).
function chat_terminated_stream_still_completes() -> string {
    root.llm_mock.mock_sse_serve(
        [
            root.llm_mock.sse_data(`{"id":"c","choices":[{"index":0,"delta":{"content":"ok"}}]}`),
            root.llm_mock.sse_data(`[DONE]`),
        ],
        (mock: root.llm_mock.MockSseServer) -> {
            let cl = openai.ChatClient.new(api_key = "k", base_url = mock.base_url());
            cl.invoke_stream(_chat_input("hi")).final_turn().terminal_text() ?? "-"
        },
    )
}

test "chat_terminated_stream_still_completes" {
    assert.equal(chat_terminated_stream_still_completes(), "ok")
}

// A message's authored metadata is a per-part provider directive on this family
// (OpenAI `prompt_cache_breakpoint`, OpenRouter's Anthropic-style
// `cache_control`). It rides on the message's LAST content part, forwarded
// verbatim — and a message carrying one never collapses to a bare string, which
// would drop it.
function ChatCached(question: string) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `${ai.Role { name: "system", metadata: { "prompt_cache_breakpoint": { "type": "enabled" } } }}Follow the rules.${role("user")}${question}`
}

function _chat_cached_input() -> ai.ModelTurnInput {
    let spec = ChatCached@spec("hi");
    ai.ModelTurnInput {
        prompt: spec.prompt_template,
        journal: ai.Journal { log: [] },
        toolbox: spec.tools(),
        output_type: spec.output_type(),
    }
}

function chat_metadata_rides_last_content_part() -> string {
    root.llm_mock.mock_json_serve(
        [
            root.llm_mock.MockResponse.ok(_chat_ok_body()),
            root.llm_mock.MockResponse.ok(_chat_ok_body()),
        ],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ChatClient.new(api_key = "k", base_url = mock.base_url());
            let _ = cl.invoke(_chat_cached_input());
            let body = mock.last_request().json();
            // The collapsing wrapper must not swallow the directive either.
            let collapsing = openai.GenericClient.new(
                model = "qwen3",
                base_url = mock.base_url(),
            );
            let _ = collapsing.invoke(_chat_cached_input());
            let collapsed = mock.last_request().json();
            [
                baml.json.path<string>(body, ".messages[0].content[0].type"),
                baml.json.path<string>(body, ".messages[0].content[0].text").trim(),
                baml.json.path<string>(
                    body,
                    ".messages[0].content[0].prompt_cache_breakpoint.type",
                ),
                baml.json.path_or<string>(
                    body,
                    ".messages[1].content[0].prompt_cache_breakpoint.type",
                    "absent",
                ),
                baml.json.path<string>(
                    collapsed,
                    ".messages[0].content[0].prompt_cache_breakpoint.type",
                ),
            ].join("|")
        },
    )
}

test "chat_metadata_rides_last_content_part" {
    assert.equal(
        chat_metadata_rides_last_content_part(),
        "text|Follow the rules.|enabled|absent|enabled",
    )
}

// "Forwarded verbatim" includes an explicitly authored `null`. A directive is
// the caller telling the endpoint something; `null` is a thing to say (it is
// how OpenRouter turns a cache breakpoint OFF for one message), and it is NOT
// the same as omitting the key. The body's null-pruning pass exists to drop the
// CLIENT's unset options — it must not reach into an authored directive, which
// is why the directives ride a parked, opaque subtree until after the prune.
function ChatCachedNull(question: string) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `${ai.Role { name: "system", metadata: { "cache_control": null, "keep": "yes" } }}Follow the rules.${role("user")}${question}`
}

function chat_authored_null_directive_survives_pruning() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_chat_ok_body())],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ChatClient.new(api_key = "k", base_url = mock.base_url());
            let spec = ChatCachedNull@spec("hi");
            let _ = cl.invoke(
                ai.ModelTurnInput {
                    prompt: spec.prompt_template,
                    journal: ai.Journal { log: [] },
                    toolbox: spec.tools(),
                    output_type: spec.output_type(),
                },
            );
            let raw = mock.last_request().body;
            let body = mock.last_request().json();
            [
                // the authored null is ON the wire ...
                `null_kept=${raw.includes(`"cache_control":null`)}`,
                // ... alongside its non-null sibling ...
                baml.json.path<string>(body, ".messages[0].content[0].keep"),
                // ... and the reserved parking key never reaches it.
                `parked=${raw.includes("__baml_directives")}`,
            ].join("|")
        },
    )
}

test "chat_authored_null_directive_survives_pruning" {
    assert.equal(
        chat_authored_null_directive_survives_pruning(),
        "null_kept=true|yes|parked=false",
    )
}

// Ollama's OpenAI-compatible endpoint cannot read an image URL ("Base64 encoded
// image ✓ / Image URL ✗"), so its client fetches and inlines one. The plain
// OpenAI client still passes the URL straight through.
function chat_ollama_inlines_image_urls() -> string {
    root.llm_mock.mock_json_serve(
        [
            root.llm_mock.MockResponse.with_headers(
                200,
                "PNGDATA",
                { "content-type": "image/png" },
            ),
            root.llm_mock.MockResponse.ok(_chat_ok_body()),
            root.llm_mock.MockResponse.ok(_chat_ok_body()),
        ],
        (mock: root.llm_mock.MockJsonServer) -> {
            let url = mock.base_url() + "/cat.png";
            let ollama = openai.OllamaClient.new(model = "llava", base_url = mock.base_url());
            let _ = ollama.invoke(_chat_vision_input(baml.media.Image.from_url(url, null)));
            let inlined = mock.last_request().json();
            let openai_chat = openai.ChatClient.new(api_key = "k", base_url = mock.base_url());
            let _ = openai_chat.invoke(_chat_vision_input(baml.media.Image.from_url(url, null)));
            let passed_through = baml.json.path<string>(
                mock.last_request().json(),
                ".messages[0].content[1].image_url.url",
            );
            baml.json.path<string>(inlined, ".messages[0].content[1].image_url.url")
                + "|"
                + `${passed_through == url}`
        },
    )
}

test "chat_ollama_inlines_image_urls" {
    assert.equal(
        chat_ollama_inlines_image_urls(),
        "data:image/png;base64,UE5HREFUQQ==|true",
    )
}

// A 429's `Retry-After` is the provider telling us how long to wait;
// `ai.clients.Backoff` honours it over its own computed delay, so the
// classifier has to be given the response headers.
function chat_rate_limit_carries_retry_after() -> string {
    root.llm_mock.mock_json_serve(
        [
            root.llm_mock.MockResponse.with_headers(
                429,
                `{"error":{"message":"slow down"}}`,
                { "retry-after": "3" },
            ),
        ],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ChatClient.new(api_key = "k", base_url = mock.base_url());
            let turn = cl.invoke(_chat_input("hi")) catch_all (e) {
                _ => {
                    return match (ai.errors.normalize(e)) {
                        let limited: ai.errors.RateLimited => `${limited.retry_after_ms ?? -1}`,
                        _ => "not-rate-limited",
                    };
                },
            };
            let _ = turn;
            "no-error"
        },
    )
}

test "chat_rate_limit_carries_retry_after" {
    assert.equal(chat_rate_limit_carries_retry_after(), "3000")
}

// A tool call whose `arguments` is not valid JSON must not degrade to `{}`: the
// runner would then invoke a real tool with no arguments and nobody would learn
// they were discarded.
function chat_bad_tool_arguments_throw() -> string {
    let canned = "{\"id\":\"c\",\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"tool_calls\":[{\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"chat_get_weather\",\"arguments\":\"{not json\"}}]},\"finish_reason\":\"tool_calls\"}]}";
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(canned)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ChatClient.new(api_key = "k", base_url = mock.base_url());
            let turn = cl.invoke(_chat_input("weather?")) catch_all (e) {
                _ => {
                    return _chat_failure_label(e);
                },
            };
            baml.json.stringify(baml.json.to_json(turn.tool_uses().at(0)?.args))
        },
    )
}

test "chat_bad_tool_arguments_throw" {
    assert.equal(chat_bad_tool_arguments_throw(), "ParseFailed")
}

// A streamed refusal arrives in its own `delta.refusal` field, never in
// `content`; without it the turn ends empty and the caller never learns the
// model declined.
function chat_stream_surfaces_refusal() -> string {
    root.llm_mock.mock_sse_serve(
        [
            root.llm_mock.sse_data(`{"id":"c","choices":[{"index":0,"delta":{"role":"assistant","refusal":"I cannot help with that."}}]}`),
            root.llm_mock.sse_data(`{"id":"c","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}`),
            root.llm_mock.sse_data(`[DONE]`),
        ],
        (mock: root.llm_mock.MockSseServer) -> {
            let cl = openai.ChatClient.new(api_key = "k", base_url = mock.base_url());
            cl.invoke_stream(_chat_input("something bad")).final_turn().terminal_text() ?? "-"
        },
    )
}

test "chat_stream_surfaces_refusal" {
    assert.equal(chat_stream_surfaces_refusal(), "I cannot help with that.")
}

// A payload that claims to be a chunk but does not decode is data loss, not
// noise: skipping it silently drops whatever text it carried. Anything that is
// not object-shaped (a keepalive, a vendor sentinel) is still ignored.
function chat_stream_rejects_corrupt_chunk() -> string {
    root.llm_mock.mock_sse_serve(
        [
            root.llm_mock.sse_data(`{"id":"c","choices":[{"index":0,"delta":{"content":"a"}}]}`),
            root.llm_mock.sse_data(`{"id":"c","choices":[{"index":0,"delta":{"cont`),
            root.llm_mock.sse_data(`{"id":"c","choices":[{"index":0,"delta":{"content":"b"}}]}`),
            root.llm_mock.sse_data(`[DONE]`),
        ],
        (mock: root.llm_mock.MockSseServer) -> {
            let cl = openai.ChatClient.new(api_key = "k", base_url = mock.base_url());
            let text = cl.invoke_stream(_chat_input("hi")).final_turn().terminal_text()
                catch_all (e) {
                    _ => {
                        return _chat_failure_label(e);
                    },
                };
            `no-error/${text ?? "-"}`
        },
    )
}

test "chat_stream_rejects_corrupt_chunk" {
    assert.equal(chat_stream_rejects_corrupt_chunk(), "ParseFailed")
}

// A non-JSON keepalive between chunks is still skipped.
function chat_stream_skips_keepalive() -> string {
    root.llm_mock.mock_sse_serve(
        [
            root.llm_mock.sse_data(`{"id":"c","choices":[{"index":0,"delta":{"content":"a"}}]}`),
            root.llm_mock.sse_data(`OPENROUTER PROCESSING`),
            root.llm_mock.sse_data(`{"id":"c","choices":[{"index":0,"delta":{"content":"b"}}]}`),
            root.llm_mock.sse_data(`[DONE]`),
        ],
        (mock: root.llm_mock.MockSseServer) -> {
            let cl = openai.ChatClient.new(api_key = "k", base_url = mock.base_url());
            cl.invoke_stream(_chat_input("hi")).final_turn().terminal_text() ?? "-"
        },
    )
}

test "chat_stream_skips_keepalive" {
    assert.equal(chat_stream_skips_keepalive(), "ab")
}

// Audio output (reachable through the `request_body` passthrough) carries the
// spoken bytes and their transcript; both used to be dropped. The mime type is
// recovered from the `audio.format` the request asked for, since the response
// does not echo it.
function chat_parses_audio_output() -> string {
    let canned = "{\"id\":\"c\",\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":null,\"audio\":{\"id\":\"audio_1\",\"data\":\"UE5HREFUQQ==\",\"transcript\":\"hello there\",\"expires_at\":1}},\"finish_reason\":\"stop\"}]}";
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(canned)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let modalities: baml.json.json = {
                "modalities": ["text", "audio"],
                "audio": { "voice": "alloy", "format": "mp3" },
            };
            let cl = openai.ChatClient.new(
                api_key = "k",
                base_url = mock.base_url(),
                request_body = modalities,
            );
            let turn = cl.invoke(_chat_input("say hi"));
            let media: ai.content.Media[] = [];
            for (let block in turn.content) {
                if let m: ai.content.Media = block {
                    media.push(m);
                }
            }
            let first = media.at(0);
            [
                turn.terminal_text() ?? "-",
                `${media.length()}`,
                first?.kind() ?? "-",
                first?.provider_id ?? "-",
                match (first?.media) {
                    let audio: baml.media.Audio => audio.mime_type() ?? "-",
                    _ => "-",
                },
            ].join("|")
        },
    )
}

test "chat_parses_audio_output" {
    assert.equal(
        chat_parses_audio_output(),
        "hello there|1|audio|audio_1|audio/mpeg",
    )
}

// ─── live (opt-in; see ../baml.toml and ../ns_llm_mock/README.md) ────────────
//
// Selected only by `--profile live`, and each leaf is registered only when its
// key is present. Ollama has no leaf: it needs a server running on the machine,
// which CI and a laptop without `ollama serve` do not have.

function live_openai_chat_answers() -> bool {
    let cl = openai.ChatClient.new(model = "gpt-4o-mini", max_tokens = 16);
    let result = ai.Agent.new(client = cl).run(ChatEcho@spec("Say pong."));
    result.value.trim().length() > 0
}

function live_openai_chat_streams() -> bool {
    let cl = openai.ChatClient.new(model = "gpt-4o-mini", max_tokens = 16);
    let stream = ai.stream.from_spec<string | null, string>(
        ChatEcho@spec("Say pong."),
        client = cl,
    );
    stream.final().trim().length() > 0
}

function live_openrouter_answers() -> bool {
    let cl = openai.OpenRouterClient.new(model = "openai/gpt-4o-mini", max_tokens = 16);
    let result = ai.Agent.new(client = cl).run(ChatEcho@spec("Say pong."));
    result.value.trim().length() > 0
}

testset "live" with testing.Sequential() {
    // Always registered, so a keyless `--profile live -i openai_chat` still
    // selects something (an empty selection exits 5).
    test "preconditions" {
        assert.is_true(true)
    }

    if (baml.env.get("OPENAI_API_KEY") != null) {
        test "openai_chat" {
            assert.is_true(live_openai_chat_answers())
        }
        test "openai_chat_stream" {
            assert.is_true(live_openai_chat_streams())
        }
    } else {
        log.warn("live::openai_chat not registered: OPENAI_API_KEY is unset")
    }

    if (baml.env.get("OPENROUTER_API_KEY") != null) {
        test "openrouter" {
            assert.is_true(live_openrouter_answers())
        }
    } else {
        log.warn("live::openrouter not registered: OPENROUTER_API_KEY is unset")
    }
}
