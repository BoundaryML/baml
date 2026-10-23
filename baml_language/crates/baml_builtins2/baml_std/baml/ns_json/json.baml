/// An untyped JSON value.
///
/// JSON's one number type splits into three arms here. `parse` reads an integral
/// number as `int` when it fits BAML's 63-bit `int`, as `bigint` otherwise, and
/// reads a number with a fraction or exponent as `float`. Decoding into a
/// declared `int` (`to<int>`, `decode<int>`) is strict and rejects values
/// outside the `int` range instead.
///
/// Reach an arm with `match`, or hand the value to `to<T>`.
type json = null | bool | int | bigint | float | string | json[] | map<string, json>;

/// Which names a class's fields and an enum's variants go by in JSON.
///
/// `decode`, `to`, `from`, `stringify`, and `schema` each take one, and agree
/// on every key when given the same one.
enum Naming {
    /// The declared names. `@alias` and `@skip` have no effect: every field
    /// and variant appears, under the name it is declared with. This is the
    /// default everywhere.
    Declared,
    /// The names a model is shown. A field or variant goes by its `@alias`
    /// when it has one, and a `@skip` field or variant does not appear. A
    /// `@skip` field read from JSON holds its type's empty value (`null`, `0`,
    /// `""`, `[]`, ...), as it does when `baml.sap` parses a model's answer.
    Aliased,
}

/// A JSON syntax error.
///
/// Only the functions that read text raise this: `parse` and `decode<T>`.
/// Everything that consumes an already-parsed `json` throws `DecodeError`
/// instead. `message` is the parser's own diagnostic,
/// locating the offending line and column.
class ParseError {
    message: string,
}

/// A parsed JSON value that does not fit the expected BAML type.
///
/// `path` locates the offending node from the decode root in jq-ish
/// notation: `.field` for a class field, `[0]` for an array element,
/// `["key"]` for a map entry; `""` is the root itself.
class DecodeError {
    message: string,
    path: string,
}

/// A BAML value with no JSON form.
///
/// `path` locates the offending node in the jq-ish notation `DecodeError`
/// uses. `reason` is a short stable tag for the construct that had no JSON
/// form — `uint8array`, `function`, `future`, `unserializable`, `class`,
/// `enum`, `media`, and so on. Branch on it rather than on `message`, which is
/// prose.
class SerializationError {
    message: string,
    path: string,
    reason: string,
}

//baml:mut_vm
/// Parses one JSON document into an untyped `json` value.
///
/// The whole string must be that document; trailing content is a syntax error,
/// not a stopping point. To land in a declared type instead, use `decode<T>`.
///
/// # Throws
///
/// - `baml.json.ParseError` if `s` is not valid JSON.
function parse(s: string) -> json throws ParseError {
    $rust_function
}

/// Parses JSON text and decodes it into `T`.
///
/// `decode<T>(s)` is `to<T>(parse(s))`: the text is parsed first, then `T`
/// drives the walk by the rules of `to<T>`, `baml.FromJson` overrides
/// included. The walk is a shape check rather than a coercion, so
/// `decode<int>("\"5\"")` fails: a JSON string does not become an `int`.
///
/// `naming` picks the names fields and variants go by; see `to<T>`.
///
/// # Throws
///
/// - `baml.json.ParseError` if `s` is not valid JSON.
/// - `baml.json.DecodeError` if the parsed JSON does not fit `T`, or if a
///   `baml.FromJson` override rejects it. `path` locates the offending node.
///
/// # Examples
///
/// ```baml
/// class User {
///     name: string,
///     age: int,
/// }
///
/// let user = baml.json.decode<User>(`{"name": "Ada", "age": 36}`);
/// assert.equal(user.age, 36);
/// ```
function decode<T>(
    s: string,
    naming: Naming = Naming.Declared,
) -> T throws ParseError | DecodeError {
    to<T>(parse(s), naming = naming)
}

// OpenAI structured outputs decode with this. `schema` describes a type by
// its fields and ignores `baml.FromJson` overrides, so the answer to that
// schema must be read the same way. B-1993 tracks giving overrides a schema.
/// (internal) `decode<T>` without `baml.FromJson` overrides: every class and
/// enum decodes by its fields and variants. Names go by `@alias` when
/// `aliased` holds.
//baml:mut_vm
function _decode_structural<T>(s: string, aliased: bool) -> T throws ParseError | DecodeError {
    $rust_function
}

