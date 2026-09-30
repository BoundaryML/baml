// Live provider matrix: the 28 leaf clients in a BAML v0 user's
// `baml_src/clients/`, rebuilt on the v1 stdlib clients, so one host test can
// exercise real inference against each of them. Their fallback chains and
// retry policies are composition over these and are not covered here.
// See customizable/test_llm_providers.py in the python_pydantic2 crate;
// nothing here makes a network call without live credentials.
//
// Clients are built inside `leaf_client` rather than declared as
// `client Name = ...`: `aws.BedrockClient` takes its static key pair and
// region as plain strings (not `env.NAME` refs), and header values are plain
// strings too, so both must be read at call time, which `$init` cannot do.
//
// v0 options with no v1 counterpart, dropped here:
//   * http.connect_timeout_ms / http.idle_timeout_ms (every client);
//   * http.request_timeout_ms / http.time_to_first_token_timeout_ms on
//     aws.BedrockClient and google.GeminiClient, which have no timeouts;
//   * retry_policy (v1 wraps a client in `ai.clients.Retry`; not exercised).

class Capital {
  city string
  country string
}

// The spec's own client is never used by the tests: every call passes an
// explicit client from `leaf_client`.
function AnswerCapital(country: string) -> Capital {
  client: "openai/gpt-4o-mini"
  prompt: `
    ${role("user")}
    What is the capital city of ${country}?

    ${ctx.output_format()}
  `
}

// One non-streaming call through `name`'s client.
function provider_answer(name: string, country: string) -> Capital {
  let cl = leaf_client(name);
  ai.Agent.new(client = cl).run(AnswerCapital@spec(country)).value
}

// One streamed call through `name`'s client, drained to its final value.
function provider_stream_answer(name: string, country: string) -> Capital {
  let cl = leaf_client(name);
  let streaming: ai.stream.StreamingClient = match (cl) {
    let s: ai.stream.StreamingClient => s,
    _ => {
      throw ai.errors.StreamingUnsupported { client_id: cl.id() }
    },
  };
  ai.stream.from_spec<Capital>(AnswerCapital@spec(country), client = streaming).final()
}

// The request a leaf client would send, rendered without sending it. Header
// values are left out so a preview never carries a credential; only the names
// are kept.
class RequestPreview {
  method string
  url string
  header_names string[]
  body string
}

function provider_request(name: string, country: string) -> RequestPreview {
  let req = AnswerCapital@spec(country).build_request(client = leaf_client(name));
  RequestPreview {
    method: req.method,
    url: req.url,
    header_names: req.headers.keys(),
    body: req.body,
  }
}

// --- AWS Bedrock (bedrock.baml): static IAM key pair ------------------------

function bedrock(
  model: string,
  max_tokens: int,
  temperature: float,
  extra: baml.json.json?,
) -> ai.Client {
  aws.BedrockClient.new(
    model = model,
    region = baml.env.get_or_panic("BEDROCK_AWS_REGION"),
    access_key_id = baml.env.get_or_panic("BEDROCK_AWS_ACCESS_KEY_ID"),
    secret_access_key = baml.env.get_or_panic("BEDROCK_AWS_SECRET_ACCESS_KEY"),
    max_tokens = max_tokens,
    temperature = temperature,
    additional_model_request_fields = extra,
  )
}

function claude_adaptive(effort: string) -> baml.json.json {
  baml.json.parse(`{"thinking":{"type":"adaptive"},"output_config":{"effort":"${effort}"}}`)
}

function claude_thinking(budget_tokens: int) -> baml.json.json {
  baml.json.parse(`{"thinking":{"type":"enabled","budget_tokens":${budget_tokens}}}`)
}

// --- Bedrock Mantle (bedrock_mantle.baml): Responses API, bearer key --------

