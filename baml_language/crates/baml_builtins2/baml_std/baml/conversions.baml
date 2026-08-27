/// Opt-in human-readable string conversion.
///
/// A type implements `baml.ToString` to control how `string.from(value)`
/// renders it. Conversion is total — `to_string` never throws.
///
/// ```
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
/// Unlike a magic method, `to_string` only exists on types that opt in.
/// `string.from` provides a default structural rendering for everything else
/// (see [`_to_string_default`]).
///
/// `to_string` has a default body — the same structural rendering `string.from`
/// falls back to for non-implementors — so `implements baml.ToString {}` with no
/// override is valid and renders identically to a non-implementor.
interface ToString {
  function to_string(self) -> string throws never {
    root._to_string_default(self)
  }
}

/// Renders a reflected type's name. Declared here, beside the interface it
/// implements, rather than in the `reflect` package: `reflect.Type` is the
/// compiler metatype (no package dependency arises from naming it), while
/// `reflect` deliberately references nothing from `baml`, keeping the stdlib
/// dependency graph acyclic by construction.
implement root.ToString for reflect.Type {
  function to_string(self) -> string throws never {
    self._to_string_impl()
  }
}

/// Internal: default structural rendering used by `string.from` for values
/// whose runtime type does not implement `baml.ToString`. Primitives render
/// naturally (`5`, `true`, `null`); containers and class instances render
/// structurally (`[1, 2]`, `Point { x: 1, y: 2 }`). A nested value whose runtime
/// class *does* implement `baml.ToString` is rendered via that override (so the
/// default honors overrides at every depth, like `string.from`). Never throws.
//baml:mut_vm
//baml:may_yield
function _to_string_default<T>(value: T) -> string throws never {
    $rust_function
}

/// Internal: BEP-042 `cleanup` run-once guard. Compiled into the prologue of a
/// user class's magic `cleanup(self) -> void` method as
/// `if (baml._cleanup_begin(self)) { <body> }`. Atomically test-and-sets the
/// receiver instance's per-instance "cleaned" latch and returns `true` iff this
/// is the *first* invocation (so the body should run); a second `cleanup` on the
/// same instance — from an explicit call, a `defer`, or (later) the GC finalizer
/// — returns `false`, so the body is skipped. The latch is set on entry, so a
/// `cleanup` that throws is still considered cleaned (BEP-042). For a non-instance
/// receiver it returns `true` defensively (the guard is only emitted for class
/// `cleanup` methods, whose `self` is always an instance).
//baml:vm
function _cleanup_begin<T>(value: T) -> bool throws never {
    $rust_function
}

/// Opt-in JSON conversion.
///
/// A type implements `baml.ToJson` to control how `baml.json.from(value)`
/// renders it. This is the json analog of `baml.ToString`: `baml.json.from`
/// provides a default structural rendering for non-implementors (primitives
/// natural, lists/maps/instances structural), and an implementor customizes its
/// own json shape.
///
/// ```
/// class Temp {
///   celsius float
///   implements baml.ToJson {
///     function to_json(self) -> baml.json.json throws baml.json.JsonSerializationError {
///       { "c": baml.json.from(self.celsius), "f": baml.json.from(self.celsius * 1.8 + 32.0) }
///     }
///   }
/// }
/// ```
///
/// Unlike `to_string`, conversion can fail: a value with no json representation
/// (`uint8array` without explicit encoding, function values, ...) throws
/// `baml.json.JsonSerializationError`.
///
/// `to_json` has a default body — the same structural rendering `baml.json.from`
/// falls back to for non-implementors — so `implements baml.ToJson {}` with no
/// override is valid and renders identically to a non-implementor.
interface ToJson {
  function to_json(self) -> baml.json.json throws baml.json.JsonSerializationError {
    root._to_json_default(self)
  }
}

/// Internal: default structural json rendering used by `baml.json.from` for
/// values whose runtime type does not implement `baml.ToJson`. Primitives render
/// naturally; containers and class instances render structurally (`[1, 2]`,
/// `{"field": value}`), enums as their variant name, media as the tagged form. A
/// nested value whose runtime class *does* implement `baml.ToJson` is rendered
/// via that override (so the default honors overrides at every depth, like
/// `baml.json.from`). Throws `JsonSerializationError` for non-representable values.
//baml:mut_vm
//baml:may_yield
function _to_json_default<T>(value: T) -> baml.json.json throws baml.json.JsonSerializationError {
    $rust_function
}

/// Internal: dispatch `to_json` on `value`'s *runtime* class, honoring a
/// user-defined override. `baml.json.from` routes through this rather than a
/// static `match (value) { baml.ToJson => value.to_json() }`, for the same reason
/// `string.from` uses [`_to_string_shim`]: static interface dispatch from this
/// (stdlib) package can only see implementors visible here, not downstream user
/// classes. Resolving on the receiver's runtime class instead picks up a user
/// `implements baml.ToJson { function to_json ... }` override; a class that
/// inherits the structural default body falls back to [`_to_json_default`].
///
/// TODO: temporary. Once Kai's TIR rework moves interface conformance checks to
/// runtime, `baml.json.from` can use the literal `is baml.ToJson` form and this
/// shim can be removed.
//baml:mut_vm
//baml:may_yield
function _to_json_shim<T>(value: T) -> baml.json.json throws baml.json.JsonSerializationError {
    $rust_function
}

/// Opt-in JSON deserialization — the inverse of `baml.ToJson`.
///
/// A type implements `baml.FromJson` to control how `baml.json.to<T>(j)` (and
/// JSON deserialization generally) constructs it from a `json` value. Unlike
/// `to_json`, `from_json` has no `self`: it is an associated constructor that
/// takes a `json` and returns `Self`. `baml.json.to` provides a default
/// structural decode for non-implementors (per-field, honoring nested
/// `baml.FromJson` overrides), so a type need only implement `baml.FromJson`
/// when its JSON shape differs from its structural field layout.
///
/// ```
/// class Temp {
///   celsius float
///   implements baml.FromJson {
///     function from_json(j: baml.json.json) -> Self throws baml.json.JsonDecodeError {
///       Temp { celsius: baml.json.to<float>(baml.json.field(j, "c")) }
///     }
///   }
/// }
/// ```
///
/// `from_json` is a *required* method (no default body): a type either
/// implements `baml.FromJson` to customize decoding, or relies on the structural
/// decode that `baml.json.to` provides for non-implementors. (Unlike `to_json`,
/// it can't have a default body — an interface default method may not return
/// `Self`; the structural fallback lives in `baml.json.to` instead.)
interface FromJson {
  function from_json(j: baml.json.json) -> Self throws baml.json.JsonDecodeError
}

/// Internal: dispatch `from_json` on the target type `T`, honoring a
/// user-defined override. `baml.json.to` routes through this rather than a
/// static `match` for the same package-boundary reason `baml.json.from` uses
/// [`_to_json_shim`]: static interface dispatch from this (stdlib) package can
/// only see implementors visible here, not downstream user classes. Resolving on
/// the runtime target type instead picks up a user `implements baml.FromJson {
/// function from_json ... }` override; a non-implementor falls back to the
/// per-field structural decode `baml.json.to` provides (there is no default
/// body — `FromJson.from_json` is a required method).
///
/// TODO: temporary. Once Kai's TIR rework moves interface conformance checks to
/// runtime, `baml.json.to` can use the literal `is baml.FromJson` form and this
/// shim can be removed.
//baml:mut_vm
//baml:may_yield
function _from_json_shim<T>(j: baml.json.json) -> T throws baml.json.JsonDecodeError {
    $rust_function
}