/// Writes any BAML value as JSON text.
///
/// The value is converted the way `from` converts it, `baml.ToJson`
/// overrides included, and then written out. Not to be confused with
/// `string.from`, which writes BAML's own display form rather than JSON.
///
/// `naming` picks the names fields and variants go by, and `omit_nulls`
/// leaves out class fields that hold `null`; see `from`.
///
/// `indent` sets the layout. An `int` is a number of spaces per nesting level.
/// A `string` is written as-is for each level, for example `"\t"`. The default,
/// `0`, writes compact text with no whitespace at all, and so does `""`.
///
/// # Throws
///
/// - `baml.json.SerializationError` if any value in the graph has no JSON
///   form; see `from`.
/// - `baml.json.SerializationError` with `reason` `"indent"` if `indent` is a
///   negative number, a number above 10, a string longer than 10 characters,
///   or a string holding anything other than spaces, tabs, and line breaks.
///   JavaScript's `JSON.stringify` stops at the same width of 10.
///
/// # Examples
///
/// ```baml
/// let doc: json = { "name": "Ada", "tags": ["math"] };
/// assert.equal(baml.json.stringify(doc), `{"name":"Ada","tags":["math"]}`);
/// assert.equal(baml.json.stringify([1], indent = 2), "[\n  1\n]");
/// ```
function stringify(
    value: unknown,
    naming: Naming = Naming.Declared,
    indent: int | string = 0,
    omit_nulls: bool = false,
) -> string throws SerializationError {
    _stringify(value, naming == Naming.Aliased, indent, omit_nulls)
}

/// (internal) `stringify`, with `naming` as a flag: aliased names when
/// `aliased` holds.
//baml:mut_vm
//baml:may_yield
function _stringify(
    value: unknown,
    aliased: bool,
    indent: int | string,
    omit_nulls: bool,
) -> string throws SerializationError {
    $rust_function
}

/// Converts any BAML value to a `json` value.
///
/// The conversion follows the value's runtime type. Primitives, lists, and
/// maps take their natural JSON shape. A class instance becomes an object of
/// its fields, an enum value its variant's name, and media the tagged object
/// `{ kind, source, value, mime, name? }`. A `bigint` becomes an
/// arbitrary-precision JSON number, and a non-finite `float` becomes `null`.
///
/// A value whose runtime class implements `baml.ToJson` with its own
/// `to_json` is converted by that method instead, at any depth, and the
/// method owns its whole subtree: `naming` and `omit_nulls` do not reach
/// inside it. The method is found from the value's runtime class, so one
/// declared in any package is honored.
///
/// `naming` picks the names fields and variants go by. The default,
/// `Naming.Declared`, writes every field under its declared name.
/// `Naming.Aliased` writes a field or variant under its `@alias`, and leaves
/// out a `@skip` field.
///
/// `omit_nulls` leaves out a class field that holds `null`, so an unset
/// optional field writes `{"name": "Ada"}` rather than
/// `{"name": "Ada", "nickname": null}`. Nothing else is left out: a `null`
/// map value, a `null` list element, and a `null` nested in a `json` value
/// stay, because there `null` is data.
///
/// # Throws
///
/// - `baml.json.SerializationError` if any value in the graph has no JSON
///   form: a `uint8array` (encode it explicitly with `to_base64()` or
///   `to_hex()`), a function, a future, or any other opaque runtime object.
///   `reason` names which.
/// - `baml.json.SerializationError` if `naming` is `Naming.Aliased` and the
///   value holds a `@skip` enum variant, which then has no name.
function from(
    value: unknown,
    naming: Naming = Naming.Declared,
    omit_nulls: bool = false,
) -> json throws SerializationError {
    _from(value, naming == Naming.Aliased, omit_nulls)
}

/// (internal) `from`, with `naming` as a flag: aliased names when `aliased`
/// holds.
//baml:mut_vm
//baml:may_yield
function _from(value: unknown, aliased: bool, omit_nulls: bool) -> json throws SerializationError {
    $rust_function
}

