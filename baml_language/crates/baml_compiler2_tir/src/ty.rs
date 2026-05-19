//! Resolved type representation — the output of type resolution.

use std::fmt;

use baml_base::Name;
pub use baml_base::attr::TyAttr;

/// A qualified type name with separate package and local name.
///
/// Used in `Ty::Class`, `Ty::Enum`, and `Ty::TypeAlias` to unambiguously
/// identify a type by its definition's package (e.g. `"user"`, `"baml"`)
/// and its short name (e.g. `"Foo"`, `"PrimitiveClient"`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct QualifiedTypeName {
    /// The package this type is defined in (e.g. `"user"`, `"baml"`).
    pkg: Name,
    /// The namespace this type is defined in (e.g. `["llm"]`).
    namespace: Vec<Name>,
    /// The short/local name of the type (e.g. `"Foo"`).
    name: Name,
    /// Unresolved generic type parameter names (e.g. `["T"]` for `Array<T>`).
    /// Empty for non-generic types or when concrete type args are substituted.
    pub generic_params: Vec<Name>,
}

impl QualifiedTypeName {
    pub fn new(pkg: Name, namespace: Vec<Name>, name: Name) -> Self {
        Self::new_with_generic_params(pkg, namespace, name, Vec::new())
    }

    pub fn new_with_generic_params(
        pkg: Name,
        namespace: Vec<Name>,
        name: Name,
        generic_params: Vec<Name>,
    ) -> Self {
        Self {
            pkg,
            namespace,
            name,
            generic_params,
        }
    }

    pub fn package(&self) -> &Name {
        &self.pkg
    }

    pub fn namespace(&self) -> &Vec<Name> {
        &self.namespace
    }

    pub fn name(&self) -> &Name {
        &self.name
    }

    pub fn is_builtin_root_type(&self, name: &str) -> bool {
        self.pkg.as_str() == "baml" && self.namespace.is_empty() && self.name.as_str() == name
    }

    /// Returns `true` if this type lives in the `baml.panics` namespace
    /// (i.e. it is a panic class or the `Panic` type alias).
    pub fn is_panic_type(&self) -> bool {
        baml_base::is_panic_namespace(self.pkg.as_str(), &self.namespace)
    }

    pub fn to_path_in_package(&self) -> Vec<Name> {
        self.namespace
            .iter()
            .chain(std::iter::once(&self.name))
            .cloned()
            .collect::<Vec<_>>()
    }
}

impl fmt::Display for QualifiedTypeName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let namespace = self
            .namespace
            .iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>()
            .join(".");
        if !namespace.is_empty() {
            write!(f, "{}.{}.{}", self.pkg, namespace, self.name)?;
        } else {
            write!(f, "{}.{}", self.pkg, self.name)?;
        }
        if !self.generic_params.is_empty() {
            let params: Vec<_> = self
                .generic_params
                .iter()
                .map(std::string::ToString::to_string)
                .collect();
            write!(f, "<{}>", params.join(", "))?;
        }
        Ok(())
    }
}

