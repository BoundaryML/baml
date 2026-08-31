/// One parameter of a function signature (BEP-062).
class Arg {
    /// The parameter's name. When the callable carries no name for a
    /// positional parameter (a host callable from a language without
    /// parameter-name introspection), this is the placeholder `$argN` for
    /// position N. `$` cannot appear in a user identifier, so a placeholder
    /// can never collide with a declared parameter or named-argument key.
    name: string,
    /// The parameter's type.
    type: reflect.Type,
}

/// Metadata baked into reflected schema definitions.
class Meta {
    alias: string?,
    description: string?,
    docstring: string?,
    other: map<string, string>,
}

/// A value paired with schema metadata. For type values this lets metadata be
/// attached without mutating the minted definition.
class WithMeta<T> {
    ty: T,
    alias: string?,
    description: string?,
    docstring: string?,
    other: map<string, string>,
}

/// A source range attached to a compiler diagnostic. Compile- and eval-door
/// diagnostics carry their source range; structured construction diagnostics
/// report `span = null`, including constructors invoked from BAML.
class Span {
    file: string?,
    start: int,
    end: int,
}

/// The stable structured diagnostic surfaced by reflection constructors.
class Diagnostic {
    code: string,
    span: Span?,
    message: string,
}

/// Common identity-preserving surface shared by every reflection kind view.
interface TypeView {
  function as_type(self) -> reflect.Type throws never
}

/// The closed set of reflection views for a runtime `type` value.
type TypeKind = reflect.class.Type
    | reflect.enum.Type
    | reflect.union.Type
    | reflect.literal.Type
    | reflect.array.Type
    | reflect.map.Type
    | reflect.interface.Type
    | reflect.primitive.Type
    | reflect.function.Type;

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

/// A read-only reflected view implemented automatically by class values.
///
/// Narrow an opaque value with `let value: reflect.AnyClass = opaque else { ... }`
/// before using this surface. Ordinary class instances implement it, including
/// runtime-minted classes. Of the sealed reflection-kind views, only
/// `reflect.class.Type` implements it.
interface AnyClass {
  /// Read a field as `T`. A missing field returns `null`; use `has_field` to
  /// distinguish that case from a present field whose value is null.
  function get<T>(self, name: string) -> T? throws reflect.errors.TypeMismatch {
    if (!self.has_field(name)) {
      return null
    }
    // NOTE: spelled with `let … else` rather than `self.get_field(name)?.value<T>()`
    // because an optional-chained call does not thread `T` to the callee.
    let field: reflect.class.Field = self.get_field(name) else {
      return null
    }
    field.value<T>()
  }

  /// Return a value-bound field handle, or `null` when no such field exists.
  function get_field(self, name: string) -> reflect.class.Field? throws never {
    let class_type: reflect.class.Type = reflect.Type.of_value(self).as_class() else {
      return null
    }
    for (let field in class_type.fields()) {
      if (field.name == name) {
        return reflect.class.Field {
          name: field.name,
          type: field.type,
          meta: field.meta,
          _owner: self,
        }
      }
    }
    null
  }

  function has_field(self, name: string) -> bool throws never {
    let class_type: reflect.class.Type = reflect.Type.of_value(self).as_class() else {
      return false
    }
    for (let field in class_type.fields()) {
      if (field.name == name) {
        return true
      }
    }
    false
  }

  /// Reflected fields bound to this value for `Field.value<T>()`.
  function list_fields(self) -> reflect.class.Field[] throws never {
    let class_type: reflect.class.Type = reflect.Type.of_value(self).as_class() else {
      return []
    }
    let fields: reflect.class.Field[] = []
    for (let field in class_type.fields()) {
      fields.push(reflect.class.Field {
        name: field.name,
        type: field.type,
        meta: field.meta,
        _owner: self,
      })
    }
    fields
  }

  function name(self) -> string throws never {
    reflect.Type.of_value(self).to_string()
  }

  function type(self) -> reflect.Type throws never {
    reflect.Type.of_value(self)
  }

  function attributes(self) -> reflect.Meta throws never {
    let class_type: reflect.class.Type = reflect.Type.of_value(self).as_class() else {
      return reflect.Meta {
        alias: null,
        description: null,
        docstring: null,
        other: {},
      }
    }
    class_type.meta()
  }
}

implement<T> AnyClass for T {}

/// The runtime signature of a function value, reconstructed from the value
/// itself (BEP-062). Positional (required) parameters appear in `args` in
/// declaration order; named/optional parameters appear in `opts`, keyed by
/// name. A bound method reports its receiver-adjusted signature (no `self`).
class Signature {
    /// The name the function was declared with; null for host callables and
    /// compiler-synthesized callables (including lambdas).
    name: string?,
    /// Positional (required) parameters, in order.
    args: Arg[],
    /// Named/optional parameters, keyed by parameter name.
    opts: map<string, Arg>,
    /// The return type.
    returns: reflect.Type,
    /// The error type; `never` if the function cannot throw.
    errors: reflect.Type,
    /// The function's doc comment (joined `///` lines); null when absent.
    docstring: string?,
}