/// Decodes a `json` value into `T`.
///
/// `T` drives the walk. Wherever the walk meets a class or enum — at the
/// root, as a field, under an optional, in a list, in a map — a type that
/// implements `baml.FromJson` with its own `from_json` is built by that
/// method, and anything else decodes structurally, field by field, its
/// children by the same rule. The method owns its whole subtree: `naming`
/// does not reach inside it. It is found from `T`, so one declared in any
/// package is honored.
///
/// The walk is a shape check rather than a coercion. A JSON string does not
/// become an `int`, and a number outside BAML's 63-bit `int` is rejected
/// rather than widened to `float`. A `bigint` decodes from an integral JSON
/// number, or from a JSON string holding an optional sign and ASCII decimal
/// digits; leading zeroes and negative zero are normalized, and fractions,
/// exponents, whitespace, and other bases are rejected.
///
/// A class field that can be `null` may be absent and decodes to `null`;
/// every other field must be present. Unrecognized keys are ignored. A union
/// takes the first member the value fits by shape, and that member then
/// decodes as above. A recursive alias decodes as its definition.
///
/// `T` matters here in a way it does not for `from`: decoding has only the
/// target type to go on, so a wider `T` changes the result, not just its
/// static type.
///
/// `naming` picks the names fields and variants go by. The default,
/// `Naming.Declared`, reads declared names, and `@alias` and `@skip` have no
/// effect. `Naming.Aliased` reads the names a model is shown, which
/// `schema(t, naming = Naming.Aliased)` describes: a member's `@alias`, and
/// no `@skip` member. A `@skip` field then holds its type's empty value
/// (`null`, `0`, `""`, `[]`, ...), and is an error when its type has none
/// (media, a function, a class that contains itself).
///
/// # Throws
///
/// - `baml.json.DecodeError` if `j` does not fit `T`, if a `baml.FromJson`
///   override rejects it, or if `j` contains itself. `path` locates the
///   offending node.
function to<T>(j: json, naming: Naming = Naming.Declared) -> T throws DecodeError {
    _to<T>(j, naming == Naming.Aliased)
}

/// (internal) `to<T>`, with `naming` as a flag: aliased names when `aliased`
/// holds.
//baml:mut_vm
//baml:may_yield
function _to<T>(j: json, aliased: bool) -> T throws DecodeError {
    $rust_function
}

/// Looks up `key` in a `json` value, or `null` if `j` is not a map or the key
/// is absent.
///
/// Never fails, which saves a hand-written `baml.FromJson` override the
/// trouble of match-narrowing the `json` union to its map arm before every
/// field read. The cost is that an absent key and a present `null` one look
/// the same — when you need that distinction, narrow and use `map.has`
/// yourself.
//baml:mut_vm
function field(j: json, key: string) -> json throws never {
    $rust_function
}

/// A `path` selector that is malformed, does not resolve, or selects a leaf
/// that will not coerce.
///
/// `selector` is the whole selector as written, not the step that failed;
/// `message` says which step that was. One error class covers both a bad
/// selector and a selector that does not fit the document, because `path` has
/// no useful partial result to hand back either way.
class PathError {
    message: string,
    selector: string,
}

/// (internal) One `.field` step of a `path` walk. Use `path`.
function _path_field(current: json, key: string, selector: string) -> json throws PathError {
    match (current) {
        let m: map<string, json> => {
            if (!m.has(key)) {
                throw PathError { message: "missing field '" + key + "'", selector: selector };
            }
            m.get(key)
        },
        _ => {
            throw PathError {
      message: "field access '." + key + "' on a non-object",
      selector: selector,
    }
        },
    }
}

/// (internal) One `[i]` step of a `path` walk. Use `path`.
function _path_index(current: json, idx: int, selector: string) -> json throws PathError {
    match (current) {
        let arr: json[] => {
            if (idx < 0 || idx >= arr.length()) {
                throw PathError {
          message: "index [" + idx.to_string() + "] out of bounds (length " + arr.length().to_string() + ")",
          selector: selector,
        };
            }
            arr.at(idx)
        },
        _ => {
            throw PathError {
      message: "index [" + idx.to_string() + "] on a non-array",
      selector: selector,
    }
        },
    }
}

