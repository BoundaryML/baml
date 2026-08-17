/// Returns the runtime `type` value for the type argument `T` (BEP-066 K-13).
///
/// Spelled `type.of<T>()`: `type` is the keyword shorthand for this
/// `baml.type` namespace, exactly as `reflect` is for `baml.reflect`.
///
/// For concrete type arguments (no generic-parameter references), the `type`
/// value is materialised at compile time via a single `LoadType` instruction —
/// zero runtime overhead.
///
/// Usage:
///   let t: type = type.of<User>()
///   assert.equal(t.to_string(), "User")
///   assert.equal(type.of<int[]>().to_string(), "int[]")
///
/// The returned `type` value supports:
///   - `.to_string()` — returns the BAML type name (e.g. "User", "int[]", "map<string, User>")
///   - `==` / `!=`   — minted identity equality (canonical static values of
///                     the same type compare equal)
function of<T>() -> type {
    $compiler_intrinsic
}

/// Returns the runtime `type` value describing the concrete type of `v`
/// (BEP-066 K-13).
///
/// The reconstructed type is the value's runtime type, never a literal type
/// (K-12): `type.of_value(5)` is `int`, not `5`. Arrays and maps report the
/// element/key/value types they carry (`type.of_value([1])` is `int[]`);
/// function values report their reconstructed signature type; a `type` value
/// reports `type` itself.
///
/// A value with no reconstructable BAML type — a compile-time definition
/// object (a package, class, enum, interface, or impl rule) or an opaque
/// native handle — yields the `unknown` type value, mirroring how
/// `reflect.signature` / `reflect.call_any` treat unreconstructable types.
///
/// Usage:
///   assert.equal(type.of_value(5), type.of<int>())
///   assert.equal(type.of_value("hi").to_string(), "string")
//baml:mut_vm
//baml:fallible
function of_value(v: unknown) -> type throws never {
  $rust_function
}
