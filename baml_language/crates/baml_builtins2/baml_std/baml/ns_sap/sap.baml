//! Public Schema-Aligned Parsing (SAP) exposes the same coercion used by BAML's
//! built-in LLM path to BAML-authored providers. The cache and no-yield marker
//! below are parser implementation details shared by ordinary SAP and
//! `ai.stream.Stream`; they are not prompt or transport concepts.

/// (internal) Cached type and assertion data used by incremental schema-aligned
/// parsing.
class _ParseCache<T> {
    _data: $rust_type,

    //baml:mut_vm
    /// (internal) Final SAP operation shared by streams and public SAP.
    function _parse_final(self, json: string) -> T throws root.errors.LlmClient {
        $rust_function
    }

    //baml:mut_vm
    /// (internal) Partial SAP operation used by `ai.stream.Stream`.
    function _parse_partial(self, json: string) -> T | _NoYield throws root.errors.LlmClient {
        $rust_function
    }
}

/// (internal) sentinel returned when an incremental parse needs more text.
class _NoYield {
}

//baml:mut_vm
//baml:fallible
/// (internal) Builds a parse cache for `T`.
///
/// A free function rather than a `_ParseCache` static method: a *static*
/// method's class type args have no receiver to ride in on, and the call path
/// derives them from the receiver's type — so a static method would be given
/// operand slots the call site never fills. Instance methods on a generic
/// class (`_parse_final`, `_parse_partial`) are threaded correctly.
function _new_parse_cache<T>() -> _ParseCache<T> throws never {
    $rust_function
}

/// Parse an LLM's raw text output into `T` with Schema-Aligned Parsing.
///
/// # Throws
/// - `baml.errors.ParseError` if parsing fails.
function parse<T>(text: string) -> T throws root.errors.ParseError {
    let cache = _new_parse_cache<T>();
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

/// Coerce an already-parsed JSON value into `T`: Schema-Aligned Parsing
/// without its text stage.
///
/// `parse` first finds and repairs the JSON in a model's text, then aligns it
/// to `T`. A value that arrives parsed, such as the arguments of a tool call,
/// needs only the second step. The alignment is the same: fields and variants
/// are read under the names the output format shows, and a `@skip` field is
/// not read.
///
/// The type is often data here (`type T = unreflect(t)`), so a `T` that the
/// parser cannot model is thrown, where `parse` panics: `unknown`,
/// `uint8array`, a function, a type that holds one of those, and a class with
/// a `@skip` field whose type has no value to fill in.
///
/// # Throws
/// - `baml.errors.ParseError` if `value` does not align to `T`, or if the
///   parser cannot model `T`.
//baml:mut_vm
function coerce<T>(value: root.json.json) -> T throws root.errors.ParseError {
    $rust_function
}
