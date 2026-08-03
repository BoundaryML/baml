// Public Schema-Aligned Parsing (SAP) exposes the same coercion used by BAML's
// built-in LLM path to BAML-authored providers.

/// Parse an LLM's raw text output into `T` with Schema-Aligned Parsing.
function parse<T>(text: string) -> T throws root.errors.LlmClient {
    let cache = root.llm.__new_stream_cache<T, T>();
    root.llm.__sap_parse_final<T, T>(text, cache)
}

/// Parse against a runtime `type` value when the schema is data rather than a
/// lexical type parameter. Narrow the returned `unknown` at the call site.
function parse_type(t: type, text: string) -> unknown throws root.errors.LlmClient {
    let cache: root.llm.StreamCache<unknown, unknown> = root.llm.StreamCache.new(t, t);
    root.llm.__sap_parse_final<unknown, unknown>(text, cache)
}