/// Resolved type — the output of type resolution (Pass 2).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Ty {
    /// A class type — just the name, no expansion.
    Class(QualifiedTypeName, Vec<Ty>, TyAttr),
    /// An enum type.
    Enum(QualifiedTypeName, TyAttr),
    /// An enum variant — Enum(qualified) . Variant(name).
    EnumVariant(QualifiedTypeName, Name, TyAttr),
    /// A type alias — opaque name reference, NOT expanded.
    /// Expansion happens lazily at subtype-checking time.
    TypeAlias(QualifiedTypeName, TyAttr),
    /// Primitive types.
    Primitive(PrimitiveType, TyAttr),
    /// T[]
    List(Box<Ty>, TyAttr),
    /// map<K, V>
    Map(Box<Ty>, Box<Ty>, TyAttr),
    /// A | B | C
    Union(Vec<Ty>, TyAttr),
    /// T?
    Optional(Box<Ty>, TyAttr),
    /// Literal string/int/bool as a type.
    ///
    /// Carries a `Freshness` flag modeled after TypeScript's fresh/regular
    /// literal types. Fresh literals (from expressions) widen to their base
    /// primitive at mutable binding sites. Regular literals (from type
    /// annotations or contextual typing) are preserved.
    Literal(baml_base::Literal, Freshness, TyAttr),
    /// Evolving list — created from empty array literal `[]` at mutable
    /// binding sites (via `make_evolving()`). Element type starts as `Never`
    /// and is refined by mutations (`.push()`, index assignment).
    ///
    /// Reading the variable in expression position produces the fixed
    /// `List(T)` type; the local entry stays `EvolvingList` so further
    /// mutations still work.
    ///
    /// Parallel to `Freshness` on literals: `make_evolving()` is the mirror
    /// of `widen_fresh()` — both called at `let` binding sites without
    /// type annotations.
    ///
    /// # Two parallel paths for container mutations
    ///
    /// There are two ways container method calls (e.g. `.push()`) are resolved:
    ///
    /// 1. **Evolving path** (`try_container_method_call` in `builder.rs`): For
    ///    mutable locals, `.push(x)` is intercepted *before* normal method
    ///    resolution. It widens the element type in-place (e.g. `EvolvingList(Never)`
    ///    → `EvolvingList(int)`) and returns `Void`. This path takes priority.
    ///
    /// 2. **Builtin method path** (`resolve_builtin_method` in `builder.rs`): For
    ///    typed arrays (e.g. `let arr: int[] = ...`), `.push(x)` is resolved via
    ///    the `Array<T>` class declared in `baml_builtins2/baml/containers.baml`.
    ///    The type checker bridges `Ty::List(int)` → `Array<int>`, binds `T = int`,
    ///    and type-checks the call against the method signature. This path does NOT
    ///    widen — the element type is already known.
    ///
    /// The evolving path exists because empty containers (`[]`, `{}`) need
    /// flow-sensitive type refinement that the static builtin signatures can't
    /// express. Once an evolving container is read, it freezes to a normal
    /// `List`/`Map` and subsequent method calls go through the builtin path.
    EvolvingList(Box<Ty>, TyAttr),
    /// Evolving map — created from empty map literal at mutable binding sites.
    /// Same semantics as `EvolvingList` but for maps (see doc on `EvolvingList`).
    EvolvingMap(Box<Ty>, Box<Ty>, TyAttr),
    /// Function type: (params) -> return.
    Function {
        params: Vec<FunctionParamTy>,
        ret: Box<Ty>,
        throws: Box<Ty>,
        attr: TyAttr,
    },
    /// A type variable (generic parameter) — e.g. `T` in `Array<T>`.
    ///
    /// First-class in the resolved type system. At definition sites, `T` is
    /// represented as `TypeVar("T")` rather than `Ty::Unknown`. At call sites,
    /// the inference algorithm in `check_expr` substitutes concrete types.
    /// Any `TypeVar` remaining after inference is erased to `Ty::Unknown` with
    /// a `CannotInferTypeParameter` diagnostic before reaching VIR/runtime.
    TypeVar(Name, TyAttr),
    /// The bottom type — expression never produces a value.
    /// Assigned to `return`, `break`, `continue`, and blocks that always diverge.
    /// `Never` is a subtype of every type: `join(Never, T) = T`.
    Never { attr: TyAttr },
    /// The void type — produced by statements and expressions that don't yield
    /// a useful value (e.g. `if` without `else`, bare function calls used as
    /// statements, `while` loops).
    ///
    /// `Void` is **not** a subtype of any other type. Consuming a `Void` value
    /// (assigning it, passing it as an argument, returning it) is a type error.
    /// In statement position the value is simply discarded.
    ///
    /// Analogous to TypeScript's fresh-object excess-property check pattern:
    /// the type is valid only when nobody reads the value.
    Void { attr: TyAttr },
    /// The explicit `unknown` keyword type — a top type (supertype of everything).
    ///
    /// Any `T` is a subtype of `BuiltinUnknown`, but `BuiltinUnknown` is NOT a
    /// subtype of any specific type. Analogous to TypeScript's `unknown`.
    ///
    /// Used in builtin function signatures where any value may be accepted, e.g.:
    /// ```baml
    /// function render_prompt(function_name: string, args: map<string, unknown>) -> PromptAst
    /// ```
    ///
    /// NOTE: This is **distinct** from `Ty::Unknown` which is the error-recovery
    /// sentinel meaning "type inference failed". `BuiltinUnknown` is a well-formed
    /// type that appears in valid programs; `Unknown` signals a compiler error.
    BuiltinUnknown { attr: TyAttr },
    /// Opaque Rust-managed state.
    ///
    /// Used for `$rust_type` fields in builtin class stubs (e.g., `Media._data`,
    /// `Response._body`). The containing class is non-constructable from user code.
    /// Fields of this type cannot be accessed directly from BAML code.
    ///
    /// This is distinct from `Ty::Unknown` (which means "type inference failed") —
    /// `RustType` is intentional and well-formed in the builtin stubs.
    RustType { attr: TyAttr },
    /// The `type` metatype keyword — represents a BAML type value at runtime.
    ///
    /// Modeled as a dedicated variant (like `RustType`) rather than collapsing to
    /// `BuiltinUnknown`, because:
    /// - `type` is semantically distinct from `unknown` (a string is not a type value)
    /// - Future methods (`.name()`, `.fields()`) need a concrete type to dispatch on
    /// - Maps to `Ty::Opaque("baml.reflect.Type")` at the MIR level
    ///
    /// Follows the same pattern as `RustType`: opaque builtin, leaf type, no inner
    /// structure. Group with `RustType` in match arms.
    Type { attr: TyAttr },
    /// Error recovery — the type is structurally unknown (e.g., name unresolved).
    Unknown { attr: TyAttr },
    /// Error sentinel — a hard error was emitted for this expression.
    Error { attr: TyAttr },
    /// BEP-034 `Future<T, E>` — the result of `spawn { ... }` or a sys-op
    /// call before `await`. Carries both the resolved value type and the
    /// errors the producing computation may throw.
    Future(Box<Ty>, Box<Ty>, TyAttr),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FunctionParamTy {
    pub name: Option<Name>,
    pub ty: Ty,
    pub mode: FunctionParamMode,
}

