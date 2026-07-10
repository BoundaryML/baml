// 28 — provider diversity is typed provider values behind one marker interface.
function s28_resolve(spec: string) -> root.ai.Provider
    throws baml.errors.Io | baml.errors.ParseError {
  if (spec.starts_with("anthropic/")) {
    return root.ai.Anthropic {
      model: spec.substring(10, spec.length()), api_key: baml.env.get("ANTHROPIC_API_KEY") ?? "",
      base_url: null, max_tokens: null, anthropic_version: null,
      extra_headers: null, extra_body: null, append_output_schema: null,
    };
  }
  if (spec.starts_with("gemini/")) {
    return root.ai.Gemini {
      model: spec.substring(7, spec.length()), api_key: baml.env.get("GOOGLE_API_KEY") ?? "",
      base_url: null, extra_headers: null, extra_body: null, append_output_schema: null,
    };
  }
  root.ai.OpenAi {
    model: if (spec.starts_with("openai/")) { spec.substring(7, spec.length()) } else { spec },
    api_key: baml.env.get("OPENAI_API_KEY") ?? "", base_url: null,
  }
}
