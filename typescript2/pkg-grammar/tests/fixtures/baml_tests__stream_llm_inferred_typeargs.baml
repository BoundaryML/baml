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
