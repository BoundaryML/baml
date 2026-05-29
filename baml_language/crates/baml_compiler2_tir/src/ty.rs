//! Resolved type representation — the output of type resolution.

use std::fmt;

use baml_base::Name;
pub use baml_base::attr::TyAttr;

/// A qualified type name with separate package and local name.
///
/// Used in `Ty::Class`, `Ty::Enum`, and `Ty::TypeAlias` to unambiguously
/// identify a type by its definition's package (e.g. `"user"`, `"baml"`)
/// and its short name (e.g. `"Foo"`, `"PrimitiveClient"`).
/// Which package a type is defined in. `Local` is the user's own (implicit
/// root) package — the "current" package for everything a user writes;
/// `Dep(name)` is a named dependency (e.g. `baml`). Encoding this as a type
/// rather than a magic `"user"` string means the local-vs-dependency
/// distinction is checked by the compiler, not by string comparison: the only
/// place the `"user"` string appears is [`Package::from_name`] (the boundary
/// where upstream `Name`-based package info is classified).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Package {
    /// The user's own implicit root package (`RESERVED_USER_PACKAGE`).
    Local,
    /// A named dependency package.
    Dep(Name),
}

/// The interned `Name` of the reserved implicit `user` package, materialized
/// once so [`QualifiedTypeName::package`] can hand out a `&Name` for `Local`.
static USER_PACKAGE_NAME: Name = Name::new_inline(RESERVED_USER_PACKAGE);

impl Package {
    /// Classify an upstream package `Name`: the reserved `user` package becomes
    /// [`Package::Local`], everything else a [`Package::Dep`]. This is the one
    /// spot the `"user"` magic string is read.
    pub fn from_name(name: Name) -> Self {
        if name.as_str() == RESERVED_USER_PACKAGE {
            Package::Local
        } else {
            Package::Dep(name)
        }
    }

    /// The package's `Name` (`Local` resolves to the reserved `user` name).
    pub fn as_name(&self) -> &Name {
        match self {
            Package::Local => &USER_PACKAGE_NAME,
            Package::Dep(name) => name,
        }
    }
}

// Order/sort by the package *name* string, preserving the pre-enum `Ord`
// (where `pkg` was a `Name`) so `QualifiedTypeName`'s derived ordering — and
// any sorted output keyed on it — is unchanged.
impl Ord for Package {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_name().as_str().cmp(other.as_name().as_str())
    }
}

impl PartialOrd for Package {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct QualifiedTypeName {
    /// The package this type is defined in (`Local` for user code, `Dep` for a
    /// dependency like `baml`).
    pkg: Package,
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
            pkg: Package::from_name(pkg),
            namespace,
            name,
            generic_params,
        }
    }

    pub fn package(&self) -> &Name {
        self.pkg.as_name()
    }

    /// Whether this type lives in the user's own (implicit root) package — the
    /// "current" package for everything a user writes. User-facing rendering
    /// omits the package for these; only dependency types carry a package
    /// qualifier. Use this instead of comparing `package()` to `"user"`.
    pub fn is_local(&self) -> bool {
        matches!(self.pkg, Package::Local)
    }

    pub fn namespace(&self) -> &Vec<Name> {
        &self.namespace
    }

    pub fn name(&self) -> &Name {
        &self.name
    }

    pub fn is_builtin_root_type(&self, name: &str) -> bool {
        self.package().as_str() == "baml"
            && self.namespace.is_empty()
            && self.name.as_str() == name
    }

    /// Returns `true` if this type lives in the `baml.panics` namespace
    /// (i.e. it is a panic class or the `Panic` type alias).
    pub fn is_panic_type(&self) -> bool {
        baml_base::is_panic_namespace(self.package().as_str(), &self.namespace)
    }

    pub fn to_path_in_package(&self) -> Vec<Name> {
        self.namespace
            .iter()
            .chain(std::iter::once(&self.name))
            .cloned()
            .collect::<Vec<_>>()
    }

    /// The dotted path `package.namespace.name` (no `<generic_params>` suffix).
    /// When `user_facing`, the reserved implicit `user` package is elided
    /// ([`RESERVED_USER_PACKAGE`]) — the single structural source of the
    /// "no `user.` in names" rule. The canonical form (`user_facing = false`)
    /// keeps the package for dumps/identity.
    pub fn render_dotted(&self, user_facing: bool) -> String {
        let namespace = self
            .namespace
            .iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>()
            .join(".");
        let elide = user_facing && self.is_local();
        let pkg = self.package();
        match (elide, namespace.is_empty()) {
            (true, true) => self.name.to_string(),
            (true, false) => format!("{namespace}.{}", self.name),
            (false, true) => format!("{}.{}", pkg, self.name),
            (false, false) => format!("{}.{namespace}.{}", pkg, self.name),
        }
    }

    /// Like [`render_dotted`](Self::render_dotted) plus the declared
    /// `<generic_params>` suffix (e.g. `Array<T>`).
    pub fn render_qualified(&self, user_facing: bool) -> String {
        let mut out = self.render_dotted(user_facing);
        if !self.generic_params.is_empty() {
            let params: Vec<_> = self
                .generic_params
                .iter()
                .map(std::string::ToString::to_string)
                .collect();
            out.push('<');
            out.push_str(&params.join(", "));
            out.push('>');
        }
        out
    }

    /// User-facing rendering of the qualified name: identical to the canonical
    /// [`fmt::Display`] except the reserved implicit `user` package is elided.
    /// Call this instead of post-processing the canonical string.
    pub fn render_user_facing(&self) -> String {
        self.render_qualified(true)
    }
}

