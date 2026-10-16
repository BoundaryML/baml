/// One parameter of a function signature (BEP-062).
class Arg {
    /// The parameter's name. When the callable carries no name for a
    /// positional parameter (a host callable from a language without
    /// parameter-name introspection), this is the placeholder `$argN` for
    /// position N. `$` cannot appear in a user identifier, so a placeholder
    /// can never collide with a declared parameter or named-argument key.
    name: string,
    /// The parameter's type.
    type: root.Type,
}

/// Metadata baked into reflected schema definitions.
class Meta {
    alias: string?,
    description: string?,
    docstring: string?,
    other: map<string, string>,
    /// True when a class field or enum variant is marked `@skip`: it is left
    /// out of prompts, output schemas and parsing. Always false for the
    /// metadata of a class or enum itself.
    skip: bool,
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

/// A UTF-8 byte range inside a diagnostic headline or annotation label.
class DiagnosticHighlight {
    start: int,
    end: int,
    kind: "identifier.type"
        | "identifier.function"
        | "identifier.field"
        | "identifier.variable"
        | "identifier.enum_variant"
        | "identifier.attribute"
        | "type_expression"
        | "code",
}

/// A primary or secondary source annotation. `message = null` represents a
/// span-only annotation.
class DiagnosticAnnotation {
    span: Span,
    message: string?,
    message_highlights: DiagnosticHighlight[],
    is_primary: bool,
}

/// Additional source information associated with a diagnostic.
class DiagnosticRelatedInfo {
    span: Span,
    message: string,
    message_highlights: DiagnosticHighlight[],
    file_path: string?,
}

/// The stable structured diagnostic surfaced by reflection constructors and
/// runtime compilation. `message` retains the historical flattened
/// headline-plus-primary-label representation; use `headline` and
/// `primary_label` when those parts must be inspected independently.
class Diagnostic {
    code: string,
    span: Span?,
    message: string,
    /// Severity retained from the compiler diagnostic stream.
    severity: "error" | "warning" | "info",
    /// Compiler stage that emitted a source diagnostic. Runtime-generated
    /// diagnostics which do not belong to a compiler stage report `null`.
    phase: ("parse" | "hir" | "validation" | "type")?,
    headline: string,
    primary_label: string?,
    message_highlights: DiagnosticHighlight[],
    annotations: DiagnosticAnnotation[],
    related_info: DiagnosticRelatedInfo[],
}

/// Any `type` value: `reflect.Type` itself and every reflection kind view.
///
/// Every reflection API that takes a type takes a `TypeView`, so a kind view
/// goes wherever a `reflect.Type` does, and so does a class of your own that
/// implements `TypeView`: the API calls its `as_type()`. The shared type
/// operations below are default methods that run on the wrapped `type` value;
/// `reflect.Type` declares its own. `to_string` comes from `baml.ToString`,
/// which `reflect.Type` and every kind view implement.
interface TypeView {
    /// The `type` value this view wraps, restoring the unnarrowed type.
    /// `reflect.Type` returns itself.
    function as_type(self) -> root.Type throws never;

    /// Render deterministic canonical BAML source for this definition.
    /// Non-equivalent declarations with the same display name are rejected.
    function to_baml(self) -> string throws root.errors.CompilationError {
        self.as_type().to_baml()
    }

    /// Construct an array type whose element is this type.
    function array(self) -> root.Array throws never {
        self.as_type().array()
    }

    /// Construct the canonical union of this type and null.
    function optional(self) -> root.Union throws never {
        self.as_type().optional()
    }

    /// Returns the precise kind view of this type value.
    function kind(self) -> root.TypeKind throws never {
        self.as_type().kind()
    }

    /// Pair this type value with schema metadata without changing its identity.
    function meta(
        self,
        alias: string? = null,
        description: string? = null,
        docstring: string? = null,
        other: map<string, string>? = null,
    ) -> root.WithMeta<root.Type> throws never {
        self.as_type().meta(
            alias = alias,
            description = description,
            docstring = docstring,
            other = other,
        )
    }

