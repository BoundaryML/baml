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
    /// The package this type is defined in (e.g. `"user"` for user code,
    /// `"baml"` for a dependency).
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

    /// Whether this type lives in the user's own (implicit root) package — the
    /// "current" package for everything a user writes. User-facing rendering
    /// omits the package for these; only dependency types carry a package
    /// qualifier. Use this instead of comparing `package()` to `"user"`.
    pub fn is_local(&self) -> bool {
        self.pkg.as_str() == RESERVED_USER_PACKAGE
    }

    pub fn namespace(&self) -> &Vec<Name> {
        &self.namespace
    }

    pub fn name(&self) -> &Name {
        &self.name
    }

    pub fn is_builtin_root_type(&self, name: &str) -> bool {
        self.package().as_str() == baml_base::BAML_PACKAGE
            && self.namespace.is_empty()
            && self.name.as_str() == name
    }

    /// Whether this is the builtin `baml.future.Future` type. The single source
    /// of truth for the `future`/`Future` identity — `lower_type_expr` keys off
    /// it to produce the dedicated `Ty::Future` variant.
    pub fn is_builtin_future(&self) -> bool {
        self.package().as_str() == baml_base::BAML_PACKAGE
            && self.namespace.len() == 1
            && self.namespace[0].as_str() == FUTURE_NAMESPACE
            && self.name.as_str() == FUTURE_TYPE
    }

    /// Returns `true` if this type lives in the `baml.panics` namespace
    /// (i.e. it is a panic class or the `Panic` type alias).
    pub fn is_panic_type(&self) -> bool {
        baml_base::is_panic_namespace(self.package().as_str(), &self.namespace)
    }

    /// The dotted path `package.namespace.name` (no `<generic_params>` suffix).
    /// When `user_facing`, the reserved implicit `user` package is elided
    /// ([`RESERVED_USER_PACKAGE`]) — the single structural source of the
    /// "no `user.` in names" rule. The canonical form (`user_facing = false`)
    /// keeps the package for dumps/identity.
    fn render_dotted(&self, user_facing: bool) -> String {
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
    fn render_qualified(&self, user_facing: bool) -> String {
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

/// Namespace and name of the builtin `baml.future.Future` type. Centralizes the
/// magic strings used by [`QualifiedTypeName::is_builtin_future`] and the
/// `Ty::Future → class path` mapping in MIR lowering.
pub const FUTURE_NAMESPACE: &str = "future";
pub const FUTURE_TYPE: &str = "Future";

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
    Class(QualifiedTypeName, Vec<Ty>),
    /// An interface type (BEP-044) — nominal contract. Subtyping is
    /// determined by explicit `implements I { ... }` blocks on classes.
    /// Generic args follow the same shape as `Class` for parameterised
    /// interfaces like `Container<T>`.
    Interface(QualifiedTypeName, Vec<Ty>),
    /// An enum type.
    Enum(QualifiedTypeName),
    /// An enum variant — Enum(qualified) . Variant(name).
    EnumVariant(QualifiedTypeName, Name),
    /// A type alias — opaque name reference, NOT expanded.
    /// Expansion happens lazily at subtype-checking time.
    TypeAlias(QualifiedTypeName),
    /// Primitive types.
    Primitive(PrimitiveType),
    /// T[]
    List(Box<Ty>),
    /// map<K, V>
    Map(Box<Ty>, Box<Ty>),
    /// A | B | C
    Union(Vec<Ty>),
    /// T?
    Optional(Box<Ty>),
    /// Literal string/int/bool as a type.
    ///
    /// Carries a `Freshness` flag modeled after TypeScript's fresh/regular
    /// literal types. Fresh literals (from expressions) widen to their base
    /// primitive at mutable binding sites. Regular literals (from type
    /// annotations or contextual typing) are preserved.
    Literal(baml_base::Literal, Freshness),
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
    EvolvingList(Box<Ty>),
    /// Evolving map — created from empty map literal at mutable binding sites.
    /// Same semantics as `EvolvingList` but for maps (see doc on `EvolvingList`).
    EvolvingMap(Box<Ty>, Box<Ty>),
    /// Function type: (params) -> return.
    Function {
        generic_params: Vec<Name>,
        generic_param_bounds: Vec<Option<Ty>>,
        params: Vec<FunctionParamTy>,
        ret: Box<Ty>,
        throws: Box<Ty>,
    },
    /// A type variable (generic parameter) — e.g. `T` in `Array<T>`.
    ///
    /// First-class in the resolved type system. At definition sites, `T` is
    /// represented as `TypeVar("T")` rather than `Ty::Unknown`. At call sites,
    /// the inference algorithm in `check_expr` substitutes concrete types.
    /// Any `TypeVar` remaining after inference is erased to `Ty::Unknown` with
    /// a `CannotInferTypeParameter` diagnostic before reaching VIR/runtime.
    TypeVar(Name),
    /// The bottom type — expression never produces a value.
    /// Assigned to `return`, `break`, `continue`, and blocks that always diverge.
    /// `Never` is a subtype of every type: `join(Never, T) = T`.
    Never,
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
    Void,
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
    BuiltinUnknown,
    /// Opaque Rust-managed state.
    ///
    /// Used for `$rust_type` fields in builtin class stubs (e.g., `Media._data`,
    /// `Response._body`). The containing class is non-constructable from user code.
    /// Fields of this type cannot be accessed directly from BAML code.
    ///
    /// This is distinct from `Ty::Unknown` (which means "type inference failed") —
    /// `RustType` is intentional and well-formed in the builtin stubs.
    RustType,
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
    Type,
    /// Error recovery — the type is structurally unknown (e.g., name unresolved).
    Unknown,
    /// Error sentinel — a hard error was emitted for this expression.
    Error,
    /// BEP-034 `Future<T, E>` — the result of `spawn { ... }` or a sys-op
    /// call before `await`. Carries both the resolved value type and the
    /// errors the producing computation may throw.
    Future(Box<Ty>, Box<Ty>),
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

/// Collapse a list of union members into a single `Ty` without deduplicating:
/// `0 => Never` / `1 => unwrap` / `_ => Union`. Callers that need flattening or
/// deduplication should do it before calling this (see [`dedup_and_collapse`]).
pub(crate) fn union_of(members: Vec<Ty>) -> Ty {
    match members.len() {
        0 => Ty::Never,
        1 => members.into_iter().next().unwrap(),
        _ => Ty::Union(members),
    }
}

/// Flatten, deduplicate, and collapse a vec of widened types into a single `Ty`.
///
/// After `widen_fresh()` has run on each union member, multiple members may
/// have widened to the same primitive (e.g. `[Literal(1,Fresh), Literal(2,Fresh)]`
/// both become `Primitive(Int)`). This helper deduplicates and collapses:
/// - Flattens nested unions one level
/// - Deduplicates by `PartialEq`
/// - Unwraps singletons
fn dedup_and_collapse(types: Vec<Ty>) -> Ty {
    let mut members: Vec<Ty> = Vec::new();
    for ty in types {
        match ty {
            Ty::Union(inner) => {
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
    union_of(members)
}

impl Ty {
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
            Ty::Literal(lit, Freshness::Fresh) => Ty::Primitive(PrimitiveType::from_literal(&lit)),
            Ty::Union(members) => {
                let widened: Vec<Ty> = members.into_iter().map(Ty::widen_fresh).collect();
                dedup_and_collapse(widened)
            }
            Ty::List(inner) => Ty::List(Box::new((*inner).widen_fresh())),
            Ty::Map(k, v) => Ty::Map(Box::new((*k).widen_fresh()), Box::new((*v).widen_fresh())),
            Ty::Optional(inner) => Ty::Optional(Box::new((*inner).widen_fresh())),
            Ty::Class(name, type_args) => {
                let widened: Vec<Ty> = type_args.into_iter().map(Ty::widen_fresh).collect();
                Ty::Class(name, widened)
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
            Ty::List(inner) if matches!(*inner, Ty::Never) => Ty::EvolvingList(inner),
            Ty::Map(k, v) if matches!(*k, Ty::Never) && matches!(*v, Ty::Never) => {
                Ty::EvolvingMap(k, v)
            }
            other => other,
        }
    }
}

// ── Display impls ────────────────────────────────────────────────────────────

/// Render an evolving container: the bare `placeholder` when every inner type is
/// `Never`, otherwise `base()` with a ` (evolving)` suffix when `show_evolving`.
fn render_evolving(
    placeholder: &str,
    base: impl FnOnce() -> String,
    all_never: bool,
    show_evolving: bool,
) -> String {
    if all_never {
        placeholder.to_string()
    } else if show_evolving {
        format!("{} (evolving)", base())
    } else {
        base()
    }
}

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

    /// Render, wrapping in parens when `needs` is set. Used for postfix
    /// (`[]`/`?`) context (`needs_postfix_parens()`) and function-return
    /// position (nested `Function`, so the outer `throws` clause stays
    /// associated with the outer callable rather than the returned one).
    fn render_parenthesized_if(&self, s: &dyn TyRenderStrategy, needs: bool) -> String {
        let inner = self.render_with(s);
        if needs { format!("({inner})") } else { inner }
    }

    /// Render with parentheses if needed for postfix (`[]`/`?`) context.
    fn render_as_postfix_base(&self, s: &dyn TyRenderStrategy) -> String {
        self.render_parenthesized_if(s, self.needs_postfix_parens())
    }

    /// The single structural renderer. Walks the type, delegating every
    /// package-, type-var-, and presentation-policy decision to `s`. All type
    /// rendering — canonical `Display`, user-facing diagnostics, LSP hover —
    /// funnels through here so the structure is described in exactly one place.
    pub fn render_with(&self, s: &dyn TyRenderStrategy) -> String {
        match self {
            Ty::Class(qn, type_args) | Ty::Interface(qn, type_args) => {
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
            Ty::Enum(qn) | Ty::TypeAlias(qn) => s.qtn(qn, true),
            Ty::EnumVariant(qn, v) => format!("{}.{v}", s.qtn(qn, true)),
            Ty::Primitive(p) => p.to_string(),
            Ty::List(inner) => format!("{}[]", inner.render_as_postfix_base(s)),
            Ty::Map(k, v) => format!("map<{}, {}>", k.render_with(s), v.render_with(s)),
            Ty::EvolvingList(inner) => {
                let all_never = matches!(**inner, Ty::Never);
                render_evolving(
                    "_[]",
                    || format!("{}[]", inner.render_as_postfix_base(s)),
                    all_never,
                    s.show_evolving(),
                )
            }
            Ty::EvolvingMap(k, v) => {
                let all_never = matches!(**k, Ty::Never) && matches!(**v, Ty::Never);
                render_evolving(
                    "map<_, _>",
                    || format!("map<{}, {}>", k.render_with(s), v.render_with(s)),
                    all_never,
                    s.show_evolving(),
                )
            }
            Ty::Union(members) => members
                .iter()
                .map(|m| m.render_with(s))
                .collect::<Vec<_>>()
                .join(" | "),
            Ty::Optional(inner) => format!("{}?", inner.render_as_postfix_base(s)),
            Ty::Literal(lit, _freshness) => lit.to_string(),
            Ty::Function {
                generic_params,
                generic_param_bounds,
                params,
                ret,
                throws,
                ..
            } => {
                use std::fmt::Write as _;

                let mut out = String::new();
                if !generic_params.is_empty() {
                    out.push('<');
                    for (i, param) in generic_params.iter().enumerate() {
                        if i > 0 {
                            out.push_str(", ");
                        }
                        out.push_str(param.as_ref());
                        if let Some(bound) = generic_param_bounds.get(i).and_then(Option::as_ref) {
                            let _ = write!(out, " extends {}", bound.render_with(s));
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
                    ret.render_parenthesized_if(s, matches!(**ret, Ty::Function { .. })),
                    throws.render_with(s),
                )
            }
            Ty::TypeVar(name) => s.type_var(name),
            Ty::Never => "never".to_string(),
            Ty::Void => "void".to_string(),
            Ty::BuiltinUnknown | Ty::Unknown => "unknown".to_string(),
            Ty::RustType => "$rust_type".to_string(),
            Ty::Type => "type".to_string(),
            Ty::Error => "!error".to_string(),
            Ty::Future(value, error) => {
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
