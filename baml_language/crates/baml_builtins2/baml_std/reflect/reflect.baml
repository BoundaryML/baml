/// One parameter of a function signature (BEP-062).
class Arg {
  /// The parameter's name. When the callable carries no name for a
  /// positional parameter (a host callable from a language without
  /// parameter-name introspection), this is the placeholder `$argN` for
  /// position N. `$` cannot appear in a user identifier, so a placeholder
  /// can never collide with a declared parameter or named-argument key.
  name string
  /// The parameter's type.
  type reflect.Type
}

/// Metadata baked into reflected schema definitions.
class Meta {
  alias string?
  description string?
  docstring string?
  other map<string, string>
}

/// A value paired with schema metadata. For type values this lets metadata be
/// attached without mutating the minted definition.
class WithMeta<T> {
  ty T
  alias string?
  description string?
  docstring string?
  other map<string, string>
}

/// A source range attached to a compiler diagnostic. Compile- and eval-door
/// diagnostics carry their source range; structured construction diagnostics
/// report `span = null`, including constructors invoked from BAML.
class Span {
  file string?
  start int
  end int
}

/// The stable structured diagnostic surfaced by reflection constructors.
class Diagnostic {
  code string
  span Span?
  message string
}

/// Common identity-preserving surface shared by every reflection kind view.
interface TypeView {
  function as_type(self) -> reflect.Type throws never
}

/// The closed set of reflection views for a runtime `type` value.
type TypeKind = reflect.class.Type | reflect.enum.Type | reflect.union.Type | reflect.literal.Type | reflect.array.Type | reflect.map.Type | reflect.interface.Type | reflect.primitive.Type | reflect.function.Type

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
  returns reflect.Type
  /// The error type; `never` if the function cannot throw.
  errors reflect.Type
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
  expected reflect.Type
  got reflect.Type
}

/// An isolated runtime-compiled package.
class Package {
  _inner unknown

  /// The package lexically enclosing this call site.
  function current() -> Package throws never {
    $compiler_intrinsic
  }

  function compile(files: map<string, string>, packages: map<string, Package> = {}) -> Package throws unknown {
    let artifact = Package._compile(files, packages)
    Package._finish(artifact, packages)
  }

  function _compile(files: map<string, string>, packages: map<string, Package>) -> Package throws reflect.errors.CompilationError {
    $rust_io_function
  }

  //baml:mut_vm
  //baml:may_yield
  function _finish(artifact: Package, packages: map<string, Package>) -> Package throws unknown {
    $rust_function
  }

  //baml:mut_vm
  function get_class(self, name: string) -> reflect.class.Type? throws never {
    $rust_function
  }

  //baml:mut_vm
  function get_enum(self, name: string) -> reflect.enum.Type? throws never {
    $rust_function
  }

  //baml:mut_vm
  function get_interface(self, name: string) -> reflect.interface.Type? throws never {
    $rust_function
  }

  /// Return a derived package view that exports exact runtime type values
  /// under source-visible names for subsequent Package.compile calls.
  //baml:mut_vm
  function with_types(self, types: map<string, reflect.Type | reflect.TypeView>) -> Package throws reflect.errors.CompilationError {
    $rust_function
  }
  //baml:mut_vm
  /// Extract an exported function through an ordinary function-type contract.
  ///
  /// A lookup by name has nowhere to put type arguments, so an unspecialized
  /// generic function is refused here. Reach it through `functions()` instead:
  /// the descriptor there takes `specialize(args)` and hands the specialized
  /// callable back through its own `get<F>()`.
  function get_function<F>(self, name: string) -> F? throws reflect.errors.CompilationError {
    $rust_function
  }

  //baml:mut_vm
  function classes(self) -> map<string, reflect.class.Type> throws never {
    $rust_function
  }

  //baml:mut_vm
  function enums(self) -> map<string, reflect.enum.Type> throws never {
    $rust_function
  }

  //baml:mut_vm
  function interfaces(self) -> map<string, reflect.interface.Type> throws never {
    $rust_function
  }

  //baml:mut_vm
  /// Exported free-function descriptors, keyed by package-local name.
  ///
  /// Every declared function is listed, including generic ones and their
  /// generated companions. A generic entry reports `is_generic() == true` and
  /// has no readable signature until `specialize(args)` supplies its type
  /// arguments; the specialized descriptor's `get<F>()` yields the callable.
  function functions(self) -> map<string, reflect.function.Type> throws never {
    $rust_function
  }

  /// Bundled tests as directly invocable zero-argument callables.
  //baml:mut_vm
  //baml:may_yield
  function tests(self) -> map<string, () -> null throws unknown> throws unknown {
    $rust_function
  }

  //baml:mut_vm
  function diagnostics(self) -> Diagnostic[] throws never {
    $rust_function
  }
}

/// A live incremental evaluation environment. Its dependency aliases are
/// fixed at construction; every eval checks in a fresh compiler context and
/// commits successful statements to this Session's persistent runtime image.
class Session {
  _inner unknown

  function new(packages: map<string, Package> = {}) -> Session throws reflect.errors.CompilationError {
    Session._new(packages)
  }

  //baml:mut_vm
  function _new(packages: map<string, Package>) -> Session throws reflect.errors.CompilationError {
    $rust_function
  }

  function eval<T>(self, source: string) -> T throws reflect.errors.CompilationError | reflect.errors.EvaluationError | reflect.errors.SessionBusy {
    let artifact = self._compile<T>(source)
    self._finish<T>(artifact) catch (_) {
      _ => throw reflect.errors.EvaluationError { message: "session evaluation failed" }
    }
  }

  function _compile<T>(self, source: string) -> Package throws reflect.errors.CompilationError | reflect.errors.SessionBusy {
    $rust_io_function
  }

  //baml:mut_vm
  //baml:may_yield
  function _finish<T>(self, artifact: Package) -> T throws unknown {
    $rust_function
  }

  //baml:mut_vm
  function diagnostics(self) -> Diagnostic[] throws never {
    $rust_function
  }
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
/// throws `unknown` (which covers `InvalidArgumentError` and
/// `reflect.errors.CompilationError`).
//baml:mut_vm
//baml:may_yield
function call_any<R, E>(f: baml.AnyFunction<Returns = R, Throws = E>, args: map<string, unknown>) -> R throws E | InvalidArgumentError | reflect.errors.CompilationError {
  $rust_function
}