impl FunctionParamTy {
    pub fn required(name: Option<Name>, ty: Ty) -> Self {
        Self {
            name,
            ty,
            mode: FunctionParamMode::Required,
        }
    }

    pub fn optional(name: Option<Name>, ty: Ty) -> Self {
        Self {
            name,
            ty,
            mode: FunctionParamMode::Optional,
        }
    }

    pub fn is_required(&self) -> bool {
        matches!(self.mode, FunctionParamMode::Required)
    }

    pub fn is_optional(&self) -> bool {
        matches!(self.mode, FunctionParamMode::Optional)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FunctionParamMode {
    Required,
    Optional,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PrimitiveType {
    Int,
    Float,
    String,
    Bool,
    Null,
    Uint8Array,
    Image,
    Audio,
    Video,
    Pdf,
}

impl PrimitiveType {
    /// Map primitives with builtin companion classes to their class path in the `baml` package.
    ///
    /// Media primitives (`image`, `audio`, `video`, `pdf`) have corresponding
    /// classes in `baml_builtins2/baml_std/baml/ns_media/media.baml`, and
    /// `uint8array` has its class in `baml_builtins2/baml_std/baml/uint8array.baml`.
    pub fn builtin_class_path(&self) -> &'static [&'static str] {
        match self {
            Self::Int => &["Int"],
            Self::Float => &["Float"],
            Self::Bool => &["Bool"],
            Self::Null => &["Null"],
            Self::String => &["String"],
            Self::Uint8Array => &["Uint8Array"],
            Self::Image => &["media", "Image"],
            Self::Audio => &["media", "Audio"],
            Self::Video => &["media", "Video"],
            Self::Pdf => &["media", "Pdf"],
        }
    }

    pub fn from_literal(lit: &baml_base::Literal) -> Self {
        match lit {
            baml_base::Literal::Int(_) => Self::Int,
            baml_base::Literal::Float(_) => Self::Float,
            baml_base::Literal::String(_) => Self::String,
            baml_base::Literal::Bool(_) => Self::Bool,
        }
    }
}

/// Freshness flag for literal types.
///
/// Modeled after TypeScript's fresh/regular literal type distinction.
/// - **Fresh**: produced by literal expressions (`1`, `"hello"`). Widens to
///   the base primitive at mutable binding sites (`let x = 1` → `int`).
/// - **Regular**: produced by type annotations (`let x: 1 = 1`) or contextual
///   typing. Preserved through mutable bindings.
///
/// Freshness is **ignored** by the subtype checker — `Literal(1, Fresh)` and
/// `Literal(1, Regular)` are structurally identical for assignability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Freshness {
    Fresh,
    Regular,
}

/// Re-export `baml_base::Literal` as `LiteralValue` for backward compatibility.
pub type LiteralValue = baml_base::Literal;

/// Flatten, deduplicate, and collapse a vec of widened types into a single `Ty`.
///
/// After `widen_fresh()` has run on each union member, multiple members may
/// have widened to the same primitive (e.g. `[Literal(1,Fresh), Literal(2,Fresh)]`
/// both become `Primitive(Int)`). This helper deduplicates and collapses:
/// - Flattens nested unions one level
/// - Deduplicates by `PartialEq`
/// - Unwraps singletons
fn dedup_and_collapse(types: Vec<Ty>, attr: TyAttr) -> Ty {
    let mut members: Vec<Ty> = Vec::new();
    for ty in types {
        match ty {
            Ty::Union(inner, _) => {
                for m in inner {
                    if !members.contains(&m) {
                        members.push(m);
                    }
                }
            }
            _ => {
                if !members.contains(&ty) {
                    members.push(ty);
                }
            }
        }
    }
    match members.len() {
        0 => Ty::Never { attr },
        1 => members.into_iter().next().unwrap(),
        _ => Ty::Union(members, attr),
    }
}

impl Ty {
    /// Access the `TyAttr` on this type.
    pub fn attr(&self) -> &TyAttr {
        match self {
            Ty::Class(_, _, a)
            | Ty::Enum(_, a)
            | Ty::EnumVariant(_, _, a)
            | Ty::TypeAlias(_, a)
            | Ty::Primitive(_, a)
            | Ty::List(_, a)
            | Ty::Map(_, _, a)
            | Ty::Union(_, a)
            | Ty::Optional(_, a)
            | Ty::Literal(_, _, a)
            | Ty::EvolvingList(_, a)
            | Ty::EvolvingMap(_, _, a)
            | Ty::TypeVar(_, a)
            | Ty::Future(_, _, a) => a,
            Ty::Function { attr, .. }
            | Ty::Never { attr }
            | Ty::Void { attr }
            | Ty::BuiltinUnknown { attr }
            | Ty::RustType { attr }
            | Ty::Type { attr }
            | Ty::Unknown { attr }
            | Ty::Error { attr } => attr,
        }
    }

