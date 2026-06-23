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

/// Opt-in parsing of a string into a value — the inverse of `baml.ToString`.
///
/// A type implements `baml.FromString` to control how `string.to<T>(s)` parses
/// it back from text. Unlike `to_string`, there is no structural default:
/// `string.to<T>` parses the primitive types directly, and otherwise requires
/// `T` to implement `baml.FromString`. A parse failure throws
/// `baml.errors.ParseError`.
///
/// ```
/// class Point {
///   x int
///   y int
///   implements baml.FromString {
///     function from_string(s: string) -> Self throws baml.errors.ParseError {
///       // parse "(x, y)" back into a Point
///     }
///   }
/// }
///
/// string.to<Point>("(1, 2)")  // Point { x: 1, y: 2 }
/// ```
interface FromString {
  function from_string(s: string) -> Self throws baml.errors.ParseError
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

/// Internal: dispatch `to_string` on `value`'s *runtime* class, honoring a
/// user-defined override. `string.from` routes through this rather than a static
/// `match (value) { baml.ToString => value.to_string() }`, for the same reason
/// `Sortable.sort` uses [`_compare_shim`]: static interface dispatch from this
/// (stdlib) package can only see implementors visible here, not downstream user
/// classes. Resolving on the receiver's runtime class instead picks up a user
/// `implements baml.ToString { function to_string ... }` override; a class that
/// inherits the structural default body falls back to [`_to_string_default`].
///
/// TODO: temporary. Once Kai's TIR rework moves interface conformance checks to
/// runtime, `string.from` can use the literal `is baml.ToString` form and this
/// shim can be removed.
//baml:mut_vm
//baml:may_yield
function _to_string_shim<T>(value: T) -> string throws never {
  $rust_function
}
