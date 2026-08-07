/// One parameter of a function signature (BEP-062).
class Arg {
  /// The parameter's name. When the callable carries no name for a
  /// positional parameter (a host callable from a language without
  /// parameter-name introspection), this is the placeholder `$argN` for
  /// position N. `$` cannot appear in a user identifier, so a placeholder
  /// can never collide with a declared parameter or named-argument key.
  name string
  /// The parameter's type.
  type type
}

/// Metadata baked into reflected schema definitions.
class Meta {
  alias string?
  description string?
  docstring string?
  other map<string, string>
}

/// Common identity-preserving surface shared by every reflection kind view.
interface TypeView {
  function as_type(self) -> type throws never
}

/// The closed set of reflection views for a runtime `type` value.
type TypeKind = baml.reflect.class.Type | baml.reflect.enum.Type | baml.reflect.union.Type | baml.reflect.literal.Type | baml.reflect.array.Type | baml.reflect.map.Type | baml.reflect.interface.Type | baml.reflect.primitive.Type | baml.reflect.function.Type

/// The runtime signature of a function value, reconstructed from the value
/// itself (BEP-062). Positional (required) parameters appear in `args` in
/// declaration order; named/optional parameters appear in `opts`, keyed by
/// name. A bound method reports its receiver-adjusted signature (no `self`).
class Signature {
  /// The name the function was declared with; null for host callables and
  /// compiler-synthesized callables (including lambdas).
  name string?
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
/// callee's runtime signature. `argument` names the offending key (`$argN`
/// placeholders included). An ill-typed value reports the parameter's and
/// the value's types; a missing required parameter reports the parameter's
/// type and `never`; a key naming no parameter reports the callee's whole
/// function type and the value's type.
class InvalidArgumentError {
  argument string
  expected type
  got type
}

/// Returns the runtime signature of `f`.
///
/// Usage:
///   let sig = reflect.signature(f)
///   sig.returns.to_string()
//baml:mut_vm
//baml:fallible
function signature(f: baml.AnyFunction) -> Signature throws never {
  $rust_function
}

/// Calls `f` with `args`, every argument keyed by parameter name (a nameless
/// positional is addressed by its `$argN` placeholder), checked at runtime
/// against `f`'s signature: a missing required parameter, a key naming no
/// parameter, or a value that does not fit its parameter's type throws
/// `InvalidArgumentError`. An absent optional fires the callee's own
/// default. `R` and `E` are `f`'s `Returns`/`Throws` pins, so a pinned
/// `AnyFunction<Returns = json, Throws = ToolError>` yields typed results
/// and an exhaustive catch; a bare `AnyFunction` returns `unknown` and
/// throws `unknown` (which covers `InvalidArgumentError`).
//baml:mut_vm
//baml:may_yield
function call_any<R, E>(f: baml.AnyFunction<Returns = R, Throws = E>, args: map<string, unknown>) -> R throws E | InvalidArgumentError {
  $rust_function
}
