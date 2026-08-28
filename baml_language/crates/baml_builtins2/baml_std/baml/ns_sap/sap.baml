// Public Schema-Aligned Parsing (SAP) exposes the same coercion used by BAML's
// built-in LLM path to BAML-authored providers. The cache and no-yield marker
// below are parser implementation details shared by ordinary SAP and
// `ai.stream.Stream`; they are not prompt or transport concepts.

/// Cached type and assertion data used by incremental schema-aligned parsing.
class ParseCache<TStream, TFinal> {
    _data: $rust_type,

    /// Internal constructor. Callers normally use `__new_parse_cache` so the
    /// type arguments determine both runtime type descriptors.
    ///
    /// Panics if schema-aligned parsing cannot model `t_stream`/`t_final`: the
    /// type arguments come from the caller's own `parse<T>`, so an unmodelable
    /// `T` is a program bug rather than a recoverable condition.
    function new(
        t_stream: reflect.Type,
        t_final: reflect.Type,
    ) -> ParseCache<TStream, TFinal> throws never {
        $rust_io_function
    }
}

/// Internal sentinel returned when an incremental parse needs more text.
class NoYield {
}

/// Internal typed cache constructor shared by `ai.stream` and public SAP.
function __new_parse_cache<TStream, TFinal>() -> ParseCache<TStream, TFinal> throws never {
    let cache: ParseCache<TStream, TFinal> = ParseCache.new(reflect.Type.of<TStream>(), reflect.Type.of<TFinal>());
    cache
}

/// Internal final SAP operation shared by streams and public SAP.
function __parse_final<TStream, TFinal>(
    json: string,
    cache: ParseCache<TStream, TFinal>,
) -> TFinal throws root.errors.LlmClient {
    $rust_io_function
}

/// Internal partial SAP operation used by `ai.stream.Stream`.
function __parse_partial<TStream, TFinal>(
    json: string,
    cache: ParseCache<TStream, TFinal>,
) -> TStream | NoYield throws root.errors.LlmClient {
    $rust_io_function
}

/// Parse an LLM's raw text output into `T` with Schema-Aligned Parsing.
function parse<T>(text: string) -> T throws root.errors.LlmClient {
    let cache = __new_parse_cache<T, T>();
    __parse_final<T, T>(text, cache)
}

/// Parse against a runtime `type` value when the schema is data rather than a
/// lexical type parameter. Narrow the returned `unknown` at the call site.
function parse_type(t: reflect.Type, text: string) -> unknown throws root.errors.LlmClient {
    let cache: ParseCache<unknown, unknown> = ParseCache.new(t, t);
    __parse_final<unknown, unknown>(text, cache)
}
