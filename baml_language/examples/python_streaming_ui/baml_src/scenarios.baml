// The six (provider, v1 client) scenarios from 02-stream-test-matrix.md, one representative each.
// Constructors mirror sdk_tests/fixtures/llm_providers (PR #5041); keep them in sync.

// 1 = Bedrock, 2 = Bedrock Mantle, 3 = Azure AI Foundry, 4 = Google AI Studio, 5 = Vertex AI, 6 = Vertex Model Garden.
function scenario_client(scenario: int) -> ai.Client {
    match (scenario) {
        // BedrockHaiku45. Key pair and region are plain strings, so read at call time.
        1 => {
            aws.BedrockClient.new(
                model = "global.anthropic.claude-haiku-4-5-20251001-v1:0",
                region = baml.env.get_or_panic("BEDROCK_AWS_REGION"),
                access_key_id = baml.env.get_or_panic("BEDROCK_AWS_ACCESS_KEY_ID"),
                secret_access_key = baml.env.get_or_panic("BEDROCK_AWS_SECRET_ACCESS_KEY"),
                max_tokens = 64000,
                temperature = 0.0,
            )
        },
        // BedrockGptOss20b. gpt-oss is served under `/v1`, not BEDROCK_MANTLE_BASE_URL's `/openai/v1`.
        2 => {
            openai.ResponsesClient.new(
                model = "openai.gpt-oss-20b",
                base_url = "https://bedrock-mantle.us-east-1.api.aws/v1",
                api_key = env.BEDROCK_MANTLE_API_KEY,
                reasoning_effort = "low",
                store = false,
            )
        },
        // AzureGpt5MiniLow (stand-in until the user's models get quota).
        3 => azure_client(),
        // Gemini31FlashLiteMinimal.
        4 => {
            google.GeminiClient.new(
                model = "gemini-3.1-flash-lite",
                api_key = env.GOOGLE_API_KEY,
                request_body = gemini_thinking("minimal"),
            )
        },
        // VertexGeminiFlashLiteLow. Project comes from GOOGLE_APPLICATION_CREDENTIALS_CONTENT.
        5 => {
            google.VertexClient.new(
                model = "gemini-3.5-flash-lite",
                location = "global",
                temperature = 0.0,
                request_body = gemini_thinking("low"),
            )
        },
        // VertexLlama4ScoutRouter. The endpoint needs max_tokens; 8192 is its ceiling.
        6 => {
            openai.GenericClient.new(
                model = "meta/llama-4-scout-17b-16e-instruct-maas",
                base_url = env.VERTEX_LLAMA_OPENAI_BASE_URL,
                api_key = vertex_access_token(),
                temperature = 0.0,
                max_tokens = 8192,
            )
        },
        _ => {
            throw baml.errors.InvalidArgument { message: `unknown scenario: ${scenario}` }
        },
    }
}

function azure_client() -> ai.Client {
    let key = baml.env.get_or_panic("AZURE_OPENAI_API_KEY");
    openai.ResponsesClient.new(
        model = "gpt-5-mini",
        base_url = env.AZURE_OPENAI_RESPONSES_BASE_URL,
        api_key = key,
        headers = { "api-key": key },
        reasoning_effort = "low",
        store = false,
    )
}

function gemini_thinking(level: string) -> baml.json.json {
    baml.json.parse(`{"generationConfig":{"thinkingConfig":{"thinkingLevel":"${level}"}}}`)
}

// Access tokens expire within the hour, so mint one per call unless one is supplied.
function vertex_access_token() -> string {
    baml.env.get("VERTEX_ACCESS_TOKEN")
        ?? google.internal.gcp_access_token(
            baml.env.get("GOOGLE_APPLICATION_CREDENTIALS_CONTENT"),
            "https://www.googleapis.com/auth/cloud-platform",
        )
}