/// The reserved implicit root package for user-authored code. It is the
/// *current* package for everything a user writes, so it must never be shown in
/// user-facing output (`user.Dog` → `Dog`). The canonical `Display` keeps it
/// (for dumps/identity); only the user-facing path elides it.
pub const RESERVED_USER_PACKAGE: &str = "user";

/// Prefix of synthetic effect-polymorphism type parameters. These are an
/// internal encoding (`__effect_param_0`, …); user-facing rendering shows them
/// as `callback`. Single source of truth — use [`is_synthetic_effect_param`]
/// rather than re-deriving this prefix check.
pub const SYNTHETIC_EFFECT_PARAM_PREFIX: &str = "__effect_param_";

/// Whether `name` is a synthesized effect-polymorphism type parameter
/// (`__effect_param_N`). The single source of truth for this check — TIR, MIR,
/// and the LSP all call here instead of re-implementing the prefix match.
pub fn is_synthetic_effect_param(name: &Name) -> bool {
    name.as_str()
        .strip_prefix(SYNTHETIC_EFFECT_PARAM_PREFIX)
        .is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()))
}

impl fmt::Display for QualifiedTypeName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.render_qualified(false))
    }
}

/// Resolved type — the output of type resolution (Pass 2).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Ty {
    /// A class type — just the name, no expansion.
    Class(QualifiedTypeName, Vec<Ty>, TyAttr),
    /// An interface type (BEP-044) — nominal contract. Subtyping is
    /// determined by explicit `implements I { ... }` blocks on classes.
    /// Generic args follow the same shape as `Class` for parameterised
    /// interfaces like `Container<T>`.
    Interface(QualifiedTypeName, Vec<Ty>, TyAttr),
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
        generic_params: Vec<Name>,
        generic_param_bounds: Vec<Option<Ty>>,
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
    Bigint,
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
            Self::Bigint => &["Bigint"],
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
            baml_base::Literal::Bigint(_) => Self::Bigint,
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
            | Ty::Interface(_, _, a)
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
            | Ty::Interface(_, _, a)
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

