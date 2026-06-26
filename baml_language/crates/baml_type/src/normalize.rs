//! Canonical type normalization, equivalence, and subtyping for [`Ty`].
//!
//! This is the **current-context** type algebra: it answers "do these two types
//! denote the same set of values *as the program is written*?" and "is every
//! value of `sub` also a value of `sup`?". Type variables and interface
//! existentials are compared **by identity** (`T` is the same type as `T`, `dyn I`
//! is the same type as `dyn I`, but `dyn I` is *not* the same type as a concrete
//! implementor `C`). This is distinct from the coherence overlap checker's
//! "possible-worlds" view (could these *ever* overlap under some instantiation?),
//! which is intentionally more permissive.
//!
//! # Representation
//!
//! Everything operates on [`Ty`] — the widest representation. The runtime
//! subenum layers ([`crate::RuntimeTy`], [`crate::RealizedTy`],
//! [`crate::ConcreteTy`]) are subsets of `Ty`, so a runtime caller widens up via
//! the infallible `Ty::from(..)` and calls the same checker. The compiler-only
//! variants (`Unknown`, `Error`, `EvolvingList`, `EvolvingMap`) are handled here
//! because inference asks subtype questions while those variants are live; a
//! runtime caller simply never produces them.
//!
//! # Context
//!
//! The nominal facts the algebra needs — what a type alias expands to, whether a
//! concrete type implements an interface, an interface's required interfaces, a
//! type variable's bound, and an enum's full variant set — are supplied by a
//! [`TypeContext`] the caller implements over its own registries. Following the
//! golden rule (a value never violates its static type), **every lookup fails
//! safe**: when the context cannot answer, the algebra declines to collapse,
//! absorb, or equate, so a missing fact can only yield "not *necessarily*
//! equivalent / subtype", never a false claim of equivalence or membership.

use std::collections::HashSet;

use crate::{
    FunctionParamMode, FunctionParamTy, Interface, Literal, MediaKind, Name, QualifiedTypeName, Ty,
    TyAttr,
};

// ═══════════════════════════════════════════════════════════════════════════
// CONTEXT
// ═══════════════════════════════════════════════════════════════════════════

/// Semantic lookups the type algebra needs, supplied by the caller over its own
/// registries.
///
/// Every method **fails safe**: a `None`/`false` answer (whether "definitely
/// not" or merely "cannot determine") makes the algebra conservative — it will
/// not collapse, absorb, or equate what it cannot confirm. A missing fact
/// therefore degrades only to "not necessarily equivalent / subtype".
pub trait TypeContext {
    /// The type a type alias expands to, or `None` if the alias is unknown.
    ///
    /// Recursion is discovered structurally (an alias that re-references itself
    /// during expansion becomes a μ-binder), so this need only return the direct
    /// right-hand side. An unknown alias is treated as an opaque leaf (equal only
    /// to the same unknown alias), never equated to any expansion.
    ///
    /// **Completeness is a precondition.** Well-formed BAML never contains an
    /// unresolved alias — a bad name is a compile error (`Ty::Unknown`) and only
    /// recursive aliases survive lowering as `Ty::TypeAlias`, so the map must
    /// cover the package's own aliases plus every exported dependency alias.
    /// This holds for dynamically compiled/grafted packages too: each is a
    /// complete, valid package before it is loaded. A `None` therefore is not an
    /// expected "partial context" — it means the supplied map was incomplete,
    /// and the algebra degrades conservatively (opaque, never equated; "not
    /// necessarily equivalent") rather than panicking or over-equating.
    fn alias_def(&self, name: &QualifiedTypeName) -> Option<Ty>;

    /// Whether the non-interface, non-type-variable `concrete` type implements
    /// `interface`, accounting for the interface's generic
    /// arguments, associated-type bindings, and the impl's bounds.
    ///
    /// Powers `C <: I` subtyping and the `C | I == I` union absorption (a
    /// concrete member subsumed by an existential member). `false` ⇒ no
    /// membership is claimed.
    fn implements_interface(&self, concrete: &Ty, interface: &Interface) -> bool;

    /// The declared bound of type variable `name` (an interface or a union of
    /// interfaces), or `None` if it is unbounded or unknown.
    ///
    /// Powers `T <: I` (and the `T | I == I` absorption) when `T`'s bound
    /// is — or transitively requires — `I`.
    fn type_var_bound(&self, name: &Name) -> Vec<Interface>;

    /// Whether interface `sub` requires interface `sup` (reflexively and
    /// transitively), accounting for generic arguments.
    ///
    /// Powers `A <: B` subtyping and the `A | B == B` absorption for
    /// existentials. `false` ⇒ no requirement is claimed.
    fn interface_requires(&self, sub: &Interface, sup: &Interface) -> bool;

    /// The complete set of variant names of an enum, or `None` if the enum is
    /// unknown.
    ///
    /// Powers the completeness collapse `E.A | E.B | … == E` (a union of *all* of
    /// an enum's variants is the enum itself). `None` ⇒ no collapse.
    fn enum_variants(&self, name: &QualifiedTypeName) -> Option<Vec<Name>>;

