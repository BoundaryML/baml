/// Human-readable string conversion with an implicit structural default.
///
/// A type implements `baml.ToString` to control how `string.from(value)`
/// renders it. Conversion is total — `to_string` never throws.
///
/// ```baml
/// class Point {
///   x int
///   y int
///   implements baml.ToString {
///     function to_string(self) -> string throws never {
///       "(" + string.from(self.x) + ", " + string.from(self.y) + ")"
///     }
///   }
/// }
/// ```
///
/// Every type has an implicit implementation. An explicit implementation
/// overrides the structural rendering (see [`_to_string_default`]).
///
/// `to_string` carries that same structural rendering as its default body, so
/// `implements baml.ToString {}` with no override is valid and renders exactly
/// like a non-implementor.
interface ToString {
    function to_string(self) -> string throws never {
        root._to_string_default(self)
    }
}

/// Renders a reflected type's name.
///
/// Declared here beside the interface it implements rather than in the
/// `reflect` package: `baml` depends on `reflect`, so `reflect` cannot name
/// `baml.ToString` without a dependency cycle. The kind views below are
/// declared here for the same reason.
implement root.ToString for reflect.Type {
  function to_string(self) -> string throws never {
    self._to_string_impl()
  }
}

/// Renders a `reflect.Class` as the type it wraps, like `reflect.Type`.
implement root.ToString for reflect.Class {
  function to_string(self) -> string throws never {
    self.as_type()._to_string_impl()
  }
}

/// Renders a `reflect.Enum` as the type it wraps, like `reflect.Type`.
implement root.ToString for reflect.Enum {
  function to_string(self) -> string throws never {
    self.as_type()._to_string_impl()
  }
}

/// Renders a `reflect.Union` as the type it wraps, like `reflect.Type`.
implement root.ToString for reflect.Union {
  function to_string(self) -> string throws never {
    self.as_type()._to_string_impl()
  }
}

/// Renders a `reflect.Literal` as the type it wraps, like `reflect.Type`.
implement root.ToString for reflect.Literal {
  function to_string(self) -> string throws never {
    self.as_type()._to_string_impl()
  }
}

/// Renders a `reflect.Array` as the type it wraps, like `reflect.Type`.
implement root.ToString for reflect.Array {
  function to_string(self) -> string throws never {
    self.as_type()._to_string_impl()
  }
}

/// Renders a `reflect.Map` as the type it wraps, like `reflect.Type`.
implement root.ToString for reflect.Map {
  function to_string(self) -> string throws never {
    self.as_type()._to_string_impl()
  }
}

/// Renders a `reflect.Interface` as the type it wraps, like `reflect.Type`.
implement root.ToString for reflect.Interface {
  function to_string(self) -> string throws never {
    self.as_type()._to_string_impl()
  }
}

/// Renders a `reflect.Primitive` as the type it wraps, like `reflect.Type`.
implement root.ToString for reflect.Primitive {
  function to_string(self) -> string throws never {
    self.as_type()._to_string_impl()
  }
}

/// Renders a `reflect.Function` as the type it wraps, like `reflect.Type`.
implement root.ToString for reflect.Function {
  function to_string(self) -> string throws never {
    self.as_type()._to_string_impl()
  }
}

/// (internal) The structural rendering used by the default `baml.ToString` body.
///
/// Primitives render naturally (`5`, `true`, `null`); containers and class
/// instances render structurally (`[1, 2]`, `Point { x: 1, y: 2 }`). A nested
/// value whose runtime class *does* implement `baml.ToString` renders through
/// that override, so the default honors overrides at every depth, just as
/// `string.from` does. Never throws.
//baml:mut_vm
//baml:may_yield
function _to_string_default<T>(value: T) -> string throws never {
    $rust_function
}

/// (internal) Run-once guard for a user class's magic `cleanup(self) -> void`
/// method (BEP-042), compiled into the method's prologue as
/// `if (!baml._cleanup_begin(self)) { return; }`.
///
/// Atomically test-and-sets the receiver instance's per-instance "cleaned"
/// latch and returns `true` only on the *first* invocation, so the body runs
/// once. A second `cleanup` on the same instance — from an explicit call, a
/// `defer`, or (later) the GC finalizer — returns `false` and the body is
/// skipped. The latch is set on entry, so a `cleanup` that throws still counts
/// as cleaned (BEP-042). A non-instance receiver returns `true` defensively;
/// the guard is only emitted for class `cleanup` methods, whose `self` is
/// always an instance.
//baml:vm
function _cleanup_begin<T>(value: T) -> bool throws never {
    $rust_function
}

/// JSON conversion with an implicit structural default.
///
/// A type implements `baml.ToJson` to control how `baml.json.from(value)`
/// renders it, the json analog of `baml.ToString`. Implicit implementations
/// render primitives naturally and containers and instances structurally.
/// An explicit implementation customizes its own json shape.
///
/// ```baml
/// class Temp {
///   celsius float
///   implements baml.ToJson {
///     function to_json(self) -> baml.json.json throws baml.json.SerializationError {
///       { "c": baml.json.from(self.celsius), "f": baml.json.from(self.celsius * 1.8 + 32.0) }
///     }
///   }
/// }
/// ```
///
/// Unlike `to_string`, the conversion can fail: a value with no json
/// representation — a `uint8array` without an explicit encoding, a function
/// value — throws `baml.json.SerializationError`.
///
/// `to_json` carries the structural rendering as its default body, so
/// `implements baml.ToJson {}` with no override is valid and renders exactly
/// like a non-implementor.
interface ToJson {
    function to_json(self) -> root.json.json throws root.json.SerializationError {
        root._to_json_default(self)
    }
}

