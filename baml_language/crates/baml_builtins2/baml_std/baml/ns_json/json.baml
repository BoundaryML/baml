/// An untyped JSON value.
///
/// JSON's one number type splits into two arms here. `parse` reads a number as
/// `int` when it is integral and fits BAML's 63-bit `int`, and as `float`
/// otherwise. An integer too large to hold therefore arrives with
/// double-precision rounding rather than as an error. Decoding into a declared
/// `int` (`to<int>`, `from_string<int>`) is strict and rejects it instead.
///
/// Reach an arm with `match`, or hand the value to `to<T>`.
type json = null | bool | int | float | string | json[] | map<string, json>;

/// A JSON syntax error.
///
/// Only the entry points that read text raise this: `parse`, `from_string<T>`,
/// and `deserialize<T>`. Everything that consumes an already-parsed `json`
/// throws `DecodeError` instead. `message` is the parser's own diagnostic,
/// locating the offending line and column.
class ParseError {
    message: string,
}

/// A parsed JSON value that does not fit the expected BAML type.
///
/// `path` locates the offending node in jq-ish notation: `.field` for a class
/// field, `[0]` for an array element, `["key"]` for a map entry. Only
/// `from_string<T>` threads it end to end from the decode root. The
/// override-honoring drivers (`to<T>`, `from_json<T>`, `deserialize<T>`)
/// re-enter per node, so there it is `""` or relative to the failing leaf.
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
/// not a stopping point. To land in a declared type instead, use
/// `from_string<T>` (structural) or `deserialize<T>` (override-honoring).
///
/// # Throws
///
/// - `baml.json.ParseError` if `s` is not valid JSON.
function parse(s: string) -> json throws ParseError {
    $rust_function
}

//baml:mut_vm
/// Serializes a `json` value to compact JSON text.
///
/// Never fails: every `json` arm has a JSON form. The one lossy case is a
/// non-finite `float` — JSON has no NaN or infinity, so those become `null`.
/// For a value that is not already a `json`, use `to_string`.
function stringify(j: json) -> string throws never {
    $rust_function
}

//baml:mut_vm
/// Serializes a `json` value to indented JSON text.
///
/// The same value mapping as `stringify`, non-finite floats included; only the
/// layout differs.
function stringify_pretty(j: json) -> string throws never {
    $rust_function
}

/// Serializes any BAML value to its canonical JSON text.
///
/// `stringify` composed with `to_json`: dispatch is on the value's runtime
/// type, and `baml.ToJson` overrides apply at every depth. Reach for this when
/// you want JSON text, and for `from<T>` when you want a `json` value to keep
/// working with. Not to be confused with `string.from`, which renders BAML's
/// own display form rather than JSON.
///
/// # Throws
///
/// - `baml.json.SerializationError` if any value in the graph has no JSON form
///   — a `uint8array` without explicit encoding, a function, a future. See
///   `from<T>` for the full rendering rules.
function to_string(value: unknown) -> string throws SerializationError {
    stringify(to_json(value))
}

/// Serializes any BAML value to its `json` representation, dispatching on the
/// value's runtime type.
///
/// The untyped spelling of `from<T>` — same walk, same `baml.ToJson` override
/// handling at every depth, same output. Only the signature differs: `unknown`
/// in place of a type parameter, which makes it callable where no static type
/// is available to instantiate `from<T>` with, and callable by name from a
/// host.
///
/// Prefer `from<T>` in BAML code. It is what a bare `v.to_json()` desugars to,
/// and it keeps the argument's type at the call site instead of erasing it to
/// `unknown`. This one exists so `to_string` and engine-side callers have a
/// non-generic symbol to bind.
///
/// # Throws
///
/// - `baml.json.SerializationError` if any value in the graph has no JSON
///   form. See `from<T>` for the rendering rules.
//baml:mut_vm
//baml:may_yield
function to_json(value: unknown) -> json throws SerializationError {
    $rust_function
}