/// Strategy controlling how a [`Ty`] renders its leaf names plus a couple of
/// presentation choices. A single recursive renderer ([`Ty::render_with`])
/// walks the structure; everything package-, type-var-, or context-specific
/// lives behind this trait. This is the one place type *structure* is turned
/// into text — the canonical [`fmt::Display`], user-facing diagnostics, and the
/// LSP's context-aware hover all implement this trait instead of re-walking
/// `Ty` (the former "~10 renderers").
pub trait TyRenderStrategy {
    /// Render a qualified name's dotted path (package/namespace/name) *without*
    /// any `<...>` suffix; the renderer appends type args or placeholders. When
    /// `with_generic_params` is set, append the name's own declared
    /// `<generic_params>` — used for the name-only positions (enums, aliases,
    /// enum variants) where the canonical form shows them.
    fn qtn(&self, qtn: &QualifiedTypeName, with_generic_params: bool) -> String;

    /// Render a type-variable name (`T`, or a synthetic effect param).
    fn type_var(&self, name: &Name) -> String;

    /// Whether unspecialized generic classes/interfaces render their declared
    /// params as `<_, _>` placeholders. Canonical/user-facing: yes; the LSP's
    /// hover renders the bare name.
    fn show_unspecialized_placeholders(&self) -> bool {
        true
    }

    /// Whether evolving list/map types are annotated `(evolving)`.
    /// Canonical/user-facing: yes; the LSP's hover hides it.
    fn show_evolving(&self) -> bool {
        true
    }
}

impl Ty {
    /// User-facing rendering: identical to the canonical [`fmt::Display`] except the
    /// reserved implicit `user` package is elided ([`RESERVED_USER_PACKAGE`])
    /// and synthetic effect params show as `callback`. This is the single
    /// structural source of the "no `user.` in messages" rule — diagnostics
    /// render through here instead of post-processing the canonical string.
    pub fn render_user_facing(&self) -> String {
        self.render_with(&CanonicalTyRender { user_facing: true })
    }

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

    /// Render with parentheses if needed for postfix (`[]`/`?`) context.
    fn render_as_postfix_base(&self, s: &dyn TyRenderStrategy) -> String {
        let inner = self.render_with(s);
        if self.needs_postfix_parens() {
            format!("({inner})")
        } else {
            inner
        }
    }

    /// Render with parentheses if needed in a function-return position.
    fn render_as_function_result(&self, s: &dyn TyRenderStrategy) -> String {
        let inner = self.render_with(s);
        if self.needs_function_result_parens() {
            format!("({inner})")
        } else {
            inner
        }
    }