function mantle(model: string, effort: string, slow: bool, cache: baml.json.json) -> ai.Client {
  openai.ResponsesClient.new(
    model = model,
    base_url = env.BEDROCK_MANTLE_BASE_URL,
    api_key = env.BEDROCK_MANTLE_API_KEY,
    reasoning_effort = effort,
    store = false,
    // v0 `prompt_cache_key` / `prompt_cache_options` have no typed field.
    request_body = cache,
    request_timeout_ms = if (slow) { 120000 } else { 10000 },
    time_to_first_token_timeout_ms = if (slow) { 30000 } else { 4000 },
  )
}

// --- Azure AI Foundry (azure_foundry.baml): key in an `api-key` header ------

function azure(model: string, effort: string, request_ms: int, ttft_ms: int) -> ai.Client {
  let key = baml.env.get_or_panic("AZURE_OPENAI_API_KEY");
  openai.ResponsesClient.new(
    model = model,
    base_url = env.AZURE_OPENAI_RESPONSES_BASE_URL,
    api_key = key,
    headers = { "api-key": key },
    reasoning_effort = effort,
    store = false,
    request_timeout_ms = request_ms,
    time_to_first_token_timeout_ms = ttft_ms,
  )
}

// --- Google AI Studio (google_ai.baml): API key ------------------------------

function gemini_thinking(level: string) -> baml.json.json {
  baml.json.parse(`{"generationConfig":{"thinkingConfig":{"thinkingLevel":"${level}"}}}`)
}

function gemini(model: string, level: string, temperature: float?) -> ai.Client {
  google.GeminiClient.new(
    model = model,
    api_key = env.GOOGLE_API_KEY,
    temperature = temperature,
    request_body = gemini_thinking(level),
  )
}

// --- Vertex (vertex.baml) ----------------------------------------------------

// The v0 app passes a pre-minted token in VERTEX_ACCESS_TOKEN. That token
// expires within the hour, so it cannot live in a secret store: when it is
// unset, mint one from the service account in
// GOOGLE_APPLICATION_CREDENTIALS_CONTENT (the same document Vertex Gemini
// authenticates with).
function vertex_access_token() -> string {
  baml.env.get("VERTEX_ACCESS_TOKEN") ?? google.internal.gcp_access_token(
    baml.env.get("GOOGLE_APPLICATION_CREDENTIALS_CONTENT"),
    "https://www.googleapis.com/auth/cloud-platform",
  )
}

function vertex_maas(model: string, base_url: baml.env.Ref) -> ai.Client {
  openai.GenericClient.new(
    model = model,
    base_url = base_url,
    api_key = vertex_access_token(),
    temperature = 0.0,
  )
}

// --- leaf clients --------------------------------------------------------------