/// Navigates a jq-style selector into a `json` value and coerces the leaf to
/// `T`.
///
/// Selectors start with `.` or `[`. Dot and quoted-bracket steps address
/// object fields; integer bracket steps address arrays. A dot step runs to the
/// next `.` or `[`, so a field name containing either must use the bracket
/// form: `["a.b"]`. Bracket keys accept single or double quotes, and take `\`
/// as a literal escape for one following character. Indices are non-negative
/// and absolute — there is no jq-style negative indexing, and `[-1]` reports
/// out of bounds. A bare `.` selects nothing, so `"."` is the identity
/// selector.
///
/// The leaf is coerced with `to<T>`, so `baml.FromJson` overrides apply and
/// `T` may be a whole class, not only a scalar. Every way this can fail —
/// malformed selector, missing field, wrong shape, out-of-range index, leaf
/// that will not decode — collapses into one `PathError`, which is what makes
/// `path_or` a clean total wrapper.
///
/// # Throws
///
/// - `baml.json.PathError` if the selector is malformed, if a step does not
///   resolve, or if the selected leaf does not decode to `T`.
///
/// # Examples
///
/// ```baml
/// let doc: json = { "users": [{ "name": "Ada", "age": 36 }] };
/// assert.equal(baml.json.path<string>(doc, ".users[0].name"), "Ada");
/// assert.equal(baml.json.path<int>(doc, "[\"users\"][0][\"age\"]"), 36);
/// ```
function path<T>(j: json, selector: string) -> T throws PathError {
    if (!(selector.starts_with(".") || selector.starts_with("["))) {
        throw PathError {
      message: "selector must start with '.' or '[' (jq style), got '" + selector + "'",
      selector: selector,
    };
    }
    let current: json = j;
    let i = 0;
    while (i < selector.length()) {
        let marker = selector.slice(i, i + 1);
        if (marker == ".") {
            i += 1;
            let start = i;
            while (
                i < selector.length()
                    && selector.slice(i, i + 1) != "."
                    && selector.slice(i, i + 1) != "["
            ) {
                i += 1;
            }
            if (i > start) {
                current = _path_field(current, selector.slice(start, i), selector);
            }
            continue;
        }

        if (marker != "[") {
            throw PathError {
        message: "expected '.' or '[' at offset " + i.to_string(),
        selector: selector,
      };
        }

        i += 1;
        if (i >= selector.length()) {
            throw PathError { message: "unterminated '[' segment", selector: selector };
        }

        let first = selector.slice(i, i + 1);
        if (first == "\"" || first == "'") {
            let quote = first;
            i += 1;
            let key = "";
            let closed = false;
            while (i < selector.length()) {
                let ch = selector.slice(i, i + 1);
                if (ch == "\\") {
                    if (i + 1 >= selector.length()) {
                        throw PathError { message: "unterminated escape in quoted bracket key", selector: selector };
                    }
                    key = key + selector.slice(i + 1, i + 2);
                    i += 2;
                    continue;
                }
                if (ch == quote) {
                    closed = true;
                    i += 1;
                    break;
                }
                key = key + ch;
                i += 1;
            }
            if (!closed || i >= selector.length() || selector.slice(i, i + 1) != "]") {
                throw PathError { message: "unterminated quoted bracket key", selector: selector };
            }
            i += 1;
            current = _path_field(current, key, selector);
            continue;
        }

        let start = i;
        while (i < selector.length() && selector.slice(i, i + 1) != "]") {
            i += 1;
        }
        if (i >= selector.length()) {
            throw PathError { message: "unterminated '[' segment", selector: selector };
        }
        let token = selector.slice(start, i);
        i += 1;
        let idx = int.parse(token) catch (_) {
            root.errors.ParseError => {
                throw PathError { message: "bad bracket token '[" + token + "]'", selector: selector };
            },
        };
        current = _path_index(current, idx, selector);
    }
    to<T>(current) catch (_) {
        DecodeError => {
            throw PathError {
      message: "leaf does not coerce to the requested type",
      selector: selector,
    }
        },
    }
}

