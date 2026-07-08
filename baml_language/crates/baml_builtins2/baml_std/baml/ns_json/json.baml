/// The BAML JSON value type — a union of all JSON-representable values.
type json = null | bool | int | float | string | json[] | map<string, json>;

/// Thrown when a JSON string cannot be parsed (syntax error).
class JsonParseError {
    message: string,
}

/// Thrown when a parsed JSON value does not match the expected BAML type.
class JsonDecodeError {
    message: string,
    path: string,
}

/// Thrown when a BAML value cannot be serialized to JSON.
class JsonSerializationError {
    message: string,
    path: string,
    reason: string,
}

//baml:mut_vm
/// Parses a JSON string `s` and returns a `json` value. Throws `JsonParseError` on invalid JSON.
function parse(s: string) -> json throws JsonParseError {
    $rust_function
}

//baml:mut_vm
/// Serializes a `json` value `j` to a compact JSON string.
function stringify(j: json) -> string throws never {
    $rust_function
}

//baml:mut_vm
/// Serializes a `json` value `j` to a pretty-printed (indented) JSON string.
function stringify_pretty(j: json) -> string throws never {
    $rust_function
}

/// Serialize any BAML value `v` of type `T` to its canonical JSON string.
///
/// The type argument `T` is delivered to the native handler at runtime via
/// the BEP-039 type-arg channel, so the walker can produce a tagged JSON
/// document for media values, the variant name for enums, and the natural
/// JSON shape for primitives / lists / maps / classes.
///
/// Throws `JsonSerializationError` for non-representable types
/// (`uint8array` without explicit encoding, function values, etc.).
//baml:mut_vm
function to_string<T>(v: T) -> string throws JsonSerializationError {
    $rust_function
}

/// Serialize any BAML value to its `json` representation by dispatching on
/// the value's runtime type.
///
/// Equivalent to `v.to_json()` when `T` is concrete, but works through generic
/// boundaries: a synthesized `to_json` body on `class Box<T> { value T }` can
/// call `baml.json.to_json(self.value)` to get override-honoring dispatch
/// without knowing `T` at synthesis time. The auto-derive synthesizer
/// (`baml_compiler2_ast::auto_derive_json`) uses this for `TypeVar`-typed
/// fields where direct method dispatch would fail at runtime (the receiver's
/// concrete `to_json` only exists on the runtime value, not the static type).
///
/// Behaviour matches the per-type `to_json`: dispatches to `Int.to_json`,
/// `String.to_json`, user-class `to_json` (including overrides),
/// `Array<T>.to_json`, etc.
//baml:mut_vm
//baml:may_yield
function to_json<T>(v: T) -> json throws JsonSerializationError {
    $rust_function
}

/// Serialize any BAML value `value` of type `T` to its `json` representation,
/// honoring user-defined `baml.ToJson` overrides on classes at every depth.
///
/// The json analog of `string.from`: a structural conversion that renders
/// primitives, lists, maps, class instances (as `{"field": value}`), enums (as
/// their variant name), and media (as the tagged form) to their natural json
/// shape, routing any value whose runtime class `implements baml.ToJson` through
/// that override. Resolution is on the value's *runtime* class (via the
/// `baml._to_json_shim` native), for the same package-boundary reason
/// `string.from` uses `_to_string_shim` rather than the literal `is baml.ToJson`
/// match form.
///
/// Throws `JsonSerializationError` for non-representable values (`uint8array`
/// without explicit encoding, function values, etc.).
function from<T>(value: T) -> json throws JsonSerializationError {
    root._to_json_shim(value)
}

/// Parse a JSON string and coerce it to a BAML value of type `T`.
///
/// The type argument `T` is delivered to the native handler at runtime via
/// the BEP-039 type-arg channel.  The handler parses the string with
/// `serde_json`, then runs SAP's coercer against the runtime `Ty` to produce
/// a typed value.
///
/// Throws `JsonParseError` if the string is not valid JSON, or
/// `JsonDecodeError` if the parsed JSON does not match `T`.
//baml:mut_vm
function from_string<T>(s: string) -> T throws JsonParseError | JsonDecodeError {
    $rust_function
}

