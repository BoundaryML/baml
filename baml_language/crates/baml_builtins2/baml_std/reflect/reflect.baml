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

/// One parameter of a function signature (BEP-062).
class Arg {
  /// The parameter's name; null when the value carries none.
  name string?
  /// The parameter's type.
  type type
}

/// The runtime signature of a function value, reconstructed from the value
/// itself (BEP-062). Positional (required) parameters appear in `args` in
/// declaration order; named/optional parameters appear in `opts`, keyed by
/// name. A bound method reports its receiver-adjusted signature (no `self`).
class Signature {
  /// Positional (required) parameters, in order.
  args Arg[]
  /// Named/optional parameters, keyed by parameter name.
  opts map<string, Arg>
  /// The return type.
  returns type
  /// The error type; `never` if the function cannot throw.
  errors type
  /// The function's doc comment (joined `///` lines); null when absent.
  docstring string?
}

/// Thrown by `reflect.call_any` when the supplied arguments do not fit the
/// callee's runtime signature: wrong positional arity, a named argument the
/// callee does not declare, or a value that does not fit its parameter's
/// type. For a single bad value, `expected`/`got` are the parameter's and the
/// value's types; for an arity or unknown-name mismatch they are the callee's
/// function type and the call's shape.
class InvalidArgumentError {
  expected type
  got type
}

/// Returns the runtime signature of `f`.
///
/// Usage:
///   let sig = reflect.signature(f)
///   sig.returns.to_string()
function signature(f: baml.AnyFunction) -> Signature throws never {
  $rust_function
}

/// Calls `f` with `args` (positional) and `opts` (named), checked at
/// runtime against `f`'s signature. An absent named argument fires the
/// callee's own default. `R` and `E` are `f`'s `Returns`/`Throws` pins, so a
/// pinned `AnyFunction<Returns = json, Throws = ToolError>` yields typed
/// results and an exhaustive catch; a bare `AnyFunction` returns `unknown`
/// and throws `unknown` (which covers `InvalidArgumentError`).
function call_any<R, E>(f: baml.AnyFunction<Returns = R, Throws = E>, args: unknown[], opts: map<string, unknown> = {}) -> R throws E | InvalidArgumentError {
  $rust_function
}
