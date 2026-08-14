// Call-shape pin for the generic ai.stream constructor. The expected return
// type must infer both TStream and TFinal without explicit type arguments.

client TestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

function TestFunc(input: string) -> string {
    client TestClient
    prompt `hi ${input}`
}

function call_stream_inferred() -> ai.stream.Stream<string | null, string> {
    ai.stream.from_spec(TestFunc$spec("world"))
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// stream_llm_inferred_typeargs.baml:1:1 (comment) len=76 "// Call-shape pin for the generic ai.stream constructor. The expected return"
// stream_llm_inferred_typeargs.baml:2:1 (comment) len=75 "// type must infer both TStream and TFinal without explicit type arguments."
// stream_llm_inferred_typeargs.baml:4:1 (keyword) len=6 "client"
// stream_llm_inferred_typeargs.baml:4:19 (operator) len=1 "="
// stream_llm_inferred_typeargs.baml:4:21 (namespace) [defaultLibrary] len=6 "openai"
// stream_llm_inferred_typeargs.baml:4:27 (namespace) [defaultLibrary] len=1 "."
// stream_llm_inferred_typeargs.baml:4:28 (class) [defaultLibrary] len=15 "ResponsesClient"
// stream_llm_inferred_typeargs.baml:4:44 (method) len=3 "new"
// stream_llm_inferred_typeargs.baml:5:5 (parameter) len=5 "model"
// stream_llm_inferred_typeargs.baml:5:11 (operator) len=1 "="
// stream_llm_inferred_typeargs.baml:5:13 (string) len=13 "\"gpt-4o-mini\""
// stream_llm_inferred_typeargs.baml:6:5 (parameter) len=7 "api_key"
// stream_llm_inferred_typeargs.baml:6:13 (operator) len=1 "="
// stream_llm_inferred_typeargs.baml:6:15 (string) len=10 "\"test-key\""
// stream_llm_inferred_typeargs.baml:7:5 (parameter) len=8 "base_url"
// stream_llm_inferred_typeargs.baml:7:14 (operator) len=1 "="
// stream_llm_inferred_typeargs.baml:7:16 (string) len=23 "\"http://localhost:1234\""
// stream_llm_inferred_typeargs.baml:10:1 (keyword) len=8 "function"
// stream_llm_inferred_typeargs.baml:10:10 (function) [declaration] len=8 "TestFunc"
// stream_llm_inferred_typeargs.baml:10:19 (parameter) [declaration] len=5 "input"
// stream_llm_inferred_typeargs.baml:10:26 (type) [defaultLibrary] len=6 "string"
// stream_llm_inferred_typeargs.baml:10:37 (type) [defaultLibrary] len=6 "string"
// stream_llm_inferred_typeargs.baml:11:5 (property) len=6 "client"
// stream_llm_inferred_typeargs.baml:11:12 (variable) len=10 "TestClient"
// stream_llm_inferred_typeargs.baml:12:5 (property) [declaration] len=6 "prompt"
// stream_llm_inferred_typeargs.baml:12:12 (string) len=1 "`"
// stream_llm_inferred_typeargs.baml:12:13 (string) len=2 "hi"
// stream_llm_inferred_typeargs.baml:12:18 (parameter) len=5 "input"
// stream_llm_inferred_typeargs.baml:12:24 (string) len=1 "`"
// stream_llm_inferred_typeargs.baml:15:1 (keyword) len=8 "function"
// stream_llm_inferred_typeargs.baml:15:10 (function) [declaration] len=20 "call_stream_inferred"
// stream_llm_inferred_typeargs.baml:15:36 (namespace) [defaultLibrary] len=2 "ai"
// stream_llm_inferred_typeargs.baml:15:38 (namespace) [defaultLibrary] len=1 "."
// stream_llm_inferred_typeargs.baml:15:39 (namespace) [defaultLibrary] len=6 "stream"
// stream_llm_inferred_typeargs.baml:15:45 (namespace) [defaultLibrary] len=1 "."
// stream_llm_inferred_typeargs.baml:15:46 (class) [defaultLibrary] len=6 "Stream"
// stream_llm_inferred_typeargs.baml:15:52 (operator) len=1 "<"
// stream_llm_inferred_typeargs.baml:15:53 (type) [defaultLibrary] len=6 "string"
// stream_llm_inferred_typeargs.baml:15:60 (operator) len=1 "|"
// stream_llm_inferred_typeargs.baml:15:62 (type) [defaultLibrary] len=4 "null"
// stream_llm_inferred_typeargs.baml:15:68 (type) [defaultLibrary] len=6 "string"
// stream_llm_inferred_typeargs.baml:15:74 (operator) len=1 ">"
// stream_llm_inferred_typeargs.baml:16:5 (namespace) [defaultLibrary] len=2 "ai"
// stream_llm_inferred_typeargs.baml:16:7 (namespace) [defaultLibrary] len=1 "."
// stream_llm_inferred_typeargs.baml:16:8 (namespace) [defaultLibrary] len=6 "stream"
// stream_llm_inferred_typeargs.baml:16:14 (namespace) [defaultLibrary] len=1 "."
// stream_llm_inferred_typeargs.baml:16:15 (function) len=9 "from_spec"
// stream_llm_inferred_typeargs.baml:16:25 (function) len=13 "TestFunc$spec"
// stream_llm_inferred_typeargs.baml:16:39 (string) len=7 "\"world\""