/// Serializes a value to its `json` representation, honoring `baml.ToJson`
/// overrides on classes at every depth.
///
/// The json analog of `string.from`. Primitives, lists, and maps render to
/// their natural json shape; a class instance becomes `{"field": value}`, an
/// enum its variant name, media the tagged form. A `bigint` renders as a
/// decimal *string*, since JSON numbers cannot carry it, and a non-finite
/// `float` renders as `null`.
///
/// A value whose runtime class `implements baml.ToJson` routes through that
/// override instead, and an override owns its whole subtree — the walk stops
/// descending once it dispatches one. Resolution is on the value's *runtime*
/// class, so an override declared in any package is honored, not just one
/// visible to this one.
///
/// `T` selects nothing. It is the call site's static type and no more; every
/// dispatch decision is made on the value in hand.
///
/// This is the caller-facing serializer, and what `v.to_json()` desugars to.
/// `to_json` is the same walk behind an `unknown` parameter.
///
/// # Throws
///
/// - `baml.json.SerializationError` if any value in the graph has no JSON
///   form: a `uint8array` (encode it explicitly with `to_base64()` or
///   `to_hex()`), a function, a future, or any other opaque runtime object.
///   `reason` names which.
function from<T>(value: T) -> json throws SerializationError {
    root._to_json_shim(value)
}

/// Parses JSON text and decodes it structurally into `T`, ignoring
/// `baml.FromJson` overrides.
///
/// `JSON.parse` and Python's `json.loads` hand back whatever the document
/// happened to contain. This does the opposite: `T` drives the walk, and the
/// walk is a shape check rather than a coercion. A JSON string does not become
/// an `int`, and a number outside BAML's 63-bit `int` is rejected rather than
/// widened to `float`.
///
/// `T` is delivered over the BEP-039 type-arg channel and read at runtime, so
/// it need not be statically known at the call site. The text is parsed first,
/// then the parsed tree is walked driven by that runtime type.
///
/// Per BEP-038, JSON interchange uses raw field names, so `@alias` and `@skip`
/// have no effect here. Every declared field must be present, except that an
/// optional (`T?`) field may be absent and decodes to `null`. Unrecognized
/// keys in the object are ignored. A union tries its members in declaration
/// order and takes the first that decodes.
///
/// This is the *structural* text path, and the only one that fills
/// `DecodeError.path` with a full location. `deserialize<T>` is the same job
/// with `baml.FromJson` overrides honored; reach for that unless you
/// specifically want overrides bypassed.
///
/// # Throws
///
/// - `baml.json.ParseError` if `s` is not valid JSON.
/// - `baml.json.DecodeError` if the parsed JSON does not fit `T`.
//baml:mut_vm
function from_string<T>(s: string) -> T throws ParseError | DecodeError {
    $rust_function
}

/// Decodes a `json` value into `T`, honoring `baml.FromJson` overrides on
/// classes through arrays, maps, and optionals.
///
/// The second spelling of `to<T>` — same dispatcher, same override handling,
/// same result. `to<T>` reaches it through one extra dispatch hop, which is the
/// only difference. Both read `T` from the runtime type-arg channel, so neither
/// needs `T` statically known at the call site.
///
/// Prefer `to<T>` in BAML code. It is what a `Type.from_json(j)` call desugars
/// to, and it names the direction: `from` serializes, `to` decodes. This one
/// is kept as the symbol that `deserialize<T>` and engine-side callers bind
/// to.
///
/// # Throws
///
/// - `baml.json.DecodeError` if `j` does not fit `T`. Decoding an
///   already-parsed `json` never re-parses, so no `ParseError` arises here.
//baml:mut_vm
//baml:may_yield
function from_json<T>(j: json) -> T throws DecodeError {
    $rust_function
}

