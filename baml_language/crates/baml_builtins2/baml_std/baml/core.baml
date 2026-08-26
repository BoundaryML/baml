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