    /// Return a copy of this type with the given `TyAttr`.
    #[must_use]
    pub fn with_attr(mut self, new_attr: TyAttr) -> Ty {
        match &mut self {
            Ty::Class(_, _, a)
            | Ty::Enum(_, a)
            | Ty::EnumVariant(_, _, a)
            | Ty::TypeAlias(_, a)
            | Ty::Primitive(_, a)
            | Ty::List(_, a)
            | Ty::Map(_, _, a)
            | Ty::Union(_, a)
            | Ty::Optional(_, a)
            | Ty::Literal(_, _, a)
            | Ty::EvolvingList(_, a)
            | Ty::EvolvingMap(_, _, a)
            | Ty::TypeVar(_, a)
            | Ty::Future(_, _, a) => *a = new_attr,
            Ty::Function { attr, .. }
            | Ty::Never { attr }
            | Ty::Void { attr }
            | Ty::BuiltinUnknown { attr }
            | Ty::RustType { attr }
            | Ty::Type { attr }
            | Ty::Unknown { attr }
            | Ty::Error { attr } => *attr = new_attr,
        }
        self
    }

    /// Widen fresh literal types to their base primitive.
    ///
    /// Called at mutable binding sites (`let` without annotation).
    /// Regular (non-fresh) literals pass through unchanged.
    ///
    /// Recurses into `Union`, `List`, `Map`, and `Optional` so that compound
    /// types like `(1 | 2 | 3)[]` widen to `int[]` at unannotated bindings.
    #[must_use]
    pub fn widen_fresh(self) -> Ty {
        match self {
            Ty::Literal(lit, Freshness::Fresh, attr) => {
                Ty::Primitive(PrimitiveType::from_literal(&lit), attr)
            }
            Ty::Union(members, attr) => {
                let widened: Vec<Ty> = members.into_iter().map(Ty::widen_fresh).collect();
                dedup_and_collapse(widened, attr)
            }
            Ty::List(inner, attr) => Ty::List(Box::new((*inner).widen_fresh()), attr),
            Ty::Map(k, v, attr) => Ty::Map(
                Box::new((*k).widen_fresh()),
                Box::new((*v).widen_fresh()),
                attr,
            ),
            Ty::Optional(inner, attr) => Ty::Optional(Box::new((*inner).widen_fresh()), attr),
            Ty::Class(name, type_args, attr) => {
                let widened: Vec<Ty> = type_args.into_iter().map(Ty::widen_fresh).collect();
                Ty::Class(name, widened, attr)
            }
            other => other,
        }
    }