/// Decodes a `json` value into `T`, honoring `baml.FromJson` overrides on the
/// target type at every depth.
///
/// The decode counterpart of `baml.json.from<T>`: `json.from` serializes a `T`
/// to json, `json.to` reads json back as a `T`. A type that
/// `implements baml.FromJson` is constructed through its override; everything
/// else decodes structurally, field by field, walking lists, maps, and
/// optionals and honoring nested overrides. Resolution is on the target type
/// `T`, delivered via the type-arg channel, so an override declared in any
/// package is honored, not just one visible to this one.
///
/// `T` is not inert here, as it is in `from<T>`. Serialization dispatches on
/// the runtime *value*; decoding has only the target type to go on. Supplying
/// a wider `T` than you need therefore changes the result, not just the static
/// type.
///
/// Structural decoding follows BEP-038's raw field names, so `@alias` and
/// `@skip` have no effect. A missing optional (`T?`) field decodes to `null`,
/// a missing required field throws, and unrecognized object keys are ignored.
///
/// This is the caller-facing decoder, and what `Type.from_json(j)` desugars
/// to. `from_json<T>` is the same dispatcher one hop further in.
///
/// # Throws
///
/// - `baml.json.DecodeError` if `j` does not fit `T`, or if a
///   `baml.FromJson` override rejects it. Decoding an already-parsed `json`
///   never re-parses, so no `ParseError` arises here.
//baml:mut_vm
//baml:may_yield
function to<T>(j: json) -> T throws DecodeError {
    root._from_json_shim<T>(j)
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
/// let doc = baml.json.parse("{\"users\": [{\"name\": \"Ada\", \"age\": 36}]}");
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
/// `enum` of their variant names, `uint8array` to `string`, and `unknown` to
/// the empty schema `{}`. A nullable type becomes a `["T", "null"]` type list
/// where it can, and `anyOf` otherwise.
///
/// This pass *does* respect `@alias` and `@skip`, where the value conversions
/// do not — a schema is read by something outside BAML, so it must describe
/// the wire names. That is also why an enum variant lowers under its alias.
///
/// The root type is emitted inline, and every class and named alias beneath it
/// goes into `$defs` to be reached by `$ref`. That is what lets a recursive
/// graph terminate: a recursive root is emitted both inline and into `$defs`,
/// so the `$ref` back to itself resolves.
///
/// # Throws
///
/// - `baml.errors.Unsupported` for a type with no JSON Schema form, and for a
///   class, enum, or alias the reflection context cannot resolve.
function schema(t: reflect.Type) -> json throws root.errors.Unsupported {
    $rust_io_function
}

/// Serializes `v` to JSON text, honoring `baml.ToJson` overrides on classes
/// (via `to_json`). Equivalent to `to_string(v)`.
///
/// For a host that wants to print a target's return value as JSON while still
/// respecting class-level `to_json` overrides. From BAML, call `to_string`
/// directly; nothing here adds behaviour.
///
/// # Throws
///
/// - `baml.json.SerializationError` if any value in the graph has no JSON
///   form; see `from<T>`.
function serialize<T>(v: T) -> string throws SerializationError {
    // HACK: a thin shim that exists only so `baml_exec::dispatch` has a stable
    // named entry point to call from Rust, via
    // `engine.call_function("baml.json.serialize", ...)`. The composition
    // `to_string(v)` is what callers would write inline anyway; owning a
    // wrapper gives the engine one symbol to look up instead of two, and a
    // place to evolve the override semantics if they drift. Remove this if the
    // host gains a direct way to invoke generic compositions without a named
    // landing pad.
    to_string(v)
}

/// Parses JSON text into a value of type `T`, honoring `baml.FromJson`
/// overrides on classes (via `from_json<T>`). Equivalent to
/// `from_json<T>(parse(s))`.
///
/// For a host that wants to coerce a JSON-shaped input (such as `--json-args`)
/// into a typed BAML value while respecting class-level `from_json` overrides.
/// `from_string<T>` is the structural path, which bypasses overrides.
///
/// # Throws
///
/// - `baml.json.ParseError` if `s` is not valid JSON.
/// - `baml.json.DecodeError` if the parsed document does not fit `T`.
function deserialize<T>(s: string) -> T throws ParseError | DecodeError {
    // HACK: same story as `serialize<T>` — this wrapper exists so
    // `baml_exec::dispatch::build_args_from_signature` can resolve a single
    // stable name (`baml.json.deserialize`) to coerce `--json-args` payloads
    // through `from_json<T>(parse(s))`. Inlining the composition into the Rust
    // caller would mean threading two generic engine calls and their
    // throws-types instead of one, so the wrapper pays for itself.
    from_json<T>(parse(s))
}
