// Live provider matrix; excluded by the default offline profile.
// See README.md for the Infisical command.

function HelloWorld() -> string {
    client: "openai/gpt-4o-mini"
    prompt: `
        ${role("user")}
        Reply with exactly: hello world
        Do not add quotes, punctuation, formatting, or any other text.
    `
}

function check_hello_world(client: ai.Client) -> void {
    assert.equal(ai.Agent.new(client = client).run(HelloWorld@spec()).value, "hello world");
    let streaming: ai.stream.StreamingClient = match (client) {
        let s: ai.stream.StreamingClient => s,
        _ => {
            throw ai.errors.StreamingUnsupported { client_id: client.id() }
        },
    };
    assert.equal(
        ai.stream.from_spec<string>(HelloWorld@spec(), client = streaming).final(),
        "hello world",
    );
}

function check_BedrockHaiku45() -> void {
    check_hello_world(
        aws.BedrockClient.new(
            model = "global.anthropic.claude-haiku-4-5-20251001-v1:0",
            region = baml.env.get_or_panic("BEDROCK_AWS_REGION"),
            access_key_id = baml.env.get_or_panic("BEDROCK_AWS_ACCESS_KEY_ID"),
            secret_access_key = baml.env.get_or_panic("BEDROCK_AWS_SECRET_ACCESS_KEY"),
            max_tokens = 256,
            temperature = 0.0,
        ),
    )
}

// A prompt with a stable prefix, a `${cache()}` after it, and a question
// that changes. The delimiter carries no args, so the client that sends the
// request supplies its own default marker. The nonce makes the prefix new to the provider on each run, so
// the first call has to write the cache.
function CachedReference(nonce: string, reference: string, question: string) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `
        ${role("user")}
        Reference document ${nonce}.
        ${reference}
        ${cache()}
        ${question}
    `
}

// About 12,000 tokens of filler for a cached prefix: three times Claude Haiku
// 4.5's 4,096-token minimum, and well over OpenAI's 1,024.
function cache_reference() -> string {
    let lines: string[] = [];
    let i = 0;
    while (i < 600) {
        lines.push(`Entry ${i}: the quick brown fox jumps over the lazy dog, item ${i * 7}.`);
        i += 1;
    }
    lines.join("\n")
}

// Two calls sharing a fresh prefix, the first answering "one" and the second
// "two". Returns the two calls' usage, logged under `name`.
function cache_round_trip(name: string, client: ai.Client) -> ai.events.Usage[] {
    let nonce = baml.time.Instant.now().to_string();
    let reference = cache_reference();
    let agent = ai.Agent.new(client = client);

    let first = agent.run(CachedReference@spec(nonce, reference, "Reply with exactly: one"));
    log.info(`${name} first call: ${baml.json.stringify(baml.json.to_json(first.usage))}`);
    assert.equal(first.value, "one");

    let second = agent.run(CachedReference@spec(nonce, reference, "Reply with exactly: two"));
    log.info(`${name} second call: ${baml.json.stringify(baml.json.to_json(second.usage))}`);
    assert.equal(second.value, "two");
    [first.usage, second.usage]
}

// Claude caches only where it is marked, so the default marker is what makes
// the first call write the prefix and the second read the same count back.
function check_claude_cache(name: string, client: ai.Client) -> void {
    let usage = cache_round_trip(name, client);
    assert.is_true((usage[0].cache_write_input_tokens ?? 0) > 4096);
    assert.equal(usage[1].cached_input_tokens, usage[0].cache_write_input_tokens);
}

// OpenAI also caches a prefix it was not told about, so a read on the second
// call does not by itself show the marker worked. What this checks is that
// the endpoint accepts the request `${cache()}` produces for this model (on
// GPT-5.5 and earlier, a marker would be a 400) and that the prefix is read
// back.
function check_openai_cache(name: string, client: ai.Client) -> void {
    let usage = cache_round_trip(name, client);
    assert.is_true((usage[1].cached_input_tokens ?? 0) > 1024);
}