/// Decode a `json` value to a BAML value of type `T`, honoring user
/// `from_json` overrides on classes (including through arrays, maps, and
/// optionals).
///
/// Equivalent to `T.from_json(j)` when `T` is a concrete class, but works
/// through generic boundaries: a synthesized `from_json` body on
/// `class Box<T> { value T }` calls `baml.json.from_json<T>(...)` to
/// dispatch on the runtime substitution of `T` without knowing it at
/// synthesis time. The auto-derive synthesizer
/// (`baml_compiler2_ast::auto_derive_json`) routes every field through
/// this function so override dispatch is centralized.
///
/// Behaviour by type-arg:
/// - User class with `from_json` method: dispatch to that method.
/// - List / Map / Optional of class: walk structurally and dispatch the
///   inner class's `from_json` per element / value.
/// - Primitives, enums, media, type-aliases: structural decode equivalent
///   to `from_string<T>(stringify(j))`.
///
/// Throws `JsonDecodeError` on shape mismatch (decoding an already-parsed
/// `json` value never re-parses, so no `JsonParseError` arises here).
//baml:mut_vm
//baml:may_yield
function from_json<T>(j: json) -> T throws JsonDecodeError {
    $rust_function
}

/// Decode a `json` value `j` into a BAML value of type `T`, honoring user
/// `baml.FromJson` overrides on the target type at every depth.
///
/// The deserialize counterpart of `baml.json.from<T>` (`json.from` serializes a
/// `T` to json; `json.to` decodes json back to a `T`). Resolution is on the
/// target type `T` (delivered via the type-arg channel), via the
/// `baml._from_json_shim` native — for the same package-boundary reason
/// `baml.json.from` uses `_to_json_shim`. A type that `implements baml.FromJson`
/// is constructed via its override; everything else decodes structurally
/// (per-field, walking lists/maps/optionals and honoring nested overrides).
///
/// Throws `JsonDecodeError` on shape mismatch (decoding an already-parsed
/// `json` value never re-parses, so no `JsonParseError` arises here).
//baml:mut_vm
//baml:may_yield
function to<T>(j: json) -> T throws JsonDecodeError {
    root._from_json_shim<T>(j)
}

/// Look up `key` in a `json` value when it is a `map<string, json>`; returns
/// `null` if `j` is not a map or the key is absent.
///
/// Used by the auto-derive `from_json` synthesizer to extract field values
/// from a `json` object without requiring the caller to match-narrow the
/// `json` union to its map arm.
//baml:mut_vm
function field(j: json, key: string) -> json throws never {
    $rust_function
}

/// Error thrown by [`path<T>`]: a missing field, a shape mismatch (field access on a
/// non-object, index on a non-array), an out-of-bounds index, a malformed selector, or
/// a leaf that does not coerce to the requested type.
class JsonPathError {
    message: string,
    selector: string,
}

/// Object-field step for [`path<T>`]. Distinguishes a missing key from a present-but-null
/// value (only the former throws).
function _path_field(current: json, key: string, selector: string) -> json throws JsonPathError {
    match (current) {
        let m: map<string, json> => {
            if (!m.has(key)) {
                throw JsonPathError { message: "missing field '" + key + "'", selector: selector };
            }
            m.get(key)
        },
        _ => {
            throw JsonPathError { message: "field access '." + key + "' on a non-object", selector: selector }
        },
    }
}

/// Array-index step for [`path<T>`].
function _path_index(current: json, idx: int, selector: string) -> json throws JsonPathError {
    match (current) {
        let arr: json[] => {
            if (idx < 0 || idx >= arr.length()) {
                throw JsonPathError {
          message: "index [" + idx.to_string() + "] out of bounds (length " + arr.length().to_string() + ")",
          selector: selector,
        };
            }
            arr.at(idx)
        },
        _ => {
            throw JsonPathError { message: "index [" + idx.to_string() + "] on a non-array", selector: selector }
        },
    }
}

/// Navigate a jq-style selector into a `json` value and coerce the leaf to `T`:
///
///   baml.json.path<int>(j, ".usage.input_tokens")
///   baml.json.path<string>(j, ".choices[0].message.content")
///   baml.json.path<string>(j, ".by_name[\"anna.k\"].id")   // quoted key = object access (dots ok)
///   baml.json.path<float>(j, "[0]")                         // root array index
///   baml.json.path<MyClass>(j, ".")                         // identity: coerce the whole value
///
/// Selectors follow the jq shape: they start with `.` (or `[` on a root array); `.name`
/// is object-field access, `["key"]`/`['key']` is object access with a quoted key (which
/// may contain dots), `[0]` is array indexing. Keys containing `[` or `]` are not
/// supported. Throws [`JsonPathError`] on a missing field, shape mismatch, out-of-bounds
/// index, malformed selector, or a leaf that does not coerce to `T`. For whole envelopes
/// (or `type`-tagged arrays that need filtering) prefer typed wire classes decoded via
/// `baml.sap.parse<Wire>` with `@alias` fields.
/// The coalescing sibling of [`path`]: a missing field, shape mismatch,
/// out-of-bounds index, malformed selector, or uncoercible leaf yields
/// `default` instead of throwing — the "field with default" idiom every
/// ecosystem grows (Python's `d.get(k, default)`, serde's `.unwrap_or(...)`,
/// jq's `// default`, gjson's zero-value gets). Use `path` when a miss is a
/// bug to surface; use `path_or` for best-effort reads of noisy wire JSON:
///
///   baml.json.path_or(tc, ".function.name", "")     // T inferred from the default
///   baml.json.path_or<int>(j, ".usage.input_tokens", 0)
function path_or<T>(j: json, selector: string, default: T) -> T throws never {
    path<T>(j, selector) catch (e) {
        _ => default,
    }
}

