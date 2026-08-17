/// Returns a deep copy of `value` — all nested structures are recursively cloned.
//baml:mut_vm
function deep_copy<T>(value: T) -> T throws never {
    $rust_function
}

/// Implemented by all concrete types.
/// Can be used to check or bound a type to only include concrete types.
interface Concrete {

}

implement<T> Concrete for T {

}

/// Implemented automatically by every function value, with its parameter
/// list erased. `Returns` pins the held function's return type and `Throws`
/// its error type; both default to `unknown`, so a bare `AnyFunction`
/// accepts any function. A compiler builtin: it cannot be implemented by
/// hand, cannot be used as a generic bound, and declares no methods. Call
/// the held function through `reflect.call_any`; read its signature with
/// `reflect.signature`.
interface AnyFunction {
  type Returns = unknown
  type Throws = unknown
}

/// The structured value a tagged template literal produces.
///
/// BEP-049 §10. A `tag` ` `…` ` call site lowers to `tag(body = () -> TaggedString { … })`;
/// when `body()` runs it returns this struct. The invariant is
/// `parts.length == values.length + 1` (literals and values alternate, starting and
/// ending with a literal — empty strings fill the leading/trailing slot when the
/// template begins or ends with an interpolation).
class TaggedString {
    /// Literal text segments between interpolations, in source order.
    parts: string[],
    /// Interpolated values, in source order. Heterogeneous: each entry has the
    /// type of its `${expr}` site; tags inspect element types at runtime.
    values: unknown[],
}

/// Builtin companion for the source-level `unknown` primitive.
/// Users never construct this class.
class Unknown {
  /// Assert-read a named field from a runtime-shaped class instance.
  /// This is the method spelling of `reflect.class.get_field<T>(self, name)`.
  function get_field<T>(self, name: string) -> T throws baml.reflect.errors.CompilationError {
    reflect.class.get_field<T>(self, name)
  }
}
