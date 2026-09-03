//! Public Schema-Aligned Parsing (SAP) exposes the same coercion used by BAML's
//! built-in LLM path to BAML-authored providers. The cache and no-yield marker
//! below are parser implementation details shared by ordinary SAP and
//! `ai.stream.Stream`; they are not prompt or transport concepts.

/// (internal) Cached type and assertion data used by incremental schema-aligned
/// parsing.
class _ParseCache<TStream, TFinal> {
    _data: $rust_type,

    /// (internal) Final SAP operation shared by streams and public SAP.
    function _parse_final(self, json: string) -> TFinal throws root.errors.LlmClient {
        $rust_io_function
    }

    /// (internal) Partial SAP operation used by `ai.stream.Stream`.
    function _parse_partial(self, json: string) -> TStream | _NoYield throws root.errors.LlmClient {
        $rust_io_function
    }
}

/// (internal) sentinel returned when an incremental parse needs more text.
class _NoYield {
}

/// (internal) Builds a parse cache for `TStream`/`TFinal`.
///
/// A free function rather than a `_ParseCache` static method: a *static*
/// method's class type args have no receiver to ride in on, and the call path
/// derives them from the receiver's type — so a static method would be given
/// operand slots the call site never fills. Instance methods on a generic
/// class (`_parse_final`, `_parse_partial`) are threaded correctly.
function _new_parse_cache<TStream, TFinal>() -> _ParseCache<TStream, TFinal> throws never {
    $rust_io_function
}

/// Parse an LLM's raw text output into `T` with Schema-Aligned Parsing.
///
/// # Throws
/// - `baml.errors.ParseError` if parsing fails.
function parse<T>(text: string) -> T throws root.errors.ParseError {
    let cache = _new_parse_cache<T, T>();
    cache._parse_final(text) catch (_) {
        root.errors.LlmClient { message } => {
            throw root.errors.ParseError { message: message }
        },
    }
}

/// Parse against a runtime `type` value when the schema is data rather than a
/// lexical type parameter. Narrow the returned `unknown` at the call site.
///
/// It is generally recommended to use `baml.sap.parse` instead.
function parse_type(t: reflect.Type, text: string) -> unknown throws root.errors.ParseError {
    type T = unreflect(t);
    parse<T>(text)
}