/// Returns `default` when `path` cannot resolve or coerce the selected value.
///
/// Swallows every `PathError`, a misspelled selector included. That makes it
/// the wrong tool while you are still getting a selector right: a typo is
/// indistinguishable from a genuinely absent field.
function path_or<T>(j: json, selector: string, default: T) -> T throws never {
    path<T>(j, selector) catch (_) {
        PathError => default,
    }
}

/// Lowers a BAML type to ordinary, provider-neutral JSON Schema.
///
/// The type-level counterpart to the value conversions above: they move a
/// *value* between BAML and `json`, while this describes the *shape* a value
/// of `t` would take.
///
/// Optional fields are omitted from `required`, maps use schema-valued
/// `additionalProperties`, and unions use `anyOf`. Enums lower to a string
/// `enum` of their variant names. A nullable type becomes a `["T", "null"]`
/// type list where it can, and `anyOf` otherwise. A nullable enum also lists
/// `null` in its `enum`, and a nullable literal uses `anyOf`, because `enum`
/// and `const` restrict the value whatever `type` says.
///
/// An `int` carries its range as `minimum` and `maximum`. A `bigint` is a
/// decimal string (`"-12"`), because a JSON number that large loses precision
/// in most parsers; a bigint literal is that string as a `const`. A media value
/// is the tagged object `{ kind, source, value, mime, name? }`, where `source`
/// is `"url"` or `"base64"`. A map's key type must be `string`, or string
/// literals, which become `propertyNames`.
///
/// Some types have values but no JSON form: `uint8array`, functions, futures,
/// `reflect.Type`, a class that requires one of those, and a class whose every
/// value is cyclic (JSON holds only finite values). Others have no values at
/// all: `never`, an enum with no variants, and a class that requires one of
/// those. Both kinds are dropped from a union, so `uint8array | int | string`
/// lowers as `int | string`, and an optional field of either kind is only
/// `null`. They differ inside a list or map:
///
///     never[]                 // only []: { "type": "array", "maxItems": 0 }
///     map<string, never>      // only {}
///     uint8array[]            // no JSON form: the list is dropped like its items
///     map<string, uint8array> // no JSON form
///
/// A list of a type with no values is still a list, the empty one. A list of
/// `uint8array` is not reduced to `[]`: that would describe only the lists the
/// type can hold when they happen to be empty.
///
/// A type whose values all lack a JSON form, or that has no values, is
/// rejected rather than lowered to a schema nothing can satisfy.
///
/// `naming` picks the names fields and variants go by, as it does for
/// `decode<T>`, so a schema and the decoder given the same `naming` agree on
/// every key. With `Naming.Aliased`, a `@skip` field holds its type's
/// empty value, so a class with a `@skip` field whose type has none (media, a
/// function, a class that contains itself) has no JSON form.
///
/// Documentation becomes `description`, with the text the output format
/// shows: a class's or a field's `@description`, then its `///` docstring. A
/// field's `description` sits beside the `$ref` when the field is a class. An
/// enum's `description` holds its own documentation and one
/// `- <variant>: <documentation>` line per documented variant, because JSON
/// Schema has no description per `enum` value.
///
/// The root type is emitted inline, and every class and named alias beneath it
/// goes into `$defs` to be reached by `$ref`. That is what lets a recursive
/// graph terminate: a recursive root is emitted both inline and into `$defs`,
/// so the `$ref` back to itself resolves. Definitions are keyed by name; when
/// two share one, the later gets a `_2` (then `_3`, ...) suffix.
///
/// # Throws
///
/// - `baml.errors.Unsupported` when no value of `t` has a JSON form, or `t` has
///   no values.
/// - `baml.errors.Unsupported` for a type no schema describes exactly:
///   `unknown`, an interface type, media that does not name its kind, a map
///   whose key is not `string` or string literals, and a generic class
///   instance.
/// - `baml.errors.Unsupported` for a class, enum, or alias the reflection
///   context cannot resolve.
function schema(
    t: reflect.Type,
    naming: Naming = Naming.Declared,
) -> json throws root.errors.Unsupported {
    _schema(t, naming == Naming.Aliased)
}

/// `schema`, with `naming` as a flag: aliased names when `aliased` holds.
function _schema(t: reflect.Type, aliased: bool) -> json throws root.errors.Unsupported {
    $rust_io_function
}
