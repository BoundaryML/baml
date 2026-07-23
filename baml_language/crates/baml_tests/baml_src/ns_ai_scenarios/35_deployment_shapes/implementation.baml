// 35 — deployment changes provider construction, never the task body.
function s35_server_client() -> root.ai.Provider throws baml.errors.Io | baml.errors.ParseError {
  root.ai.OpenAiCompatible {
    model: "gpt-5.6-luna", base_url: "https://api.openai.com/v1",
    auth: root.ai.BearerAuth { token: baml.env.get("OPENAI_API_KEY") ?? "" },
    extra_headers: null, extra_body: null, append_output_schema: null,
  }
}

function s35_proxy_client() -> root.ai.Provider throws baml.errors.Io | baml.errors.ParseError {
  root.ai.OpenAiCompatible {
    model: "gpt-5.6-luna", base_url: baml.env.get("LLM_GATEWAY_URL") ?? "https://gateway.internal/v1",
    auth: root.ai.HeaderAuth { name: "x-gateway-key", value: baml.env.get("LLM_GATEWAY_TOKEN") ?? "" },
    extra_headers: null, extra_body: null, append_output_schema: null,
  }
}

function s35_local_client() -> root.ai.Provider throws never {
  root.ai.OpenAiCompatible {
    model: "local-model", base_url: "http://localhost:11434/v1",
    auth: root.ai.NoAuth {}, extra_headers: null, extra_body: null, append_output_schema: null,
  }
}