function path<T>(j: json, selector: string) -> T throws JsonPathError {
    if (!(selector.starts_with(".") || selector.starts_with("["))) {
        throw JsonPathError {
      message: "selector must start with '.' or '[' (jq style), got '" + selector + "'",
      selector: selector,
    };
    }
    let current: json = j;
    // Split on '[' so quoted keys (which may contain dots) stay intact: every piece after
    // the first starts with one bracket token terminated by ']'.
    let pieces = selector.split("[");
    let i = 0;
    for (let piece in pieces) {
        let rest = piece;
        if (i > 0) {
            let closing = rest.split("]");
            if (closing.length() < 2) {
                throw JsonPathError { message: "unterminated '[' segment", selector: selector };
            }
            let token = closing.at(0) ?? "";
            rest = closing.at(1) ?? "";
            let quoted = (token.starts_with("\"") && token.ends_with("\"") && token.length() >= 2)
                || (token.starts_with("'") && token.ends_with("'") && token.length() >= 2);
            if (quoted) {
                current = _path_field(current, token.substring(1, token.length() - 1), selector);
            } else {
                let idx = int.parse(token) catch (e) {
                    _ => {
                        throw JsonPathError { message: "bad bracket token '[" + token + "]'", selector: selector };
                    }
                };
                current = _path_index(current, idx, selector);
            }
        }
        for (let name in rest.split(".")) {
            if (name == "") {
                continue;
            }
            current = _path_field(current, name, selector);
        }
        i = i + 1;
    }
    to<T>(current) catch (e) {
        _ => {
            throw JsonPathError { message: "leaf does not coerce to the requested type", selector: selector }
        },
    }
}

/// Serialize `v` to JSON text, honoring user-defined `to_json` overrides on
/// classes (via [`to_json<T>`]). Equivalent to `stringify(to_json<T>(v))`.
///
/// Use this from a host that wants to print a target's return value as JSON
/// while still respecting class-level `to_json` overrides. The structural
/// path is [`to_string<T>`], which walks fields directly and bypasses
/// overrides — appropriate when override behavior would be a problem.
///
/// HACK: this is a thin shim that only exists so `baml_exec::dispatch`
/// has a stable named entry point to call from Rust (via
/// `engine.call_function("baml.json.serialize", ...)`). The composition
/// `stringify(to_json<T>(v))` is what callers would write inline anyway;
/// owning a wrapper function gives us one engine-side symbol to look up
/// instead of two, and a place to evolve the override semantics if they
/// drift. Remove this if the host gains a direct way to invoke generic
/// compositions without a named landing pad.
function serialize<T>(v: T) -> string throws JsonSerializationError {
    stringify(to_json<T>(v))
}

/// Parse JSON text into a value of type `T`, honoring user-defined
/// `from_json` overrides on classes (via [`from_json<T>`]). Equivalent to
/// `from_json<T>(parse(s))`.
///
/// Use this from a host that wants to coerce a JSON-shaped input (such as
/// `--json-args`) into a typed BAML value while respecting class-level
/// `from_json` overrides. The structural path is [`from_string<T>`], which
/// decodes structurally and bypasses overrides.
///
/// HACK: same story as [`serialize<T>`] — this wrapper exists so
/// `baml_exec::dispatch::build_args_from_signature` can resolve a single
/// stable name (`baml.json.deserialize`) to coerce `--json-args` payloads
/// through `from_json<T>(parse(s))`. Inlining the composition into the
/// Rust caller would mean threading two generic engine calls + their
/// throws-types instead of one, so the wrapper pays for itself.
function deserialize<T>(s: string) -> T throws JsonParseError | JsonDecodeError {
    from_json<T>(parse(s))
}
