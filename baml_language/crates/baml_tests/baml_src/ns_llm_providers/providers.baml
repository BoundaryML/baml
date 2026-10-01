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