    /// True iff this type nominally implements the interface `other`. Any
    /// concrete type can: a class, an enum, a primitive, a list, a map, ...
    /// `other` pins the interface's associated types (an omitted one takes
    /// its default), and only an implementation with exactly those matches:
    /// see `reflect.Type.implements`. `false` when `other` is not an
    /// interface, or when this type is not a concrete type.
    function implements(self, other: root.TypeView) -> bool throws never {
        self.as_type().implements(other)
    }

    /// Reverse of `implements`: `iface.implemented_by(t)` is the same as
    /// `t.implements(iface)`. `false` when this type is not an interface, or
    /// when `other` is not a concrete type.
    function implemented_by(self, other: root.TypeView) -> bool throws never {
        self.as_type().implemented_by(other)
    }
}

/// (internal) `types` with each view replaced by the `type` value it views.
///
/// Every API that takes a `TypeView` converts it in BAML before it reaches a
/// native, so a class of the caller's own that implements `TypeView` runs
/// its own `as_type()`, and the native only ever receives `type` values.
function _as_types(types: root.TypeView[]) -> root.Type[] throws never {
    let out: root.Type[] = [];
    // NOTE: an index loop, not `for ... in`; see `AnyClass.get_field`.
    for (let i = 0; i < types.length(); i += 1) {
        out.push(types[i].as_type());
    }
    out
}

/// (internal) Class field rows with each view replaced by the `type` value it
/// views, and each `WithMeta` row kept as is; see `_as_types`.
function _as_field_rows(
    fields: map<string, root.TypeView | root.WithMeta<root.Type>>,
) -> map<string, root.Type | root.WithMeta<root.Type>> throws never {
    let rows: map<string, root.Type | root.WithMeta<root.Type>> = {};
    // NOTE: an index loop, not `for ... in`; see `AnyClass.get_field`.
    let names = fields.keys();
    for (let i = 0; i < names.length(); i += 1) {
        let name = names[i];
        rows[name] = match (fields[name]) {
            let view: root.TypeView => view.as_type(),
            let row => row,
        };
    }
    rows
}

/// The closed set of reflection views for a runtime `type` value.
type TypeKind = root.Class
    | root.Enum
    | root.Union
    | root.Literal
    | root.Array
    | root.Map
    | root.Interface
    | root.Primitive
    | root.Function;

/// Implemented automatically by every function value, with its parameter
/// list erased. `Returns` pins the held function's return type and `Throws`
/// its error type; both default to `unknown`, so a bare `AnyFunction`
/// accepts any function. A compiler builtin: it cannot be implemented by
/// hand, cannot be used as a generic bound, and declares no methods. Call
/// the held function through `reflect.call_any`; read its signature with
/// `reflect.signature`.
interface AnyFunction {
    /// The held function's return type. `unknown` accepts any return.
    type Returns = unknown;
    /// The held function's error type. `unknown` accepts any throws clause.
    type Throws = unknown;
}

/// A read-only reflected view implemented automatically by class values.
///
/// Narrow an opaque value with `let value: reflect.AnyClass = opaque else { ... }`
/// before using this surface. Ordinary class instances implement it, including
/// runtime-minted classes. Of the sealed reflection-kind views, only
/// `reflect.Class` implements it.
interface AnyClass {
    /// Read the field called `name` as `T`. Read a nullable field with a
    /// nullable `T` (`get<int?>`). `find` returns `null` for a missing field
    /// instead of throwing.
    ///
    /// # Throws
    ///
    /// - `reflect.errors.NotFound` if this value's class declares no field
    ///   called `name`.
    /// - `reflect.errors.TypeMismatch` if the field's value is not a `T`.
    function get<T>(
        self,
        name: string,
    ) -> T throws root.errors.NotFound | root.errors.TypeMismatch {
        if (!self.has_field(name)) {
            // NOTE: `_to_string_impl()`, not `to_string()`; see `name`.
            throw root.errors.NotFound {
                message: "no field named " + name + " on " + root.Type.of_value(self)._to_string_impl(),
            };
        }
        root.Class.get_field<T>(self, name) catch (e) {
            root.errors.CompilationError { message } => {
                throw root.errors.TypeMismatch { message: message }
            }
        }
    }