function check_BedrockHaiku45Cache() -> void {
    check_claude_cache(
        "BedrockHaiku45Cache",
        aws.BedrockClient.new(
            model = "global.anthropic.claude-haiku-4-5-20251001-v1:0",
            region = baml.env.get_or_panic("BEDROCK_AWS_REGION"),
            access_key_id = baml.env.get_or_panic("BEDROCK_AWS_ACCESS_KEY_ID"),
            secret_access_key = baml.env.get_or_panic("BEDROCK_AWS_SECRET_ACCESS_KEY"),
            max_tokens = 256,
            temperature = 0.0,
        ),
    )
}

function check_AnthropicHaiku45Cache() -> void {
    check_claude_cache(
        "AnthropicHaiku45Cache",
        anthropic.Client.new(
            model = "claude-haiku-4-5",
            api_key = env.ANTHROPIC_API_KEY,
            max_tokens = 256,
            temperature = 0.0,
        ),
    )
}

function check_OpenAIChatGpt56Cache() -> void {
    let cl = openai.ChatClient.new(model = "gpt-5.6-luna", api_key = env.OPENAI_API_KEY);
    assert.equal(cl.default_cache_args(), { "mode": "explicit" });
    check_openai_cache("OpenAIChatGpt56Cache", cl)
}

function check_OpenAIResponsesGpt56Cache() -> void {
    let cl = openai.ResponsesClient.new(model = "gpt-5.6-luna", api_key = env.OPENAI_API_KEY, store = false);
    assert.equal(cl.default_cache_args(), { "mode": "explicit" });
    check_openai_cache("OpenAIResponsesGpt56Cache", cl)
}

// A deployment on the resource behind AZURE_OPENAI_RESPONSES_BASE_URL, through
// the chat completions client. Each deployment is named after its model.
function azure_chat(deployment: string) -> openai.AzureClient {
    let resource = baml.env.get_or_panic("AZURE_OPENAI_RESPONSES_BASE_URL").replace("/openai/v1", "");
    openai.AzureClient.new(
        model = deployment,
        base_url = `${resource}/openai/deployments/${deployment}`,
        api_key = env.AZURE_OPENAI_API_KEY,
        // GPT-5 models reject the `max_tokens` Azure would otherwise default to.
        max_completion_tokens = 1024,
    )
}

// Azure takes a few seconds to make a cache write readable: back to back, the
// second call almost always misses and writes the prefix again. So the Azure
// GPT-5.6 tests assert the write, which the first call reports for the
// `${cache()}` marker, and not a read.
function check_azure_cache(name: string, client: ai.Client) -> void {
    let usage = cache_round_trip(name, client);
    assert.is_true((usage[0].cache_write_input_tokens ?? 0) > 1024);
}

function check_AzureChatGpt56Cache() -> void {
    let cl = azure_chat("gpt-5.6-luna");
    assert.equal(cl.default_cache_args(), { "mode": "explicit" });
    check_azure_cache("AzureChatGpt56Cache", cl)
}

// gpt-5-mini rejects `prompt_cache_breakpoint`, so `${cache()}` sends no marker
// and both calls go through. Nothing is asserted about the cache: with no
// marker, Azure's implicit cache is all there is, and it misses often enough
// on the second call to flake.
function check_AzureChatGpt5MiniCache() -> void {
    let cl = azure_chat("gpt-5-mini");
    assert.equal(cl.default_cache_args(), null);
    let _ = cache_round_trip("AzureChatGpt5MiniCache", cl);
}

// `prompt_cache_key` rides in `request_body`, through the
// `resource_name` + `deployment_id` path: the request carries the key as
// written, and the endpoint accepts it alongside the default cache marker.
function check_AzureChatGpt56CacheKey() -> void {
    // The `{name}` in `https://{name}.cognitiveservices.azure.com/openai/v1`.
    let host = baml.env.get_or_panic("AZURE_OPENAI_RESPONSES_BASE_URL").replace("https://", "");
    let resource_name = host.split(".").at(0) ?? "";
    let cl = openai.AzureClient.new(
        model = "gpt-5.6-luna",
        resource_name = resource_name,
        deployment_id = "gpt-5.6-luna",
        api_key = env.AZURE_OPENAI_API_KEY,
        max_completion_tokens = 1024,
        request_body = { "prompt_cache_key": `lorem-ipsum-dolor` },
    );
    let body = baml.json.parse(CachedReference@build_request("n", "r", "q", client = cl).body);
    assert.equal(baml.json.path<json>(body, ".prompt_cache_key"), "lorem-ipsum-dolor");
    check_azure_cache("AzureChatGpt56CacheKey", cl)
}

