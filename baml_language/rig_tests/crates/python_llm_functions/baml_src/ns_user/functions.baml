// Stub LLM client; mirrors `python_example_09a`.
client<llm> Stub {
  provider openai
  options {
    model "gpt-4o-mini"
    api_key env.OPENAI_API_KEY
  }
}

// Two LLM functions. PPIR auto-generates the four companion
// functions (`$build_request`, `$render_prompt`, `$parse`,
// `$parse_stream`) on each, plus sync + async siblings — the rig
// test asserts the full set is present.
function ExtractResume(text: string) -> Resume {
  client Stub
  prompt #"
    Extract a resume from the following text.

    Respond in JSON with this exact schema:
    {"name": "string", "email": "string or null"}

    Text:
    {{ text }}

    JSON:
  "#
}

function StreamingExtract(text: string) -> StreamingDoc {
  client Stub
  prompt #"
    Extract a streaming doc from the following text.

    Respond in JSON with this exact schema:
    {"title": "string", "body": "string or null", "word_count": "integer"}

    Text:
    {{ text }}

    JSON:
  "#
}
