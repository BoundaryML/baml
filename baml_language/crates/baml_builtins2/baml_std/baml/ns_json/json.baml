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

/// A missing field, shape mismatch, out-of-bounds index, malformed selector,
/// or leaf that does not coerce to the requested type.
class JsonPathError {
    message: string,
    selector: string,
}

function _path_field(current: json, key: string, selector: string) -> json throws JsonPathError {
    match (current) {
        let m: map<string, json> => {
            if (!m.has(key)) {
                throw JsonPathError { message: "missing field '" + key + "'", selector: selector };
            }
            m.get(key)
        },
        _ => {
            throw JsonPathError {
      message: "field access '." + key + "' on a non-object",
      selector: selector,
    }
        },
    }
}

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
            throw JsonPathError {
      message: "index [" + idx.to_string() + "] on a non-array",
      selector: selector,
    }
        },
    }
}

/// Navigate a jq-style selector into a JSON value and coerce the leaf to `T`.
/// Selectors start with `.` or `[`. Dot and quoted-bracket steps address object
/// fields; integer bracket steps address arrays.
function path<T>(j: json, selector: string) -> T throws JsonPathError {
    if (!(selector.starts_with(".") || selector.starts_with("["))) {
        throw JsonPathError {
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
            throw JsonPathError {
        message: "expected '.' or '[' at offset " + i.to_string(),
        selector: selector,
      };
        }

        i += 1;
        if (i >= selector.length()) {
            throw JsonPathError { message: "unterminated '[' segment", selector: selector };
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
                        throw JsonPathError { message: "unterminated escape in quoted bracket key", selector: selector };
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
                throw JsonPathError { message: "unterminated quoted bracket key", selector: selector };
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
            throw JsonPathError { message: "unterminated '[' segment", selector: selector };
        }
        let token = selector.slice(start, i);
        i += 1;
        let idx = int.parse(token) catch (_) {
            root.errors.ParseError => {
                throw JsonPathError { message: "bad bracket token '[" + token + "]'", selector: selector };
            },
        };
        current = _path_index(current, idx, selector);
    }
    to<T>(current) catch (_) {
        JsonDecodeError => {
            throw JsonPathError {
      message: "leaf does not coerce to the requested type",
      selector: selector,
    }
        },
    }
}

/// Return `default` when [`path`] cannot resolve or coerce the selected value.
function path_or<T>(j: json, selector: string, default: T) -> T throws never {
    path<T>(j, selector) catch (_) {
        JsonPathError => default,
    }
}

/// Lower a BAML `type` to ordinary, provider-neutral JSON Schema. Optional
/// fields are omitted from `required`, maps use schema-valued
/// `additionalProperties`, unions use `anyOf`, and recursive class graphs use
/// `$defs` and `$ref`.
///
/// This is the type-level counterpart to the value-level conversions above:
/// everything else here moves a *value* between BAML and `json`, while this
/// describes the *shape* a value of `t` would take.
///
/// Throws `root.errors.Unsupported` for constructs without a JSON Schema form.
function schema(t: type) -> json throws root.errors.Unsupported {
    $rust_io_function
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
