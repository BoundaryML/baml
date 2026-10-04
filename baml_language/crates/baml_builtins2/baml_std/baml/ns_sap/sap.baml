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

//baml:mut_vm
//baml:fallible
/// (internal) Whether schema-aligned parsing can model `T`.
function _can_parse<T>() -> bool throws never {
    $rust_function
}

/// Whether schema-aligned parsing can model the runtime type `t`.
///
/// `parse` and `parse_type` panic for a type they cannot model: `unknown`,
/// `uint8array`, a function, or a type that holds one of those. They also
/// panic when they build a class with a `@skip` field whose type has no empty
/// value to fill in (media, a function). Code that receives its types as data
/// asks here first: `false` covers both.
function can_parse(t: reflect.Type) -> bool throws never {
    type T = unreflect(t);
    _can_parse<T>()
}

/// Parse an LLM's raw text output into `T` with Schema-Aligned Parsing.
///
/// A list in the text must hold only values of its element type. An item that
/// does not fit fails the parse, and the error names the item's position and
/// the reason: the result is never the list without that item. One value where
/// a list is expected is a list of that value when it fits, and fails the
/// parse when it does not. `null` and a missing field read as an empty list.
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