    /// Promote empty containers to evolving containers.
    ///
    /// Called at mutable binding sites (`let` without annotation), right
    /// after `widen_fresh()`. This is the mirror of `widen_fresh()`:
    /// - `widen_fresh` *removes* literal specificity (1 → int)
    /// - `make_evolving` *adds* container mutability (List(Never) → EvolvingList(Never))
    ///
    /// Only converts `List(Never)` and `Map(Never, Never)` — non-empty
    /// container literals already have a known element type and don't need
    /// evolving semantics.
    #[must_use]
    pub fn make_evolving(self) -> Ty {
        match self {
            Ty::List(inner, attr) if matches!(*inner, Ty::Never { .. }) => {
                Ty::EvolvingList(inner, attr)
            }
            Ty::Map(k, v, attr)
                if matches!(*k, Ty::Never { .. }) && matches!(*v, Ty::Never { .. }) =>
            {
                Ty::EvolvingMap(k, v, attr)
            }
            other => other,
        }
    }
}

// ── Display impls ────────────────────────────────────────────────────────────

impl Ty {
    /// Whether this type needs parentheses when a postfix modifier (`[]`, `?`)
    /// is applied. Unions must be grouped because postfix binds tighter than `|`.
    fn needs_postfix_parens(&self) -> bool {
        matches!(self, Ty::Union(..) | Ty::Function { .. })
    }

    /// Nested function returns need grouping so the outer `throws` clause is
    /// visually associated with the outer callable rather than the returned one.
    fn needs_function_result_parens(&self) -> bool {
        matches!(self, Ty::Function { .. })
    }

    /// Format with parentheses if needed for postfix context.
    fn fmt_as_postfix_base(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.needs_postfix_parens() {
            write!(f, "({self})")
        } else {
            write!(f, "{self}")
        }
    }

