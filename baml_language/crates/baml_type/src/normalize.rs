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

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
};

use crate::{
    FunctionParamMode, FunctionParamTy, Head, Interface, Literal, MediaKind, Name, ParamTy,
    QualifiedTypeName, Ty, TyAttr,
};

/// The one declaration the algebra special-cases by identity: `AnyFunction`'s
/// pins are covariant, unlike every other interface.
///
/// Built once rather than per comparison — the subtyping walk reaches the arms
/// that consult it on every interface-vs-interface node. Resolving it to a head
/// still goes through [`TypeContext::head_lookup`] per call, which is cheap for
/// a name-based context and a registry hit for a runtime one.
static ANY_FUNCTION: std::sync::LazyLock<QualifiedTypeName> = std::sync::LazyLock::new(|| {
    QualifiedTypeName::new(Name::new("baml"), Vec::new(), Name::new("AnyFunction"))
});

/// Whether `head` is the [`ANY_FUNCTION`] declaration, decided by identity
/// against the head `ctx` uses for it rather than by inspecting `head` itself.
///
/// An unknown declaration is not `AnyFunction` as far as this can tell, so the
/// covariance special case does not fire — conservative, per the context's
/// fail-safe contract.
fn is_any_function<H: Head, C: TypeContext<H>>(head: &H, ctx: &C) -> bool {
    ctx.head_lookup(&ANY_FUNCTION)
        .is_some_and(|any_function| *head == any_function)
}

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
pub enum ProjectionStep<H: Head = QualifiedTypeName> {
    /// The projection *is* this type — the impl's binding or the qualifier's pin.
    /// `(int as Foo).Assoc` with `impl Foo for int { type Assoc = string }` reduces
    /// to `string`; the projection is a pure, side-effect-free type-level operator,
    /// so its canonical form is the reduced type (assignable from / equal to it).
    Reduced(Ty<H>),
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
pub trait TypeContext<H: Head = QualifiedTypeName> {
    /// The head this context represents the declaration at `qtn` with.
    ///
    /// The algebra never inspects a head's spelling — heads are opaque values it
    /// threads through — so recognizing a *particular* declaration is done by
    /// obtaining its head here and comparing with `==`. That indirection is what
    /// lets the question be answered by a representation with no name to
    /// inspect: a name-based context returns the name itself (the identity,
    /// always available), while a runtime context resolves the declaration on
    /// its heap and hands back a handle — state only the context has, and the
    /// reason this cannot live on [`Head`](crate::Head).
    ///
    /// Used for the algebra's one nominal special case, `baml.AnyFunction`,
    /// whose pins are covariant unlike every other interface.
    ///
    /// `None` means the declaration could not be resolved *at all*, and fails
    /// safe in the usual way: the special case does not fire, which is
    /// conservative. Note this is about resolvability, not knowledge — a
    /// name-based context answers unconditionally, since naming a declaration
    /// asserts nothing about it.
    fn head_lookup(&self, qtn: &QualifiedTypeName) -> Option<H>;

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
    fn alias_def(&self, name: &H) -> Option<Ty<H>>;

    /// Whether the non-interface, non-type-variable `concrete` type implements
    /// `interface`, accounting for the interface's generic
    /// arguments, associated-type bindings, and the impl's bounds.
    ///
    /// Powers `C <: I` subtyping and the `C | I == I` union absorption (a
    /// concrete member subsumed by an existential member). `false` ⇒ no
    /// membership is claimed.
    fn implements_interface(&self, concrete: &Ty<H>, interface: &Interface<H>) -> bool;

    /// The declared bound of type variable `name` (an interface or a union of
    /// interfaces); empty if it is unbounded or unknown.
    ///
    /// Powers `T <: I` (and the `T | I == I` absorption) when `T`'s bound
    /// is — or transitively requires — `I`.
    fn type_var_bound(&self, param: &ParamTy) -> Vec<Interface<H>>;

    /// Whether interface `sub` *properly* (transitively, not reflexively)
    /// requires interface `sup`, accounting for generic arguments.
    ///
    /// Powers `A <: B` subtyping and the `A | B == B` absorption for
    /// existentials. `false` ⇒ no requirement is claimed. Implementations need
    /// not report same-name reflexivity — the normalizer handles structural
    /// equality before consulting this, so a same-name query only arises for
    /// distinct instantiations, which are not requirements.
    fn interface_requires(&self, sub: &Interface<H>, sup: &Interface<H>) -> bool;

    /// The complete set of variant names of an enum, or `None` if the enum is
    /// unknown.
    ///
    /// Powers the completeness collapse `E.A | E.B | … == E` (a union of *all* of
    /// an enum's variants is the enum itself). `None` ⇒ no collapse.
    fn enum_variants(&self, name: &H) -> Option<Vec<Name>>;

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
    fn associated_type_bound(&self, interface: &Interface<H>, assoc: Name) -> Vec<Interface<H>>;

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
    fn project(
        &self,
        base: &Ty<H>,
        interface: &Interface<H>,
        member: &Name,
        fuel: u32,
    ) -> ProjectionStep<H>;

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
    /// and enum-completeness collapse, interface absorption, alias expansion, and
    /// μ-canonicalization of recursive aliases) so that distinct spellings of the
    /// same type converge.
    ///
    /// # Recursion renders via alias names
    ///
    /// Surface syntax has no μ-binder, so recursion is spelled with alias names:
    /// a recursive alias at the *root* is unfolded once (exposing its head
    /// constructor — impl-subject classification, dispatch-target resolution,
    /// and pattern-matrix specialization rely on it), while *nested* recursion
    /// stays folded as an alias name (`json[]` renders as `baml.json.json[]`,
    /// not the unfolding). The output is idempotent
    /// (`normalize(normalize(t)) == normalize(t)`) and always
    /// [`Self::equivalent`] to the input, but it is canonical only **up to the
    /// naming of recursion back-references**: names are recorded per run, so two
    /// equivalent spellings may render with different alias names
    /// (`normalize(A) = int | A[]` vs `normalize(B) = int | B[]` for
    /// α-equivalent `A`/`B`). Canonical *identity* is the
    /// [`Self::equivalent`] judgment, never syntactic equality of rendered
    /// output.
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
    fn normalize(&self, ty: &Ty<H>) -> Ty<H>
    where
        Self: Sized,
    {
        NormalTy::canonical_render(ty, self)
    }

