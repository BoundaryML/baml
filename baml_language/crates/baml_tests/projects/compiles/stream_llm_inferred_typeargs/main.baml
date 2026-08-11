// TODO(stream-migration): this project pinned the call shape of the removed
// `baml.prompt.stream_llm_function<TStream, TFinal>` machinery (inference-reliant
// call sites without explicit type args). The `$stream`/`$parse_stream`
// companions and `stream_llm_function` were removed with the legacy LLM path.
// When streaming returns, restore the inferred-typeargs call-shape pin from
// git history. Until then this project keeps a minimal non-streaming LLM
// function so it still compiles cleanly.

client TestClient = openai.OpenAiClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

function TestFunc(input: string) -> string {
    client TestClient
    prompt `hi ${input}`
}
