// Inline shorthand client — exercises the `from_shorthand` rust path
// that synthesizes a `PrimitiveClient` from `"<provider>/<model>"`.
// Whether the api_key is sourced from the environment is verified by
// `test_extract_resume_build_request_includes_openai_api_key` in
// customizable/test_main.py.
function ExtractResume(text: string) -> Resume {
  client "openai/gpt-4o-mini"
  prompt #"
    Extract a resume from the following text.

    Respond in JSON with this exact schema:
    {"name": "string", "email": "string or null"}

    Text:
    {{ text }}

    JSON:
  "#
}

// Paired with `StreamingDoc` to exercise the always-different
// `$stream` companion branch (previously covered by the standalone
// `python_llm_functions` crate). Uses the `openai-responses` shorthand
// so `test_streaming_extract_build_request_includes_openai_api_key`
// pins that the env-var fallback applies to that provider too.
function StreamingExtract(text: string) -> StreamingDoc {
  client "openai-responses/gpt-4o-mini"
  prompt #"
    Extract a streaming doc from the following text.

    Respond in JSON with this exact schema:
    {"title": "string", "body": "string or null", "word_count": "integer"}

    Text:
    {{ text }}

    JSON:
  "#
}