    /// Read the field called `name` as `T`, or `null` if this value's class
    /// declares no such field. A present field holding null also reads as
    /// `null`; use `has_field` or `get<T?>` to tell the two apart.
    ///
    /// # Throws
    ///
    /// - `reflect.errors.TypeMismatch` if the field's value is not a `T`.
    function find<T>(self, name: string) -> T? throws root.errors.TypeMismatch {
        if (!self.has_field(name)) {
            return null;
        }
        self.get<T>(name) catch (e) {
            root.errors.NotFound => null
        }
    }

    /// A value-bound handle to the field called `name`. `find_field` returns
    /// `null` instead of throwing.
    ///
    /// # Throws
    ///
    /// - `reflect.errors.NotFound` if this value's class declares no field
    ///   called `name`.
    function get_field(self, name: string) -> root.class.Field throws root.errors.NotFound {
        let field: root.class.Field = self.find_field(name) else {
            // NOTE: `_to_string_impl()`, not `to_string()`; see `name`.
            throw root.errors.NotFound {
                message: "no field named " + name + " on " + root.Type.of_value(self)._to_string_impl(),
            };
        };
        field
    }

    /// A value-bound handle to the field called `name`, or `null` if this
    /// value's class declares no such field.
    function find_field(self, name: string) -> root.class.Field? throws never {
        let class_type: root.Class = root.Type.of_value(self).as_class() else {
            return null;
        };
        // NOTE: an index loop, not `for (let field in ...)`. `reflect` must not
        // depend on `baml`, and code generation looks up the list's `Iterable`
        // implementation (declared in `baml`) only in the packages this one
        // depends on: a `for ... in` over a list type-checks here but fails to
        // compile. Methods on the list itself (`length`, indexing) need no
        // dependency.
        let class_fields = class_type.fields();
        for (let i = 0; i < class_fields.length(); i += 1) {
            let field = class_fields[i];
            if (field.name == name) {
                return root.class.Field {
                    name: field.name,
                    type: field.type,
                    meta: field.meta,
                    _owner: self,
                };
            }
        }
        null
    }

    /// Whether this value's runtime class declares a field called `name`.
    /// `false` when the runtime type is not a class.
    function has_field(self, name: string) -> bool throws never {
        let class_type: root.Class = root.Type.of_value(self).as_class() else {
            return false;
        };
        // NOTE: an index loop, not `for ... in`; see `get_field`.
        let class_fields = class_type.fields();
        for (let i = 0; i < class_fields.length(); i += 1) {
            let field = class_fields[i];
            if (field.name == name) {
                return true;
            }
        }
        false
    }

    /// Reflected fields bound to this value for `Field.value<T>()`.
    function list_fields(self) -> root.class.Field[] throws never {
        let class_type: root.Class = root.Type.of_value(self).as_class() else {
            return [];
        };
        let fields: root.class.Field[] = [];
        // NOTE: an index loop, not `for ... in`; see `get_field`.
        let class_fields = class_type.fields();
        for (let i = 0; i < class_fields.length(); i += 1) {
            let field = class_fields[i];
            fields.push(
                root.class.Field {
                    name: field.name,
                    type: field.type,
                    meta: field.meta,
                    _owner: self,
                },
            )
        }
        fields
    }

    /// The rendered name of this value's concrete runtime type.
    function name(self) -> string throws never {
        // NOTE: `_to_string_impl()`, not `to_string()`. `reflect.Type`'s
        // `baml.ToString` implementation is declared in `baml`, which `reflect`
        // must not depend on, so here `to_string()` would silently bind to the
        // interface's default body instead and crash the VM.
        root.Type.of_value(self)._to_string_impl()
    }

