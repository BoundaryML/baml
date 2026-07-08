// Public Schema-Aligned Parsing (SAP) — the parse mechanism BAML uses to coerce an
// LLM's raw text output into a typed value `T`. Robust to markdown fences, minor JSON
// deviations, and plain text (for `T = string`, the text passes through). This is the
// public wrapper over the internal non-streaming SAP entrypoint (plan P5: "expose it,
// don't rebuild it"), so a BAML-authored provider can decode responses the same way
// the built-in orchestrator does.

/// Parse `text` (an LLM's raw output) into `T` via Schema-Aligned Parsing.
function parse<T>(text: string) -> T throws root.errors.LlmClient {
    root.llm
        .__sap_parse_final<T, T>(
            text,
            root.llm.StreamCache.new(reflect.type_of<T>(), reflect.type_of<T>()),
        )
}

/// D6: SAP-parse `text` against a RUNTIME `type` value instead of a lexical `T`
/// — for seams where the schema is data, not a type in scope (e.g. validating
/// `ToolCall.args` against a stored `Tool.params_type`). The parse is driven by
/// the `StreamCache`'s stored runtime types, so no generic threading is needed;
/// the result is `unknown` — narrow it with a `match` arm where a concrete type
/// IS in scope.
function parse_type(t: type, text: string) -> unknown throws root.errors.LlmClient {
    let cache: root.llm.StreamCache<unknown, unknown> = root.llm.StreamCache.new(t, t);
    root.llm.__sap_parse_final<unknown, unknown>(text, cache)
}
