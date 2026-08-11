// TODO(stream-migration): this fixture pinned the call shape of the removed
// `baml.prompt.stream_llm_function<TStream, TFinal>` machinery (inference-reliant
// call sites without explicit type args). The `$stream`/`$parse_stream`
// companions and `stream_llm_function` were removed with the legacy LLM path.
// When streaming returns, restore the inferred-typeargs call-shape pin from
// git history. Until then this fixture keeps a minimal non-streaming LLM
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

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// stream_llm_inferred_typeargs.baml:1:1 (comment) len=76 "// TODO(stream-migration): this fixture pinned the call shape of the removed"
// stream_llm_inferred_typeargs.baml:2:1 (comment) len=82 "// `baml.prompt.stream_llm_function<TStream, TFinal>` machinery (inference-reliant"
// stream_llm_inferred_typeargs.baml:3:1 (comment) len=72 "// call sites without explicit type args). The `$stream`/`$parse_stream`"
// stream_llm_inferred_typeargs.baml:4:1 (comment) len=78 "// companions and `stream_llm_function` were removed with the legacy LLM path."
// stream_llm_inferred_typeargs.baml:5:1 (comment) len=76 "// When streaming returns, restore the inferred-typeargs call-shape pin from"
// stream_llm_inferred_typeargs.baml:6:1 (comment) len=73 "// git history. Until then this fixture keeps a minimal non-streaming LLM"
// stream_llm_inferred_typeargs.baml:7:1 (comment) len=41 "// function so it still compiles cleanly."
// stream_llm_inferred_typeargs.baml:9:1 (keyword) len=6 "client"
// stream_llm_inferred_typeargs.baml:9:19 (operator) len=1 "="
// stream_llm_inferred_typeargs.baml:9:21 (namespace) [defaultLibrary] len=6 "openai"
// stream_llm_inferred_typeargs.baml:9:27 (namespace) [defaultLibrary] len=1 "."
// stream_llm_inferred_typeargs.baml:9:28 (class) [defaultLibrary] len=12 "OpenAiClient"
// stream_llm_inferred_typeargs.baml:9:41 (method) len=3 "new"
// stream_llm_inferred_typeargs.baml:10:5 (parameter) len=5 "model"
// stream_llm_inferred_typeargs.baml:10:11 (operator) len=1 "="
// stream_llm_inferred_typeargs.baml:10:13 (string) len=13 "\"gpt-4o-mini\""
// stream_llm_inferred_typeargs.baml:11:5 (parameter) len=7 "api_key"
// stream_llm_inferred_typeargs.baml:11:13 (operator) len=1 "="
// stream_llm_inferred_typeargs.baml:11:15 (string) len=10 "\"test-key\""
// stream_llm_inferred_typeargs.baml:12:5 (parameter) len=8 "base_url"
// stream_llm_inferred_typeargs.baml:12:14 (operator) len=1 "="
// stream_llm_inferred_typeargs.baml:12:16 (string) len=23 "\"http://localhost:1234\""
// stream_llm_inferred_typeargs.baml:15:1 (keyword) len=8 "function"
// stream_llm_inferred_typeargs.baml:15:10 (function) [declaration] len=8 "TestFunc"
// stream_llm_inferred_typeargs.baml:15:19 (parameter) [declaration] len=5 "input"
// stream_llm_inferred_typeargs.baml:15:26 (type) [defaultLibrary] len=6 "string"
// stream_llm_inferred_typeargs.baml:15:37 (type) [defaultLibrary] len=6 "string"
// stream_llm_inferred_typeargs.baml:16:5 (property) len=6 "client"
// stream_llm_inferred_typeargs.baml:16:12 (variable) len=10 "TestClient"
// stream_llm_inferred_typeargs.baml:17:5 (property) [declaration] len=6 "prompt"
// stream_llm_inferred_typeargs.baml:17:12 (string) len=1 "`"
// stream_llm_inferred_typeargs.baml:17:13 (string) len=2 "hi"
// stream_llm_inferred_typeargs.baml:17:18 (parameter) len=5 "input"
// stream_llm_inferred_typeargs.baml:17:24 (string) len=1 "`"
