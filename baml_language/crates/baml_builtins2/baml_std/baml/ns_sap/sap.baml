//! Public Schema-Aligned Parsing (SAP) exposes the same coercion used by BAML's
//! built-in LLM path to BAML-authored providers. The cache and no-yield marker
//! below are parser implementation details shared by ordinary SAP and
//! `ai.stream.Stream`; they are not prompt or transport concepts.

/// (internal) Cached type and assertion data used by incremental schema-aligned
/// parsing.
class _ParseCache<T> {
    _data: $rust_type,

    //baml:mut_vm
    //baml:may_yield
    /// (internal) Final SAP operation shared by streams and public SAP.
    function _parse_final(self, json: string) -> T throws root.errors.LlmClient {
        $rust_function
    }

    //baml:mut_vm
    //baml:may_yield
    /// (internal) Partial SAP operation used by `ai.stream.Stream`.
    function _parse_partial(self, json: string) -> T | _NoYield throws root.errors.LlmClient {
        $rust_function
    }
}

/// (internal) sentinel returned when an incremental parse needs more text.
class _NoYield {
}

/// (internal) A shape value that `from_llm_output` refused, with the message
/// it threw. The parser then reads the text as if the type could not hold
/// that value: a union tries its next member.
class _Rejected {
    message: string,

    /// (internal) Builds a `T` from the shape value the parser read, or says
    /// why `T` refused it. A panic in `from_llm_output` is a bug, not a
    /// refusal, so it propagates.
    function _build<T extends root.FromLlmOutput>(shape: T.Shape) -> T | _Rejected throws never {
        T.from_llm_output(shape) catch (e) {
            root.json.DecodeError => _Rejected { message: e.message },
        }
    }
}

//baml:mut_vm
//baml:fallible
/// (internal) Builds a parse cache for `T`, or throws `ParseError` for a `T`
/// the parser cannot model.
///
/// A free function rather than a `_ParseCache` static method: a *static*
/// method's class type args have no receiver to ride in on, and the call path
/// derives them from the receiver's type — so a static method would be given
/// operand slots the call site never fills. Instance methods on a generic
/// class (`_parse_final`, `_parse_partial`) are threaded correctly.
function _new_parse_cache<T>() -> _ParseCache<T> throws root.errors.ParseError {
    $rust_function
}

/// Parse an LLM's raw text output into `T` with Schema-Aligned Parsing.
///
/// A union member with no JSON form or no values is never the result:
/// `parse<uint8array | string>` reads a string.
///
/// A type that implements `baml.FromLlmOutput` is read as its `Shape`, and
/// its `from_llm_output` builds it from the value read. When
/// `from_llm_output` throws, the value does not fit that type: a union tries
/// its next member.
///
/// # Throws
/// - `baml.errors.ParseError` if parsing fails, if `from_llm_output` throws
///   for a value no other member of a union takes (the message carries what
///   it threw), or if the parser cannot model `T`: `unknown`, or a type with
///   no JSON form or no values, such as `uint8array`, `never`, or a class
///   that requires one.
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
    let type T = t;
    parse<T>(text)
}

/// Coerce an already-parsed JSON value into `T`: Schema-Aligned Parsing
/// without its text stage.
///
/// `parse` first finds and repairs the JSON in a model's text, then aligns it
/// to `T`. A value that arrives parsed, such as the arguments of a tool call,
/// needs only the second step. The alignment is the same: fields and variants
/// are read under the names the output format shows, a `@skip` field is not
/// read, and a type that implements `baml.FromLlmOutput` is read as its
/// `Shape` and built by its `from_llm_output`.
///
/// A union member with no JSON form or no values is dropped:
/// `coerce<uint8array | string>` reads a string, and an optional field of
/// such a type is only `null`.
///
/// # Throws
/// - `baml.errors.ParseError` if `value` does not align to `T`, if
///   `from_llm_output` throws for a value no other member of a union takes
///   (the message carries what it threw), or if the parser cannot model `T`:
///   `unknown`, a type with no JSON form or no values, such as `uint8array`,
///   `never` or a function, a type that requires one of those, and a class
///   with a `@skip` field whose type has no value to fill in.
//baml:mut_vm
//baml:may_yield
function coerce<T>(value: root.json.json) -> T throws root.errors.ParseError {
    $rust_function
}