    /// Whether `a` and `b` denote the same type under the current context —
    /// mutual subtyping, decided as structural equality of canonical forms
    /// (which coincide: canonical forms are unique representatives of the
    /// equirecursive equivalence class).
    ///
    /// This is invariant equality, not assignability: use it where two spellings
    /// must denote *the same* type (e.g. exact-type operator operands, interface
    /// field implementations), not merely compatible ones.
    ///
    /// Recursive aliases are **equirecursive** (TYPE_SYSTEM.md §Type Aliases and
    /// Recursive Types): equality holds across alias renaming
    /// (`type A = int | A[]` ≡ `type B = int | B[]`), finite unfolding depth,
    /// and mutually recursive definitions. The residual divergences from mutual
    /// subtyping are deliberate: the error-recovery sentinels (`Unknown`/`Error`
    /// are bidirectionally *compatible* with everything but equivalent only to
    /// themselves), and fact sets with a mutual `requires` cycle (mutual
    /// subtypes as existentials, nominally distinct — rejected in well-formed
    /// programs by the interface `requires`-cycle check).
    fn equivalent(&self, a: &Ty<H>, b: &Ty<H>) -> bool
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
    fn is_subtype(&self, sub: &Ty<H>, sup: &Ty<H>) -> bool
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
    fn definitely_disjoint(&self, a: &Ty<H>, b: &Ty<H>) -> bool
    where
        Self: Sized,
    {
        NormalTy::canonical(a, self).is_disjoint_from(&NormalTy::canonical(b, self), self)
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
    fn definitely_equal(&self, a: &Ty<H>, b: &Ty<H>) -> bool
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
    fn constant_equality(&self, a: &Ty<H>, b: &Ty<H>) -> Option<bool>
    where
        Self: Sized,
    {
        let a = NormalTy::canonical(a, self);
        let b = NormalTy::canonical(b, self);
        if a.is_disjoint_from(&b, self) {
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
    /// The identity, like every name-based context: naming a declaration is not
    /// a *fact* about it, so this is answerable even here. Returning `None`
    /// would silently disable the `AnyFunction` covariance rule, which used to
    /// fire under this context on the name alone.
    fn head_lookup(&self, qtn: &QualifiedTypeName) -> Option<QualifiedTypeName> {
        Some(qtn.clone())
    }

    fn alias_def(&self, _name: &QualifiedTypeName) -> Option<Ty> {
        None
    }

    fn implements_interface(&self, _concrete: &Ty, _interface: &Interface) -> bool {
        false
    }

    fn type_var_bound(&self, _param: &ParamTy) -> Vec<Interface> {
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
    ) -> ProjectionStep<QualifiedTypeName> {
        ProjectionStep::Opaque
    }
}

/// Free-function form of [`TypeContext::normalize`], for a context held by value.
/// Pending removal once every caller uses the method form.
pub fn normalize<H: Head, C: TypeContext<H>>(ty: &Ty<H>, ctx: &C) -> Ty<H> {
    ctx.normalize(ty)
}

/// Free-function form of [`TypeContext::equivalent`], for a context held by value.
/// Pending removal once every caller uses the method form.
pub fn equivalent<H: Head, C: TypeContext<H>>(a: &Ty<H>, b: &Ty<H>, ctx: &C) -> bool {
    ctx.equivalent(a, b)
}

/// Free-function form of [`TypeContext::is_subtype`], for a context held by value.
/// Pending removal once every caller uses the method form.
pub fn is_subtype<H: Head, C: TypeContext<H>>(sub: &Ty<H>, sup: &Ty<H>, ctx: &C) -> bool {
    ctx.is_subtype(sub, sup)
}

/// A deterministic 64-bit digest of `ty`'s **canonical form** under `ctx` —
/// the identity basis for statically-spelled runtime `type` values (BEP-066
/// `MintId::Static`).
///
/// Exact basis: canonicalize `ty` (the same walk [`TypeContext::equivalent`]
/// performs on each operand — unique representative of the equirecursive
/// equivalence class, so μ-recursion, union ordering, attr erasure, and every
/// context fact are already folded in), serialize its derived `Hash` token
/// stream with every numeric token in big-endian fixed width (`usize`/`isize`
/// widened to 64 bits), then feed those bytes into fixed-seed FNV-1a-64
/// (offset basis `0xcbf29ce484222325`, prime `0x100000001b3`). Consequences:
///
/// * `equivalent(a, b, ctx)` ⟹ `canonical_digest(a, ctx) ==
///   canonical_digest(b, ctx)` — equivalent spellings (`string?` vs
///   `string | null`, permuted unions, renamed recursive aliases) share a
///   digest. The converse holds up to 64-bit collision odds, which the mint
///   design accepts (a collision over-equates two *static* types).
/// * The digest hashes only value data (names as strings, structure, de
///   Bruijn indices — canonical `NormalTy` equality is α-invariant and its
///   display metadata is hash-transparent). No pointers, no interner state:
///   two processes running the same build over the same program facts produce
///   identical digests.
/// * The digest is **not** an on-wire format (BEP-066 H-4: identity never
///   crosses the boundary). It may change across compiler versions — nothing
///   may persist it; a decoded type value re-derives its mint.
/// * Determinism requires only that `ctx` answers from immutable program
///   facts, as the VM's context does; digests minted under *different* fact
///   sets (e.g. a fact-free boundary context) agree exactly when
///   canonicalization never consults a fact that differs.
pub fn canonical_digest<C: TypeContext>(ty: &Ty, ctx: &C) -> u64 {
    /// FNV-1a, 64-bit. Local on purpose: the digest contract above is this
    /// exact algorithm; routing through a swappable `Hasher` dependency would
    /// invite silently changing the basis.
    struct Fnv1a(u64);

    impl std::hash::Hasher for Fnv1a {
        fn finish(&self) -> u64 {
            self.0
        }

        fn write(&mut self, bytes: &[u8]) {
            for byte in bytes {
                self.0 ^= u64::from(*byte);
                self.0 = self.0.wrapping_mul(0x100_0000_01b3);
            }
        }

        // `Hasher`'s default integer methods use native-endian bytes, which
        // would make the digest architecture-dependent. Override the complete
        // numeric surface so the derived `Hash` walk becomes a canonical byte
        // serialization. Lengths and enum discriminants written as pointer-
        // sized integers are widened, making 32- and 64-bit processes agree.
        fn write_u8(&mut self, value: u8) {
            self.write(&value.to_be_bytes());
        }

        fn write_u16(&mut self, value: u16) {
            self.write(&value.to_be_bytes());
        }

        fn write_u32(&mut self, value: u32) {
            self.write(&value.to_be_bytes());
        }

        fn write_u64(&mut self, value: u64) {
            self.write(&value.to_be_bytes());
        }

        fn write_u128(&mut self, value: u128) {
            self.write(&value.to_be_bytes());
        }

        fn write_usize(&mut self, value: usize) {
            self.write_u64(value as u64);
        }

        fn write_i8(&mut self, value: i8) {
            self.write(&value.to_be_bytes());
        }

        fn write_i16(&mut self, value: i16) {
            self.write(&value.to_be_bytes());
        }

        fn write_i32(&mut self, value: i32) {
            self.write(&value.to_be_bytes());
        }

        fn write_i64(&mut self, value: i64) {
            self.write(&value.to_be_bytes());
        }

        fn write_i128(&mut self, value: i128) {
            self.write(&value.to_be_bytes());
        }

        fn write_isize(&mut self, value: isize) {
            self.write_i64(value as i64);
        }
    }

    let canonical = NormalTy::canonical(ty, ctx);
    let mut hasher = Fnv1a(0xcbf2_9ce4_8422_2325);
    std::hash::Hash::hash(&canonical, &mut hasher);
    std::hash::Hasher::finish(&hasher)
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
fn heads_definitely_differ<H: Head>(a: &Ty<H>, b: &Ty<H>) -> bool {
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
pub fn definitely_disjoint<H: Head, C: TypeContext<H>>(a: &Ty<H>, b: &Ty<H>, ctx: &C) -> bool {
    ctx.definitely_disjoint(a, b)
}

/// Free-function form of [`TypeContext::definitely_equal`], for a context held by
/// value. Pending removal once every caller uses the method form.
pub fn definitely_equal<H: Head, C: TypeContext<H>>(a: &Ty<H>, b: &Ty<H>, ctx: &C) -> bool {
    ctx.definitely_equal(a, b)
}

impl<H: Head> NormalTy<H> {
    /// Normalize and canonicalize a [`Ty`] in one step (the shared entry point):
    /// build the named intermediate, strictly resolve its binders to the
    /// canonical de Bruijn phase, run the bottom-up set-theoretic algebra, and —
    /// only when recursion is present — the μ-canonicalization automaton
    /// ([`mu`]), which makes the result a unique representative of the
    /// equirecursive equivalence class.
    fn canonical<C: TypeContext<H>>(ty: &Ty<H>, ctx: &C) -> NormalTy<H> {
        Self::canonical_with(ty, ctx, &mut HashSet::new())
    }

    /// [`Self::canonical`] with the caller's co-inductive assumption set
    /// threaded through the union algebra (`absorb_subtypes`' subtype
    /// probes). The expanding arms of `is_subtype_of` MUST use this: a
    /// bound canonicalized under a FRESH set re-enters the very subtype
    /// question that triggered it, and a self-referential bound
    /// (`T extends Foo<T | int>`) then recurses unboundedly - a stack
    /// overflow, B-1091. Threading extends the declared co-inductive
    /// semantics to the re-entry instead of restarting it.
    fn canonical_with<C: TypeContext<H>>(
        ty: &Ty<H>,
        ctx: &C,
        assumptions: &mut HashSet<(NormalTy<H>, NormalTy<H>)>,
    ) -> NormalTy<H> {
        let (t, saw_mu) = Self::canonical_bottom_up(ty, ctx, assumptions);
        if saw_mu && t.contains_mu() {
            mu::canonicalize_mu(t, ctx)
        } else {
            t
        }
    }

    /// [`Self::canonical`] rendered back as a [`Ty`] - the body of
    /// [`TypeContext::normalize`]. On the mu path the automaton renders the
    /// root directly (root-unfold-once: a recursive alias exposes its head
    /// constructor; nested recursion stays folded as alias names), so
    /// `normalize` never calls [`Self::into_ty`] on a mu root.
    fn canonical_render<C: TypeContext<H>>(ty: &Ty<H>, ctx: &C) -> Ty<H> {
        let (t, saw_mu) = Self::canonical_bottom_up(ty, ctx, &mut HashSet::new());
        if saw_mu && t.contains_mu() {
            mu::canonicalize_mu_with_render(t, ctx).1
        } else {
            t.into_ty()
        }
    }

    /// The shared pre-automaton pipeline: named intermediate -> strict binder
    /// resolution -> bottom-up algebra (with open-member absorption deferred).
    fn canonical_bottom_up<C: TypeContext<H>>(
        ty: &Ty<H>,
        ctx: &C,
        assumptions: &mut HashSet<(NormalTy<H>, NormalTy<H>)>,
    ) -> (NormalTy<H>, bool) {
        let named = NormalTy::from_ty(ty, ctx, &mut HashSet::new(), PROJECTION_REDUCTION_FUEL);
        let mut saw_mu = false;
        let resolved = named.resolve_binders(&mut Vec::new(), &mut saw_mu);
        (resolved.canonicalize(ctx, saw_mu, assumptions), saw_mu)
    }

    /// Whether a μ-binder survives in this term — the automaton trigger. Checked
    /// only when [`Self::resolve_binders`] saw one (bottom-up absorption can
    /// still eliminate a closed μ member, e.g. under `unknown`), so the
    /// recursion-free hot path never runs this walk.
    fn contains_mu(&self) -> bool {
        match self {
            NormalTy::Mu { .. } => true,
            // A free `RecVar` cannot occur without its binder in a closed term.
            NormalTy::RecVar(_) => false,
            NormalTy::List(inner) => inner.contains_mu(),
            NormalTy::Map { key, value } | NormalTy::Future(key, value) => {
                key.contains_mu() || value.contains_mu()
            }
            NormalTy::Union(members) => members.iter().any(NormalTy::contains_mu),
            NormalTy::Class(_, args) => args.iter().any(NormalTy::contains_mu),
            NormalTy::Interface(_, args, bindings) => {
                args.iter().any(NormalTy::contains_mu)
                    || bindings.iter().any(|(_, t)| t.contains_mu())
            }
            NormalTy::Function {
                params,
                ret,
                throws,
            } => {
                params.iter().any(|p| p.ty.contains_mu())
                    || ret.contains_mu()
                    || throws.contains_mu()
            }
            NormalTy::AssociatedTypeProjection {
                base, interface, ..
            } => base.contains_mu() || interface.contains_mu(),
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

impl<H: Head> NormalTy<H> {
    /// Top-level concrete category of this type, or `None` for a non-ground head
    /// (union, interface, hole, type variable, …) for which no disjointness is
    /// provable.
    /// Whether `head` is one of the reflection type-kind classes
    /// (`baml.reflect.<kind>.Type`).
    ///
    /// The algebra's second nominal special case, recognized the same way as the
    /// first (`baml.AnyFunction`): by asking the context for each known name's
    /// head and comparing, never by inspecting the head itself. A head is opaque
    /// here — see [`Head`](crate::Head) — so the context is the only thing that
    /// can turn a name the algebra knows into a head it can compare.
    fn head_is_type_kind_class<C: TypeContext<H>>(head: &H, ctx: &C) -> bool {
        crate::type_kind::TypeKind::ALL.iter().any(|kind| {
            let name = crate::QualifiedTypeName::new(
                crate::Name::new("baml"),
                vec![crate::Name::new("reflect"), crate::Name::new(kind.namespace())],
                crate::Name::new("Type"),
            );
            ctx.head_lookup(&name).is_some_and(|known| &known == head)
        })
    }

    fn head_category<C: TypeContext<H>>(&self, ctx: &C) -> Option<Category> {
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
            NormalTy::Class(name, _) if Self::head_is_type_kind_class(name, ctx) => {
                Category::Type
            }
            NormalTy::Class(..) => Category::Class,
            NormalTy::List(_) => Category::List,
            NormalTy::Map { .. } => Category::Map,
            NormalTy::Enum(_) | NormalTy::EnumVariant(..) => Category::Enum,
            NormalTy::Function { .. } => Category::Function,
            NormalTy::Future(..) => Category::Future,
            // A μ is transparent to its head: the head of `μX.T` is the head of
            // its unfolding, and the walk into the body terminates (no unfold
            // happens here). A non-constructor body head (e.g. a still-unguarded
            // union, pending the ε-closure step) answers `None` through the arms
            // below — conservative.
            NormalTy::Mu { body, .. } => return body.head_category(ctx),
            // Not a ground concrete head — nothing provable. (A free `RecVar`
            // only occurs under its binder, which the μ arm above looks through.)
            NormalTy::Interface(..)
            | NormalTy::Union(_)
            | NormalTy::AssociatedTypeProjection { .. }
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
    ///
    /// Structural `!=` proves distinctness only on **canonical** forms, and the
    /// μ-unfolding arms of [`Self::is_disjoint_from`] hand this method
    /// *unfolded* (non-canonical) spellings — `μX.(int | X[])` next to
    /// `int | (μX.(int | X[]))[]` are one type — so a μ anywhere in either
    /// argument leaves disjointness unprovable here. (Same-category μ pairs
    /// still resolve through the nominal-head and category arms, which
    /// unfolding preserves.)
    fn arg_forces_disjoint(&self, other: &NormalTy<H>) -> bool {
        self.is_ground()
            && other.is_ground()
            && self != other
            && !self.contains_mu()
            && !other.contains_mu()
    }

    /// Whether no value of `self` can ever be `==`-equal to a value of `other`
    /// (the structural core of [`definitely_disjoint`]).
    fn is_disjoint_from<C: TypeContext<H>>(&self, other: &NormalTy<H>, ctx: &C) -> bool {
        match (self, other) {
            // A μ is its unfolding — expose the constructor head before the
            // structural arms. Terminates without an assumption set because
            // canonical μ bodies are constructor-headed (the automaton's
            // ε-closure eliminated unguarded spines): after both sides are
            // unfolded, every arm below is terminal (categories; invariant args
            // compared by equality) except union decomposition into
            // constructor-headed, non-union members — so the recursion is
            // bounded by two unfolds plus one member decomposition per side.
            //
            // The read-back bail (`canonicalize_mu` falling back to the
            // pre-automaton term) is the one path that hands this method a μ
            // with an unguarded spine; unfolding such a μ re-injects it into
            // its own union spine without ever crossing a constructor. Nothing
            // is provable about it here — answer "not provably disjoint".
            (NormalTy::Mu { .. }, _) | (_, NormalTy::Mu { .. })
                if self.has_unguarded_mu() || other.has_unguarded_mu() =>
            {
                false
            }
            (NormalTy::Mu { .. }, _) => self.unfold().is_disjoint_from(other, ctx),
            (_, NormalTy::Mu { .. }) => self.is_disjoint_from(&other.unfold(), ctx),

            // A union is disjoint from `rhs` iff every member is.
            (NormalTy::Union(members), rhs) => members.iter().all(|m| m.is_disjoint_from(rhs, ctx)),
            (lhs, NormalTy::Union(members)) => members.iter().all(|m| lhs.is_disjoint_from(m, ctx)),

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
                match rhs.head_category(ctx) {
                    Some(cat) => cat != Category::of_literal(lit),
                    None => false,
                }
            }

            // Otherwise: disjoint iff both are ground concrete heads of different
            // categories (`int` vs `string`, `list` vs `map`, a class vs an enum).
            _ => match (self.head_category(ctx), other.head_category(ctx)) {
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

/// Phase parameter of [`NormalTy`]: selects the μ-binder representation, so a
/// cross-phase term is *unrepresentable* — a named binder cannot occur inside a
/// canonical form, nor a de Bruijn index inside the `from_ty` intermediate, and
/// the only way from one phase to the other is the strict, total conversion
/// [`NormalTy::resolve_binders`].
trait MuPhase<H: Head> {
    /// Payload carried by a μ-binder ([`NormalTy::Mu`]).
    type Binder: Clone + std::fmt::Debug + PartialEq + Eq + PartialOrd + Ord + std::hash::Hash;
    /// Payload carried by a recursion variable ([`NormalTy::RecVar`]).
    type Var: Clone + std::fmt::Debug + PartialEq + Eq + PartialOrd + Ord + std::hash::Hash;
}

/// The `from_ty` intermediate phase: binders and back-references carry the alias
/// name whose expansion introduced them. This phase exists only between
/// `from_ty` and [`NormalTy::resolve_binders`]; nothing compares it for
/// identity, subtypes it, or renders it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Named {}

impl<H: Head> MuPhase<H> for Named {
    type Binder = H;
    type Var = H;
}

/// The canonical phase: back-references are de Bruijn indices — so the derived
/// equality on canonical forms *is* α-equivalence (`type A = int | A[]` and
/// `type B = int | B[]` share one canonical form) — and binders carry only the
/// equality-transparent [`MuDisplay`] payload for rendering.
///
/// INVARIANT (closed-term): a canonical form at a public boundary is closed —
/// every `RecVar(i)` has more than `i` enclosing `Mu`s. Unfolding
/// ([`NormalTy::unfold`]) therefore substitutes a *closed* term for the
/// outermost binder, which needs no index shifting and keeps derived-`==`
/// assumption probes exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Canonical {}

impl<H: Head> MuPhase<H> for Canonical {
    type Binder = MuDisplay<H>;
    type Var = u32;
}

/// Normalized structural type: aliases resolved, attributes and literal
/// freshness erased, recursion made explicit with μ-binders (representation per
/// phase `P` — see [`MuPhase`]). The default phase is [`Canonical`]: bare
/// `NormalTy` throughout the algebra means the canonical form, and only the
/// short-lived `from_ty` intermediate spells its phase (`NormalTy<H, Named>`).
///
/// Ordering (`PartialOrd`/`Ord`) is the canonical sort key for union members; it
/// has no semantic meaning beyond producing a deterministic canonical form.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum NormalTy<H: Head = QualifiedTypeName, P: MuPhase<H> = Canonical> {
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
    Class(H, Vec<NormalTy<H, P>>),
    Interface(H, Vec<NormalTy<H, P>>, Vec<(Name, NormalTy<H, P>)>),
    Enum(H),
    EnumVariant(H, Name),
    // Constructors
    List(Box<NormalTy<H, P>>),
    Map {
        key: Box<NormalTy<H, P>>,
        value: Box<NormalTy<H, P>>,
    },
    Union(Vec<NormalTy<H, P>>),
    Function {
        params: Vec<NormalParam<H, P>>,
        ret: Box<NormalTy<H, P>>,
        throws: Box<NormalTy<H, P>>,
    },
    Future(Box<NormalTy<H, P>>, Box<NormalTy<H, P>>),
    AssociatedTypeProjection {
        base: Box<NormalTy<H, P>>,
        /// The declaring interface (a normalized `NormalTy::Interface`), always
        /// present — mirrors the non-optional `Ty::AssociatedTypeProjection`
        /// qualifier it is built from, and is what makes a realized-base
        /// projection reducible via [`TypeContext::project`].
        interface: Box<NormalTy<H, P>>,
        member: Name,
    },
    // Recursion. `Named`: binder/variable carry the alias name whose expansion
    // introduced them. `Canonical`: the binder carries its equality-transparent
    // display payload and the variable a de Bruijn index (0 = innermost
    // enclosing binder) — see [`Canonical`] for the closed-term invariant.
    Mu {
        binder: P::Binder,
        body: Box<NormalTy<H, P>>,
    },
    /// μ-bound recursion variable (a back-reference to an enclosing [`NormalTy::Mu`]).
    RecVar(P::Var),
    /// A generic type parameter — opaque, compatible only with itself, its
    /// bound's supertypes, and the top type.
    TypeVar(ParamTy),
    /// An alias the context could not resolve — opaque, equal only to the same
    /// unresolved alias (fail-safe; never equated to an expansion).
    OpaqueAlias(H),
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
struct NormalParam<H: Head, P: MuPhase<H> = Canonical> {
    name: Option<Name>,
    ty: NormalTy<H, P>,
    mode: FunctionParamMode,
}

/// Display payload of a canonical μ-binder ([`NormalTy::Mu`] at [`Canonical`]).
///
/// `rendered` is the whole μ-subterm as a [`Ty`] — surface syntax has no binder,
/// so recursion is spelled via alias names — and is what [`NormalTy::into_ty`]
/// emits for the binder (it never descends into the body). Two producers:
///
/// - [`NormalTy::resolve_binders`] fills the **legacy** rendering (the body with
///   back-references spelled as alias names — the historical `normalize` output),
///   which pre-μ-canonicalization fact-oracle calls observe during bottom-up
///   absorption of closed μ members.
/// - The μ-canonicalization automaton ([`super::normalize::mu`]) replaces it with
///   the **named-cut** rendering (recursion folded to alias names at every named
///   cycle state), the canonical output form.
///
/// `name` is the alias whose expansion introduced the binder — `None` only for
/// automaton read-back binders that land on a cycle state no alias denotes (e.g.
/// the list state of `A[]` for `type A = int | A[]`); such a binder is exactly
/// why `rendered` must be precomputed (no single alias name can spell it).
///
/// **Equality-transparent by design**: all values compare equal, order equal, and
/// hash identically, so the derived `PartialEq`/`Ord`/`Hash` on [`NormalTy`] see
/// only the de Bruijn structure — canonical equality *is* α-equivalence, and the
/// rendering (which necessarily picks concrete alias names) can never split it.
/// This is the same discipline as spans ignored by AST equality: a description of
/// the value, not part of it.
#[derive(Debug, Clone)]
struct MuDisplay<H: Head> {
    name: Option<H>,
    rendered: Box<Ty<H>>,
}

impl<H: Head> PartialEq for MuDisplay<H> {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl<H: Head> Eq for MuDisplay<H> {}

impl<H: Head> PartialOrd for MuDisplay<H> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<H: Head> Ord for MuDisplay<H> {
    fn cmp(&self, _other: &Self) -> std::cmp::Ordering {
        std::cmp::Ordering::Equal
    }
}

impl<H: Head> std::hash::Hash for MuDisplay<H> {
    fn hash<S: std::hash::Hasher>(&self, _state: &mut S) {}
}

impl<H: Head> NormalTy<H, Named> {
    // ── conversion in: Ty → NormalTy<Named> ────────────────────────────────

    fn from_ty<C: TypeContext<H>>(
        ty: &Ty<H>,
        ctx: &C,
        expanding: &mut HashSet<H>,
        // Remaining projection-reduction steps along this path. Reducing a
        // projection (`(int as Foo).Assoc` → `string`) is a pure type-level
        // operator, but could loop on a cyclic `type A = (C as I).B` /
        // `type B = (C as J).A`; each reduction spends one unit, and on exhaustion
        // the projection stays opaque (conservative — never over-equates).
        fuel: u32,
    ) -> NormalTy<H, Named> {
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
                        binder: qn.clone(),
                        body: Box::new(body),
                    }
                } else {
                    body
                }
            }
        }
    }

    fn from_tys<C: TypeContext<H>>(
        tys: &[Ty<H>],
        ctx: &C,
        expanding: &mut HashSet<H>,
        fuel: u32,
    ) -> Vec<NormalTy<H, Named>> {
        tys.iter()
            .map(|t| Self::from_ty(t, ctx, expanding, fuel))
            .collect()
    }

    /// Whether this type contains a back-reference to μ-variable `var`.
    fn mentions_rec_var(&self, var: &H) -> bool {
        match self {
            NormalTy::RecVar(v) => v == var,
            // A nested μ shadowing the same name rebinds it; stop descending.
            NormalTy::Mu { binder: v, body } => v != var && body.mentions_rec_var(var),
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

    /// Render a named-phase term as a [`Ty`], with every binder dropped and every
    /// back-reference spelled as its alias name — the historical `normalize`
    /// output shape. This fills the *interim* [`MuDisplay::rendered`] payload in
    /// [`Self::resolve_binders`]: it is what fact-oracle calls
    /// (`implements_interface`, `interface_requires`) observe when bottom-up
    /// absorption compares a closed μ member, keeping their view identical to the
    /// pre-μ-canonicalization behavior. The automaton replaces it with the
    /// canonical named-cut rendering.
    fn legacy_render(&self) -> Ty<H> {
        let attr = TyAttr::default();
        match self {
            NormalTy::Int => Ty::Int { attr },
            NormalTy::Bigint => Ty::Bigint { attr },
            NormalTy::Float => Ty::Float { attr },
            NormalTy::String => Ty::String { attr },
            NormalTy::Bool => Ty::Bool { attr },
            NormalTy::Null => Ty::Null { attr },
            NormalTy::Uint8Array => Ty::Uint8Array { attr },
            NormalTy::Media(kind) => Ty::Media(*kind, attr),
            NormalTy::Void => Ty::Void { attr },
            NormalTy::RustType => Ty::RustType { attr },
            NormalTy::Type => Ty::Type { attr },
            NormalTy::Resource => Ty::Resource { attr },
            NormalTy::PromptAst => Ty::PromptAst { attr },
            NormalTy::BuiltinUnknown => Ty::BuiltinUnknown { attr },
            NormalTy::Never => Ty::Never { attr },
            NormalTy::Unknown => Ty::Unknown { attr },
            NormalTy::Error => Ty::Error { attr },
            NormalTy::Literal(lit) => Ty::Literal(lit.clone(), crate::Freshness::Regular, attr),
            NormalTy::Class(qn, args) => Ty::Class(
                qn.clone(),
                args.iter().map(NormalTy::legacy_render).collect(),
                attr,
            ),
            NormalTy::Interface(qn, args, bindings) => Ty::Interface(
                qn.clone(),
                args.iter().map(NormalTy::legacy_render).collect(),
                bindings
                    .iter()
                    .map(|(name, ty)| (name.clone(), ty.legacy_render()))
                    .collect(),
                attr,
            ),
            NormalTy::Enum(qn) => Ty::Enum(qn.clone(), attr),
            NormalTy::EnumVariant(qn, v) => Ty::EnumVariant(qn.clone(), v.clone(), attr),
            NormalTy::List(inner) => Ty::List(Box::new(inner.legacy_render()), attr),
            NormalTy::Map { key, value } => Ty::Map {
                key: Box::new(key.legacy_render()),
                value: Box::new(value.legacy_render()),
                attr,
            },
            NormalTy::Union(members) => {
                Ty::Union(members.iter().map(NormalTy::legacy_render).collect(), attr)
            }
            NormalTy::Function {
                params,
                ret,
                throws,
            } => Ty::Function {
                params: params
                    .iter()
                    .map(|p| FunctionParamTy {
                        name: p.name.clone(),
                        ty: p.ty.legacy_render(),
                        mode: p.mode,
                    })
                    .collect(),
                ret: Box::new(ret.legacy_render()),
                throws: Box::new(throws.legacy_render()),
                attr,
            },
            NormalTy::Future(value, error) => Ty::Future(
                Box::new(value.legacy_render()),
                Box::new(error.legacy_render()),
                attr,
            ),
            NormalTy::AssociatedTypeProjection {
                base,
                interface,
                member,
            } => Ty::AssociatedTypeProjection {
                base: Box::new(base.legacy_render()),
                interface: Box::new(match &**interface {
                    NormalTy::Interface(name, generics, bindings) => Interface {
                        name: name.clone(),
                        generics: generics.iter().map(NormalTy::legacy_render).collect(),
                        associated_types: bindings
                            .iter()
                            .map(|(name, ty)| (name.clone(), ty.legacy_render()))
                            .collect(),
                    },
                    _ => unreachable!("projection qualifier is an interface"),
                }),
                member: member.clone(),
                attr,
            },
            NormalTy::TypeVar(name) => Ty::TypeVar(name.clone(), attr),
            // The binder has no surface syntax (its body renders in place); a
            // back-reference is spelled as its alias name — which in the named
            // phase the variable itself carries.
            NormalTy::Mu { body, .. } => body.legacy_render(),
            NormalTy::RecVar(qn) | NormalTy::OpaqueAlias(qn) => Ty::TypeAlias(qn.clone(), attr),
        }
    }

    // ── phase conversion: NormalTy<Named> → NormalTy ────────────

    /// Strictly convert the `from_ty` intermediate to the canonical de Bruijn
    /// phase (the only path between the phases): each back-reference becomes the
    /// distance to its binder, each binder keeps its alias name as the
    /// equality-transparent display payload, and `saw_mu` reports whether any
    /// binder was emitted (the recursive-type slow-path flag).
    ///
    /// This runs on the *completed* named term, where binder-hood is already
    /// decided — computing indices during `from_ty` itself would be off by one
    /// whenever an intervening alias expansion turns out not to bind (mutual
    /// recursion), because `from_ty` only wraps a binder after seeing the whole
    /// body.
    fn resolve_binders(self, stack: &mut Vec<H>, saw_mu: &mut bool) -> NormalTy<H> {
        match self {
            NormalTy::Mu { binder, body } => {
                // `from_ty` wraps a binder only when the body mentions it, and
                // nothing between `from_ty` and this conversion can remove a
                // back-reference — so a vacuous binder is a bug, not a case.
                debug_assert!(
                    body.mentions_rec_var(&binder),
                    "from_ty emitted a vacuous μ-binder"
                );
                *saw_mu = true;
                let display = MuDisplay {
                    name: Some(binder.clone()),
                    rendered: Box::new(body.legacy_render()),
                };
                stack.push(binder);
                let body = body.resolve_binders(stack, saw_mu);
                stack.pop();
                NormalTy::Mu {
                    binder: display,
                    body: Box::new(body),
                }
            }
            NormalTy::RecVar(qn) => {
                let index = stack
                    .iter()
                    .rev()
                    .position(|v| *v == qn)
                    .unwrap_or_else(|| {
                        unreachable!(
                            "from_ty emits a RecVar only while its alias is on the \
                             expanding path, so its binder is on the stack"
                        )
                    });
                NormalTy::RecVar(index as u32)
            }
            NormalTy::Class(qn, args) => NormalTy::Class(
                qn,
                args.into_iter()
                    .map(|a| a.resolve_binders(stack, saw_mu))
                    .collect(),
            ),
            NormalTy::Interface(qn, args, bindings) => NormalTy::Interface(
                qn,
                args.into_iter()
                    .map(|a| a.resolve_binders(stack, saw_mu))
                    .collect(),
                bindings
                    .into_iter()
                    .map(|(n, t)| (n, t.resolve_binders(stack, saw_mu)))
                    .collect(),
            ),
            NormalTy::List(inner) => NormalTy::List(Box::new(inner.resolve_binders(stack, saw_mu))),
            NormalTy::Map { key, value } => NormalTy::Map {
                key: Box::new(key.resolve_binders(stack, saw_mu)),
                value: Box::new(value.resolve_binders(stack, saw_mu)),
            },
            NormalTy::Union(members) => NormalTy::Union(
                members
                    .into_iter()
                    .map(|m| m.resolve_binders(stack, saw_mu))
                    .collect(),
            ),
            NormalTy::Function {
                params,
                ret,
                throws,
            } => NormalTy::Function {
                params: params
                    .into_iter()
                    .map(|p| NormalParam {
                        name: p.name,
                        ty: p.ty.resolve_binders(stack, saw_mu),
                        mode: p.mode,
                    })
                    .collect(),
                ret: Box::new(ret.resolve_binders(stack, saw_mu)),
                throws: Box::new(throws.resolve_binders(stack, saw_mu)),
            },
            NormalTy::Future(value, error) => NormalTy::Future(
                Box::new(value.resolve_binders(stack, saw_mu)),
                Box::new(error.resolve_binders(stack, saw_mu)),
            ),
            NormalTy::AssociatedTypeProjection {
                base,
                interface,
                member,
            } => NormalTy::AssociatedTypeProjection {
                base: Box::new(base.resolve_binders(stack, saw_mu)),
                interface: Box::new(interface.resolve_binders(stack, saw_mu)),
                member,
            },
            NormalTy::Int => NormalTy::Int,
            NormalTy::Bigint => NormalTy::Bigint,
            NormalTy::Float => NormalTy::Float,
            NormalTy::String => NormalTy::String,
            NormalTy::Bool => NormalTy::Bool,
            NormalTy::Null => NormalTy::Null,
            NormalTy::Uint8Array => NormalTy::Uint8Array,
            NormalTy::Media(kind) => NormalTy::Media(kind),
            NormalTy::Void => NormalTy::Void,
            NormalTy::RustType => NormalTy::RustType,
            NormalTy::Type => NormalTy::Type,
            NormalTy::Resource => NormalTy::Resource,
            NormalTy::PromptAst => NormalTy::PromptAst,
            NormalTy::Literal(lit) => NormalTy::Literal(lit),
            NormalTy::Enum(qn) => NormalTy::Enum(qn),
            NormalTy::EnumVariant(qn, v) => NormalTy::EnumVariant(qn, v),
            NormalTy::TypeVar(name) => NormalTy::TypeVar(name),
            NormalTy::OpaqueAlias(qn) => NormalTy::OpaqueAlias(qn),
            NormalTy::Never => NormalTy::Never,
            NormalTy::BuiltinUnknown => NormalTy::BuiltinUnknown,
            NormalTy::Unknown => NormalTy::Unknown,
            NormalTy::Error => NormalTy::Error,
        }
    }
}

impl<H: Head> NormalTy<H> {
    // ── canonicalization ───────────────────────────────────────────────────

    /// Rewrite to a unique canonical form: children canonicalized bottom-up,
    /// unions reduced by the full set algebra. `saw_mu` is the recursive-type
    /// flag from [`NormalTy::resolve_binders`]; when clear, the μ-related guards
    /// below vanish from the hot path.
    fn canonicalize<C: TypeContext<H>>(
        self,
        ctx: &C,
        saw_mu: bool,
        assumptions: &mut HashSet<(NormalTy<H>, NormalTy<H>)>,
    ) -> NormalTy<H> {
        match self {
            NormalTy::Class(qn, args) => NormalTy::Class(
                qn,
                args.into_iter()
                    .map(|a| a.canonicalize(ctx, saw_mu, assumptions))
                    .collect(),
            ),
            NormalTy::Interface(qn, args, bindings) => {
                let mut bindings: Vec<_> = bindings
                    .into_iter()
                    .map(|(name, ty)| (name, ty.canonicalize(ctx, saw_mu, assumptions)))
                    .collect();
                bindings.sort_by(|(a, _), (b, _)| a.cmp(b));
                NormalTy::Interface(
                    qn,
                    args.into_iter()
                        .map(|a| a.canonicalize(ctx, saw_mu, assumptions))
                        .collect(),
                    bindings,
                )
            }
            NormalTy::List(inner) => {
                NormalTy::List(Box::new(inner.canonicalize(ctx, saw_mu, assumptions)))
            }
            NormalTy::Map { key, value } => NormalTy::Map {
                key: Box::new(key.canonicalize(ctx, saw_mu, assumptions)),
                value: Box::new(value.canonicalize(ctx, saw_mu, assumptions)),
            },
            NormalTy::Future(value, error) => NormalTy::Future(
                Box::new(value.canonicalize(ctx, saw_mu, assumptions)),
                Box::new(error.canonicalize(ctx, saw_mu, assumptions)),
            ),
            NormalTy::AssociatedTypeProjection {
                base,
                interface,
                member,
            } => NormalTy::AssociatedTypeProjection {
                base: Box::new(base.canonicalize(ctx, saw_mu, assumptions)),
                interface: Box::new(interface.canonicalize(ctx, saw_mu, assumptions)),
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
                    let ty = p.ty.canonicalize(ctx, saw_mu, assumptions);
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
                    ret: Box::new(ret.canonicalize(ctx, saw_mu, assumptions)),
                    throws: Box::new(throws.canonicalize(ctx, saw_mu, assumptions)),
                }
            }
            // A binder stays put here even if an algebra step strands it —
            // `unknown`-absorption can swallow a whole union including its
            // back-references, leaving a μ over a body that no longer mentions
            // it. That is harmless: the automaton's read-back emits binders only
            // for states that are actually back-referenced, so a vacuous binder
            // self-heals downstream.
            NormalTy::Mu { binder, body } => NormalTy::Mu {
                binder,
                body: Box::new(body.canonicalize(ctx, saw_mu, assumptions)),
            },
            NormalTy::Union(members) => {
                let members = members
                    .into_iter()
                    .map(|m| m.canonicalize(ctx, saw_mu, assumptions))
                    .collect();
                Self::canonicalize_union(members, ctx, saw_mu, assumptions)
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
    fn invariant_compatible<C: TypeContext<H>>(
        &self,
        other: &NormalTy<H>,
        ctx: &C,
        assumptions: &mut HashSet<(NormalTy<H>, NormalTy<H>)>,
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
    fn pin_is_tautological<C: TypeContext<H>>(
        var: &ParamTy,
        qn: &H,
        args: &[NormalTy<H>],
        pin: &(Name, NormalTy<H>),
        ctx: &C,
        assumptions: &mut HashSet<(NormalTy<H>, NormalTy<H>)>,
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
    fn is_subtype_of<C: TypeContext<H>>(
        &self,
        sup: &NormalTy<H>,
        ctx: &C,
        assumptions: &mut HashSet<(NormalTy<H>, NormalTy<H>)>,
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
        //   * μ-unfolding — `unfold` can reproduce the same pair (`(_, Mu)` on
        //     the right, `(Mu, _)` on the left);
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
    fn is_subtype_of_inner<C: TypeContext<H>>(
        &self,
        sup: &NormalTy<H>,
        ctx: &C,
        assumptions: &mut HashSet<(NormalTy<H>, NormalTy<H>)>,
    ) -> bool {
        match (self, sup) {
            // μ-unfolding (equirecursive). The closed-term substitution needs no
            // index shifting, and the outer `is_subtype_of` recorded this pair on
            // the expanding-arm assumption set.
            (NormalTy::Mu { .. }, _) => self.unfold().is_subtype_of(sup, ctx, assumptions),
            (_, NormalTy::Mu { .. }) => self.is_subtype_of(&sup.unfold(), ctx, assumptions),

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
                    NormalTy::canonical_with(&bound.to_ty(), ctx, assumptions).is_subtype_of(
                        &stripped,
                        ctx,
                        assumptions,
                    )
                })
            }
            (NormalTy::TypeVar(name), _) => ctx.type_var_bound(name).iter().any(|bound| {
                NormalTy::canonical_with(&bound.to_ty(), ctx, assumptions).is_subtype_of(
                    sup,
                    ctx,
                    assumptions,
                )
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
            ) => {
                (**iface).clone().into_interface().is_some_and(|i| {
                    ctx.associated_type_bound(&i, member.clone())
                        .iter()
                        .any(|bound| {
                            NormalTy::canonical_with(&bound.to_ty(), ctx, assumptions)
                                .is_subtype_of(sup, ctx, assumptions)
                        })
                })
            }

            // BEP-062: `baml.AnyFunction` is a compiler builtin implemented by
            // every function type, with the parameter list erased. Conformance
            // is derived right here rather than from an `implements` block
            // (function types are not impl subjects): the return type must fit
            // the `Returns` pin and the throws type the `Throws` pin. Omitted
            // pins were filled with their `unknown` defaults when the
            // existential was lowered; a pin missing anyway degrades to that
            // same top-type default (accepts everything).
            (NormalTy::Function { ret, throws, .. }, NormalTy::Interface(qn, _, bindings))
                if is_any_function(qn, ctx) =>
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
            // write-through position. A pin missing on the sub side reads as
            // its declared `unknown` default (BEP-062: a bare `AnyFunction`
            // holds SOME function whose channels are unconstrained - exactly
            // the top type), so `AnyFunction <: AnyFunction<Returns = unknown>`
            // holds without eager default-filling at the lowering layer.
            (
                NormalTy::Interface(sub_qn, _, sub_bindings),
                NormalTy::Interface(sup_qn, _, sup_bindings),
            ) if is_any_function(sub_qn, ctx) && is_any_function(sup_qn, ctx) => {
                sup_bindings.iter().all(|(name, sup_pin)| {
                    match sub_bindings.iter().find(|(n, _)| n == name) {
                        Some((_, sub_pin)) => sub_pin.is_subtype_of(sup_pin, ctx, assumptions),
                        None => NormalTy::BuiltinUnknown.is_subtype_of(sup_pin, ctx, assumptions),
                    }
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
            // BEP-066: the nine reflection-kind classes form one sealed family
            // beneath the `type` carrier. Because membership is hard-coded to
            // builtin qualified names, user classes cannot acquire this edge.
            (NormalTy::Class(name, _), NormalTy::Type)
                if Self::head_is_type_kind_class(name, ctx) =>
            {
                true
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

    // ── conversion out: NormalTy → Ty ───────────────────────────

    /// Render a **closed** canonical form back as a [`Ty`]. Surface syntax has no
    /// μ-binder, so a μ-subterm renders as its precomputed [`MuDisplay`] payload
    /// (recursion spelled via alias names) — this never descends into a μ body,
    /// which is why a `RecVar` (always under its binder in a closed term) is
    /// unreachable here.
    fn into_ty(self) -> Ty<H> {
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
                // it round-trips back to an `Interface` here. (A qualifier
                // argument capturing an enclosing recursion variable is fine: the
                // enclosing μ renders as its display, so this arm only ever sees
                // qualifiers whose free variables were closed off above.)
                interface: Box::new(
                    interface
                        .into_interface()
                        .unwrap_or_else(|| unreachable!("projection qualifier is an interface")),
                ),
                member,
                attr,
            },
            NormalTy::TypeVar(name) => Ty::TypeVar(name, attr),
            // A μ-subterm renders as its precomputed display — the named-cut
            // rendering from the canonicalization automaton (or the legacy
            // rendering on the short-lived pre-automaton intermediate).
            NormalTy::Mu { binder, .. } => *binder.rendered,
            // Unreachable for closed terms: the μ arm above never descends into
            // its body, so every `RecVar` stays behind its binder's display.
            NormalTy::RecVar(_) => unreachable!(
                "into_ty on a free RecVar; canonical forms at public boundaries \
                 are closed and render recursion via their binder's display"
            ),
            NormalTy::OpaqueAlias(qn) => Ty::TypeAlias(qn, attr),
        }
    }
}

impl<H: Head> NormalTy<H> {
    fn into_tys(tys: Vec<NormalTy<H>>) -> Vec<Ty<H>> {
        tys.into_iter().map(NormalTy::into_ty).collect()
    }

    /// The [`Interface`] constraint denoted by a `NormalTy::Interface`'s parts —
    /// its name, generic input arguments, and associated-type bindings, converted
    /// back to `Ty`. This is the precise interface shape handed to the
    /// [`TypeContext`] membership (`implements_interface`) and requires
    /// (`interface_requires`) oracles, so they never have to re-destructure a
    /// loose `Ty` to recover it. The parts are always closed here: the subtype
    /// arms that build a constraint fire only after μ-unfolding the operand.
    fn interface_constraint(
        name: &H,
        generics: &[NormalTy<H>],
        bindings: &[(Name, NormalTy<H>)],
    ) -> Interface<H> {
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
    ///
    /// A μ-wrapped interface recovers the constraint from its **display**: when
    /// a projection's qualifier mentions the enclosing recursive alias
    /// (`type A = I<A> | (C as I<A>).M`), minimization merges the qualifier
    /// state with the standalone `I<A>` member's, so the canonical qualifier is
    /// `μX.I<…X…>`. Unfolding here would loop — each unfold re-injects the μ
    /// into the argument that contains the projection, whose qualifier unfolds
    /// again — but the display is a finite alias-based spelling of the whole
    /// qualifier (`I<A>`), computed by the renderer, so its parts are read off
    /// directly. A display that is not interface-shaped (an exotic cover
    /// rendering) degrades to `None`, conservative.
    fn into_interface(self) -> Option<Interface<H>> {
        match self {
            NormalTy::Interface(name, generics, bindings) => Some(Interface {
                name,
                generics: Self::into_tys(generics),
                associated_types: bindings
                    .into_iter()
                    .map(|(name, ty)| (name, ty.into_ty()))
                    .collect(),
            }),
            NormalTy::Mu { binder, .. } => match *binder.rendered {
                Ty::Interface(name, generics, associated_types, _) => Some(Interface {
                    name,
                    generics,
                    associated_types,
                }),
                _ => None,
            },
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

impl<H: Head> NormalTy<H> {
    /// Reduce a union of already-canonical members to canonical form: flatten,
    /// remove `never`, absorb under `unknown`, collapse complete enums, absorb
    /// subtype-members, then sort/dedup and unwrap singletons.
    fn canonicalize_union<C: TypeContext<H>>(
        members: Vec<NormalTy<H>>,
        ctx: &C,
        saw_mu: bool,
        assumptions: &mut HashSet<(NormalTy<H>, NormalTy<H>)>,
    ) -> NormalTy<H> {
        // Flatten one level (members are canonical, but a μ-unfold or alias could
        // surface a nested union) and drop `never`.
        let mut flat: Vec<NormalTy<H>> = Vec::new();
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
        Self::collapse_complete_bools(&mut flat);
        let mut flat = Self::absorb_subtypes(&flat, ctx, saw_mu, assumptions);

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
    fn collapse_complete_enums<C: TypeContext<H>>(members: &mut Vec<NormalTy<H>>, ctx: &C) {
        // Distinct enums that have at least one variant present.
        let mut enums: Vec<H> = members
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
            // Requires at least two variants: collapsing a single-variant enum
            // would split one value set into two canonical spellings (`E.A | E.A`
            // as a union collapses while a bare `E.A` cannot), breaking
            // idempotence. `E.A` ≡ `E` for a one-variant enum stays an
            // (acknowledged) incompleteness instead.
            if all.len() >= 2 && all.iter().all(|v| present.contains(v)) {
                members.retain(|m| !matches!(m, NormalTy::EnumVariant(en, _) if *en == e));
                members.push(NormalTy::Enum(e));
            }
        }
    }

    /// Whether any μ-binder in this term is **non-contractive**: its body
    /// reaches a back-reference to that same binder through union spines alone
    /// (an unguarded member, e.g. the μ of `type A = A | A[]`). The coinductive
    /// subtype checker is sound only on contractive operands — on an unguarded μ
    /// the assumption probe can close a derivation without ever crossing a
    /// constructor and prove `T <: μ` for arbitrary `T` — so such members must
    /// not reach it until the automaton's ε-closure has resolved the spine.
    fn has_unguarded_mu(&self) -> bool {
        fn spine_has_rec<H: Head>(t: &NormalTy<H>, depth: u32) -> bool {
            match t {
                NormalTy::RecVar(i) => *i == depth,
                NormalTy::Union(members) => members.iter().any(|m| spine_has_rec(m, depth)),
                NormalTy::Mu { body, .. } => spine_has_rec(body, depth + 1),
                _ => false,
            }
        }
        match self {
            NormalTy::Mu { body, .. } => spine_has_rec(body, 0) || body.has_unguarded_mu(),
            NormalTy::RecVar(_) => false,
            NormalTy::List(inner) => inner.has_unguarded_mu(),
            NormalTy::Map { key, value } | NormalTy::Future(key, value) => {
                key.has_unguarded_mu() || value.has_unguarded_mu()
            }
            NormalTy::Union(members) => members.iter().any(NormalTy::has_unguarded_mu),
            NormalTy::Class(_, args) => args.iter().any(NormalTy::has_unguarded_mu),
            NormalTy::Interface(_, args, bindings) => {
                args.iter().any(NormalTy::has_unguarded_mu)
                    || bindings.iter().any(|(_, t)| t.has_unguarded_mu())
            }
            NormalTy::Function {
                params,
                ret,
                throws,
            } => {
                params.iter().any(|p| p.ty.has_unguarded_mu())
                    || ret.has_unguarded_mu()
                    || throws.has_unguarded_mu()
            }
            NormalTy::AssociatedTypeProjection {
                base, interface, ..
            } => base.has_unguarded_mu() || interface.has_unguarded_mu(),
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

    /// Replace the complete pair of bool literals with `bool`
    /// (`true | false == bool`, TYPE_SYSTEM.md §Subtyping Cases) — the bool
    /// analogue of enum completeness, context-free because the variant family is
    /// closed and its equality unoverridable.
    fn collapse_complete_bools(members: &mut Vec<NormalTy<H>>) {
        let has = |members: &[NormalTy<H>], b: bool| {
            members
                .iter()
                .any(|m| matches!(m, NormalTy::Literal(Literal::Bool(x)) if *x == b))
        };
        if has(members, true) && has(members, false) {
            members.retain(|m| !matches!(m, NormalTy::Literal(Literal::Bool(_))));
            members.push(NormalTy::Bool);
        }
    }

    /// Remove any member subsumed by another (`X | Y == Y` when `X <: Y`). Covers
    /// literal-into-base, variant-into-enum, `C | I == I`, `A | B == B`, and
    /// `T | I == I`. Error-recovery sentinels never absorb or are absorbed.
    ///
    /// Pairs involving a **deferred** member are skipped, for two reasons that
    /// share one guard:
    /// - an *open* member (a free `RecVar` — this bottom-up pass runs inside
    ///   μ-bodies) must never reach the subtype checker or a [`TypeContext`]
    ///   callback: its recursion variables are bound by a binder we cannot see;
    /// - a member containing a *non-contractive* μ (an unguarded spine, e.g.
    ///   `type A = A | A[]` before ε-closure) would let the coinductive
    ///   assumption probe prove `T <: μ` for arbitrary `T` without crossing a
    ///   constructor, silently absorbing real siblings.
    ///
    /// Closed, contractive members participate normally. Deferring is
    /// conservative (a union keeps a member another semantically covers) — the
    /// μ-canonicalization automaton re-runs absorption over closed, ε-closed
    /// per-state read-backs and completes it.
    fn absorb_subtypes<C: TypeContext<H>>(
        members: &[NormalTy<H>],
        ctx: &C,
        saw_mu: bool,
        assumptions: &mut HashSet<(NormalTy<H>, NormalTy<H>)>,
    ) -> Vec<NormalTy<H>> {
        let n = members.len();
        // Only computed when a μ exists somewhere in the term (rare) — the hot
        // path pays one branch.
        let open: Vec<bool> = if saw_mu {
            members
                .iter()
                .map(|m| m.has_free_rec_var(0) || m.has_unguarded_mu())
                .collect()
        } else {
            vec![false; n]
        };
        let mut keep = vec![true; n];
        for i in 0..n {
            if members[i].is_sentinel() || open[i] {
                continue;
            }
            for j in 0..n {
                if i == j || !keep[j] || members[j].is_sentinel() || open[j] {
                    continue;
                }
                if !members[i].is_subtype_of(&members[j], ctx, assumptions) {
                    continue;
                }
                // `members[i] <: members[j]`. Drop `i`, unless they are mutual
                // subtypes (equivalent but not structurally equal — e.g. cyclic
                // `requires`); then keep the lower index deterministically.
                let mutual = members[j].is_subtype_of(&members[i], ctx, assumptions);
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
// μ-UNFOLDING & FUNCTION PARAMETERS
// ═══════════════════════════════════════════════════════════════════════════

impl<H: Head> NormalTy<H> {
    /// One unfold step of a closed canonical μ: `μX.body ↦ body[μX.body/X]`,
    /// i.e. every `RecVar` bound by *this* binder is replaced with the whole μ-term.
    ///
    /// Because `self` is closed (the public canonical-form invariant), the
    /// replacement is closed too, so grafting it under deeper binders can neither
    /// capture nor be captured — no de Bruijn shifting is needed, and the result
    /// is again closed with a unique (α-canonical) representation, keeping the
    /// derived-`==` probes of the co-inductive assumption set exact.
    fn unfold(&self) -> NormalTy<H> {
        let NormalTy::Mu { body, .. } = self else {
            unreachable!("unfold on a non-μ canonical form");
        };
        debug_assert!(
            self.is_closed(),
            "unfold on an open term violates the closed-term invariant"
        );
        body.replace_rec_var(0, self)
    }

    /// Whether every `RecVar` is bound by an enclosing μ-binder (debug-assert
    /// support for the closed-term invariant).
    fn is_closed(&self) -> bool {
        !self.has_free_rec_var(0)
    }

    /// Whether a `RecVar` with index ≥ `depth` (i.e. free relative to `depth`
    /// enclosing binders) occurs in this term.
    fn has_free_rec_var(&self, depth: u32) -> bool {
        match self {
            NormalTy::RecVar(i) => *i >= depth,
            NormalTy::Mu { body, .. } => body.has_free_rec_var(depth + 1),
            NormalTy::List(inner) => inner.has_free_rec_var(depth),
            NormalTy::Map { key, value } | NormalTy::Future(key, value) => {
                key.has_free_rec_var(depth) || value.has_free_rec_var(depth)
            }
            NormalTy::Union(members) => members.iter().any(|m| m.has_free_rec_var(depth)),
            NormalTy::Class(_, args) => args.iter().any(|a| a.has_free_rec_var(depth)),
            NormalTy::Interface(_, args, bindings) => {
                args.iter().any(|a| a.has_free_rec_var(depth))
                    || bindings.iter().any(|(_, t)| t.has_free_rec_var(depth))
            }
            NormalTy::Function {
                params,
                ret,
                throws,
            } => {
                params.iter().any(|p| p.ty.has_free_rec_var(depth))
                    || ret.has_free_rec_var(depth)
                    || throws.has_free_rec_var(depth)
            }
            NormalTy::AssociatedTypeProjection {
                base, interface, ..
            } => base.has_free_rec_var(depth) || interface.has_free_rec_var(depth),
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

    /// Replace every `RecVar` bound by the binder `depth` levels out with
    /// `replacement` (which must be closed — see [`Self::unfold`]).
    fn replace_rec_var(&self, depth: u32, replacement: &NormalTy<H>) -> NormalTy<H> {
        match self {
            NormalTy::RecVar(i) if *i == depth => replacement.clone(),
            NormalTy::Mu { binder, body } => NormalTy::Mu {
                binder: binder.clone(),
                body: Box::new(body.replace_rec_var(depth + 1, replacement)),
            },
            NormalTy::Class(qn, args) => NormalTy::Class(
                qn.clone(),
                args.iter()
                    .map(|a| a.replace_rec_var(depth, replacement))
                    .collect(),
            ),
            NormalTy::Interface(qn, args, bindings) => NormalTy::Interface(
                qn.clone(),
                args.iter()
                    .map(|a| a.replace_rec_var(depth, replacement))
                    .collect(),
                bindings
                    .iter()
                    .map(|(n, t)| (n.clone(), t.replace_rec_var(depth, replacement)))
                    .collect(),
            ),
            NormalTy::List(inner) => {
                NormalTy::List(Box::new(inner.replace_rec_var(depth, replacement)))
            }
            NormalTy::Map { key, value } => NormalTy::Map {
                key: Box::new(key.replace_rec_var(depth, replacement)),
                value: Box::new(value.replace_rec_var(depth, replacement)),
            },
            NormalTy::Union(members) => NormalTy::Union(
                members
                    .iter()
                    .map(|m| m.replace_rec_var(depth, replacement))
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
                        ty: p.ty.replace_rec_var(depth, replacement),
                        mode: p.mode,
                    })
                    .collect(),
                ret: Box::new(ret.replace_rec_var(depth, replacement)),
                throws: Box::new(throws.replace_rec_var(depth, replacement)),
            },
            NormalTy::Future(value, error) => NormalTy::Future(
                Box::new(value.replace_rec_var(depth, replacement)),
                Box::new(error.replace_rec_var(depth, replacement)),
            ),
            NormalTy::AssociatedTypeProjection {
                base,
                interface,
                member,
            } => NormalTy::AssociatedTypeProjection {
                base: Box::new(base.replace_rec_var(depth, replacement)),
                interface: Box::new(interface.replace_rec_var(depth, replacement)),
                member: member.clone(),
            },
            // Leaves and non-matching indices are untouched.
            _ => self.clone(),
        }
    }
}

impl<H: Head, P: MuPhase<H>> NormalParam<H, P> {
    fn is_required(&self) -> bool {
        matches!(self.mode, FunctionParamMode::Required)
    }
}

impl<H: Head> NormalParam<H> {
    /// Function parameter-list subtyping (contravariant): required params
    /// positional and matched in order, optional params matched by name.
    fn list_subtype<C: TypeContext<H>>(
        sub: &[NormalParam<H>],
        sup: &[NormalParam<H>],
        ctx: &C,
        assumptions: &mut HashSet<(NormalTy<H>, NormalTy<H>)>,
    ) -> bool {
        let sub_required: Vec<&NormalParam<H>> = sub.iter().filter(|p| p.is_required()).collect();
        let sup_required: Vec<&NormalParam<H>> = sup.iter().filter(|p| p.is_required()).collect();
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

mod mu;

#[cfg(test)]
mod tests;

// ═══════════════════════════════════════════════════════════════════════════
// INTERNED ENTRY
// ═══════════════════════════════════════════════════════════════════════════
//
// The hir_ty inference engine's ingestion path: interned types enter the
// normalizer directly, at the same cost the plain enum pays via `from_ty`,
// with no intermediate materialization. Facts still exchange plain types at
// the `TypeContext` boundary (alias definitions, projection reductions) -
// those are small and rare, and reduction results continue through the plain
// path. Output-producing entries materialize once via `into_ty` and
// re-intern.

use crate::interned;

/// Memoizes canonical forms for a sequence of interned relations evaluated
/// under one immutable [`TypeContext`]. The cache deliberately takes the
/// context at each call rather than owning it; callers are responsible for
/// keeping one cache scoped to one fact set (compiler inference uses one per
/// body). Inference-bearing types should be resolved before entering because
/// their meaning is table-relative rather than a property of the handle alone.
#[derive(Default)]
pub struct InternedCanonicalCache {
    canonical: RefCell<HashMap<interned::Ty, NormalTy>>,
}

impl InternedCanonicalCache {
    fn canonical<C: TypeContext>(&self, ty: &interned::Ty, ctx: &C) -> NormalTy {
        debug_assert!(!ty.has_infer());
        if let Some(canonical) = self.canonical.borrow().get(ty) {
            return canonical.clone();
        }
        let canonical = NormalTy::canonical_interned(ty, ctx);
        self.canonical
            .borrow_mut()
            .insert(ty.clone(), canonical.clone());
        canonical
    }

    pub fn equivalent<C: TypeContext>(&self, a: &interned::Ty, b: &interned::Ty, ctx: &C) -> bool {
        if a == b {
            return true;
        }
        if interned_heads_definitely_differ(a, b) {
            return false;
        }
        self.canonical(a, ctx) == self.canonical(b, ctx)
    }

    pub fn is_subtype<C: TypeContext>(
        &self,
        sub: &interned::Ty,
        sup: &interned::Ty,
        ctx: &C,
    ) -> bool {
        if sub == sup {
            return true;
        }
        if !matches!(
            (sub.kind(), sup.kind()),
            (
                interned::TyKind::Interface(..),
                interned::TyKind::Interface(..)
            )
        ) && interned_heads_definitely_differ(sub, sup)
        {
            return false;
        }
        self.canonical(sub, ctx)
            .is_subtype_of(&self.canonical(sup, ctx), ctx, &mut HashSet::new())
    }
}

impl NormalTy {
    /// [`NormalTy::canonical_bottom_up`] for the interned representation: the
    /// same named-intermediate -> binder-resolution -> bottom-up-algebra
    /// pipeline, entered from `interned::Ty`.
    fn canonical_bottom_up_interned<C: TypeContext>(
        ty: &interned::Ty,
        ctx: &C,
    ) -> (NormalTy, bool) {
        let named =
            NormalTy::from_interned(ty, ctx, &mut HashSet::new(), PROJECTION_REDUCTION_FUEL);
        let mut saw_mu = false;
        let resolved = named.resolve_binders(&mut Vec::new(), &mut saw_mu);
        (
            resolved.canonicalize(ctx, saw_mu, &mut HashSet::new()),
            saw_mu,
        )
    }

    /// [`NormalTy::canonical`] for the interned representation.
    fn canonical_interned<C: TypeContext>(ty: &interned::Ty, ctx: &C) -> NormalTy {
        let (t, saw_mu) = Self::canonical_bottom_up_interned(ty, ctx);
        if saw_mu && t.contains_mu() {
            mu::canonicalize_mu(t, ctx)
        } else {
            t
        }
    }
}

/// The interned representation is name-headed by construction
/// (`interned::TyKind::Class(TypeName, ..)`), so this whole ingestion path is
/// fixed at `H = QualifiedTypeName`; only the μ-phase varies.
impl NormalTy<QualifiedTypeName, Named> {
    /// [`NormalTy::from_ty`], mirrored over `interned::TyKind`. The one
    /// naming trap: the interned `Unknown` is the TOP type (the plain enum's
    /// `BuiltinUnknown`); TIR's `Unknown` recovery sentinel is
    /// unrepresentable in the interned form by design.
    fn from_interned<C: TypeContext>(
        ty: &interned::Ty,
        ctx: &C,
        expanding: &mut HashSet<QualifiedTypeName>,
        fuel: u32,
    ) -> NormalTy<QualifiedTypeName, Named> {
        use interned::TyKind as K;
        match ty.kind() {
            K::Int { .. } => NormalTy::Int,
            K::Bigint { .. } => NormalTy::Bigint,
            K::Float { .. } => NormalTy::Float,
            K::String { .. } => NormalTy::String,
            K::Bool { .. } => NormalTy::Bool,
            K::Null { .. } => NormalTy::Null,
            K::Uint8Array { .. } => NormalTy::Uint8Array,
            K::Media(kind, _) => NormalTy::Media(*kind),
            K::Void { .. } => NormalTy::Void,
            K::RustType { .. } => NormalTy::RustType,
            K::Type { .. } => NormalTy::Type,
            K::Resource { .. } => NormalTy::Resource,
            K::PromptAst { .. } => NormalTy::PromptAst,
            K::Unknown { .. } => NormalTy::BuiltinUnknown,
            K::Never { .. } => NormalTy::Never,
            K::Error { .. } => NormalTy::Error,
            // Same invariant as the plain arm: holes are filled (or made
            // `Error`) and live variables are resolved or deferred BEFORE any
            // oracle query; either reaching normalization is a compiler bug.
            K::Infer { .. } => unreachable!(
                "inference hole/variable reached type normalization; holes must \
                 be instantiated and variables resolved (or the check deferred) \
                 before any equivalence/subtype query"
            ),
            K::Literal(lit, _freshness, _) => NormalTy::Literal(lit.clone()),
            K::Class(qn, args, _) => NormalTy::Class(
                qn.clone(),
                Self::from_interned_all(args, ctx, expanding, fuel),
            ),
            K::Interface(qn, args, bindings, _) => {
                let mut bindings: Vec<_> = bindings
                    .iter()
                    .map(|(name, ty)| (name.clone(), Self::from_interned(ty, ctx, expanding, fuel)))
                    .collect();
                bindings.sort_by(|(a, _), (b, _)| a.cmp(b));
                NormalTy::Interface(
                    qn.clone(),
                    Self::from_interned_all(args, ctx, expanding, fuel),
                    bindings,
                )
            }
            K::Enum(qn, _) => NormalTy::Enum(qn.clone()),
            K::EnumVariant(qn, variant, _) => NormalTy::EnumVariant(qn.clone(), variant.clone()),
            K::List(inner, _) => {
                NormalTy::List(Box::new(Self::from_interned(inner, ctx, expanding, fuel)))
            }
            K::Map { key, value, .. } => NormalTy::Map {
                key: Box::new(Self::from_interned(key, ctx, expanding, fuel)),
                value: Box::new(Self::from_interned(value, ctx, expanding, fuel)),
            },
            K::Union(members, _) => {
                NormalTy::Union(Self::from_interned_all(members, ctx, expanding, fuel))
            }
            K::Function {
                params,
                ret,
                throws,
                ..
            } => NormalTy::Function {
                params: params
                    .iter()
                    .map(|p| NormalParam {
                        name: p.name.clone(),
                        ty: Self::from_interned(&p.ty, ctx, expanding, fuel),
                        mode: p.mode,
                    })
                    .collect(),
                ret: Box::new(Self::from_interned(ret, ctx, expanding, fuel)),
                throws: Box::new(Self::from_interned(throws, ctx, expanding, fuel)),
            },
            K::Future(value, error, _) => NormalTy::Future(
                Box::new(Self::from_interned(value, ctx, expanding, fuel)),
                Box::new(Self::from_interned(error, ctx, expanding, fuel)),
            ),
            K::TypeVar(param, _) => NormalTy::TypeVar(param.clone()),
            K::AssociatedTypeProjection {
                base,
                interface,
                member,
                ..
            } => {
                // The fact boundary exchanges plain types; a projection's
                // pieces are small. A reduction continues through the plain
                // path, exactly like the plain arm.
                let plain_base = base.to_plain();
                let plain_interface = crate::Interface::new(
                    interface.name.clone(),
                    interface
                        .generics
                        .iter()
                        .map(interned::Ty::to_plain)
                        .collect(),
                    interface
                        .associated_types
                        .iter()
                        .map(|(name, ty)| (name.clone(), ty.to_plain()))
                        .collect(),
                );
                if fuel > 0
                    && let ProjectionStep::Reduced(reduced) =
                        ctx.project(&plain_base, &plain_interface, member, fuel)
                {
                    return Self::from_ty(&reduced, ctx, expanding, fuel - 1);
                }
                let mut bindings: Vec<_> = interface
                    .associated_types
                    .iter()
                    .map(|(name, ty)| (name.clone(), Self::from_interned(ty, ctx, expanding, fuel)))
                    .collect();
                bindings.sort_by(|(a, _), (b, _)| a.cmp(b));
                NormalTy::AssociatedTypeProjection {
                    base: Box::new(Self::from_interned(base, ctx, expanding, fuel)),
                    interface: Box::new(NormalTy::Interface(
                        interface.name.clone(),
                        Self::from_interned_all(&interface.generics, ctx, expanding, fuel),
                        bindings,
                    )),
                    member: member.clone(),
                }
            }
            K::TypeAlias(qn, _) => {
                if expanding.contains(qn) {
                    return NormalTy::RecVar(qn.clone());
                }
                let Some(def) = ctx.alias_def(qn) else {
                    return NormalTy::OpaqueAlias(qn.clone());
                };
                expanding.insert(qn.clone());
                // The alias definition is a plain fact; its expansion
                // continues through the plain path.
                let body = Self::from_ty(&def, ctx, expanding, fuel);
                expanding.remove(qn);
                if body.mentions_rec_var(qn) {
                    NormalTy::Mu {
                        binder: qn.clone(),
                        body: Box::new(body),
                    }
                } else {
                    body
                }
            }
        }
    }

    fn from_interned_all<C: TypeContext>(
        tys: &[interned::Ty],
        ctx: &C,
        expanding: &mut HashSet<QualifiedTypeName>,
        fuel: u32,
    ) -> Vec<NormalTy<QualifiedTypeName, Named>> {
        tys.iter()
            .map(|ty| Self::from_interned(ty, ctx, expanding, fuel))
            .collect()
    }
}

/// [`TypeContext::is_subtype`] for interned types: the subset relation,
/// entered without materializing plain trees. Pointer identity is the
/// reflexivity fast path.
pub fn is_subtype_interned<C: TypeContext>(
    sub: &interned::Ty,
    sup: &interned::Ty,
    ctx: &C,
) -> bool {
    if sub == sup {
        return true;
    }
    if !matches!(
        (sub.kind(), sup.kind()),
        (
            interned::TyKind::Interface(..),
            interned::TyKind::Interface(..)
        )
    ) && interned_heads_definitely_differ(sub, sup)
    {
        return false;
    }
    let sub = NormalTy::canonical_interned(sub, ctx);
    let sup = NormalTy::canonical_interned(sup, ctx);
    sub.is_subtype_of(&sup, ctx, &mut HashSet::new())
}

/// [`TypeContext::equivalent`] for interned types.
pub fn equivalent_interned<C: TypeContext>(a: &interned::Ty, b: &interned::Ty, ctx: &C) -> bool {
    if a == b {
        return true;
    }
    if interned_heads_definitely_differ(a, b) {
        return false;
    }
    NormalTy::canonical_interned(a, ctx) == NormalTy::canonical_interned(b, ctx)
}

/// Interned counterpart of [`heads_definitely_differ`]. Hash-consing makes
/// the identical-head fast path cheaper, but distinct nominal heads are still
/// common during inference and cannot be changed by canonicalization.
fn interned_heads_definitely_differ(a: &interned::Ty, b: &interned::Ty) -> bool {
    use interned::TyKind as K;
    match (a.kind(), b.kind()) {
        (K::Class(q1, ..), K::Class(q2, ..))
        | (K::Interface(q1, ..), K::Interface(q2, ..))
        | (K::Enum(q1, ..), K::Enum(q2, ..)) => q1 != q2,
        (K::EnumVariant(q1, v1, ..), K::EnumVariant(q2, v2, ..)) => q1 != q2 || v1 != v2,
        _ => false,
    }
}

/// [`TypeContext::normalize`] for interned types. Materializes once on the
/// way out (attrs erased, like the plain form), with the mu root rendered
/// exactly as `NormalTy::canonical_render` renders it (root-unfold-once).
pub fn normalize_interned<C: TypeContext>(ty: &interned::Ty, ctx: &C) -> interned::Ty {
    let (t, saw_mu) = NormalTy::canonical_bottom_up_interned(ty, ctx);
    let plain = if saw_mu && t.contains_mu() {
        mu::canonicalize_mu_with_render(t, ctx).1
    } else {
        t.into_ty()
    };
    interned::Ty::from_plain(&plain)
}

/// The canonical union of `members` - the join operation for control-flow
/// merge points and throws accumulation: flattens, dedups, absorbs subsumed
/// members (`1 | int` collapses to `int`), and collapses complete sets
/// (`true | false` to `bool`). An empty member list is `never` (the join
/// identity).
pub fn canonical_union_interned<C: TypeContext>(members: &[interned::Ty], ctx: &C) -> interned::Ty {
    let named = NormalTy::Union(
        members
            .iter()
            .map(|member| {
                NormalTy::from_interned(member, ctx, &mut HashSet::new(), PROJECTION_REDUCTION_FUEL)
            })
            .collect(),
    );
    let mut saw_mu = false;
    let resolved = named.resolve_binders(&mut Vec::new(), &mut saw_mu);
    let t = resolved.canonicalize(ctx, saw_mu, &mut HashSet::new());
    let t = if saw_mu && t.contains_mu() {
        mu::canonicalize_mu(t, ctx)
    } else {
        t
    };
    interned::Ty::from_plain(&t.into_ty())
}