/// (internal) The structural rendering used by the default `baml.ToJson` body.
///
/// Primitives render naturally; containers and class instances render
/// structurally (`[1, 2]`, `{"field": value}`), enums as their variant name,
/// media as the tagged form. A nested value whose runtime class *does*
/// implement `baml.ToJson` renders through that override, so the default honors
/// overrides at every depth, just as `baml.json.from` does. Throws
/// `baml.json.SerializationError` for non-representable values.
//baml:mut_vm
//baml:may_yield
function _to_json_default<T>(value: T) -> root.json.json throws root.json.SerializationError {
    $rust_function
}

/// JSON deserialization with an implicit structural default.
///
/// A type implements `baml.FromJson` to control how `baml.json.to<T>(j)` (and
/// JSON deserialization generally) constructs it from a `json` value. Unlike
/// `to_json`, `from_json` has no `self`: it is an associated constructor taking
/// a `json` and returning `Self`. Implicit implementations decode
/// structurally, field by field, honoring nested `baml.FromJson` overrides, so
/// implement this only when a type's json shape differs from its field layout.
///
/// ```baml
/// class Temp {
///   celsius float
///   implements baml.FromJson {
///     function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError {
///       Temp { celsius: baml.json.to<float>(baml.json.field(j, "c")) }
///     }
///   }
/// }
/// ```
///
/// `from_json` is required in an explicit implementation. The compiler supplies
/// a structural implementation when no explicit implementation applies.
interface FromJson {
    function from_json(j: root.json.json) -> Self throws root.json.DecodeError;
}

/// Reading a type from a model's answer.
///
/// A model is asked for a type by its fields, and its answer is read field by
/// field. A type implements `baml.FromLlmOutput` to be asked for and read as
/// something else: its `Shape`. A timestamp is easier for a model to write as
/// `"2026-01-01T00:00:00Z"` than as a count of nanoseconds.
///
/// ```baml
/// class Money {
///     cents: int,
///     currency: string,
///
///     implements baml.FromLlmOutput {
///         type Shape = string;
///
///         function from_llm_output(shape: string) -> Self throws baml.json.DecodeError {
///             let parts = shape.trim().split(" ");
///             if (parts.length() != 2) {
///                 throw baml.json.DecodeError { message: `expected an amount and a currency, got "${shape}"`, path: "" }
///             }
///             let cents = (float.parse(parts[0]) * 100.0).iround() catch (_) {
///                 _ => throw baml.json.DecodeError { message: `"${parts[0]}" is not an amount`, path: "" },
///             };
///             Money { cents: cents, currency: parts[1] }
///         }
///     }
///
///     @@description("An amount and an ISO 4217 currency code, for example 12.50 USD")
/// }
/// ```
///
/// Wherever a model's answer is described or read, the type is its `Shape`:
///
/// - The output format of an LLM function (`ctx.output_format()`) shows the
///   `Shape`. The type's `@@description` is shown next to it as a hint, and a
///   type without one gets no hint. Its `///` docs and its fields are never
///   shown.
/// - `baml.json.schema` describes the `Shape`, with the `@@description` as the
///   schema's `description`.
/// - `baml.sap.parse` and `baml.sap.coerce` read the answer as the `Shape`,
///   with their usual leniency. Then they call `from_llm_output` with the
///   value read. This covers LLM functions, tool arguments, and structured
///   outputs.
///
/// When `from_llm_output` throws, the value read does not fit the type. In a
/// union, the parser tries the next member, so `Money | string` reads
/// `"twelve bucks"` as a string. Anywhere else the parse fails with
/// `baml.errors.ParseError`, and its message carries what was thrown.
///
/// While an answer streams in, `from_llm_output` runs only once the `Shape`
/// value is complete. Until then, a `Money?` field is `null`, and a `Money`
/// field holds back the class it is in, as an `int` field does.
///
/// A type that does not implement `baml.FromLlmOutput` is asked for and read
/// by its fields.
///
/// `baml.FromJson` is a different boundary. It reads JSON that code wrote
/// (`baml.json.decode`, `baml.json.to`, `T.from_json`). `baml.FromLlmOutput`
/// reads what a model wrote. A type can implement both, and each is used only
/// at its own boundary.
///
/// There is no method for the other direction. A value goes to a model through
/// `to_string` when it is in a prompt, and through `to_json` when it is a tool
/// result. Write `to_string` in the format the `Shape` asks for, so a value
/// shown to a model can be written back by it.
interface FromLlmOutput {
    /// What a model is shown and writes in place of this type.
    type Shape;

    /// Builds the value from what the model wrote.
    ///
    /// # Throws
    ///
    /// - `baml.json.DecodeError` if `shape` does not describe a value of this
    ///   type. The parser then treats the value as not fitting this type.
    function from_llm_output(shape: Self.Shape) -> Self throws root.json.DecodeError;
}

/// Decode the root structurally while honoring nested FromJson implementations.
//baml:mut_vm
//baml:may_yield
function _from_json_structural_default<T>(j: root.json.json) -> T throws root.json.DecodeError {
    $rust_function
}

/// Compare structurally in one cycle-aware walk, honoring nested Equals overrides.
//baml:mut_vm
//baml:may_yield
function _equals_structural_default(a: unknown, b: unknown) -> bool throws never {
    $rust_function
}
