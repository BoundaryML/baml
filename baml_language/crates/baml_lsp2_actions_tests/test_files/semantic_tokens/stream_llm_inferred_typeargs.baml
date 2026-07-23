// Call-shape pin.
//
// `stream_llm_function<TStream, TFinal>` is generic, and direct user-code
// call sites may rely on type inference (via the let-binding / return-type
// annotation) to fill `<TStream, TFinal>` rather than spelling them
// explicitly. This project locks in that *call shape* — the stdlib must
// continue to accept `stream_llm_function(client, name, args)` without
// explicit type args.
//
// Note: the PPIR-synthesized `$stream`/`$parse_stream`/`$parse` companions
// do NOT rely on inference — they pass `<STREAM_EXPANDED, ORIGINAL>` as
// explicit type args (see `ppir_expansion_items`). Inference-reliant calls
// like the one below work because TIR persists inferred instantiations in
// `CallPlan.type_args` (including phase-0 reverse inference from the
// expected return type) and MIR materializes them into the callee frame,
// where `__make_stream` reifies them via `reflect.type_of<TStream/TFinal>()`.
// There is no name-keyed registry fallback anymore — a propagation gap
// would surface as "Non-parsable type: ..." from `StreamCache.new`.
// Runtime coverage: `streaming_parsing::stream_string_final_value` and
// `streaming_parsing::stream_class_in_namespace_final_value`.

client<llm> TestClient {
    provider openai
    options {
        model "gpt-4o-mini"
        api_key "test-key"
        base_url "http://localhost:1234"
    }
}

function TestFunc(input: string) -> string {
    client TestClient
    prompt #"hi {{ input }}"#
}

