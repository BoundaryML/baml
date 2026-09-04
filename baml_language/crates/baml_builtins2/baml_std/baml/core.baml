/// Returns a deep copy of `value`: nested arrays, maps, and class instances
/// are cloned recursively.
///
/// Sharing and cycles survive the copy. Two references to the same nested
/// object still point at one copied object afterward, rather than becoming
/// independent copies. `Future` is the exception — a future is a handle onto
/// live async state the runtime owns, not a value with contents to clone, so a
/// future nested inside `value` is shared, not duplicated.
//baml:mut_vm
function deep_copy<T>(value: T) -> T throws never {
    $rust_function
}

/// Implemented by all concrete types.
///
/// Use it to test whether a type is concrete, or as a bound restricting a type
/// parameter to concrete types.
interface Concrete {

}

implement<T> Concrete for T {

}

/// The structured value a tagged template literal produces.
///
/// A tagged call site `` tag`…` `` lowers to
/// `tag(body = () -> TaggedString { … })`; running `body()` returns this
/// struct. Literals and values alternate, beginning and ending with a literal,
/// so `parts.length == values.length + 1` — an empty string fills the leading
/// or trailing slot when the template starts or ends with an interpolation.
/// See BEP-049 §10.
class TaggedString {
    /// Literal text segments between interpolations, in source order.
    parts: string[],
    /// Interpolated values, in source order. Each entry has the type of its
    /// `${expr}` site, so the array is heterogeneous; tags inspect element
    /// types at runtime.
    values: unknown[],
}