// Every leaf client. Keep the names in sync with CLIENTS in
// test_llm_providers.py.
function leaf_client(name: string) -> ai.Client {
  // AWS Bedrock (11)
  if (name == "BedrockSonnet5") {
    return bedrock("global.anthropic.claude-sonnet-5", 30000, 1.0, claude_adaptive("medium"));
  }
  if (name == "BedrockSonnet5Low") {
    return bedrock("global.anthropic.claude-sonnet-5", 30000, 1.0, claude_adaptive("low"));
  }
  if (name == "BedrockGlm5") {
    return bedrock("zai.glm-5", 30000, 0.0, null);
  }
  if (name == "BedrockOpus5") {
    return bedrock("global.anthropic.claude-opus-5", 4096, 1.0, claude_adaptive("medium"));
  }
  if (name == "BedrockSonnet46") {
    return bedrock("global.anthropic.claude-sonnet-4-6", 30000, 1.0, claude_adaptive("medium"));
  }
  if (name == "BedrockSonnet46Low") {
    return bedrock("global.anthropic.claude-sonnet-4-6", 30000, 1.0, claude_adaptive("low"));
  }
  if (name == "BedrockHaiku45") {
    return bedrock("global.anthropic.claude-haiku-4-5-20251001-v1:0", 64000, 0.0, null);
  }
  if (name == "BedrockNovaMicro") {
    return bedrock("us.amazon.nova-micro-v1:0", 256, 0.0, null);
  }
  if (name == "BedrockNova2Lite") {
    return bedrock(
      "us.amazon.nova-2-lite-v1:0",
      4096,
      0.0,
      baml.json.parse(`{"reasoningConfig":{"type":"disabled"}}`),
    );
  }
  if (name == "BedrockHaiku45Thinking1024") {
    return bedrock("global.anthropic.claude-haiku-4-5-20251001-v1:0", 64000, 1.0, claude_thinking(1024));
  }
  if (name == "BedrockHaiku45Thinking4096") {
    return bedrock("global.anthropic.claude-haiku-4-5-20251001-v1:0", 64000, 1.0, claude_thinking(4096));
  }

  // Bedrock Mantle (3)
  if (name == "BedrockGpt55Medium") {
    // Mantle rejects prompt_cache_options on GPT-5.5.
    return mantle(
      "openai.gpt-5.5",
      "medium",
      true,
      baml.json.parse(`{"prompt_cache_key":"hybrd-agent-loop-bedrock-gpt55-v1"}`),
    );
  }
  if (name == "BedrockGpt56LunaLow") {
    return mantle(
      "openai.gpt-5.6-luna",
      "low",
      false,
      baml.json.parse(`{"prompt_cache_options":{"mode":"explicit"}}`),
    );
  }
  if (name == "BedrockGpt56TerraMedium") {
    return mantle(
      "openai.gpt-5.6-terra",
      "medium",
      true,
      baml.json.parse(
        `{"prompt_cache_key":"hybrd-agent-loop-bedrock-v1","prompt_cache_options":{"mode":"implicit"}}`,
      ),
    );
  }

  // Azure AI Foundry (5)
  if (name == "AzureGpt55Medium") {
    return azure("gpt-5.5", "medium", 120000, 30000);
  }
  if (name == "AzureGpt56LunaLow") {
    return azure("gpt-5.6-luna", "low", 10000, 4000);
  }
  if (name == "AzureGpt56LunaMedium") {
    return azure("gpt-5.6-luna", "medium", 20000, 10000);
  }
  if (name == "AzureGpt56TerraMedium") {
    return azure("gpt-5.6-terra", "medium", 120000, 30000);
  }
  if (name == "AzureGpt6LunaLow") {
    return azure("gpt-6-luna", "low", 10000, 4000);
  }

  // Google AI Studio (6)
  if (name == "Gemini31FlashLiteMinimal") {
    return gemini("gemini-3.1-flash-lite", "minimal", null);
  }
  if (name == "Gemini31FlashLiteLow") {
    return gemini("gemini-3.1-flash-lite", "low", null);
  }
  if (name == "Gemini35FlashLiteMinimal") {
    return gemini("gemini-3.5-flash-lite", "minimal", 0.0);
  }
  if (name == "Gemini35FlashLiteLow") {
    return gemini("gemini-3.5-flash-lite", "low", 0.0);
  }
  if (name == "Gemini35FlashLiteMedium") {
    return gemini("gemini-3.5-flash-lite", "medium", 0.0);
  }
  if (name == "Gemini31ProMedium") {
    return gemini("gemini-3.1-pro-preview", "medium", null);
  }

  // Vertex (3)
  if (name == "VertexGeminiFlashLiteLow") {
    // v0 pins project_id "gen-lang-client-0352598317" and authenticates by
    // shelling out to `gcloud auth print-access-token`. Here the project comes
    // from the test service account in GOOGLE_APPLICATION_CREDENTIALS_CONTENT,
    // which v1 reads before the ADC chain.
    return google.VertexClient.new(
      model = "gemini-3.5-flash-lite",
      location = "global",
      temperature = 0.0,
      request_body = gemini_thinking("low"),
    );
  }
  if (name == "VertexLlama4ScoutRouter") {
    return vertex_maas("meta/llama-4-scout-17b-16e-instruct-maas", env.VERTEX_LLAMA_OPENAI_BASE_URL);
  }
  if (name == "VertexQwen3NextRouter") {
    return vertex_maas("qwen/qwen3-next-80b-a3b-instruct-maas", env.VERTEX_QWEN_OPENAI_BASE_URL);
  }

  throw baml.errors.InvalidArgument { message: `unknown provider client: ${name}` }
}