function call_stream_inferred() -> baml.llm.Stream<null | string, string> {
    baml.llm.stream_llm_function(TestClient, "TestFunc", { "input": "world" })
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// stream_llm_inferred_typeargs.baml:1:1 (comment) len=18 "// Call-shape pin."
// stream_llm_inferred_typeargs.baml:2:1 (comment) len=2 "//"
// stream_llm_inferred_typeargs.baml:3:1 (comment) len=74 "// `stream_llm_function<TStream, TFinal>` is generic, and direct user-code"
// stream_llm_inferred_typeargs.baml:4:1 (comment) len=75 "// call sites may rely on type inference (via the let-binding / return-type"
// stream_llm_inferred_typeargs.baml:5:1 (comment) len=68 "// annotation) to fill `<TStream, TFinal>` rather than spelling them"
// stream_llm_inferred_typeargs.baml:6:1 (comment) len=74 "// explicitly. This project locks in that *call shape* — the stdlib must"
// stream_llm_inferred_typeargs.baml:7:1 (comment) len=71 "// continue to accept `stream_llm_function(client, name, args)` without"
// stream_llm_inferred_typeargs.baml:8:1 (comment) len=22 "// explicit type args."
// stream_llm_inferred_typeargs.baml:9:1 (comment) len=2 "//"
// stream_llm_inferred_typeargs.baml:10:1 (comment) len=75 "// Note: the PPIR-synthesized `$stream`/`$parse_stream`/`$parse` companions"
// stream_llm_inferred_typeargs.baml:11:1 (comment) len=74 "// do NOT rely on inference — they pass `<STREAM_EXPANDED, ORIGINAL>` as"
// stream_llm_inferred_typeargs.baml:12:1 (comment) len=75 "// explicit type args (see `ppir_expansion_items`). Inference-reliant calls"
// stream_llm_inferred_typeargs.baml:13:1 (comment) len=74 "// like the one below work because TIR persists inferred instantiations in"
// stream_llm_inferred_typeargs.baml:14:1 (comment) len=69 "// `CallPlan.type_args` (including phase-0 reverse inference from the"
// stream_llm_inferred_typeargs.baml:15:1 (comment) len=73 "// expected return type) and MIR materializes them into the callee frame,"
// stream_llm_inferred_typeargs.baml:16:1 (comment) len=78 "// where `__make_stream` reifies them via `reflect.type_of<TStream/TFinal>()`."
// stream_llm_inferred_typeargs.baml:17:1 (comment) len=73 "// There is no name-keyed registry fallback anymore — a propagation gap"
// stream_llm_inferred_typeargs.baml:18:1 (comment) len=68 "// would surface as \"Non-parsable type: ...\" from `StreamCache.new`."
// stream_llm_inferred_typeargs.baml:19:1 (comment) len=71 "// Runtime coverage: `streaming_parsing::stream_string_final_value` and"
// stream_llm_inferred_typeargs.baml:20:1 (comment) len=62 "// `streaming_parsing::stream_class_in_namespace_final_value`."
// stream_llm_inferred_typeargs.baml:22:1 (keyword) len=6 "client"
// stream_llm_inferred_typeargs.baml:22:7 (operator) len=1 "<"
// stream_llm_inferred_typeargs.baml:22:8 (type) len=3 "llm"
// stream_llm_inferred_typeargs.baml:22:11 (operator) len=1 ">"
// stream_llm_inferred_typeargs.baml:22:13 (struct) [declaration] len=10 "TestClient"
// stream_llm_inferred_typeargs.baml:23:5 (property) len=8 "provider"
// stream_llm_inferred_typeargs.baml:24:5 (property) len=7 "options"
// stream_llm_inferred_typeargs.baml:25:9 (property) len=5 "model"
// stream_llm_inferred_typeargs.baml:25:15 (string) len=13 "\"gpt-4o-mini\""
// stream_llm_inferred_typeargs.baml:26:9 (property) len=7 "api_key"
// stream_llm_inferred_typeargs.baml:26:17 (string) len=10 "\"test-key\""
// stream_llm_inferred_typeargs.baml:27:9 (property) len=8 "base_url"
// stream_llm_inferred_typeargs.baml:27:18 (string) len=23 "\"http://localhost:1234\""
// stream_llm_inferred_typeargs.baml:31:1 (keyword) len=8 "function"
// stream_llm_inferred_typeargs.baml:31:10 (function) [declaration] len=8 "TestFunc"
// stream_llm_inferred_typeargs.baml:31:19 (parameter) [declaration] len=5 "input"
// stream_llm_inferred_typeargs.baml:31:26 (type) [defaultLibrary] len=6 "string"
// stream_llm_inferred_typeargs.baml:31:37 (type) [defaultLibrary] len=6 "string"
// stream_llm_inferred_typeargs.baml:32:5 (property) len=6 "client"
// stream_llm_inferred_typeargs.baml:33:5 (property) [declaration] len=6 "prompt"
// stream_llm_inferred_typeargs.baml:33:12 (string) len=1 "#"
// stream_llm_inferred_typeargs.baml:33:13 (string) len=1 "\""
// stream_llm_inferred_typeargs.baml:33:14 (string) len=2 "hi"
// stream_llm_inferred_typeargs.baml:33:17 (string) len=1 "{"
// stream_llm_inferred_typeargs.baml:33:18 (string) len=1 "{"
// stream_llm_inferred_typeargs.baml:33:20 (string) len=5 "input"
// stream_llm_inferred_typeargs.baml:33:26 (string) len=1 "}"
// stream_llm_inferred_typeargs.baml:33:27 (string) len=1 "}"
// stream_llm_inferred_typeargs.baml:33:28 (string) len=1 "\""
// stream_llm_inferred_typeargs.baml:33:29 (string) len=1 "#"
// stream_llm_inferred_typeargs.baml:36:1 (keyword) len=8 "function"
// stream_llm_inferred_typeargs.baml:36:10 (function) [declaration] len=20 "call_stream_inferred"
// stream_llm_inferred_typeargs.baml:36:36 (namespace) [defaultLibrary] len=4 "baml"
// stream_llm_inferred_typeargs.baml:36:40 (namespace) [defaultLibrary] len=1 "."
// stream_llm_inferred_typeargs.baml:36:41 (namespace) [defaultLibrary] len=3 "llm"
// stream_llm_inferred_typeargs.baml:36:44 (namespace) [defaultLibrary] len=1 "."
// stream_llm_inferred_typeargs.baml:36:45 (class) [defaultLibrary] len=6 "Stream"
// stream_llm_inferred_typeargs.baml:36:51 (operator) len=1 "<"
// stream_llm_inferred_typeargs.baml:36:52 (type) [defaultLibrary] len=4 "null"
// stream_llm_inferred_typeargs.baml:36:57 (operator) len=1 "|"
// stream_llm_inferred_typeargs.baml:36:59 (type) [defaultLibrary] len=6 "string"
// stream_llm_inferred_typeargs.baml:36:67 (type) [defaultLibrary] len=6 "string"
// stream_llm_inferred_typeargs.baml:36:73 (operator) len=1 ">"
// stream_llm_inferred_typeargs.baml:37:5 (namespace) [defaultLibrary] len=4 "baml"
// stream_llm_inferred_typeargs.baml:37:9 (namespace) [defaultLibrary] len=1 "."
// stream_llm_inferred_typeargs.baml:37:10 (namespace) [defaultLibrary] len=3 "llm"
// stream_llm_inferred_typeargs.baml:37:13 (namespace) [defaultLibrary] len=1 "."
// stream_llm_inferred_typeargs.baml:37:14 (function) len=19 "stream_llm_function"
// stream_llm_inferred_typeargs.baml:37:34 (variable) len=10 "TestClient"
// stream_llm_inferred_typeargs.baml:37:46 (string) len=10 "\"TestFunc\""
// stream_llm_inferred_typeargs.baml:37:60 (string) len=7 "\"input\""
// stream_llm_inferred_typeargs.baml:37:69 (string) len=7 "\"world\""