    /// This value's concrete runtime type.
    function type(self) -> root.Type throws never {
        root.Type.of_value(self)
    }

    /// The class declaration's schema metadata, or empty metadata when the
    /// runtime type is not a class.
    function attributes(self) -> root.Meta throws never {
        let class_type: root.Class = root.Type.of_value(self).as_class() else {
            return root.Meta { alias: null, description: null, docstring: null, other: {}, skip: false };
        };
        class_type.metadata()
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
    returns: root.Type,
    /// The error type; `never` if the function cannot throw.
    errors: root.Type,
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
    expected: root.Type,
    got: root.Type,
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

    /// Compiles `files` into an isolated package, resolving its dependencies
    /// against `packages`.
    ///
    /// # Parameters
    ///
    /// - `files`: source text keyed by path, as the package's `baml_src`.
    /// - `packages`: packages the source may depend on, keyed by the alias it
    ///   imports them under.
    ///
    /// # Throws
    ///
    /// - `reflect.errors.CompilationError` if the source does not compile.
    /// - anything the package's top-level initialization throws, which is why
    ///   the clause is `unknown`.
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
    ) -> CompileArtifact throws root.errors.CompilationError {
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

    /// The exported class called `name`. A name the package exports as some
    /// other kind of declaration is a miss. `find_class` returns `null`
    /// instead of throwing.
    ///
    /// # Throws
    ///
    /// - `reflect.errors.NotFound` if the package exports no class called
    ///   `name`.
    //baml:mut_vm
    function get_class(self, name: string) -> root.Class throws root.errors.NotFound {
        $rust_function
    }

    /// The exported class called `name`, or `null` if the package exports
    /// none.
    //baml:mut_vm
    function find_class(self, name: string) -> root.Class? throws never {
        $rust_function
    }

    /// The exported enum called `name`. A name the package exports as some
    /// other kind of declaration is a miss. `find_enum` returns `null` instead
    /// of throwing.
    ///
    /// # Throws
    ///
    /// - `reflect.errors.NotFound` if the package exports no enum called
    ///   `name`.
    //baml:mut_vm
    function get_enum(self, name: string) -> root.Enum throws root.errors.NotFound {
        $rust_function
    }

    /// The exported enum called `name`, or `null` if the package exports none.
    //baml:mut_vm
    function find_enum(self, name: string) -> root.Enum? throws never {
        $rust_function
    }

    /// The exported interface called `name`. A name the package exports as
    /// some other kind of declaration is a miss. `find_interface` returns
    /// `null` instead of throwing.
    ///
    /// # Throws
    ///
    /// - `reflect.errors.NotFound` if the package exports no interface called
    ///   `name`.
    //baml:mut_vm
    function get_interface(self, name: string) -> root.Interface throws root.errors.NotFound {
        $rust_function
    }

    /// The exported interface called `name`, or `null` if the package exports
    /// none.
    //baml:mut_vm
    function find_interface(self, name: string) -> root.Interface? throws never {
        $rust_function
    }

    /// A package that exports everything this one does, plus `types` under
    /// the given names, for a later `Package.compile` to mount: a bare class,
    /// enum, interface, or alias declaration is re-exported under its name
    /// (the same declaration, so a consumer that mounts both packages sees
    /// one type), any other type is declared as an alias, and every
    /// declaration a mounted type reaches is exported under its own name. A
    /// name this package already exports is refused.
    function with_types(
        self,
        types: map<string, root.TypeView>,
    ) -> Package throws root.errors.CompilationError {
        let mounted: map<string, root.Type> = {};
        // NOTE: an index loop, not `for ... in`; see `AnyClass.get_field`.
        let names = types.keys();
        for (let i = 0; i < names.length(); i += 1) {
            mounted[names[i]] = types[names[i]].as_type();
        }
        self._with_types(mounted)
    }

    /// (internal) Internal native backing `with_types`; every type is a `type`
    /// value.
    //baml:mut_vm
    function _with_types(
        self,
        types: map<string, root.Type>,
    ) -> Package throws root.errors.CompilationError {
        $rust_function
    }
    //baml:mut_vm
    /// Extract an exported function through an ordinary function-type contract.
    /// `find_function` returns `null` for a missing name instead of throwing.
    ///
    /// A lookup by name has nowhere to put type arguments, so an unspecialized
    /// generic function is refused here.
    ///
    /// # Throws
    ///
    /// - `reflect.errors.NotFound` if the package exports no function called
    ///   `name`.
    /// - `reflect.errors.CompilationError` if the function exists but is not
    ///   an `F`, or is an unspecialized generic.
    function get_function<F>(
        self,
        name: string,
    ) -> F throws root.errors.NotFound | root.errors.CompilationError {
        $rust_function
    }

    //baml:mut_vm
    /// Extract an exported function through an ordinary function-type
    /// contract, or `null` if the package exports no function called `name`.
    ///
    /// # Throws
    ///
    /// - `reflect.errors.CompilationError` if the function exists but is not
    ///   an `F`, or is an unspecialized generic.
    function find_function<F>(self, name: string) -> F? throws root.errors.CompilationError {
        $rust_function
    }

    /// Exported class types, keyed by package-local name.
    //baml:mut_vm
    function classes(self) -> map<string, root.Class> throws never {
        $rust_function
    }

    /// Exported enum types, keyed by package-local name.
    //baml:mut_vm
    function enums(self) -> map<string, root.Enum> throws never {
        $rust_function
    }

    /// Exported interface types, keyed by package-local name.
    //baml:mut_vm
    function interfaces(self) -> map<string, root.Interface> throws never {
        $rust_function
    }

    //baml:mut_vm
    /// Exported free-function signatures, keyed by package-local name.
    function functions(self) -> map<string, root.Function> throws never {
        $rust_function
    }

    /// Bundled tests as directly invocable zero-argument callables.
    //baml:mut_vm
    //baml:may_yield
    function tests(self) -> map<string, () -> null throws unknown> throws unknown {
        $rust_function
    }

    /// Diagnostics produced while compiling this package.
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

    /// Opens a session whose dependency aliases are fixed to `packages`.
    ///
    /// # Throws
    ///
    /// - `reflect.errors.CompilationError` if the dependencies do not resolve.
    function new(
        packages: map<string, Package> = {},
    ) -> Session throws root.errors.CompilationError {
        Session._new(packages)
    }

    //baml:mut_vm
    function _new(packages: map<string, Package>) -> Session throws root.errors.CompilationError {
        $rust_function
    }

    /// Compiles and runs `source` against this session, returning its value
    /// as `T`.
    ///
    /// Each call checks in a fresh compiler context; statements that succeed
    /// are committed to the session's persistent image and are visible to
    /// later calls. A call that throws commits nothing.
    ///
    /// # Throws
    ///
    /// - `reflect.errors.CompilationError` if `source` does not compile.
    /// - `reflect.errors.EvaluationError` if it compiles but throws when run.
    /// - `reflect.errors.SessionBusy` if the session is already evaluating.
    function eval<T>(
        self,
        source: string,
    ) -> T throws root.errors.CompilationError | root.errors.EvaluationError | root.errors.SessionBusy {
        let artifact = self._compile<T>(source);
        self._finish<T>(artifact) catch (cause) {
            _ => {
                throw root.errors.EvaluationError {
          message: _render_cause(cause),
          cause: cause,
        }
            }
        }
    }

    function _compile<T>(
        self,
        source: string,
    ) -> CompileArtifact throws root.errors.CompilationError | root.errors.SessionBusy {
        $rust_io_function
    }

    //baml:mut_vm
    //baml:may_yield
    function _finish<T>(self, artifact: CompileArtifact) -> T throws unknown {
        $rust_function
    }

    /// Diagnostics from this session's most recent `eval`.
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
) -> R throws E | InvalidArgumentError | root.errors.CompilationError {
    $rust_function
}