    /// The declared interface bounds of associated type `assoc` on `interface` —
    /// the `extends` clause on `type assoc extends …`, specialized through
    /// `interface`'s generic arguments. Empty if the member is unbounded or
    /// unknown (fail-safe → opaque, never over-claims).
    ///
    /// Powers `(_ as I<…>).assoc <: B` for a *still-symbolic* projection: it is a
    /// subtype of its bound's supertypes — the projection analogue of
    /// [`type_var_bound`](Self::type_var_bound). A realized-base projection is
    /// resolved to a concrete type upstream and never reaches this rule.
    ///
    /// The bound is a function of `(interface, assoc)` only; a `Self`-referential
    /// bound (one mentioning the implementor) is not expressible here — resolving
    /// `Self` over each returned [`Interface`] would be a later step.
    ///
    /// Defaults to no bound, so a context that does not resolve associated types
    /// leaves such projections opaque.
    fn associated_type_bound(&self, interface: &Interface, assoc: Name) -> Vec<Interface> {
        let _ = (interface, assoc);
        Vec::new()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PUBLIC API
// ═══════════════════════════════════════════════════════════════════════════

/// Normalize `ty` to its canonical form and render it back as a [`Ty`].
///
/// Two types are [`equivalent`] iff their canonical forms are structurally
/// equal. The canonical form applies the full set-theoretic algebra (union
/// flatten/sort/dedup, `never` removal, `unknown` absorption, literal-into-base
/// and enum-completeness collapse, interface absorption, alias expansion) so
/// that distinct spellings of the same type converge.
///
/// # Attributes are erased
///
/// The returned `Ty` carries `TyAttr::default()` on every node — SAP/streaming
/// annotations (`@stream.done`, `sap_in_progress`, …) are dropped, because they
/// are parsing metadata, not part of the set of values a type denotes (and so
/// must not affect [`equivalent`]/[`is_subtype`]). This makes the output a
/// canonical form for type *identity* (equality, display, debugging) — **not** an
/// attribute-preserving rewrite. Do not feed it into a position where SAP
/// annotations must survive (an LLM function's return type, a generated stream
/// companion); derive the canonical type from the original `Ty` there instead.
pub fn normalize<C: TypeContext>(ty: &Ty, ctx: &C) -> Ty {
    NormalTy::canonical(ty, ctx).into_ty()
}

/// Whether `a` and `b` denote the same type under the current context.
///
/// This is invariant equality, not assignability: use it where two spellings
/// must denote *the same* type (e.g. exact-type operator operands, interface
/// field implementations), not merely compatible ones.
pub fn equivalent<C: TypeContext>(a: &Ty, b: &Ty, ctx: &C) -> bool {
    NormalTy::canonical(a, ctx) == NormalTy::canonical(b, ctx)
}

/// Whether every value of `sub` is also a value of `sup` under the current
/// context (the subset relation).
pub fn is_subtype<C: TypeContext>(sub: &Ty, sup: &Ty, ctx: &C) -> bool {
    let sub = NormalTy::canonical(sub, ctx);
    let sup = NormalTy::canonical(sup, ctx);
    sub.is_subtype_of(&sup, ctx, &mut HashSet::new())
}

/// Whether no value of type `a` can ever be `==`-equal to a value of type `b` —
/// so a broad `==` between operands of these types is always `false`.
///
/// Sound and conservative: `true` only when *certain*, so it is a safe basis for
/// folding `==`/`!=` to a constant and for an "always false" diagnostic. It also
/// stays correct under additive dynamic-package mutation — it never relies on the
/// *absence* of an `Equals` (a custom one could be added later), only on facts
/// that hold regardless of any `Equals` implementation.
///
/// What it proves disjoint:
/// - **Different concrete categories** — `int`/`bigint`, `int`/`string`,
///   `list<_>`/`map<_,_>`, a class vs a list, etc.
/// - **Distinct instantiations of an invariant generic** — `Box<int>` vs
///   `Box<string>`, `list<int>` vs `list<string>`, `map<string,int>` vs
///   `map<string,bool>`. Generic constructors (classes, lists, maps, futures)
///   are invariant and their type arguments are real instance data, so two
///   instantiations sharing no equal-everywhere argument list are disjoint.
/// - **Distinct primitive literals** — `1` vs `2`, `1` vs `1n` (their built-in
///   reflexive equality is unoverridable). Floats are excluded (`NaN` /
///   decimal-representation aliasing).
///
/// (`unknown` is the determined top type, so `Box<unknown>` *is* disjoint from
/// `Box<int>` — distinct invariant instantiations — even though a bare `unknown`
/// operand overlaps everything.)
///
/// What it conservatively leaves overlapping (returns `false`):
/// - **Same enum** (`E.A` vs `E.B`, `E.A` vs `E`): a value's `eq` dispatches on
///   the enum, and a custom `Equals` on `E` could equate distinct variants.
/// - **An instantiation with a not-yet-resolved argument** (`Box<T>` for a
///   generic `T`, or an error sentinel): it could still resolve to match.
/// - Functions (not invariant — contravariant/covariant), watch accessors
///   (identity), interfaces, bare type variables, and a bare `unknown`.
pub fn definitely_disjoint<C: TypeContext>(a: &Ty, b: &Ty, ctx: &C) -> bool {
    NormalTy::canonical(a, ctx).is_disjoint_from(&NormalTy::canonical(b, ctx))
}

/// Whether a broad `==` between operands of types `a` and `b` is always `true`.
///
/// Holds only when both operands are pinned to the *same* single value whose
/// equality is the built-in, reflexive one that can never be replaced: the
/// operands are equivalent and that type is a non-float primitive literal
/// (`int`/`bigint`/`string`/`bool`) or `null`.
///
/// Deliberately excluded:
/// - **Enum-variant and class singletons.** A user type's `Equals` can be added
///   later by additively mutating its (dynamic) package, so the absence of an
///   `Equals` today is not a stable basis for baking in a constant — the
///   built-in reflexive equality is guaranteed only for types whose `Equals` the
///   orphan rule forbids overriding (primitives, `null`).
pub fn definitely_equal<C: TypeContext>(a: &Ty, b: &Ty, ctx: &C) -> bool {
    let a = NormalTy::canonical(a, ctx);
    a.is_unoverridable_singleton() && a == NormalTy::canonical(b, ctx)
}

/// The statically-known result of a broad `==` between operands of types `a` and
/// `b`, or `None` if it depends on the runtime values.
///
/// Combines [`definitely_disjoint`] and [`definitely_equal`] into one pass — it
/// canonicalizes each operand once rather than twice:
/// - `Some(false)` — the types are provably disjoint, so `==` is always `false`.
/// - `Some(true)` — both operands are the same unoverridable singleton, so `==`
///   is always `true`.
/// - `None` — the result is not statically determined.
///
/// See those two functions for the exact rules and their dynamic-package
/// soundness.
pub fn constant_equality<C: TypeContext>(a: &Ty, b: &Ty, ctx: &C) -> Option<bool> {
    let a = NormalTy::canonical(a, ctx);
    let b = NormalTy::canonical(b, ctx);
    if a.is_disjoint_from(&b) {
        Some(false)
    } else if a.is_unoverridable_singleton() && a == b {
        Some(true)
    } else {
        None
    }
}

impl NormalTy {
    /// Normalize and canonicalize a [`Ty`] in one step (the shared entry point).
    fn canonical<C: TypeContext>(ty: &Ty, ctx: &C) -> NormalTy {
        NormalTy::from_ty(ty, ctx, &mut HashSet::new()).canonicalize(ctx)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CONCRETE-TYPE DISJOINTNESS
// ═══════════════════════════════════════════════════════════════════════════

/// Top-level concrete category of a ground-headed type, used only for the
/// cross-category check (a value of one category is never a value of another).
/// Same-category pairs are decided by the structural arms of
/// [`NormalTy::is_disjoint_from`], not by this.
#[derive(PartialEq, Eq)]
enum Category {
    Int,
    Bigint,
    Float,
    String,
    Bool,
    Null,
    Uint8Array,
    Media(MediaKind),
    Void,
    RustType,
    Type,
    Resource,
    PromptAst,
    Class,
    List,
    Map,
    Enum,
    Function,
    Future,
    WatchAccessor,
}

impl NormalTy {
    /// Top-level concrete category of this type, or `None` for a non-ground head
    /// (union, interface, hole, type variable, …) for which no disjointness is
    /// provable.
    fn head_category(&self) -> Option<Category> {
        Some(match self {
            NormalTy::Int | NormalTy::Literal(Literal::Int(_)) => Category::Int,
            NormalTy::Bigint | NormalTy::Literal(Literal::Bigint(_)) => Category::Bigint,
            NormalTy::Float | NormalTy::Literal(Literal::Float(_)) => Category::Float,
            NormalTy::String | NormalTy::Literal(Literal::String(_)) => Category::String,
            NormalTy::Bool | NormalTy::Literal(Literal::Bool(_)) => Category::Bool,
            NormalTy::Null => Category::Null,
            NormalTy::Uint8Array => Category::Uint8Array,
            NormalTy::Media(kind) => Category::Media(*kind),
            NormalTy::Void => Category::Void,
            NormalTy::RustType => Category::RustType,
            NormalTy::Type => Category::Type,
            NormalTy::Resource => Category::Resource,
            NormalTy::PromptAst => Category::PromptAst,
            NormalTy::Class(..) => Category::Class,
            NormalTy::List(_) => Category::List,
            NormalTy::Map { .. } => Category::Map,
            NormalTy::Enum(_) | NormalTy::EnumVariant(..) => Category::Enum,
            NormalTy::Function { .. } => Category::Function,
            NormalTy::Future(..) => Category::Future,
            NormalTy::WatchAccessor(_) => Category::WatchAccessor,
            // Not a ground concrete head — nothing provable.
            NormalTy::Interface(..)
            | NormalTy::Union(_)
            | NormalTy::AssociatedTypeProjection { .. }
            | NormalTy::Mu { .. }
            | NormalTy::RecVar(_)
            | NormalTy::TypeVar(_)
            | NormalTy::OpaqueAlias(_)
            | NormalTy::Never
            | NormalTy::BuiltinUnknown
            | NormalTy::Unknown
            | NormalTy::Error => return None,
        })
    }

    /// Whether this type is *determined* — every position is a fixed type rather
    /// than a placeholder that could still resolve to something else. Only ground
    /// arguments make two generic instantiations *provably* distinct.
    ///
    /// The non-ground cases are the error-recovery sentinels (`Unknown`, `Error`)
    /// and the not-yet-resolved variables (a generic `TypeVar`, an unresolved
    /// `AssociatedTypeProjection`, an `OpaqueAlias`) — each could later stand for
    /// the same type as the other side. The `unknown` top type (`BuiltinUnknown`)
    /// is *not* one of these: it is user-written and fully determined, so
    /// `Box<unknown>` is a distinct invariant instantiation from `Box<int>`.
    fn is_ground(&self) -> bool {
        match self {
            NormalTy::Unknown
            | NormalTy::Error
            | NormalTy::TypeVar(_)
            | NormalTy::AssociatedTypeProjection { .. }
            | NormalTy::OpaqueAlias(_) => false,
            NormalTy::Int
            | NormalTy::Bigint
            | NormalTy::Float
            | NormalTy::String
            | NormalTy::Bool
            | NormalTy::Null
            | NormalTy::Uint8Array
            | NormalTy::Media(_)
            | NormalTy::Void
            | NormalTy::RustType
            | NormalTy::Type
            | NormalTy::Resource
            | NormalTy::PromptAst
            | NormalTy::Literal(_)
            | NormalTy::Enum(_)
            | NormalTy::EnumVariant(..)
            // The `unknown` top type is a determined, user-written type.
            | NormalTy::BuiltinUnknown
            // `never` is a determined (empty) type, not an inference hole.
            | NormalTy::Never
            // A μ-bound recursion variable refers to its enclosing (ground) μ-type.
            | NormalTy::RecVar(_) => true,
            NormalTy::List(inner)
            | NormalTy::WatchAccessor(inner)
            | NormalTy::Mu { body: inner, .. } => inner.is_ground(),
            NormalTy::Map { key, value } | NormalTy::Future(key, value) => {
                key.is_ground() && value.is_ground()
            }
            NormalTy::Union(members) => members.iter().all(NormalTy::is_ground),
            NormalTy::Class(_, args) | NormalTy::Interface(_, args, _) => {
                args.iter().all(NormalTy::is_ground)
            }
            NormalTy::Function {
                params,
                ret,
                throws,
            } => {
                params.iter().all(|p| p.ty.is_ground()) && ret.is_ground() && throws.is_ground()
            }
        }
    }

    /// Whether `self` and `other`, used as invariant generic arguments, make
    /// their instantiations disjoint: both are fully ground and not the same
    /// realized type. A hole leaves it unprovable (it could realize to match).
    fn arg_forces_disjoint(&self, other: &NormalTy) -> bool {
        self.is_ground() && other.is_ground() && self != other
    }

    /// Whether no value of `self` can ever be `==`-equal to a value of `other`
    /// (the structural core of [`definitely_disjoint`]).
    fn is_disjoint_from(&self, other: &NormalTy) -> bool {
        match (self, other) {
            // A union is disjoint from `rhs` iff every member is.
            (NormalTy::Union(members), rhs) => members.iter().all(|m| m.is_disjoint_from(rhs)),
            (lhs, NormalTy::Union(members)) => members.iter().all(|m| lhs.is_disjoint_from(m)),

            // Generic constructors are invariant and their type arguments are real
            // instance data, so two instantiations are disjoint as soon as one
            // argument pair is provably a different realized type. A different
            // class name is disjoint outright (nominal).
            (NormalTy::Class(c, xa), NormalTy::Class(d, xb)) => {
                c != d
                    || xa.len() != xb.len()
                    || xa.iter().zip(xb).any(|(x, y)| x.arg_forces_disjoint(y))
            }
            (NormalTy::List(a), NormalTy::List(b)) => a.arg_forces_disjoint(b),
            (NormalTy::Map { key: k1, value: v1 }, NormalTy::Map { key: k2, value: v2 }) => {
                k1.arg_forces_disjoint(k2) || v1.arg_forces_disjoint(v2)
            }
            (NormalTy::Future(v1, e1), NormalTy::Future(v2, e2)) => {
                v1.arg_forces_disjoint(v2) || e1.arg_forces_disjoint(e2)
            }

            // Functions are *not* invariant (contravariant args, covariant
            // return/throws), and watch accessors compare by identity — neither is
            // provably disjoint from another of its kind here.
            (NormalTy::Function { .. }, NormalTy::Function { .. })
            | (NormalTy::WatchAccessor(_), NormalTy::WatchAccessor(_)) => false,

            // A value's `eq` dispatches on its enum, so only *different* enums are
            // disjoint — a custom (or later-added) `Equals` on one enum could
            // equate distinct variants of it.
            (NormalTy::Enum(e), NormalTy::Enum(f))
            | (NormalTy::EnumVariant(e, _), NormalTy::EnumVariant(f, _))
            | (NormalTy::EnumVariant(e, _), NormalTy::Enum(f))
            | (NormalTy::Enum(f), NormalTy::EnumVariant(e, _)) => e != f,

            // Primitive literals: distinct values can never be equal under their
            // unoverridable built-in equality (floats excluded — `NaN` and decimal
            // aliasing such as `1.0`/`1.00`).
            (NormalTy::Literal(x), NormalTy::Literal(y)) => {
                !matches!(x, Literal::Float(_)) && !matches!(y, Literal::Float(_)) && x != y
            }
            (NormalTy::Literal(lit), rhs) | (rhs, NormalTy::Literal(lit)) => {
                match rhs.head_category() {
                    Some(cat) => cat != Category::of_literal(lit),
                    None => false,
                }
            }

            // Otherwise: disjoint iff both are ground concrete heads of different
            // categories (`int` vs `string`, `list` vs `map`, a class vs an enum).
            _ => match (self.head_category(), other.head_category()) {
                (Some(x), Some(y)) => x != y,
                _ => false,
            },
        }
    }

    /// Whether this type pins its operand to a single value whose equality is the
    /// built-in reflexive one that the orphan rule forbids overriding — so two
    /// operands of this (same) type are unconditionally equal. Primitives' and
    /// `null`'s `Equals` can never be replaced; floats are excluded for
    /// `NaN`-safety.
    fn is_unoverridable_singleton(&self) -> bool {
        matches!(
            self,
            NormalTy::Null
                | NormalTy::Literal(
                    Literal::Int(_) | Literal::Bigint(_) | Literal::String(_) | Literal::Bool(_)
                )
        )
    }
}

impl Category {
    fn of_literal(lit: &Literal) -> Category {
        match lit {
            Literal::Int(_) => Category::Int,
            Literal::Bigint(_) => Category::Bigint,
            Literal::Float(_) => Category::Float,
            Literal::String(_) => Category::String,
            Literal::Bool(_) => Category::Bool,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// NORMALIZED TYPE (private)
// ═══════════════════════════════════════════════════════════════════════════

/// Normalized structural type: aliases resolved, attributes and literal
/// freshness erased, recursion made explicit with μ-binders.
///
/// Ordering (`PartialOrd`/`Ord`) is the canonical sort key for union members; it
/// has no semantic meaning beyond producing a deterministic canonical form.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum NormalTy {
    // Primitive leaves
    Int,
    Bigint,
    Float,
    String,
    Bool,
    Null,
    Uint8Array,
    Media(MediaKind),
    // Nominal opaque leaves — each compatible only with itself.
    Void,
    RustType,
    Type,
    Resource,
    PromptAst,
    // Literal — a single value as a type. Freshness is erased.
    Literal(Literal),
    // Nominal references
    Class(QualifiedTypeName, Vec<NormalTy>),
    Interface(QualifiedTypeName, Vec<NormalTy>, Vec<(Name, NormalTy)>),
    Enum(QualifiedTypeName),
    EnumVariant(QualifiedTypeName, Name),
    // Constructors
    List(Box<NormalTy>),
    Map {
        key: Box<NormalTy>,
        value: Box<NormalTy>,
    },
    Union(Vec<NormalTy>),
    Function {
        params: Vec<NormalParam>,
        ret: Box<NormalTy>,
        throws: Box<NormalTy>,
    },
    Future(Box<NormalTy>, Box<NormalTy>),
    WatchAccessor(Box<NormalTy>),
    AssociatedTypeProjection {
        base: Box<NormalTy>,
        interface: Option<Box<NormalTy>>,
        member: Name,
    },
    // Recursion
    Mu {
        var: QualifiedTypeName,
        body: Box<NormalTy>,
    },
    /// μ-bound recursion variable (a back-reference to an enclosing [`NormalTy::Mu`]).
    RecVar(QualifiedTypeName),
    /// A generic type parameter — opaque, compatible only with itself, its
    /// bound's supertypes, and the top type.
    TypeVar(Name),
    /// An alias the context could not resolve — opaque, equal only to the same
    /// unresolved alias (fail-safe; never equated to an expansion).
    OpaqueAlias(QualifiedTypeName),
    // Special forms
    /// Bottom — a subtype of every type.
    Never,
    /// The explicit `unknown` keyword — top, a supertype of every type.
    BuiltinUnknown,
    /// Error-recovery sentinel — bidirectionally compatible to suppress cascades.
    Unknown,
    /// Hard-error sentinel — bidirectionally compatible, like [`NormalTy::Unknown`].
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct NormalParam {
    name: Option<Name>,
    ty: NormalTy,
    mode: FunctionParamMode,
}

impl NormalTy {
    // ── conversion in: Ty → NormalTy ───────────────────────────────────────

    fn from_ty<C: TypeContext>(
        ty: &Ty,
        ctx: &C,
        expanding: &mut HashSet<QualifiedTypeName>,
    ) -> NormalTy {
        match ty {
            Ty::Int { .. } => NormalTy::Int,
            Ty::Bigint { .. } => NormalTy::Bigint,
            Ty::Float { .. } => NormalTy::Float,
            Ty::String { .. } => NormalTy::String,
            Ty::Bool { .. } => NormalTy::Bool,
            Ty::Null { .. } => NormalTy::Null,
            Ty::Uint8Array { .. } => NormalTy::Uint8Array,
            Ty::Media(kind, _) => NormalTy::Media(*kind),
            Ty::Void { .. } => NormalTy::Void,
            Ty::RustType { .. } => NormalTy::RustType,
            Ty::Type { .. } => NormalTy::Type,
            Ty::Resource { .. } => NormalTy::Resource,
            Ty::PromptAst { .. } => NormalTy::PromptAst,
            Ty::BuiltinUnknown { .. } => NormalTy::BuiltinUnknown,
            Ty::Never { .. } => NormalTy::Never,
            Ty::Unknown { .. } => NormalTy::Unknown,
            // INVARIANT: every `_` inference hole is filled — or replaced with
            // `Ty::Error` — during inference, BEFORE any normalization /
            // equivalence / subtype check. Normalizing a hole is unsound: a
            // "matches-anything" sentinel makes both `Box<int>` and `Box<string>`
            // equal to `Box<_>`, which transitively (and falsely) equates
            // `Box<int>` with `Box<string>`. There is no sound sentinel here, so a
            // hole reaching normalization is a compiler bug, not a case to
            // tolerate. (See `compiler2_tir::builder`: the `let`-binding path
            // infers, fills, and only then checks; un-fillable holes become
            // `Ty::Error` at the pattern ascription before any check runs.)
            Ty::Infer { .. } => unreachable!(
                "inference hole `_` reached type normalization; it must be filled \
                 (or replaced with `Ty::Error`) during inference before any \
                 equivalence/subtype check"
            ),
            Ty::Error { .. } => NormalTy::Error,
            // Freshness is a compiler-only widening flag, irrelevant to type identity.
            Ty::Literal(lit, _freshness, _) => NormalTy::Literal(lit.clone()),
            Ty::Class(qn, args, _) => {
                NormalTy::Class(qn.clone(), Self::from_tys(args, ctx, expanding))
            }
            Ty::Interface(qn, args, bindings, _) => {
                let mut bindings: Vec<_> = bindings
                    .iter()
                    .map(|(name, ty)| (name.clone(), Self::from_ty(ty, ctx, expanding)))
                    .collect();
                bindings.sort_by(|(a, _), (b, _)| a.cmp(b));
                NormalTy::Interface(qn.clone(), Self::from_tys(args, ctx, expanding), bindings)
            }
            Ty::Enum(qn, _) => NormalTy::Enum(qn.clone()),
            Ty::EnumVariant(qn, v, _) => NormalTy::EnumVariant(qn.clone(), v.clone()),
            // Evolving containers are the list/map analogues during inference;
            // their type identity is the same as the frozen form.
            Ty::List(inner, _) | Ty::EvolvingList(inner, _) => {
                NormalTy::List(Box::new(Self::from_ty(inner, ctx, expanding)))
            }
            Ty::Map { key, value, .. } | Ty::EvolvingMap(key, value, _) => NormalTy::Map {
                key: Box::new(Self::from_ty(key, ctx, expanding)),
                value: Box::new(Self::from_ty(value, ctx, expanding)),
            },
            Ty::Union(members, _) => NormalTy::Union(Self::from_tys(members, ctx, expanding)),
            Ty::Function {
                params,
                ret,
                throws,
                ..
            } => NormalTy::Function {
                params: params
                    .iter()
                    .map(|p| NormalParam {
                        name: p.name.clone(),
                        ty: Self::from_ty(&p.ty, ctx, expanding),
                        mode: p.mode,
                    })
                    .collect(),
                ret: Box::new(Self::from_ty(ret, ctx, expanding)),
                throws: Box::new(Self::from_ty(throws, ctx, expanding)),
            },
            Ty::Future(value, error, _) => NormalTy::Future(
                Box::new(Self::from_ty(value, ctx, expanding)),
                Box::new(Self::from_ty(error, ctx, expanding)),
            ),
            Ty::WatchAccessor(inner, _) => {
                NormalTy::WatchAccessor(Box::new(Self::from_ty(inner, ctx, expanding)))
            }
            Ty::TypeVar(name, _) => NormalTy::TypeVar(name.clone()),
            Ty::AssociatedTypeProjection {
                base,
                interface,
                member,
                ..
            } => NormalTy::AssociatedTypeProjection {
                base: Box::new(Self::from_ty(base, ctx, expanding)),
                interface: interface
                    .as_ref()
                    .map(|i| Box::new(Self::from_ty(&i.to_ty(), ctx, expanding))),
                member: member.clone(),
            },
            Ty::TypeAlias(qn, _) => {
                if expanding.contains(qn) {
                    // Back-edge: we are already expanding this alias, so this is
                    // the recursive occurrence. The enclosing expansion wraps the
                    // result in a μ-binder over `qn`.
                    return NormalTy::RecVar(qn.clone());
                }
                let Some(def) = ctx.alias_def(qn) else {
                    // Fail-safe: an unresolvable alias is opaque, never equated
                    // to an expansion we cannot see.
                    return NormalTy::OpaqueAlias(qn.clone());
                };
                expanding.insert(qn.clone());
                let body = Self::from_ty(&def, ctx, expanding);
                expanding.remove(qn);
                if body.mentions_rec_var(qn) {
                    NormalTy::Mu {
                        var: qn.clone(),
                        body: Box::new(body),
                    }
                } else {
                    body
                }
            }
        }
    }

    fn from_tys<C: TypeContext>(
        tys: &[Ty],
        ctx: &C,
        expanding: &mut HashSet<QualifiedTypeName>,
    ) -> Vec<NormalTy> {
        tys.iter()
            .map(|t| Self::from_ty(t, ctx, expanding))
            .collect()
    }

    /// Whether this type contains a back-reference to μ-variable `var`.
    fn mentions_rec_var(&self, var: &QualifiedTypeName) -> bool {
        match self {
            NormalTy::RecVar(v) => v == var,
            // A nested μ shadowing the same name rebinds it; stop descending.
            NormalTy::Mu { var: v, body } => v != var && body.mentions_rec_var(var),
            NormalTy::Class(_, args) => args.iter().any(|a| a.mentions_rec_var(var)),
            NormalTy::Interface(_, args, bindings) => {
                args.iter().any(|a| a.mentions_rec_var(var))
                    || bindings.iter().any(|(_, t)| t.mentions_rec_var(var))
            }
            NormalTy::List(inner) | NormalTy::WatchAccessor(inner) => inner.mentions_rec_var(var),
            NormalTy::Map { key, value } => {
                key.mentions_rec_var(var) || value.mentions_rec_var(var)
            }
            NormalTy::Union(members) => members.iter().any(|m| m.mentions_rec_var(var)),
            NormalTy::Function {
                params,
                ret,
                throws,
            } => {
                params.iter().any(|p| p.ty.mentions_rec_var(var))
                    || ret.mentions_rec_var(var)
                    || throws.mentions_rec_var(var)
            }
            NormalTy::Future(value, error) => {
                value.mentions_rec_var(var) || error.mentions_rec_var(var)
            }
            NormalTy::AssociatedTypeProjection {
                base, interface, ..
            } => {
                base.mentions_rec_var(var)
                    || interface.as_ref().is_some_and(|i| i.mentions_rec_var(var))
            }
            NormalTy::Int
            | NormalTy::Bigint
            | NormalTy::Float
            | NormalTy::String
            | NormalTy::Bool
            | NormalTy::Null
            | NormalTy::Uint8Array
            | NormalTy::Media(_)
            | NormalTy::Void
            | NormalTy::RustType
            | NormalTy::Type
            | NormalTy::Resource
            | NormalTy::PromptAst
            | NormalTy::Literal(_)
            | NormalTy::Enum(_)
            | NormalTy::EnumVariant(_, _)
            | NormalTy::TypeVar(_)
            | NormalTy::OpaqueAlias(_)
            | NormalTy::Never
            | NormalTy::BuiltinUnknown
            | NormalTy::Unknown
            | NormalTy::Error => false,
        }
    }

    // ── canonicalization ───────────────────────────────────────────────────

    /// Rewrite to a unique canonical form: children canonicalized bottom-up,
    /// unions reduced by the full set algebra.
    fn canonicalize<C: TypeContext>(self, ctx: &C) -> NormalTy {
        match self {
            NormalTy::Class(qn, args) => {
                NormalTy::Class(qn, args.into_iter().map(|a| a.canonicalize(ctx)).collect())
            }
            NormalTy::Interface(qn, args, bindings) => {
                let mut bindings: Vec<_> = bindings
                    .into_iter()
                    .map(|(name, ty)| (name, ty.canonicalize(ctx)))
                    .collect();
                bindings.sort_by(|(a, _), (b, _)| a.cmp(b));
                NormalTy::Interface(
                    qn,
                    args.into_iter().map(|a| a.canonicalize(ctx)).collect(),
                    bindings,
                )
            }
            NormalTy::List(inner) => NormalTy::List(Box::new(inner.canonicalize(ctx))),
            NormalTy::Map { key, value } => NormalTy::Map {
                key: Box::new(key.canonicalize(ctx)),
                value: Box::new(value.canonicalize(ctx)),
            },
            NormalTy::Future(value, error) => NormalTy::Future(
                Box::new(value.canonicalize(ctx)),
                Box::new(error.canonicalize(ctx)),
            ),
            NormalTy::WatchAccessor(inner) => {
                NormalTy::WatchAccessor(Box::new(inner.canonicalize(ctx)))
            }
            NormalTy::AssociatedTypeProjection {
                base,
                interface,
                member,
            } => NormalTy::AssociatedTypeProjection {
                base: Box::new(base.canonicalize(ctx)),
                interface: interface.map(|i| Box::new(i.canonicalize(ctx))),
                member,
            },
            NormalTy::Function {
                params,
                ret,
                throws,
            } => {
                // Required params are positional (names erased, order preserved);
                // optional params are keyed by name (order-insensitive, sorted).
                let mut required = Vec::new();
                let mut optional = Vec::new();
                for p in params {
                    let ty = p.ty.canonicalize(ctx);
                    match p.mode {
                        FunctionParamMode::Required => required.push(NormalParam {
                            name: None,
                            ty,
                            mode: FunctionParamMode::Required,
                        }),
                        FunctionParamMode::Optional => optional.push(NormalParam {
                            name: p.name,
                            ty,
                            mode: FunctionParamMode::Optional,
                        }),
                    }
                }
                optional.sort();
                required.extend(optional);
                NormalTy::Function {
                    params: required,
                    ret: Box::new(ret.canonicalize(ctx)),
                    throws: Box::new(throws.canonicalize(ctx)),
                }
            }
            NormalTy::Mu { var, body } => {
                let body = body.canonicalize(ctx);
                // A μ whose body no longer mentions its variable is not actually
                // recursive (an absorption may have removed the back-edge).
                if body.mentions_rec_var(&var) {
                    NormalTy::Mu {
                        var,
                        body: Box::new(body),
                    }
                } else {
                    body
                }
            }
            NormalTy::Union(members) => {
                let members = members.into_iter().map(|m| m.canonicalize(ctx)).collect();
                Self::canonicalize_union(members, ctx)
            }
            leaf => leaf,
        }
    }

    // ── subtyping ──────────────────────────────────────────────────────────

    /// Invariant-position compatibility for a generic type argument: the two
    /// types must be mutual subtypes (i.e. equivalent). Generic constructors —
    /// classes, lists, maps, futures, interface-existentials — are invariant in
    /// BAML (type arguments are real instance data), so `T<A>` relates to `T<B>`
    /// only when every `A`/`B` pair is compatible here.
    ///
    /// Error-recovery sentinels (`Unknown`, `Error`) satisfy this either way via
    /// [`Self::is_subtype_of`]'s bidirectional escape, so a hole argument still
    /// matches anything. The `unknown` top type does *not* — it is a real,
    /// distinct argument (`Box<unknown>` is not `Box<int>`).
    fn invariant_compatible<C: TypeContext>(
        &self,
        other: &NormalTy,
        ctx: &C,
        assumptions: &mut HashSet<(NormalTy, NormalTy)>,
    ) -> bool {
        self.is_subtype_of(other, ctx, assumptions) && other.is_subtype_of(self, ctx, assumptions)
    }

    /// Equirecursive subtyping with co-inductive assumptions. Operands must
    /// already be canonical.
    ///
    /// Purely structural except for the nominal facts drawn from the context
    /// (`C <: I`, `A <: B` via requires, `T <: I` via bound). There are no
    /// representation-changing coercions: `int` is a subtype of neither `bigint`
    /// nor `float`; the only widenings are literal-into-base and variant-into-enum.
    fn is_subtype_of<C: TypeContext>(
        &self,
        sup: &NormalTy,
        ctx: &C,
        assumptions: &mut HashSet<(NormalTy, NormalTy)>,
    ) -> bool {
        let pair = (self.clone(), sup.clone());
        if assumptions.contains(&pair) {
            return true;
        }
        if self == sup {
            return true;
        }
        // Bottom is a subtype of everything; top is a supertype of everything.
        if matches!(self, NormalTy::Never) {
            return true;
        }
        if matches!(sup, NormalTy::BuiltinUnknown) {
            return true;
        }
        // `unknown` (top) is a subtype of nothing else (reflexivity handled above).
        if matches!(self, NormalTy::BuiltinUnknown) {
            return false;
        }
        // Error-recovery sentinels are bidirectionally compatible to suppress
        // cascading diagnostics. A runtime caller never produces these.
        if matches!(self, NormalTy::Unknown | NormalTy::Error)
            || matches!(sup, NormalTy::Unknown | NormalTy::Error)
        {
            return true;
        }

        assumptions.insert(pair.clone());
        let result = match (self, sup) {
            // μ-unfolding (equirecursive).
            (NormalTy::Mu { var, body }, _) => {
                body.substitute(var, self)
                    .is_subtype_of(sup, ctx, assumptions)
            }
            (_, NormalTy::Mu { var, body }) => {
                self.is_subtype_of(&body.substitute(var, sup), ctx, assumptions)
            }

            // Union decomposition. `Union <: T` must precede `T <: Union` so a
            // union on the left is not mistaken for a single member of the right.
            (NormalTy::Union(members), _) => members
                .iter()
                .all(|m| m.is_subtype_of(sup, ctx, assumptions)),
            (_, NormalTy::Union(members)) => members
                .iter()
                .any(|m| self.is_subtype_of(m, ctx, assumptions)),

            // A type variable is a subtype of `sup` if *any* of its bounds is.
            // The bounds are a conjunction (Rust's `T: A + B`): a value filling
            // `T` implements every listed interface, so it suffices that one bound
            // already proves membership in `sup`. (Same-var reflexivity and
            // `T <: T | U` are handled by the rules above.)
            (NormalTy::TypeVar(name), _) => ctx.type_var_bound(name).iter().any(|bound| {
                NormalTy::canonical(&bound.to_ty(), ctx).is_subtype_of(sup, ctx, assumptions)
            }),

            // A still-symbolic associated-type projection is a subtype of `sup` if
            // any of its associated type's declared bounds is — the projection
            // analogue of the `TypeVar` rule above. Fires only when the projection
            // carries a resolved interface; an unresolved one (`interface: None`)
            // stays opaque (equal only to itself, via reflexivity). A realized-base
            // projection is resolved to a concrete type upstream and never reaches
            // here. Must precede the interface arms below, which would otherwise ask
            // `implements_interface` about a non-concrete projection.
            (
                NormalTy::AssociatedTypeProjection {
                    interface: Some(iface),
                    member,
                    ..
                },
                _,
            ) => (**iface).clone().into_interface().is_some_and(|i| {
                ctx.associated_type_bound(&i, member.clone())
                    .iter()
                    .any(|bound| {
                        NormalTy::canonical(&bound.to_ty(), ctx).is_subtype_of(
                            sup,
                            ctx,
                            assumptions,
                        )
                    })
            }),

            // Concrete (or any non-interface) type implementing an interface.
            (sub, NormalTy::Interface(qn, args, bindings))
                if !matches!(sub, NormalTy::Interface(..)) =>
            {
                ctx.implements_interface(
                    &sub.clone().into_ty(),
                    &Self::interface_constraint(qn, args, bindings),
                )
            }
            // Interface-to-interface: `A <: B` iff `A` requires `B`.
            (
                NormalTy::Interface(sub_qn, sub_args, sub_bindings),
                NormalTy::Interface(sup_qn, sup_args, sup_bindings),
            ) => ctx.interface_requires(
                &Self::interface_constraint(sub_qn, sub_args, sub_bindings),
                &Self::interface_constraint(sup_qn, sup_args, sup_bindings),
            ),

            // Generic arguments are invariant — for classes, lists, and maps
            // alike. This is load-bearing for soundness: with covariant elements
            // `list<Dog> <: list<Animal>` would hold, and mutating through the
            // `Animal` view could store a non-`Dog` (TYPE_SYSTEM.md §Variance). A
            // "hole" (inference placeholder) matches anything; otherwise the
            // arguments must be mutual subtypes (of which structural equality is
            // the special case). An empty `[]`/`{}` is given a concrete element
            // type at its inference site, so it needs no widening here.
            (NormalTy::Class(q1, a1), NormalTy::Class(q2, a2))
                if q1 == q2 && a1.len() == a2.len() =>
            {
                a1.iter()
                    .zip(a2.iter())
                    .all(|(a, b)| a.invariant_compatible(b, ctx, assumptions))
            }
            (NormalTy::List(a), NormalTy::List(b)) => a.invariant_compatible(b, ctx, assumptions),
            (NormalTy::Map { key: k1, value: v1 }, NormalTy::Map { key: k2, value: v2 }) => {
                k1.invariant_compatible(k2, ctx, assumptions)
                    && v1.invariant_compatible(v2, ctx, assumptions)
            }

            // Future/WatchAccessor are invariant containers.
            (NormalTy::Future(v1, e1), NormalTy::Future(v2, e2)) => {
                v1.invariant_compatible(v2, ctx, assumptions)
                    && e1.invariant_compatible(e2, ctx, assumptions)
            }
            (NormalTy::WatchAccessor(a), NormalTy::WatchAccessor(b)) => {
                a.invariant_compatible(b, ctx, assumptions)
            }

            // Literal types are subtypes of their (same-representation) base only.
            (NormalTy::Literal(Literal::Int(_)), NormalTy::Int) => true,
            (NormalTy::Literal(Literal::Bigint(_)), NormalTy::Bigint) => true,
            (NormalTy::Literal(Literal::Float(_)), NormalTy::Float) => true,
            (NormalTy::Literal(Literal::String(_)), NormalTy::String) => true,
            (NormalTy::Literal(Literal::Bool(_)), NormalTy::Bool) => true,

            // A variant is a subtype of its enum.
            (NormalTy::EnumVariant(e, _), NormalTy::Enum(sup_e)) => e == sup_e,

            // Function subtyping: contravariant params, covariant return/throws.
            (
                NormalTy::Function {
                    params: p1,
                    ret: r1,
                    throws: t1,
                },
                NormalTy::Function {
                    params: p2,
                    ret: r2,
                    throws: t2,
                },
            ) => {
                r1.is_subtype_of(r2, ctx, assumptions)
                    && t1.is_subtype_of(t2, ctx, assumptions)
                    && NormalParam::list_subtype(p1, p2, ctx, assumptions)
            }

            _ => false,
        };
        assumptions.remove(&pair);
        result
    }

    // ── conversion out: NormalTy → Ty ──────────────────────────────────────

    fn into_ty(self) -> Ty {
        let attr = TyAttr::default();
        match self {
            NormalTy::Int => Ty::Int { attr },
            NormalTy::Bigint => Ty::Bigint { attr },
            NormalTy::Float => Ty::Float { attr },
            NormalTy::String => Ty::String { attr },
            NormalTy::Bool => Ty::Bool { attr },
            NormalTy::Null => Ty::Null { attr },
            NormalTy::Uint8Array => Ty::Uint8Array { attr },
            NormalTy::Media(kind) => Ty::Media(kind, attr),
            NormalTy::Void => Ty::Void { attr },
            NormalTy::RustType => Ty::RustType { attr },
            NormalTy::Type => Ty::Type { attr },
            NormalTy::Resource => Ty::Resource { attr },
            NormalTy::PromptAst => Ty::PromptAst { attr },
            NormalTy::BuiltinUnknown => Ty::BuiltinUnknown { attr },
            NormalTy::Never => Ty::Never { attr },
            NormalTy::Unknown => Ty::Unknown { attr },
            NormalTy::Error => Ty::Error { attr },
            NormalTy::Literal(lit) => Ty::Literal(lit, crate::Freshness::Regular, attr),
            NormalTy::Class(qn, args) => Ty::Class(qn, Self::into_tys(args), attr),
            NormalTy::Interface(qn, args, bindings) => Ty::Interface(
                qn,
                Self::into_tys(args),
                bindings
                    .into_iter()
                    .map(|(name, ty)| (name, ty.into_ty()))
                    .collect(),
                attr,
            ),
            NormalTy::Enum(qn) => Ty::Enum(qn, attr),
            NormalTy::EnumVariant(qn, v) => Ty::EnumVariant(qn, v, attr),
            NormalTy::List(inner) => Ty::List(Box::new(inner.into_ty()), attr),
            NormalTy::Map { key, value } => Ty::Map {
                key: Box::new(key.into_ty()),
                value: Box::new(value.into_ty()),
                attr,
            },
            NormalTy::Union(members) => Ty::Union(Self::into_tys(members), attr),
            NormalTy::Function {
                params,
                ret,
                throws,
            } => Ty::Function {
                params: params
                    .into_iter()
                    .map(|p| FunctionParamTy {
                        name: p.name,
                        ty: p.ty.into_ty(),
                        mode: p.mode,
                    })
                    .collect(),
                ret: Box::new(ret.into_ty()),
                throws: Box::new(throws.into_ty()),
                attr,
            },
            NormalTy::Future(value, error) => {
                Ty::Future(Box::new(value.into_ty()), Box::new(error.into_ty()), attr)
            }
            NormalTy::WatchAccessor(inner) => Ty::WatchAccessor(Box::new(inner.into_ty()), attr),
            NormalTy::AssociatedTypeProjection {
                base,
                interface,
                member,
            } => Ty::AssociatedTypeProjection {
                base: Box::new(base.into_ty()),
                interface: interface.and_then(|i| i.into_interface()).map(Box::new),
                member,
                attr,
            },
            NormalTy::TypeVar(name) => Ty::TypeVar(name, attr),
            // μ-binders and recursion variables round-trip through the alias name.
            NormalTy::Mu { body, .. } => body.into_ty(),
            NormalTy::RecVar(qn) | NormalTy::OpaqueAlias(qn) => Ty::TypeAlias(qn, attr),
        }
    }
}

impl NormalTy {
    fn into_tys(tys: Vec<NormalTy>) -> Vec<Ty> {
        tys.into_iter().map(NormalTy::into_ty).collect()
    }

    /// The [`Interface`] constraint denoted by a `NormalTy::Interface`'s parts —
    /// its name, generic input arguments, and associated-type bindings, converted
    /// back to `Ty`. This is the precise interface shape handed to the
    /// [`TypeContext`] membership (`implements_interface`) and requires
    /// (`interface_requires`) oracles, so they never have to re-destructure a
    /// loose `Ty` to recover it.
    fn interface_constraint(
        name: &QualifiedTypeName,
        generics: &[NormalTy],
        bindings: &[(Name, NormalTy)],
    ) -> Interface {
        Interface {
            name: name.clone(),
            generics: generics.iter().cloned().map(NormalTy::into_ty).collect(),
            associated_types: bindings
                .iter()
                .map(|(name, ty)| (name.clone(), ty.clone().into_ty()))
                .collect(),
        }
    }

    /// Consume a normalized interface (`NormalTy::Interface`) and rebuild the
    /// [`Interface`] constraint; `None` for any other variant. Used to put the
    /// `as I` annotation of an associated-type projection back into a `Ty`.
    fn into_interface(self) -> Option<Interface> {
        match self {
            NormalTy::Interface(name, generics, bindings) => Some(Interface {
                name,
                generics: Self::into_tys(generics),
                associated_types: bindings
                    .into_iter()
                    .map(|(name, ty)| (name, ty.into_ty()))
                    .collect(),
            }),
            _ => None,
        }
    }

    /// An error-recovery sentinel, excluded from union absorption (it would
    /// otherwise swallow real members during error recovery and fabricate
    /// equivalences).
    fn is_sentinel(&self) -> bool {
        matches!(self, NormalTy::Unknown | NormalTy::Error)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// UNION CANONICALIZATION
// ═══════════════════════════════════════════════════════════════════════════

impl NormalTy {
    /// Reduce a union of already-canonical members to canonical form: flatten,
    /// remove `never`, absorb under `unknown`, collapse complete enums, absorb
    /// subtype-members, then sort/dedup and unwrap singletons.
    fn canonicalize_union<C: TypeContext>(members: Vec<NormalTy>, ctx: &C) -> NormalTy {
        // Flatten one level (members are canonical, but a μ-unfold or alias could
        // surface a nested union) and drop `never`.
        let mut flat: Vec<NormalTy> = Vec::new();
        for m in members {
            match m {
                NormalTy::Union(inner) => flat.extend(inner),
                NormalTy::Never => {}
                other => flat.push(other),
            }
        }

        // `unknown` (top) absorbs everything.
        if flat.iter().any(|m| matches!(m, NormalTy::BuiltinUnknown)) {
            return NormalTy::BuiltinUnknown;
        }

        flat.sort();
        flat.dedup();

        Self::collapse_complete_enums(&mut flat, ctx);
        let mut flat = Self::absorb_subtypes(&flat, ctx);

        flat.sort();
        flat.dedup();
        match flat.len() {
            0 => NormalTy::Never,
            1 => flat.pop().unwrap_or_else(|| unreachable!("len checked")),
            _ => NormalTy::Union(flat),
        }
    }

    /// Replace a complete set of an enum's variants with the enum itself
    /// (`E.A | E.B | … == E`). A bare `Enum(E)` already present absorbs its
    /// variants via the subtype pass, so this only handles the
    /// all-variants-no-enum case.
    fn collapse_complete_enums<C: TypeContext>(members: &mut Vec<NormalTy>, ctx: &C) {
        // Distinct enums that have at least one variant present.
        let mut enums: Vec<QualifiedTypeName> = members
            .iter()
            .filter_map(|m| match m {
                NormalTy::EnumVariant(e, _) => Some(e.clone()),
                _ => None,
            })
            .collect();
        enums.sort();
        enums.dedup();

        for e in enums {
            let Some(all) = ctx.enum_variants(&e) else {
                continue; // unknown enum → no collapse (fail-safe)
            };
            let present: HashSet<&Name> = members
                .iter()
                .filter_map(|m| match m {
                    NormalTy::EnumVariant(en, v) if *en == e => Some(v),
                    _ => None,
                })
                .collect();
            if !all.is_empty() && all.iter().all(|v| present.contains(v)) {
                members.retain(|m| !matches!(m, NormalTy::EnumVariant(en, _) if *en == e));
                members.push(NormalTy::Enum(e));
            }
        }
    }

    /// Remove any member subsumed by another (`X | Y == Y` when `X <: Y`). Covers
    /// literal-into-base, variant-into-enum, `C | I == I`, `A | B == B`, and
    /// `T | I == I`. Error-recovery sentinels never absorb or are absorbed.
    fn absorb_subtypes<C: TypeContext>(members: &[NormalTy], ctx: &C) -> Vec<NormalTy> {
        let n = members.len();
        let mut keep = vec![true; n];
        for i in 0..n {
            if members[i].is_sentinel() {
                continue;
            }
            for j in 0..n {
                if i == j || !keep[j] || members[j].is_sentinel() {
                    continue;
                }
                if !members[i].is_subtype_of(&members[j], ctx, &mut HashSet::new()) {
                    continue;
                }
                // `members[i] <: members[j]`. Drop `i`, unless they are mutual
                // subtypes (equivalent but not structurally equal — e.g. cyclic
                // `requires`); then keep the lower index deterministically.
                let mutual = members[j].is_subtype_of(&members[i], ctx, &mut HashSet::new());
                if !mutual || j < i {
                    keep[i] = false;
                    break;
                }
            }
        }
        (0..n)
            .filter(|&i| keep[i])
            .map(|i| members[i].clone())
            .collect()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SUBSTITUTION (μ-unfolding) & FUNCTION PARAMETERS
// ═══════════════════════════════════════════════════════════════════════════

impl NormalTy {
    /// Substitute recursion variable `var` with `replacement` (one μ-unfold step).
    fn substitute(&self, var: &QualifiedTypeName, replacement: &NormalTy) -> NormalTy {
        match self {
            NormalTy::RecVar(v) if v == var => replacement.clone(),
            NormalTy::Class(qn, args) => NormalTy::Class(
                qn.clone(),
                args.iter()
                    .map(|a| a.substitute(var, replacement))
                    .collect(),
            ),
            NormalTy::Interface(qn, args, bindings) => NormalTy::Interface(
                qn.clone(),
                args.iter()
                    .map(|a| a.substitute(var, replacement))
                    .collect(),
                bindings
                    .iter()
                    .map(|(n, t)| (n.clone(), t.substitute(var, replacement)))
                    .collect(),
            ),
            NormalTy::List(inner) => NormalTy::List(Box::new(inner.substitute(var, replacement))),
            NormalTy::WatchAccessor(inner) => {
                NormalTy::WatchAccessor(Box::new(inner.substitute(var, replacement)))
            }
            NormalTy::Map { key, value } => NormalTy::Map {
                key: Box::new(key.substitute(var, replacement)),
                value: Box::new(value.substitute(var, replacement)),
            },
            NormalTy::Union(members) => NormalTy::Union(
                members
                    .iter()
                    .map(|m| m.substitute(var, replacement))
                    .collect(),
            ),
            NormalTy::Function {
                params,
                ret,
                throws,
            } => NormalTy::Function {
                params: params
                    .iter()
                    .map(|p| NormalParam {
                        name: p.name.clone(),
                        ty: p.ty.substitute(var, replacement),
                        mode: p.mode,
                    })
                    .collect(),
                ret: Box::new(ret.substitute(var, replacement)),
                throws: Box::new(throws.substitute(var, replacement)),
            },
            NormalTy::Future(value, error) => NormalTy::Future(
                Box::new(value.substitute(var, replacement)),
                Box::new(error.substitute(var, replacement)),
            ),
            NormalTy::AssociatedTypeProjection {
                base,
                interface,
                member,
            } => NormalTy::AssociatedTypeProjection {
                base: Box::new(base.substitute(var, replacement)),
                interface: interface
                    .as_ref()
                    .map(|i| Box::new(i.substitute(var, replacement))),
                member: member.clone(),
            },
            // A nested μ binding the same name shadows it; do not substitute inside.
            NormalTy::Mu { var: v, body } if v != var => NormalTy::Mu {
                var: v.clone(),
                body: Box::new(body.substitute(var, replacement)),
            },
            _ => self.clone(),
        }
    }
}

impl NormalParam {
    fn is_required(&self) -> bool {
        matches!(self.mode, FunctionParamMode::Required)
    }

    /// Function parameter-list subtyping (contravariant): required params
    /// positional and matched in order, optional params matched by name.
    fn list_subtype<C: TypeContext>(
        sub: &[NormalParam],
        sup: &[NormalParam],
        ctx: &C,
        assumptions: &mut HashSet<(NormalTy, NormalTy)>,
    ) -> bool {
        let sub_required: Vec<&NormalParam> = sub.iter().filter(|p| p.is_required()).collect();
        let sup_required: Vec<&NormalParam> = sup.iter().filter(|p| p.is_required()).collect();
        if sub_required.len() != sup_required.len() {
            return false;
        }
        for (sub, sup) in sub_required.iter().zip(sup_required.iter()) {
            if !sup.ty.is_subtype_of(&sub.ty, ctx, assumptions) {
                return false;
            }
        }
        for sup in sup.iter().filter(|p| !p.is_required()) {
            let Some(name) = &sup.name else {
                return false;
            };
            let Some(sub) = sub
                .iter()
                .find(|p| !p.is_required() && p.name.as_ref() == Some(name))
            else {
                return false;
            };
            if !sup.ty.is_subtype_of(&sub.ty, ctx, assumptions) {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests;