/// Thrown by `reflect.call_any` when the supplied arguments or returned value
/// do not fit its runtime signature. `argument` names the offending key
/// (`$argN` placeholders included); a return mismatch names the
/// `reflect.call_any return value` boundary. An ill-typed value reports the
/// expected and actual types; a missing required parameter reports the
/// parameter's type and `never`; a key naming no parameter reports the
/// callee's whole function type and the value's type.
class InvalidArgumentError {
    argument: string,
    expected: reflect.Type,
    got: reflect.Type,
}

/// One-shot compiler output consumed by Package._finish or Session._finish.
class CompileArtifact {
    _inner: unknown,
}

/// An isolated runtime-compiled package.
class Package {
    _inner: unknown,

    /// The package lexically enclosing this call site.
    function current() -> Package throws never {
        $compiler_intrinsic
    }

    function compile(
        files: map<string, string>,
        packages: map<string, Package> = {},
    ) -> Package throws unknown {
        let artifact = Package._compile(files, packages);
        Package._finish(artifact, packages)
    }

    function _compile(
        files: map<string, string>,
        packages: map<string, Package>,
    ) -> CompileArtifact throws reflect.errors.CompilationError {
        $rust_io_function
    }

    //baml:mut_vm
    //baml:may_yield
    function _finish(
        artifact: CompileArtifact,
        packages: map<string, Package>,
    ) -> Package throws unknown {
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
    function with_types(
        self,
        types: map<string, reflect.Type | reflect.TypeView>,
    ) -> Package throws reflect.errors.CompilationError {
        $rust_function
    }
    //baml:mut_vm
    /// Extract an exported function through an ordinary function-type contract.
    ///
    /// A lookup by name has nowhere to put type arguments, so an unspecialized
    /// generic function is refused here.
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
    /// Exported free-function signatures, keyed by package-local name.
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
    _inner: unknown,

    function new(
        packages: map<string, Package> = {},
    ) -> Session throws reflect.errors.CompilationError {
        Session._new(packages)
    }

    //baml:mut_vm
    function _new(
        packages: map<string, Package>,
    ) -> Session throws reflect.errors.CompilationError {
        $rust_function
    }

    function eval<T>(
        self,
        source: string,
    ) -> T throws reflect.errors.CompilationError | reflect.errors.EvaluationError | reflect.errors.SessionBusy {
        let artifact = self._compile<T>(source);
        self._finish<T>(artifact) catch (cause) {
            _ => {
                throw reflect.errors.EvaluationError {
          message: _render_cause(cause),
          cause: cause,
        }
            }
        }
    }

    function _compile<T>(
        self,
        source: string,
    ) -> CompileArtifact throws reflect.errors.CompilationError | reflect.errors.SessionBusy {
        $rust_io_function
    }

    //baml:mut_vm
    //baml:may_yield
    function _finish<T>(self, artifact: CompileArtifact) -> T throws unknown {
        $rust_function
    }

    //baml:mut_vm
    function diagnostics(self) -> Diagnostic[] throws never {
        $rust_function
    }
}

/// (internal) Structural error rendering for `Session.eval` that keeps `reflect` a root
/// package. This is the dependency-free counterpart of `string.from`: its VM
/// implementation shares the same renderer without importing `baml`.
//baml:mut_vm
//baml:may_yield
function _render_cause(value: unknown) -> string throws never {
    $rust_function
}

/// Returns the runtime signature of `f`.
///
/// Usage:
///   let sig = reflect.signature(f)
///   sig.returns.to_string()
//baml:mut_vm
//baml:fallible
function signature(f: AnyFunction) -> Signature throws never {
    $rust_function
}

/// Calls `f` with `args`, every argument keyed by parameter name (a nameless
/// positional is addressed by its `$argN` placeholder), checked at runtime
/// against `f`'s signature: a missing required parameter, a key naming no
/// parameter, or a value that does not fit its parameter's type throws
/// `InvalidArgumentError`. The returned value is checked against `R` at the
/// same boundary (except when `R` is `unknown`). An absent optional fires the
/// callee's own default. `R` and `E` are `f`'s `Returns`/`Throws` pins, so a
/// pinned `AnyFunction<Returns = json, Throws = ToolError>` yields typed
/// results and an exhaustive catch; a bare `AnyFunction` returns `unknown`
/// and throws `unknown` (which covers `InvalidArgumentError` and
/// `reflect.errors.CompilationError`).
//baml:mut_vm
//baml:may_yield
function call_any<R, E>(
    f: AnyFunction<Returns = R, Throws = E>,
    args: map<string, unknown>,
) -> R throws E | InvalidArgumentError | reflect.errors.CompilationError {
    $rust_function
}
