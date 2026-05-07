/// Returns the runtime `type` value for the type argument `T`.
///
/// For concrete type arguments (no generic-parameter references), the `type`
/// value is materialised at compile time via a single `LoadType` instruction —
/// zero runtime overhead.
///
/// Usage:
///   let t: type = reflect.type_of<User>()
///   assert.equal(t.to_string(), "User")
///   assert.equal(reflect.type_of<int[]>().to_string(), "int[]")
///
/// The returned `type` value supports:
///   - `.to_string()` — returns the BAML type name (e.g. "User", "int[]", "map<string, User>")
///   - `==` / `!=`   — structural equality (two `type_of<User>()` are equal)
function type_of<T>() -> type {
    $compiler_intrinsic
}