    /// The single structural renderer. Walks the type, delegating every
    /// package-, type-var-, and presentation-policy decision to `s`. All type
    /// rendering — canonical `Display`, user-facing diagnostics, LSP hover —
    /// funnels through here so the structure is described in exactly one place.
    pub fn render_with(&self, s: &dyn TyRenderStrategy) -> String {
        match self {
            Ty::Class(qn, type_args, _) | Ty::Interface(qn, type_args, _) => {
                let mut out = s.qtn(qn, false);
                if !type_args.is_empty() {
                    let args: Vec<_> = type_args.iter().map(|a| a.render_with(s)).collect();
                    out.push('<');
                    out.push_str(&args.join(", "));
                    out.push('>');
                } else if !qn.generic_params.is_empty() && s.show_unspecialized_placeholders() {
                    // Unspecialized generic — show `_` placeholders, one per declared param.
                    let placeholders = vec!["_"; qn.generic_params.len()];
                    out.push('<');
                    out.push_str(&placeholders.join(", "));
                    out.push('>');
                }
                out
            }
            Ty::Enum(qn, _) | Ty::TypeAlias(qn, _) => s.qtn(qn, true),
            Ty::EnumVariant(qn, v, _) => format!("{}.{v}", s.qtn(qn, true)),
            Ty::Primitive(p, _) => p.to_string(),
            Ty::List(inner, _) => format!("{}[]", inner.render_as_postfix_base(s)),
            Ty::Map(k, v, _) => format!("map<{}, {}>", k.render_with(s), v.render_with(s)),
            Ty::EvolvingList(inner, _) => {
                if matches!(**inner, Ty::Never { .. }) {
                    "_[]".to_string()
                } else if s.show_evolving() {
                    format!("{}[] (evolving)", inner.render_as_postfix_base(s))
                } else {
                    format!("{}[]", inner.render_as_postfix_base(s))
                }
            }
            Ty::EvolvingMap(k, v, _) => {
                if matches!(**k, Ty::Never { .. }) && matches!(**v, Ty::Never { .. }) {
                    "map<_, _>".to_string()
                } else if s.show_evolving() {
                    format!("map<{}, {}> (evolving)", k.render_with(s), v.render_with(s))
                } else {
                    format!("map<{}, {}>", k.render_with(s), v.render_with(s))
                }
            }
            Ty::Union(members, _) => members
                .iter()
                .map(|m| m.render_with(s))
                .collect::<Vec<_>>()
                .join(" | "),
            Ty::Optional(inner, _) => format!("{}?", inner.render_as_postfix_base(s)),
            Ty::Literal(lit, _freshness, _) => lit.to_string(),
            Ty::Function {
                generic_params,
                generic_param_bounds,
                params,
                ret,
                throws,
                ..
            } => {
                let mut out = String::new();
                if !generic_params.is_empty() {
                    out.push('<');
                    for (i, param) in generic_params.iter().enumerate() {
                        if i > 0 {
                            out.push_str(", ");
                        }
                        out.push_str(&param.to_string());
                        if let Some(bound) = generic_param_bounds.get(i).and_then(Option::as_ref) {
                            out.push_str(&format!(" extends {}", bound.render_with(s)));
                        }
                    }
                    out.push('>');
                }
                let ps: Vec<String> = params
                    .iter()
                    .map(|param| {
                        let ty = param.ty.render_with(s);
                        match (&param.name, param.mode) {
                            (Some(name), FunctionParamMode::Optional) => format!("{name}?: {ty}"),
                            (Some(name), FunctionParamMode::Required) => format!("{name}: {ty}"),
                            (None, _) => ty,
                        }
                    })
                    .collect();
                format!(
                    "{out}({}) -> {} throws {}",
                    ps.join(", "),
                    ret.render_as_function_result(s),
                    throws.render_with(s),
                )
            }
            Ty::TypeVar(name, _) => s.type_var(name),
            Ty::Never { .. } => "never".to_string(),
            Ty::Void { .. } => "void".to_string(),
            Ty::BuiltinUnknown { .. } | Ty::Unknown { .. } => "unknown".to_string(),
            Ty::RustType { .. } => "$rust_type".to_string(),
            Ty::Type { .. } => "type".to_string(),
            Ty::Error { .. } => "!error".to_string(),
            Ty::Future(value, error, _) => {
                format!("Future<{}, {}>", value.render_with(s), error.render_with(s))
            }
        }
    }
}

/// The built-in strategy for canonical and user-facing rendering. When
/// `user_facing`, the reserved implicit `user` package is elided and synthetic
/// effect params show as `callback`; otherwise everything renders verbatim (for
/// dumps and identity). Both keep `(evolving)` annotations and `<_>`
/// placeholders. Canonical [`fmt::Display`] uses `user_facing = false`;
/// [`Ty::render_user_facing`] uses `true`.
struct CanonicalTyRender {
    user_facing: bool,
}

impl TyRenderStrategy for CanonicalTyRender {
    fn qtn(&self, qtn: &QualifiedTypeName, with_generic_params: bool) -> String {
        if with_generic_params {
            qtn.render_qualified(self.user_facing)
        } else {
            qtn.render_dotted(self.user_facing)
        }
    }

    fn type_var(&self, name: &Name) -> String {
        // A synthetic effect parameter (`__effect_param_N`) is an implementation
        // detail of effect-polymorphic callbacks; show it as `callback` in
        // user-facing output.
        if self.user_facing && is_synthetic_effect_param(name) {
            "callback".to_string()
        } else {
            name.to_string()
        }
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            self.render_with(&CanonicalTyRender { user_facing: false })
        )
    }
}

impl fmt::Display for PrimitiveType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrimitiveType::Int => write!(f, "int"),
            PrimitiveType::Bigint => write!(f, "bigint"),
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
