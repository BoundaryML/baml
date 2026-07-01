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
