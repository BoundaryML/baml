//! Resolved type representation — the output of type resolution.

use std::fmt;

use baml_base::Name;
pub use baml_base::attr::TyAttr;
// `Package`, `QualifiedTypeName`, the reserved-package / synthetic-effect-param
// constants, and `is_synthetic_effect_param` now live in `baml_type` (the
// single home for the shared type vocabulary). Re-exported here so existing
// `crate::ty::…` / `baml_compiler2_tir::ty::…` paths keep working.
pub use baml_type::{
    Freshness, Package, PrimitiveType, QualifiedTypeName, RESERVED_USER_PACKAGE,
    SYNTHETIC_EFFECT_PARAM_PREFIX, is_synthetic_effect_param,
};

/// Resolved type — the output of type resolution (Pass 2).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Ty {
    /// A class type — just the name, no expansion.
    Class(QualifiedTypeName, Vec<Ty>, TyAttr),
    /// An interface type (BEP-044) — nominal contract. Subtyping is
    /// determined by explicit `implements I { ... }` blocks on classes.
    /// Generic args follow the same shape as `Class` for parameterised
    /// interfaces like `Container<T>`.
    Interface(QualifiedTypeName, Vec<Ty>, Vec<(Name, Ty)>, TyAttr),
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
    /// A | B | C (a `Primitive(Null)` member encodes optionality — `T?` lowers
    /// to `T | null`).
    Union(Vec<Ty>, TyAttr),
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
    /// Associated type projection, e.g. `P.Output` or `(T as Iterator).Item`.
    AssociatedTypeProjection {
        base: Box<Ty>,
        interface: Option<Box<Ty>>,
        member: Name,
        attr: TyAttr,
    },
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
            | Ty::Interface(_, _, _, a)
            | Ty::Enum(_, a)
            | Ty::EnumVariant(_, _, a)
            | Ty::TypeAlias(_, a)
            | Ty::Primitive(_, a)
            | Ty::List(_, a)
            | Ty::Map(_, _, a)
            | Ty::Union(_, a)
            | Ty::Literal(_, _, a)
            | Ty::EvolvingList(_, a)
            | Ty::EvolvingMap(_, _, a)
            | Ty::TypeVar(_, a)
            | Ty::Future(_, _, a) => a,
            Ty::AssociatedTypeProjection { attr, .. } => attr,
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
            | Ty::Interface(_, _, _, a)
            | Ty::Enum(_, a)
            | Ty::EnumVariant(_, _, a)
            | Ty::TypeAlias(_, a)
            | Ty::Primitive(_, a)
            | Ty::List(_, a)
            | Ty::Map(_, _, a)
            | Ty::Union(_, a)
            | Ty::Literal(_, _, a)
            | Ty::EvolvingList(_, a)
            | Ty::EvolvingMap(_, _, a)
            | Ty::TypeVar(_, a)
            | Ty::Future(_, _, a) => *a = new_attr,
            Ty::AssociatedTypeProjection { attr, .. } => *attr = new_attr,
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

    /// The `null` primitive with default attributes.
    pub fn primitive_null() -> Ty {
        Ty::Primitive(PrimitiveType::Null, TyAttr::default())
    }

    /// True if this is exactly the `null` primitive.
    pub fn is_null(&self) -> bool {
        matches!(self, Ty::Primitive(PrimitiveType::Null, _))
    }

    /// True if this is a union that includes `null` — i.e. an optional type.
    /// `?` lowers to `T | null`, so this is the canonical "is nullable"
    /// predicate that replaces matching on the former `Ty::Optional`.
    pub fn is_nullable_union(&self) -> bool {
        matches!(self, Ty::Union(members, _) if members.iter().any(Ty::is_null))
    }

    /// `T?` — sugar for `T | null`. Flattens so `(A | B)?` becomes a flat
    /// `A | B | null`, and stays idempotent (`T??` == `T?`, `null?` == `null`).
    #[must_use]
    pub fn nullable(inner: Ty) -> Ty {
        match inner {
            Ty::Union(mut members, attr) => {
                if !members.iter().any(Ty::is_null) {
                    members.push(Ty::primitive_null());
                }
                Ty::Union(members, attr)
            }
            n @ Ty::Primitive(PrimitiveType::Null, _) => n,
            other => Ty::Union(vec![other, Ty::primitive_null()], TyAttr::default()),
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
            Ty::Class(qn, type_args, _) => {
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
            Ty::Interface(qn, type_args, associated_bindings, _) => {
                let mut out = s.qtn(qn, false);
                if !type_args.is_empty() || !associated_bindings.is_empty() {
                    let mut args: Vec<_> = type_args.iter().map(|a| a.render_with(s)).collect();
                    args.extend(
                        associated_bindings
                            .iter()
                            .map(|(name, ty)| format!("{name} = {}", ty.render_with(s))),
                    );
                    out.push('<');
                    out.push_str(&args.join(", "));
                    out.push('>');
                } else if !qn.generic_params.is_empty() && s.show_unspecialized_placeholders() {
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
            Ty::Union(members, _) => {
                // `?` is sugar that exists only in source/lowering; after that a
                // nullable type is a plain union and renders as `T | null`.
                // Function members are parenthesized so a nullable callback reads
                // as `((..) -> ..) | null`, not a function with `throws .. | null`.
                members
                    .iter()
                    .map(|m| {
                        let rendered = m.render_with(s);
                        if matches!(m, Ty::Function { .. }) {
                            format!("({rendered})")
                        } else {
                            rendered
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" | ")
            }
            Ty::Literal(lit, _freshness, _) => lit.to_string(),
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
                    ret.render_as_function_result(s),
                    throws.render_with(s),
                )
            }
            Ty::TypeVar(name, _) => s.type_var(name),
            Ty::AssociatedTypeProjection {
                base,
                interface,
                member,
                ..
            } => {
                if let Some(interface) = interface {
                    format!(
                        "({} as {}).{}",
                        base.render_with(s),
                        interface.render_with(s),
                        member
                    )
                } else {
                    format!("{}.{}", base.render_with(s), member)
                }
            }
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