    /// Format with parentheses if needed in a function return position.
    fn fmt_as_function_result(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.needs_function_result_parens() {
            write!(f, "({self})")
        } else {
            write!(f, "{self}")
        }
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Class(qn, type_args, _) => {
                let namespace = qn
                    .namespace()
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(".");
                if !namespace.is_empty() {
                    write!(f, "{}.{}.{}", qn.package(), namespace, qn.name())?;
                } else {
                    write!(f, "{}.{}", qn.package(), qn.name())?;
                }
                if !type_args.is_empty() {
                    let args: Vec<_> = type_args
                        .iter()
                        .map(std::string::ToString::to_string)
                        .collect();
                    write!(f, "<{}>", args.join(", "))?;
                } else if !qn.generic_params.is_empty() {
                    // Unspecialized generic class — show `_` placeholders, one per declared param.
                    let placeholders = vec!["_"; qn.generic_params.len()];
                    write!(f, "<{}>", placeholders.join(", "))?;
                }
                Ok(())
            }
            Ty::Enum(qn, _) => write!(f, "{qn}"),
            Ty::EnumVariant(qn, v, _) => write!(f, "{qn}.{v}"),
            Ty::TypeAlias(qn, _) => write!(f, "{qn}"),
            Ty::Primitive(p, _) => write!(f, "{p}"),
            Ty::List(inner, _) => {
                inner.fmt_as_postfix_base(f)?;
                write!(f, "[]")
            }
            Ty::Map(k, v, _) => write!(f, "map<{k}, {v}>"),
            Ty::EvolvingList(inner, _) => {
                if matches!(**inner, Ty::Never { .. }) {
                    write!(f, "_[]")
                } else {
                    inner.fmt_as_postfix_base(f)?;
                    write!(f, "[] (evolving)")
                }
            }
            Ty::EvolvingMap(k, v, _) => {
                if matches!(**k, Ty::Never { .. }) && matches!(**v, Ty::Never { .. }) {
                    write!(f, "map<_, _>")
                } else {
                    write!(f, "map<{k}, {v}> (evolving)")
                }
            }
            Ty::Union(members, _) => {
                let parts: Vec<_> = members
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect();
                write!(f, "{}", parts.join(" | "))
            }
            Ty::Optional(inner, _) => {
                inner.fmt_as_postfix_base(f)?;
                write!(f, "?")
            }
            Ty::Literal(lit, _freshness, _) => write!(f, "{lit}"),
            Ty::Function {
                params,
                ret,
                throws,
                ..
            } => {
                let ps: Vec<String> = params
                    .iter()
                    .map(|param| {
                        let ty = &param.ty;
                        match (&param.name, param.mode) {
                            (Some(name), FunctionParamMode::Optional) => {
                                format!("{name}?: {ty}")
                            }
                            (Some(name), FunctionParamMode::Required) => format!("{name}: {ty}"),
                            (None, _) => ty.to_string(),
                        }
                    })
                    .collect();
                write!(f, "({}) -> ", ps.join(", "))?;
                ret.fmt_as_function_result(f)?;
                write!(f, " throws {throws}")
            }
            Ty::TypeVar(name, _) => write!(f, "{name}"),
            Ty::Never { .. } => write!(f, "never"),
            Ty::Void { .. } => write!(f, "void"),
            Ty::BuiltinUnknown { .. } => write!(f, "unknown"),
            Ty::RustType { .. } => write!(f, "$rust_type"),
            Ty::Type { .. } => write!(f, "type"),
            Ty::Unknown { .. } => write!(f, "unknown"),
            Ty::Error { .. } => write!(f, "!error"),
            Ty::Future(value, error, _) => write!(f, "Future<{value}, {error}>"),
        }
    }
}

impl fmt::Display for PrimitiveType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrimitiveType::Int => write!(f, "int"),
            PrimitiveType::Float => write!(f, "float"),
            PrimitiveType::String => write!(f, "string"),
            PrimitiveType::Bool => write!(f, "bool"),
            PrimitiveType::Null => write!(f, "null"),
            PrimitiveType::Image => write!(f, "image"),
            PrimitiveType::Audio => write!(f, "audio"),
            PrimitiveType::Video => write!(f, "video"),
            PrimitiveType::Pdf => write!(f, "pdf"),
            PrimitiveType::Uint8Array => write!(f, "uint8array"),
        }
    }
}
