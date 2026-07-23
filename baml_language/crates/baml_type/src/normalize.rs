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

/// Starting fuel for projection reduction: the maximum length of a reduction
/// chain (`(A as I).X` → `(B as J).Y` → …) before a projection is left opaque (the
/// canonical algebra) or its realization fails (the runtime). Generous for any
/// real program; bounds a cyclic `type A = (C as I).B` / `type B = (C as J).A`
/// (itself a declaration-level error caught elsewhere) so both the `from_ty` walk
/// here and the runtime `TyTemplate::substitute` reduction terminate instead of
/// recursing forever. Shared by both paths so the single limit shrinks in one
/// place once declaration-level cycle rejection lands.
pub(crate) const PROJECTION_REDUCTION_FUEL: u32 = 256;

/// The result of reducing an associated-type projection `(base as I).member`
/// through [`TypeContext::project`].
pub enum ProjectionStep {
    /// The projection *is* this type — the impl's binding or the qualifier's pin.
    /// `(int as Foo).Assoc` with `impl Foo for int { type Assoc = string }` reduces
    /// to `string`; the projection is a pure, side-effect-free type-level operator,
    /// so its canonical form is the reduced type (assignable from / equal to it).
    Reduced(Ty),
    /// The projection cannot be reduced here — its base is still symbolic, or no
    /// impl determines it. It stays an opaque leaf, equal only to a
    /// structurally-identical projection.
    Opaque,
}

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

    /// Whether interface `sub` *properly* (transitively, not reflexively)
    /// requires interface `sup`, accounting for generic arguments.
    ///
    /// Powers `A <: B` subtyping and the `A | B == B` absorption for
    /// existentials. `false` ⇒ no requirement is claimed. Implementations need
    /// not report same-name reflexivity — the normalizer handles structural
    /// equality before consulting this, so a same-name query only arises for
    /// distinct instantiations, which are not requirements.
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
    /// The returned `Vec` is the *conjunction* (intersection) of the bound: an
    /// associated-type `extends` clause is always an intersection of interfaces —
    /// never a union, and never a non-interface type such as `int` or `string`.
    /// Because a value satisfies *every* bound, the projection is a subtype of the
    /// supertypes of *any* one of them (the `.any()` at the rule site), matching
    /// [`type_var_bound`](Self::type_var_bound)'s conjunction contract.
    ///
    /// Powers `(_ as I<…>).assoc <: B` for a *still-symbolic* projection: it is a
    /// subtype of its bound's supertypes — the projection analogue of
    /// [`type_var_bound`](Self::type_var_bound). An upstream pre-pass is expected to
    /// resolve realized-base projections to a concrete type before they reach the
    /// rule; this covers the remaining still-symbolic case.
    ///
    /// The bound is a function of `(interface, assoc)` only; a `Self`-referential
    /// bound (one mentioning the implementor) is not expressible here — resolving
    /// `Self` over each returned [`Interface`] would be a later step.
    ///
    /// **Required (no default).** A silently-empty default would let a context that
    /// *should* resolve associated-type bounds forget to — leaving projections
    /// opaque with no error, a silent soundness hole. Every context must decide
    /// explicitly; one that genuinely cannot encounter symbolic projections (e.g. a
    /// runtime context over already-realized values) returns an explicit
    /// `Vec::new()`, which the doc-comment there justifies.
    fn associated_type_bound(&self, interface: &Interface, assoc: Name) -> Vec<Interface>;

    /// Reduce an associated-type projection `(base as interface).member` to the type
    /// it denotes, when determinable — the pure type-level operator that makes
    /// `(int as Foo).Assoc` *be* `string` (the impl's binding), analogous to how
    /// `1 + 1` *is* `2`. [`ProjectionStep::Opaque`] when the base is still symbolic
    /// or no impl determines it, leaving the projection a leaf.
    ///
    /// **Required (no default).** A silently-`Opaque` default would let a context
    /// that *should* reduce projections forget to — leaving `(int as Foo).Assoc` a
    /// dead symbolic type with no error, a silent soundness hole (same class as
    /// [`associated_type_bound`](Self::associated_type_bound)). A context over
    /// already-realized values (the runtime) returns `Opaque` explicitly.
    ///
    /// `fuel` is the remaining projection-reduction budget. A context that itself
    /// drives the reduction recursion — the runtime, whose `project` realizes the
    /// impl binding through `TyTemplate::substitute` and can re-enter `project` —
    /// threads it on so a cyclic associated-type binding terminates. A single-step
    /// reducer whose recursion is instead bounded by its caller (the canonical
    /// `from_ty` walk, which decrements its own fuel) ignores it.
    fn project(&self, base: &Ty, interface: &Interface, member: &Name, fuel: u32)
    -> ProjectionStep;

    // ── type algebra (defaulted; the canonical implementation) ──────────────
    //
    // A context computes the set-theoretic relations over its *own* facts, so
    // `ctx.is_subtype(a, b)` reads as "does this context prove `a <: b`". These
    // methods carry the logic; the free functions below are thin wrappers over
    // them (kept for callers that hold a context by value — deletable once those
    // migrate to the method form). Each body passes `self` as the context, hence
    // `where Self: Sized`: there are no `dyn TypeContext` callers, so the bound
    // costs nothing. Do not override them — a context supplies only the facts.

    /// Normalize `ty` to its canonical form and render it back as a [`Ty`].
    ///
    /// Two types are [`Self::equivalent`] iff their canonical forms are structurally
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
    /// must not affect [`Self::equivalent`]/[`Self::is_subtype`]). This makes the
    /// output a canonical form for type *identity* (equality, display, debugging) —
    /// **not** an attribute-preserving rewrite. Do not feed it into a position where
    /// SAP annotations must survive (an LLM function's return type, a generated
    /// stream companion); derive the canonical type from the original `Ty` there
    /// instead.
    fn normalize(&self, ty: &Ty) -> Ty
    where
        Self: Sized,
    {
        NormalTy::canonical(ty, self).into_ty()
    }

    /// Whether `a` and `b` denote the same type under the current context.
    ///
    /// This is invariant equality, not assignability: use it where two spellings
    /// must denote *the same* type (e.g. exact-type operator operands, interface
    /// field implementations), not merely compatible ones.
    fn equivalent(&self, a: &Ty, b: &Ty) -> bool
    where
        Self: Sized,
    {
        // Reflexivity fast path: structurally identical spellings (attrs
        // included) trivially canonicalize to the same form.
        if a == b {
            return true;
        }
        // Cheap definite-mismatch filter. MIR impl dispatch probes every
        // candidate impl's pattern against the receiver through `equivalent`
        // (`match_ty_pattern_into`), so the overwhelmingly common case is a
        // miss between two nominal types with different names — decidable from
        // the outermost constructor alone, without the two allocation-heavy
        // canonicalization walks below.
        if heads_definitely_differ(a, b) {
            return false;
        }
        NormalTy::canonical(a, self) == NormalTy::canonical(b, self)
    }

    /// Whether every value of `sub` is also a value of `sup` under the current
    /// context (the subset relation).
    fn is_subtype(&self, sub: &Ty, sup: &Ty) -> bool
    where
        Self: Sized,
    {
        // Reflexivity fast path: structurally identical spellings canonicalize
        // to the same form (canonicalization is deterministic and attr-erasure
        // applies to both sides), and `is_subtype_of` starts with a `self == sup`
        // fast path — so the two allocation-heavy canonicalization walks below
        // would only rediscover `true`.
        if sub == sup {
            return true;
        }
        // Narrow definite-mismatch filter. Unlike `equivalent`, differing heads
        // do NOT generally refute subtyping (a literal is a subtype of its base,
        // a member of its union, a class of an interface it implements) — but
        // those pairs are different `Ty` *variants*, for which
        // `heads_definitely_differ` already answers `false`. Of the same-variant
        // nominal pairs it does claim, only interface-to-interface can still be
        // a subtype under different names (`A <: B` iff `A` requires `B`), so it
        // is excluded. For the rest the canonical walk below is a foregone
        // `false`: class heads are preserved by canonicalization and the class
        // subtype rule requires equal names (generic args are invariant), while
        // enums and enum variants are nominal with no cross-name rule at all.
        if !matches!((sub, sup), (Ty::Interface(..), Ty::Interface(..)))
            && heads_definitely_differ(sub, sup)
        {
            return false;
        }
        let sub = NormalTy::canonical(sub, self);
        let sup = NormalTy::canonical(sup, self);
        sub.is_subtype_of(&sup, self, &mut HashSet::new())
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
    /// - Functions (not invariant — contravariant/covariant), interfaces, bare
    ///   type variables, and a bare `unknown`.
    fn definitely_disjoint(&self, a: &Ty, b: &Ty) -> bool
    where
        Self: Sized,
    {
        NormalTy::canonical(a, self).is_disjoint_from(&NormalTy::canonical(b, self))
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
    fn definitely_equal(&self, a: &Ty, b: &Ty) -> bool
    where
        Self: Sized,
    {
        let a = NormalTy::canonical(a, self);
        a.is_unoverridable_singleton() && a == NormalTy::canonical(b, self)
    }

    /// The statically-known result of a broad `==` between operands of types `a` and
    /// `b`, or `None` if it depends on the runtime values.
    ///
    /// Combines [`Self::definitely_disjoint`] and [`Self::definitely_equal`] into one
    /// pass — it canonicalizes each operand once rather than twice:
    /// - `Some(false)` — the types are provably disjoint, so `==` is always `false`.
    /// - `Some(true)` — both operands are the same unoverridable singleton, so `==`
    ///   is always `true`.
    /// - `None` — the result is not statically determined.
    ///
    /// See those two methods for the exact rules and their dynamic-package
    /// soundness.
    fn constant_equality(&self, a: &Ty, b: &Ty) -> Option<bool>
    where
        Self: Sized,
    {
        let a = NormalTy::canonical(a, self);
        let b = NormalTy::canonical(b, self);
        if a.is_disjoint_from(&b) {
            Some(false)
        } else if a.is_unoverridable_singleton() && a == b {
            Some(true)
        } else {
            None
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PUBLIC API
// ═══════════════════════════════════════════════════════════════════════════

/// The [`TypeContext`] with **every nominal fact opaque** — the algebra's pure
/// structural/set-theoretic core: union flatten/sort/dedup, `never` removal,
/// literal-into-base collapse, invariant container recursion, function-type
/// variance. No alias expands, no interface membership or `requires` holds, no
/// enum completes, no type variable carries a bound, and no projection reduces —
/// each is a leaf equal only to itself.
///
/// Every answer is fail-closed: `NoFacts` can only under-approximate a
/// fact-aware context, never over-claim. But an under-approximation is still an
/// incorrect *miss* (`type A = int | A[]` ≢ `type B = int | B[]` here, though
/// they denote the same type), which is why this context is **deprecated from
/// birth**: each use site marks a boundary that has not yet been given a real
/// fact source, kept visible so it gets one rather than quietly becoming a
/// convention. Supply the richest context the site can reach; reach for
/// `NoFacts` only when none exists yet.
#[deprecated = "every NoFacts site is a boundary awaiting a real fact context — supply one (compiler: GlobalTypeContext; runtime: the VM / an engine-side context) instead of comparing fact-free"]
pub struct NoFacts;

#[expect(
    deprecated,
    reason = "naming `NoFacts` to define its own trait impl fires the lint; this is \
              the type's definition, not a consumer site to migrate off it"
)]
impl TypeContext for NoFacts {
    fn alias_def(&self, _name: &QualifiedTypeName) -> Option<Ty> {
        None
    }

    fn implements_interface(&self, _concrete: &Ty, _interface: &Interface) -> bool {
        false
    }

    fn type_var_bound(&self, _name: &Name) -> Vec<Interface> {
        Vec::new()
    }

    fn interface_requires(&self, _sub: &Interface, _sup: &Interface) -> bool {
        false
    }

    fn enum_variants(&self, _name: &QualifiedTypeName) -> Option<Vec<Name>> {
        None
    }

    fn associated_type_bound(&self, _interface: &Interface, _assoc: Name) -> Vec<Interface> {
        Vec::new()
    }

    fn project(
        &self,
        _base: &Ty,
        _interface: &Interface,
        _member: &Name,
        _fuel: u32,
    ) -> ProjectionStep {
        ProjectionStep::Opaque
    }
}

/// Free-function form of [`TypeContext::normalize`], for a context held by value.
/// Pending removal once every caller uses the method form.
pub fn normalize<C: TypeContext>(ty: &Ty, ctx: &C) -> Ty {
    ctx.normalize(ty)
}

/// Free-function form of [`TypeContext::equivalent`], for a context held by value.
/// Pending removal once every caller uses the method form.
pub fn equivalent<C: TypeContext>(a: &Ty, b: &Ty, ctx: &C) -> bool {
    ctx.equivalent(a, b)
}

/// Free-function form of [`TypeContext::is_subtype`], for a context held by value.
/// Pending removal once every caller uses the method form.
pub fn is_subtype<C: TypeContext>(sub: &Ty, sup: &Ty, ctx: &C) -> bool {
    ctx.is_subtype(sub, sup)
}

/// True only when `a` and `b` provably canonicalize to different forms, judged
/// from their outermost constructor alone — a cheap reject for [`TypeContext::
/// equivalent`] that skips the two canonicalization walks on the common
/// nominal-mismatch case (e.g. probing an `impl for Foo` pattern against a
/// receiver of an unrelated class).
///
/// Soundness rests on `NormalTy::from_ty` being *head-stable* for the variants
/// decided here: a `Class`/`Interface`/`Enum`/`EnumVariant` maps to the same
/// constructor carrying the *verbatim* qualified name (only its type arguments
/// are rewritten), and equality is nominal, so two of the same kind with
/// different names can never share a canonical form.
///
/// This is deliberately *conservative* — it decides only same-kind pairs whose
/// heads are stable and never uses a cross-kind "different discriminant ⇒
/// differ" catch-all. That catch-all would be unsound under the current
/// normalization: `List`/`EvolvingList` and `Map`/`EvolvingMap` are distinct
/// `Ty` constructors that canonicalize to the *same* `NormalTy` head, and
/// `TypeAlias` / `Union` / `AssociatedTypeProjection` / generic `Media` /
/// `Infer` heads can be rewritten into any other shape. Leaving every such case
/// undecided (`false`) preserves correctness; only the unambiguous nominal
/// misses are fast-rejected. Context-independent: canonicalization preserves
/// these nominal heads regardless of the `TypeContext`.
fn heads_definitely_differ(a: &Ty, b: &Ty) -> bool {
    match (a, b) {
        (Ty::Class(q1, ..), Ty::Class(q2, ..))
        | (Ty::Interface(q1, ..), Ty::Interface(q2, ..))
        | (Ty::Enum(q1, ..), Ty::Enum(q2, ..)) => q1 != q2,
        (Ty::EnumVariant(q1, v1, ..), Ty::EnumVariant(q2, v2, ..)) => q1 != q2 || v1 != v2,
        _ => false,
    }
}

/// Free-function form of [`TypeContext::definitely_disjoint`], for a context held
/// by value. Pending removal once every caller uses the method form.
pub fn definitely_disjoint<C: TypeContext>(a: &Ty, b: &Ty, ctx: &C) -> bool {
    ctx.definitely_disjoint(a, b)
}

/// Free-function form of [`TypeContext::definitely_equal`], for a context held by
/// value. Pending removal once every caller uses the method form.
pub fn definitely_equal<C: TypeContext>(a: &Ty, b: &Ty, ctx: &C) -> bool {
    ctx.definitely_equal(a, b)
}

impl NormalTy {
    /// Normalize and canonicalize a [`Ty`] in one step (the shared entry point).
    fn canonical<C: TypeContext>(ty: &Ty, ctx: &C) -> NormalTy {
        NormalTy::from_ty(ty, ctx, &mut HashSet::new(), PROJECTION_REDUCTION_FUEL).canonicalize(ctx)
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
            NormalTy::List(inner) | NormalTy::Mu { body: inner, .. } => inner.is_ground(),
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
            // return/throws), so they are not provably disjoint here.
            (NormalTy::Function { .. }, NormalTy::Function { .. }) => false,

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
    AssociatedTypeProjection {
        base: Box<NormalTy>,
        /// The declaring interface (a normalized `NormalTy::Interface`), always
        /// present — mirrors the non-optional `Ty::AssociatedTypeProjection`
        /// qualifier it is built from, and is what makes a realized-base
        /// projection reducible via [`TypeContext::project`].
        interface: Box<NormalTy>,
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
        // Remaining projection-reduction steps along this path. Reducing a
        // projection (`(int as Foo).Assoc` → `string`) is a pure type-level
        // operator, but could loop on a cyclic `type A = (C as I).B` /
        // `type B = (C as J).A`; each reduction spends one unit, and on exhaustion
        // the projection stays opaque (conservative — never over-equates).
        fuel: u32,
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
                NormalTy::Class(qn.clone(), Self::from_tys(args, ctx, expanding, fuel))
            }
            Ty::Interface(qn, args, bindings, _) => {
                let mut bindings: Vec<_> = bindings
                    .iter()
                    .map(|(name, ty)| (name.clone(), Self::from_ty(ty, ctx, expanding, fuel)))
                    .collect();
                bindings.sort_by(|(a, _), (b, _)| a.cmp(b));
                NormalTy::Interface(
                    qn.clone(),
                    Self::from_tys(args, ctx, expanding, fuel),
                    bindings,
                )
            }
            Ty::Enum(qn, _) => NormalTy::Enum(qn.clone()),
            Ty::EnumVariant(qn, v, _) => NormalTy::EnumVariant(qn.clone(), v.clone()),
            // Evolving containers are the list/map analogues during inference;
            // their type identity is the same as the frozen form.
            Ty::List(inner, _) | Ty::EvolvingList(inner, _) => {
                NormalTy::List(Box::new(Self::from_ty(inner, ctx, expanding, fuel)))
            }
            Ty::Map { key, value, .. } | Ty::EvolvingMap(key, value, _) => NormalTy::Map {
                key: Box::new(Self::from_ty(key, ctx, expanding, fuel)),
                value: Box::new(Self::from_ty(value, ctx, expanding, fuel)),
            },
            Ty::Union(members, _) => NormalTy::Union(Self::from_tys(members, ctx, expanding, fuel)),
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
                        ty: Self::from_ty(&p.ty, ctx, expanding, fuel),
                        mode: p.mode,
                    })
                    .collect(),
                ret: Box::new(Self::from_ty(ret, ctx, expanding, fuel)),
                throws: Box::new(Self::from_ty(throws, ctx, expanding, fuel)),
            },
            Ty::Future(value, error, _) => NormalTy::Future(
                Box::new(Self::from_ty(value, ctx, expanding, fuel)),
                Box::new(Self::from_ty(error, ctx, expanding, fuel)),
            ),
            Ty::TypeVar(name, _) => NormalTy::TypeVar(name.clone()),
            Ty::AssociatedTypeProjection {
                base,
                interface,
                member,
                ..
            } => {
                // A projection is a pure type-level operator: `(int as Foo).Assoc`
                // *is* the type the impl binds (like `1 + 1` *is* `2`). Reduce it to
                // that whenever the context can determine it — the reduced type
                // becomes the canonical form, so the projection compares equal to /
                // is assignable from its realization. The qualifier always names the
                // impl; fuel guards a cyclic reduction.
                if fuel > 0
                    && let ProjectionStep::Reduced(reduced) =
                        ctx.project(base, interface, member, fuel)
                {
                    return Self::from_ty(&reduced, ctx, expanding, fuel - 1);
                }
                // Not reducible — symbolic base or exhausted fuel: an opaque leaf,
                // equal only to a structurally-identical projection.
                NormalTy::AssociatedTypeProjection {
                    base: Box::new(Self::from_ty(base, ctx, expanding, fuel)),
                    interface: Box::new(Self::from_ty(&interface.to_ty(), ctx, expanding, fuel)),
                    member: member.clone(),
                }
            }
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
                let body = Self::from_ty(&def, ctx, expanding, fuel);
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
        fuel: u32,
    ) -> Vec<NormalTy> {
        tys.iter()
            .map(|t| Self::from_ty(t, ctx, expanding, fuel))
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
            NormalTy::List(inner) => inner.mentions_rec_var(var),
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
            } => base.mentions_rec_var(var) || interface.mentions_rec_var(var),
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
            NormalTy::AssociatedTypeProjection {
                base,
                interface,
                member,
            } => NormalTy::AssociatedTypeProjection {
                base: Box::new(base.canonicalize(ctx)),
                interface: Box::new(interface.canonicalize(ctx)),
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

    /// Whether pin `(member, value)` required of `var <: qn<args, …>` is *tautological* —
    /// the value is `var`'s own projection of that same member through that same
    /// interface, `(var as qn<args>).member`. Any `var` implementing `qn<args>` satisfies
    /// it definitionally, so the [`Self::is_subtype_of`] type-variable arm strips it
    /// before delegating to the variable's bounds. Extra pins on the projection's own
    /// qualifier don't disqualify: a qualifier narrows which interface view is meant, it
    /// never changes the member's value.
    fn pin_is_tautological<C: TypeContext>(
        var: &Name,
        qn: &QualifiedTypeName,
        args: &[NormalTy],
        pin: &(Name, NormalTy),
        ctx: &C,
        assumptions: &mut HashSet<(NormalTy, NormalTy)>,
    ) -> bool {
        let (member, value) = pin;
        let NormalTy::AssociatedTypeProjection {
            base,
            interface: iface,
            member: proj_member,
        } = value
        else {
            return false;
        };
        proj_member == member
            && matches!(&**base, NormalTy::TypeVar(base_var) if base_var == var)
            && matches!(&**iface, NormalTy::Interface(iface_qn, iface_args, _)
                if iface_qn == qn
                    && iface_args.len() == args.len()
                    && iface_args
                        .iter()
                        .zip(args)
                        .all(|(a, b)| a.invariant_compatible(b, ctx, assumptions)))
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

        // ── Termination argument ──────────────────────────────────────────
        // The co-inductive assumption set exists *only* to terminate cycles,
        // and a cycle can only arise through an arm that *expands* (regenerates)
        // a type rather than descending into a strictly-smaller subterm:
        //   * μ-unfolding — `body.substitute(var, self)` can reproduce the same
        //     pair (`(_, Mu)` on the right, `(Mu, _)` on the left);
        //   * a `TypeVar` / `AssociatedTypeProjection` on the left — its bound is
        //     looked up through the context and may mention the variable itself.
        // Every *structural* arm (unions, invariant containers, functions,
        // literals, enum-variant, the nominal interface facts) either recurses
        // into a strictly-smaller subterm of a finite regular tree or is
        // terminal, so those arms terminate WITHOUT any assumption bookkeeping.
        // Any infinite derivation must therefore pass through an expanding arm
        // infinitely often, and the pairs seen at those arms are drawn from the
        // finite subterm closure of the (regular) operands — so recording pairs
        // only at the expanding arms still guarantees a repeat is detected and
        // the recursion is capped. Restricting the bookkeeping this way spares
        // the deep `NormalTy` clone + full-tree hash on the millions of purely
        // structural steps, which profiling showed dominated subtype-checking
        // cost (this is now the *only* equivalence path post-unification).
        let expanding = matches!(
            self,
            NormalTy::Mu { .. } | NormalTy::TypeVar(_) | NormalTy::AssociatedTypeProjection { .. }
        ) || matches!(sup, NormalTy::Mu { .. });
        if !expanding {
            return self.is_subtype_of_inner(sup, ctx, assumptions);
        }
        // On the (few) expanding arms, probe by linear scan rather than cloning
        // to hash: the pairs on the current path are bounded by the
        // expanding-arm recursion depth, so a scan beats hashing the whole tree.
        if assumptions.iter().any(|(a, b)| a == self && b == sup) {
            return true;
        }
        let pair = (self.clone(), sup.clone());
        assumptions.insert(pair.clone());
        let result = self.is_subtype_of_inner(sup, ctx, assumptions);
        assumptions.remove(&pair);
        result
    }

    /// The structural rules of [`Self::is_subtype_of`]. Callers MUST go through
    /// `is_subtype_of`, which owns the reflexivity / sentinel fast paths and the
    /// co-inductive assumption bookkeeping (only performed on the expanding
    /// arms — see the termination argument there).
    fn is_subtype_of_inner<C: TypeContext>(
        &self,
        sup: &NormalTy,
        ctx: &C,
        assumptions: &mut HashSet<(NormalTy, NormalTy)>,
    ) -> bool {
        match (self, sup) {
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
            //
            // When `sup` is an interface, a *tautological* pin — one that merely names
            // the variable's own member under that same interface, `T <:
            // I<m = (T as I).m>` — is stripped first: any `T: I` satisfies it
            // definitionally (Rust's `T: Iterator` proves `T: Iterator<Item =
            // <T as Iterator>::Item>`). Such pins arise when an interface's own
            // signature (`-> Iterator<Item = Self.Item>`) is realized at a rigid `Self`.
            (NormalTy::TypeVar(name), NormalTy::Interface(qn, args, pins))
                if pins.iter().any(|pin| {
                    Self::pin_is_tautological(name, qn, args, pin, ctx, assumptions)
                }) =>
            {
                let stripped = NormalTy::Interface(
                    qn.clone(),
                    args.clone(),
                    pins.iter()
                        .filter(|pin| {
                            !Self::pin_is_tautological(name, qn, args, pin, ctx, assumptions)
                        })
                        .cloned()
                        .collect(),
                );
                ctx.type_var_bound(name).iter().any(|bound| {
                    NormalTy::canonical(&bound.to_ty(), ctx).is_subtype_of(
                        &stripped,
                        ctx,
                        assumptions,
                    )
                })
            }
            (NormalTy::TypeVar(name), _) => ctx.type_var_bound(name).iter().any(|bound| {
                NormalTy::canonical(&bound.to_ty(), ctx).is_subtype_of(sup, ctx, assumptions)
            }),

            // A still-symbolic associated-type projection is a subtype of `sup` if
            // any of its associated type's declared bounds is — the projection
            // analogue of the `TypeVar` rule above. The projection always carries
            // its declaring interface; a realized-base projection is intended to be
            // resolved to a concrete type by an upstream pre-pass before reaching
            // here. Must precede the interface arms below, which would otherwise ask
            // `implements_interface` about a non-concrete projection.
            (
                NormalTy::AssociatedTypeProjection {
                    interface: iface,
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

            // BEP-062: `baml.AnyFunction` is a compiler builtin implemented by
            // every function type, with the parameter list erased. Conformance
            // is derived right here rather than from an `implements` block
            // (function types are not impl subjects): the return type must fit
            // the `Returns` pin and the throws type the `Throws` pin. Omitted
            // pins were filled with their `unknown` defaults when the
            // existential was lowered; a pin missing anyway degrades to that
            // same top-type default (accepts everything).
            (NormalTy::Function { ret, throws, .. }, NormalTy::Interface(qn, _, bindings))
                if qn.is_builtin_root_type("AnyFunction") =>
            {
                let pin = |name: &str| {
                    bindings
                        .iter()
                        .find_map(|(n, ty)| (n.as_str() == name).then_some(ty))
                };
                pin("Returns").is_none_or(|r| ret.is_subtype_of(r, ctx, assumptions))
                    && pin("Throws").is_none_or(|t| throws.is_subtype_of(t, ctx, assumptions))
            }

            // BEP-062: `AnyFunction`'s pins are covariant, unlike every other
            // interface binding (`interface_requires` compares those
            // invariantly): `AnyFunction<Returns = Label>` fits where
            // `AnyFunction<Returns = json>` is expected, because every held
            // function returning `Label` also returns a `json`. Sound because
            // the pins only describe outputs of the held function (its return
            // and error channels); the erased parameter list leaves no
            // write-through position. A pin missing on the sub side claims
            // nothing (conservative), matching the fail-safe contract.
            (
                NormalTy::Interface(sub_qn, _, sub_bindings),
                NormalTy::Interface(sup_qn, _, sup_bindings),
            ) if sub_qn.is_builtin_root_type("AnyFunction")
                && sup_qn.is_builtin_root_type("AnyFunction") =>
            {
                sup_bindings.iter().all(|(name, sup_pin)| {
                    sub_bindings.iter().any(|(n, sub_pin)| {
                        n == name && sub_pin.is_subtype_of(sup_pin, ctx, assumptions)
                    })
                })
            }

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

            // Future is an invariant container.
            (NormalTy::Future(v1, e1), NormalTy::Future(v2, e2)) => {
                v1.invariant_compatible(v2, ctx, assumptions)
                    && e1.invariant_compatible(e2, ctx, assumptions)
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
        }
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
            NormalTy::AssociatedTypeProjection {
                base,
                interface,
                member,
            } => Ty::AssociatedTypeProjection {
                base: Box::new(base.into_ty()),
                // The projection's qualifier is always a normalized interface, so
                // it round-trips back to an `Interface` here.
                interface: Box::new(
                    interface
                        .into_interface()
                        .unwrap_or_else(|| unreachable!("projection qualifier is an interface")),
                ),
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
                interface: Box::new(interface.substitute(var, replacement)),
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