function check_BedrockGptOss20b() -> void {
    check_hello_world(
        openai.ResponsesClient.new(
            model = "openai.gpt-oss-20b",
            // Mantle serves gpt-oss under /v1 rather than /openai/v1.
            base_url = "https://bedrock-mantle.us-east-1.api.aws/v1",
            api_key = env.BEDROCK_MANTLE_API_KEY,
            reasoning_effort = "low",
            store = false,
        ),
    )
}

function check_AzureGpt5MiniLow() -> void {
    let key = baml.env.get_or_panic("AZURE_OPENAI_API_KEY");
    check_hello_world(
        openai.ResponsesClient.new(
            model = "gpt-5-mini",
            base_url = env.AZURE_OPENAI_RESPONSES_BASE_URL,
            api_key = key,
            headers = { "api-key": key },
            reasoning_effort = "low",
            store = false,
        ),
    )
}

function check_Gemini31FlashLiteMinimal() -> void {
    check_hello_world(
        google.GeminiClient.new(
            model = "gemini-3.1-flash-lite",
            api_key = env.GOOGLE_API_KEY,
            request_body = baml.json.parse(`{"generationConfig":{"thinkingConfig":{"thinkingLevel":"minimal"}}}`),
        ),
    )
}

function check_VertexGeminiFlashLiteLow() -> void {
    check_hello_world(
        google.VertexClient.new(
            model = "gemini-3.5-flash-lite",
            location = "global",
            temperature = 0.0,
            request_body = baml.json.parse(`{"generationConfig":{"thinkingConfig":{"thinkingLevel":"low"}}}`),
        ),
    )
}

function check_VertexLlama4ScoutRouter() -> void {
    let token = baml.env.get("VERTEX_ACCESS_TOKEN")
        ?? google.internal.gcp_access_token(
            baml.env.get("GOOGLE_APPLICATION_CREDENTIALS_CONTENT"),
            "https://www.googleapis.com/auth/cloud-platform",
        );
    check_hello_world(
        openai.GenericClient.new(
            model = "meta/llama-4-scout-17b-16e-instruct-maas",
            base_url = env.VERTEX_LLAMA_OPENAI_BASE_URL,
            api_key = token,
            temperature = 0.0,
            // Vertex's Llama MaaS endpoint rejects requests without max_tokens.
            max_tokens = 256,
        ),
    )
}

testset "live" with testing.Sequential() {
    test "BedrockHaiku45" {
        check_BedrockHaiku45()
    }
    test "BedrockHaiku45Cache" {
        check_BedrockHaiku45Cache()
    }
    test "AnthropicHaiku45Cache" {
        check_AnthropicHaiku45Cache()
    }
    test "OpenAIChatGpt56Cache" {
        check_OpenAIChatGpt56Cache()
    }
    test "OpenAIResponsesGpt56Cache" {
        check_OpenAIResponsesGpt56Cache()
    }
    test "AzureChatGpt56Cache" {
        check_AzureChatGpt56Cache()
    }
    test "AzureChatGpt56CacheKey" {
        check_AzureChatGpt56CacheKey()
    }
    test "AzureChatGpt5MiniCache" {
        check_AzureChatGpt5MiniCache()
    }
    test "BedrockGptOss20b" {
        check_BedrockGptOss20b()
    }
    test "AzureGpt5MiniLow" {
        check_AzureGpt5MiniLow()
    }
    test "Gemini31FlashLiteMinimal" {
        check_Gemini31FlashLiteMinimal()
    }
    test "VertexGeminiFlashLiteLow" {
        check_VertexGeminiFlashLiteLow()
    }
    test "VertexLlama4ScoutRouter" {
        check_VertexLlama4ScoutRouter()
    }
}
