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
/// `reflect` package: `reflect.Type` is the compiler metatype, so naming it
/// creates no package dependency, while `reflect` deliberately references
/// nothing from `baml`. That keeps the stdlib dependency graph acyclic by
/// construction.
implement root.ToString for reflect.Type {
  function to_string(self) -> string throws never {
    self._to_string_impl()
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
    function to_json(self) -> baml.json.json throws baml.json.SerializationError {
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
function _to_json_default<T>(value: T) -> baml.json.json throws baml.json.SerializationError {
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
    function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError;
}

/// Decode the root structurally while honoring nested FromJson implementations.
//baml:mut_vm
//baml:may_yield
function _from_json_structural_default<T>(j: baml.json.json) -> T throws baml.json.DecodeError {
    $rust_function
}

/// Compare structurally in one cycle-aware walk, honoring nested Equals overrides.
//baml:mut_vm
//baml:may_yield
function _equals_structural_default(a: unknown, b: unknown) -> bool throws never {
    $rust_function
}
