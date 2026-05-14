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

/// A first-class runtime-compiled package handle.
///
/// `reflect.Package.new()` allocates a fresh, empty runtime package. Items
/// (functions, classes, enums, top-level lets) are added later via
/// `pkg.add_compile({...})`, after which they're reachable through
/// `pkg.get<F>("name")` and `pkg.eval<T>("expr")`.
///
/// Runtime-compiled packages live on the GC heap and are collected when the
/// host drops its last handle. Static (build-time) packages — `baml`, `log`,
/// `reflect`, the host's `user` package — are constructed at engine load and
/// are not constructed via this class.
class Package {
    /// Internal handle to the underlying `Object::Package` heap primitive.
    ///
    /// This field exists so `reflect.Package` is a user-facing wrapper
    /// `Object::Instance` whose single field holds the primitive package
    /// value. Once the compiler grows a primitive `package` type the type
    /// here will be tightened from `unknown` to `package`; for now `unknown`
    /// is the most precise BAML type expressible.
    ///
    /// Direct access is not part of the public API — methods on `Package`
    /// unwrap it as needed (e.g. `add_compile`, `get`, `eval` in upcoming
    /// commits).
    _inner: unknown

    /// Allocate a fresh, empty runtime-compiled `Package`.
    ///
    /// Returns a `reflect.Package` instance wrapping a freshly-allocated
    /// `Object::Package` heap primitive. The package starts with no items
    /// and no globals; source files are added via `pkg.add_compile({...})`.
    //baml:mut_vm
    function new() -> reflect.Package {
        $rust_function
    }
}
