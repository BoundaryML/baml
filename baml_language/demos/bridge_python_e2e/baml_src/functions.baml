// Stub client — `api_key "sk-test"` keeps the demo offline. The
// `__build_request` companion only constructs the HTTP request body;
// it never actually sends it, so a fake key is fine.
client<llm> StubClient {
  provider openai
  options {
    model "gpt-4o"
    api_key "sk-test"
  }
}

// LLM function. The compiler auto-synthesizes the four companions
// (`$build_request`, `$render_prompt`, `$parse`, `$parse_stream`); the
// demo exercises `$build_request` to prove the modular API path runs
// end-to-end without an LLM round-trip.
function ExtractResume(text: string) -> Resume {
  client StubClient
  prompt #"
    Extract a resume from the following text.

    Respond in JSON with this exact schema:
    {"name": "string", "email": "string or null", ...}

    Text:
    {{ text }}

    JSON:
  "#
}
