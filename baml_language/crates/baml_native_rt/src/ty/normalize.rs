// Copied from crates/baml_type/src/normalize.rs: equivalence, subtyping and normal form,
// of realized types only. A change to either file must be made to both.

//! Canonical type normalization, equivalence, and subtyping for [`RealizedTy`].
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
//! Everything operates on [`RealizedTy`]: the runtime's types have no type
//! variables, no unreduced projections and no error sentinel, so the arms the
//! original has for those are not here. A head is the address of the object
//! it names, and is compared, ordered and hashed by that address: only within
//! one call, during which nothing moves, and never kept past it.
//!
//! # Context
//!
//! The nominal facts the algebra needs — whether a
//! concrete type implements an interface, an interface's required interfaces,
//! and an enum's full variant set — are supplied by a
//! [`TypeContext`] the caller implements over its own registries. Following the
//! golden rule (a value never violates its static type), **every lookup fails
//! safe**: when the context cannot answer, the algebra declines to collapse,
//! absorb, or equate, so a missing fact can only yield "not *necessarily*
//! equivalent / subtype", never a false claim of equivalence or membership.

use std::{collections::HashSet, ptr::NonNull};

use super::{
    AssocBinding, AssocIndex, ClassObject, CowBox, CowSlice, EnumVariantObject, FunctionParamMode,
    FunctionParamTy, InterfaceBound, InterfaceObject, Literal, MediaKind, RealizedTy, Text,
    TypeObject,
};
use crate::layout::VARIANT_ENUM;

/// The associated types of `reflect.AnyFunction`, by their position in its
/// declaration: where the original finds the pins by their names.
const ANY_FUNCTION_RETURNS: AssocIndex = AssocIndex(0);
const ANY_FUNCTION_THROWS: AssocIndex = AssocIndex(1);

/// The declarations the algebra special-cases by identity. Each is a
/// language-fixed item of the `reflect` package; a context resolves it to
/// whatever head it represents that declaration by
/// ([`TypeContext::well_known`]), so the algebra never spells one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WellKnownHead {
    /// `reflect.AnyFunction`, whose associated-type pins are covariant unlike
    /// every other interface's.
    AnyFunction,
    /// `reflect.AnyClass`, the compiler-derived interface every class value
    /// inhabits.
    AnyClass,
}

/// Whether `head` is the [`WellKnownHead::AnyFunction`] declaration, decided
/// by identity against the head `ctx` uses for it rather than by inspecting
/// `head` itself.
///
/// An unknown declaration is not `AnyFunction` as far as this can tell, so the
/// covariance special case does not fire — conservative, per the context's
/// fail-safe contract.
fn is_any_function<C: TypeContext>(head: &NonNull<InterfaceObject>, ctx: &C) -> bool {
    ctx.well_known(WellKnownHead::AnyFunction)
        .is_some_and(|any_function| *head == any_function)
}

/// [`is_any_function`] for [`WellKnownHead::AnyClass`].
fn is_any_class<C: TypeContext>(head: &NonNull<InterfaceObject>, ctx: &C) -> bool {
    ctx.well_known(WellKnownHead::AnyClass)
        .is_some_and(|any_class| *head == any_class)
}

// ═══════════════════════════════════════════════════════════════════════════
// CONTEXT
// ═══════════════════════════════════════════════════════════════════════════

/// Starting fuel for projection reduction: the maximum length of a reduction
/// chain (`(A as I).X` → `(B as J).Y` → …) before a projection is left opaque (the
/// canonical algebra) or its realization fails (the runtime). Generous for any
/// real program; bounds a cyclic `type A = (C as I).B` / `type B = (C as J).A`
/// (itself a declaration-level error caught elsewhere) so the runtime
/// `TyTemplate::substitute` reduction terminates instead of recursing forever.
pub(crate) const PROJECTION_REDUCTION_FUEL: u32 = 256;

/// The result of reducing an associated-type projection `(base as I).member`
/// through [`TypeContext::project`].
pub enum ProjectionStep {
    /// The projection *is* this type — the impl's binding or the qualifier's pin.
    /// `(int as Foo).Assoc` with `impl Foo for int { type Assoc = string }` reduces
    /// to `string`; the projection is a pure, side-effect-free type-level operator,
    /// so its canonical form is the reduced type (assignable from / equal to it).
    Reduced(RealizedTy),
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
    /// The head this context represents the well-known declaration `head` with.
    ///
    /// The algebra never inspects a head's spelling — heads are opaque values it
    /// threads through — so recognizing a *particular* declaration is done by
    /// obtaining its head here and comparing with `==`. That indirection is what
    /// lets the question be answered by every representation: the wire's
    /// name-based context spells the name, the compiler's context knows which
    /// root is `reflect`, and a runtime context resolves the declaration on its
    /// heap and hands back a handle — state only the context has.
    ///
    /// `None` means the declaration could not be resolved *at all* (the
    /// package is not installed, say), and fails safe in the usual way: the
    /// special case does not fire, which is conservative.
    fn well_known(&self, head: WellKnownHead) -> Option<NonNull<InterfaceObject>>;

    /// Whether the non-interface, non-type-variable `concrete` type implements
    /// `interface`, accounting for the interface's generic
    /// arguments, associated-type bindings, and the impl's bounds.
    ///
    /// Powers `C <: I` subtyping and the `C | I == I` union absorption (a
    /// concrete member subsumed by an existential member). `false` ⇒ no
    /// membership is claimed.
    fn implements_interface(
        &self,
        concrete: &RealizedTy,
        interface: &InterfaceBound<RealizedTy>,
    ) -> bool;

    /// Whether interface `sub` *properly* (transitively, not reflexively)
    /// requires interface `sup`, accounting for generic arguments.
    ///
    /// Powers `A <: B` subtyping and the `A | B == B` absorption for
    /// existentials. `false` ⇒ no requirement is claimed. Implementations need
    /// not report same-name reflexivity — the normalizer handles structural
    /// equality before consulting this, so a same-name query only arises for
    /// distinct instantiations, which are not requirements.
    fn interface_requires(
        &self,
        sub: &InterfaceBound<RealizedTy>,
        sup: &InterfaceBound<RealizedTy>,
    ) -> bool;

    /// The complete set of variants of an enum, or `None` if the enum is
    /// unknown.
    ///
    /// Powers the completeness collapse `E.A | E.B | … == E` (a union of *all* of
    /// an enum's variants is the enum itself). `None` ⇒ no collapse.
    fn enum_variants(&self, name: &NonNull<ClassObject>)
    -> Option<Vec<NonNull<EnumVariantObject>>>;

    /// Reduce an associated-type projection `(base as interface).member` to the type
    /// it denotes, when determinable — the pure type-level operator that makes
    /// `(int as Foo).Assoc` *be* `string` (the impl's binding), analogous to how
    /// `1 + 1` *is* `2`. [`ProjectionStep::Opaque`] when the base is still symbolic
    /// or no impl determines it, leaving the projection a leaf.
    ///
    /// **Required (no default).** A silently-`Opaque` default would let a context
    /// that *should* reduce projections forget to — leaving `(int as Foo).Assoc` a
    /// dead symbolic type with no error, a silent soundness hole.
    ///
    /// `fuel` is the remaining projection-reduction budget. A context that itself
    /// drives the reduction recursion — the runtime, whose `project` realizes the
    /// impl binding through `TyTemplate::substitute` and can re-enter `project` —
    /// threads it on so a cyclic associated-type binding terminates. A single-step
    /// reducer whose recursion is instead bounded by its caller (the canonical
    /// `from_ty` walk, which decrements its own fuel) ignores it.
    fn project(
        &self,
        base: &RealizedTy,
        interface: &InterfaceBound<RealizedTy>,
        member: AssocIndex,
        fuel: u32,
    ) -> ProjectionStep;

    // ── type algebra (defaulted; the canonical implementation) ──────────────
    //
    // A context computes the set-theoretic relations over its *own* facts, so
    // `ctx.is_subtype(a, b)` reads as "does this context prove `a <: b`". These
    // methods carry the logic. Each body passes `self` as the context, hence
    // `where Self: Sized`: there are no `dyn TypeContext` callers, so the bound
    // costs nothing. Do not override them — a context supplies only the facts.

    /// Normalize `ty` to its canonical form and render it back as a [`RealizedTy`].
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
    fn normalize(&self, ty: &RealizedTy) -> RealizedTy
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
    /// subtyping are deliberate: the error-recovery sentinel (`Error` is
    /// bidirectionally *compatible* with everything but equivalent only to
    /// itself), and fact sets with a mutual `requires` cycle (mutual
    /// subtypes as existentials, nominally distinct — rejected in well-formed
    /// programs by the interface `requires`-cycle check).
    fn equivalent(&self, a: &RealizedTy, b: &RealizedTy) -> bool
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
    fn is_subtype(&self, sub: &RealizedTy, sup: &RealizedTy) -> bool
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
        // Definite-mismatch filter. Unlike `equivalent`, differing heads do NOT
        // generally refute subtyping (a literal is a subtype of its base, a
        // member of its union, a class of an interface it implements) — but
        // `heads_definitely_differ` claims none of those pairs: a literal and
        // its base share a category, and unions and interfaces have none. Of
        // the same-variant nominal pairs it does claim, only
        // interface-to-interface can still be a subtype under different names
        // (`A <: B` iff `A` requires `B`), so it is excluded. For the rest the
        // canonical walk below is a foregone `false`: class heads are preserved
        // by canonicalization and the class subtype rule requires equal names
        // (generic args are invariant), enums and enum variants are nominal
        // with no cross-name rule at all, and no rule relates two categories.
        if !matches!(
            (sub, sup),
            (RealizedTy::Interface(..), RealizedTy::Interface(..))
        ) && heads_definitely_differ(sub, sup)
        {
            return false;
        }
        let sub = NormalTy::canonical(sub, self);
        let sup = NormalTy::canonical(sup, self);
        sub.is_subtype_of(&sup, self, &mut HashSet::new())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PUBLIC API
// ═══════════════════════════════════════════════════════════════════════════

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
/// Across kinds it decides only pairs whose heads both have a [`Category`]
/// (see [`Category::of_head`]), never with a "different discriminant ⇒ differ"
/// catch-all. That catch-all would be unsound: `TypeAlias` / `Union` /
/// `AssociatedTypeProjection` heads can be rewritten into any other shape, so
/// they have no category and stay undecided (`false`). Canonicalization keeps
/// every head that has a category, so two different categories never share a
/// canonical form — and no subtyping rule relates them either: no concrete type
/// is a subtype of another (TYPE_SYSTEM.md §Concrete Types), and a literal and
/// its base, or a variant and its enum, share a category. This is the common
/// miss of a runtime `match` on a `json` value: a string or a list tested
/// against `map<string, json>` would otherwise expand the recursive alias on
/// every test. Context-independent: canonicalization preserves these heads
/// regardless of the `TypeContext`.
fn heads_definitely_differ(a: &RealizedTy, b: &RealizedTy) -> bool {
    match (a, b) {
        (RealizedTy::Class(q1, ..), RealizedTy::Class(q2, ..))
        | (RealizedTy::Enum(q1, ..), RealizedTy::Enum(q2, ..)) => q1 != q2,
        (RealizedTy::Interface(q1, ..), RealizedTy::Interface(q2, ..)) => q1 != q2,
        // A variant's object is the variant, its enum included.
        (RealizedTy::EnumVariant(v1), RealizedTy::EnumVariant(v2)) => v1 != v2,
        _ => categories_differ(Category::of_head(a), Category::of_head(b)),
    }
}

/// Whether two heads are of different, known categories.
fn categories_differ(a: Option<Category>, b: Option<Category>) -> bool {
    matches!((a, b), (Some(a), Some(b)) if a != b)
}

impl NormalTy {
    /// Normalize and canonicalize a [`RealizedTy`] in one step (the shared entry point):
    /// build the named intermediate, strictly resolve its binders to the
    /// canonical de Bruijn phase, run the bottom-up set-theoretic algebra, and —
    /// only when recursion is present — the μ-canonicalization automaton
    /// ([`mu`]), which makes the result a unique representative of the
    /// equirecursive equivalence class.
    fn canonical<C: TypeContext>(ty: &RealizedTy, ctx: &C) -> NormalTy {
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
    fn canonical_with<C: TypeContext>(
        ty: &RealizedTy,
        ctx: &C,
        assumptions: &mut HashSet<(NormalTy, NormalTy)>,
    ) -> NormalTy {
        let (t, saw_mu) = Self::canonical_bottom_up(ty, ctx, assumptions);
        if saw_mu && t.contains_mu() {
            mu::canonicalize_mu(t, ctx)
        } else {
            t
        }
    }

    /// [`Self::canonical`] rendered back as a [`RealizedTy`] - the body of
    /// [`TypeContext::normalize`]. On the mu path the automaton renders the
    /// root directly (root-unfold-once: a recursive alias exposes its head
    /// constructor; nested recursion stays folded as alias names), so
    /// `normalize` never calls [`Self::into_ty`] on a mu root.
    fn canonical_render<C: TypeContext>(ty: &RealizedTy, ctx: &C) -> RealizedTy {
        let (t, saw_mu) = Self::canonical_bottom_up(ty, ctx, &mut HashSet::new());
        if saw_mu && t.contains_mu() {
            mu::canonicalize_mu_with_render(t, ctx).1
        } else {
            t.into_ty()
        }
    }

    /// The shared pre-automaton pipeline: named intermediate -> strict binder
    /// resolution -> bottom-up algebra (with open-member absorption deferred).
    fn canonical_bottom_up<C: TypeContext>(
        ty: &RealizedTy,
        ctx: &C,
        assumptions: &mut HashSet<(NormalTy, NormalTy)>,
    ) -> (NormalTy, bool) {
        let named = NormalTy::from_ty(ty, &mut HashSet::new());
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
            NormalTy::Union(members) | NormalTy::Tuple(members) => {
                members.iter().any(NormalTy::contains_mu)
            }
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
            NormalTy::Int
            | NormalTy::Bigint
            | NormalTy::Float
            | NormalTy::String
            | NormalTy::Bool
            | NormalTy::Null
            | NormalTy::Uint8Array
            | NormalTy::Media(_)
            | NormalTy::RustType
            | NormalTy::Type
            | NormalTy::Resource
            | NormalTy::PromptAst
            | NormalTy::Literal(_)
            | NormalTy::Enum(_)
            | NormalTy::EnumVariant(_, _)
            | NormalTy::Never
            | NormalTy::Unknown => false,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CONCRETE-TYPE DISJOINTNESS
// ═══════════════════════════════════════════════════════════════════════════

/// Top-level concrete category of a ground-headed type, for the cross-category
/// checks: no subtyping rule or canonical form
/// relates two categories ([`heads_definitely_differ`]). Same-category pairs
/// are decided by the structural arms, not by this.
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
    RustType,
    Type,
    Resource,
    PromptAst,
    Class,
    List,
    Map,
    Tuple,
    Enum,
    Function,
    Future,
}

/// The one table behind [`Category::of_head`]. `$member` is the enum's path and
/// `$none` adds the member's own variants that have no category. Exhaustive on
/// purpose: a new variant must be classified here.
macro_rules! family_head_category {
    ($ty:expr, $($member:ident)::+ $(, $none:pat)*) => {
        Some(match $ty {
            $($member)::+::Int => Category::Int,
            $($member)::+::Bigint => Category::Bigint,
            $($member)::+::Float => Category::Float,
            $($member)::+::String => Category::String,
            $($member)::+::Bool => Category::Bool,
            $($member)::+::Literal(lit, _) => Category::of_literal(lit),
            $($member)::+::Null => Category::Null,
            $($member)::+::Uint8Array => Category::Uint8Array,
            $($member)::+::Media(kind) => Category::Media(*kind),
            $($member)::+::RustType => Category::RustType,
            $($member)::+::Type => Category::Type,
            $($member)::+::Resource => Category::Resource,
            $($member)::+::PromptAst => Category::PromptAst,
            $($member)::+::Class(..) => Category::Class,
            $($member)::+::List(_) => Category::List,
            $($member)::+::Map { .. } => Category::Map,
            $($member)::+::Enum(_) | $($member)::+::EnumVariant(..) => Category::Enum,
            $($member)::+::Function { .. } => Category::Function,
            $($member)::+::Future(..) => Category::Future,
            $($member)::+::Interface(..)
            | $($member)::+::Tuple(_)
            | $($member)::+::Union(_)
            | $($member)::+::TypeAlias(_)
            | $($member)::+::Unknown
            | $($member)::+::Never
            $(| $none)* => return None,
        })
    };
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

    /// The category of a head that canonicalization keeps, read before
    /// canonicalizing. `None`
    /// for a head canonicalization can rewrite into another kind (an alias, a
    /// union), one a context-driven rule relates across kinds
    /// (an interface), and the sentinels (`unknown`, `never`).
    fn of_head(ty: &RealizedTy) -> Option<Category> {
        family_head_category!(ty, RealizedTy)
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
trait MuPhase {
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

impl MuPhase for Named {
    type Binder = NonNull<TypeObject>;
    type Var = NonNull<TypeObject>;
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

impl MuPhase for Canonical {
    type Binder = MuDisplay;
    type Var = u32;
}

/// Normalized structural type: aliases resolved, attributes and literal
/// freshness erased, recursion made explicit with μ-binders (representation per
/// phase `P` — see [`MuPhase`]). The default phase is [`Canonical`]: bare
/// `NormalTy` throughout the algebra means the canonical form, and only the
/// short-lived `from_ty` intermediate spells its phase (`NormalTy<Named>`).
///
/// Ordering (`PartialOrd`/`Ord`) is the canonical sort key for union members; it
/// has no semantic meaning beyond producing a deterministic canonical form.
/// Heads order by address, which holds within one call, when nothing moves.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum NormalTy<P: MuPhase = Canonical> {
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
    RustType,
    Type,
    Resource,
    PromptAst,
    // Literal — a single value as a type. Freshness is erased.
    Literal(Literal),
    // Nominal references
    Class(NonNull<ClassObject>, Vec<NormalTy<P>>),
    Interface(
        NonNull<InterfaceObject>,
        Vec<NormalTy<P>>,
        Vec<(AssocIndex, NormalTy<P>)>,
    ),
    Enum(NonNull<ClassObject>),
    /// The variant's enum, read from its object, and the variant.
    EnumVariant(NonNull<ClassObject>, NonNull<EnumVariantObject>),
    // Constructors
    List(Box<NormalTy<P>>),
    Map {
        key: Box<NormalTy<P>>,
        value: Box<NormalTy<P>>,
    },
    Union(Vec<NormalTy<P>>),
    Tuple(Vec<NormalTy<P>>),
    Function {
        params: Vec<NormalParam<P>>,
        ret: Box<NormalTy<P>>,
        throws: Box<NormalTy<P>>,
    },
    Future(Box<NormalTy<P>>, Box<NormalTy<P>>),
    // Recursion. `Named`: binder/variable carry the alias name whose expansion
    // introduced them. `Canonical`: the binder carries its equality-transparent
    // display payload and the variable a de Bruijn index (0 = innermost
    // enclosing binder) — see [`Canonical`] for the closed-term invariant.
    Mu {
        binder: P::Binder,
        body: Box<NormalTy<P>>,
    },
    /// μ-bound recursion variable (a back-reference to an enclosing [`NormalTy::Mu`]).
    RecVar(P::Var),
    // Special forms
    /// Bottom — a subtype of every type.
    Never,
    /// The explicit `unknown` keyword — top, a supertype of every type.
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct NormalParam<P: MuPhase = Canonical> {
    name: Option<Text>,
    ty: NormalTy<P>,
    mode: FunctionParamMode,
}

/// Display payload of a canonical μ-binder ([`NormalTy::Mu`] at [`Canonical`]).
///
/// `rendered` is the whole μ-subterm as a [`RealizedTy`] — surface syntax has no binder,
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
struct MuDisplay {
    name: Option<NonNull<TypeObject>>,
    rendered: Box<RealizedTy>,
}

impl PartialEq for MuDisplay {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Eq for MuDisplay {}

impl PartialOrd for MuDisplay {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MuDisplay {
    fn cmp(&self, _other: &Self) -> std::cmp::Ordering {
        std::cmp::Ordering::Equal
    }
}

impl std::hash::Hash for MuDisplay {
    fn hash<S: std::hash::Hasher>(&self, _state: &mut S) {}
}

impl NormalTy<Named> {
    // ── conversion in: RealizedTy → NormalTy<Named> ────────────────────────

    fn from_ty(ty: &RealizedTy, expanding: &mut HashSet<NonNull<TypeObject>>) -> NormalTy<Named> {
        match ty {
            RealizedTy::Int => NormalTy::Int,
            RealizedTy::Bigint => NormalTy::Bigint,
            RealizedTy::Float => NormalTy::Float,
            RealizedTy::String => NormalTy::String,
            RealizedTy::Bool => NormalTy::Bool,
            RealizedTy::Null => NormalTy::Null,
            RealizedTy::Uint8Array => NormalTy::Uint8Array,
            RealizedTy::Media(kind) => NormalTy::Media(*kind),
            RealizedTy::RustType => NormalTy::RustType,
            RealizedTy::Type => NormalTy::Type,
            RealizedTy::Resource => NormalTy::Resource,
            RealizedTy::PromptAst => NormalTy::PromptAst,
            RealizedTy::Unknown => NormalTy::Unknown,
            RealizedTy::Never => NormalTy::Never,
            // Freshness is a compiler-only widening flag, irrelevant to type identity.
            RealizedTy::Literal(lit, _freshness) => NormalTy::Literal(lit.clone()),
            RealizedTy::Class(qn, args) => NormalTy::Class(*qn, Self::from_tys(args, expanding)),
            RealizedTy::Interface(qn, args, bindings) => {
                let mut bindings: Vec<_> = bindings
                    .iter()
                    .map(|binding| (binding.member, Self::from_ty(&binding.ty, expanding)))
                    .collect();
                bindings.sort_by_key(|(member, _)| *member);
                NormalTy::Interface(*qn, Self::from_tys(args, expanding), bindings)
            }
            RealizedTy::Enum(qn) => NormalTy::Enum(*qn),
            RealizedTy::EnumVariant(v) => {
                // SAFETY: a variant object holds its enum's class object at
                // `VARIANT_ENUM`.
                let qn = unsafe {
                    *v.as_ptr()
                        .cast::<NonNull<ClassObject>>()
                        .byte_add(VARIANT_ENUM)
                };
                NormalTy::EnumVariant(qn, *v)
            }
            RealizedTy::List(inner) => NormalTy::List(Box::new(Self::from_ty(inner, expanding))),
            RealizedTy::Map { key, value, .. } => NormalTy::Map {
                key: Box::new(Self::from_ty(key, expanding)),
                value: Box::new(Self::from_ty(value, expanding)),
            },
            RealizedTy::Union(members) => NormalTy::Union(Self::from_tys(members, expanding)),
            RealizedTy::Tuple(elements) => NormalTy::Tuple(Self::from_tys(elements, expanding)),
            RealizedTy::Function {
                params,
                ret,
                throws,
                ..
            } => NormalTy::Function {
                params: params
                    .iter()
                    .map(|p| NormalParam {
                        name: p.name.clone(),
                        ty: Self::from_ty(&p.ty, expanding),
                        mode: p.mode,
                    })
                    .collect(),
                ret: Box::new(Self::from_ty(ret, expanding)),
                throws: Box::new(Self::from_ty(throws, expanding)),
            },
            RealizedTy::Future(value, error) => NormalTy::Future(
                Box::new(Self::from_ty(value, expanding)),
                Box::new(Self::from_ty(error, expanding)),
            ),
            RealizedTy::TypeAlias(qn) => {
                if expanding.contains(qn) {
                    // Back-edge: we are already expanding this alias, so this is
                    // the recursive occurrence. The enclosing expansion wraps the
                    // result in a μ-binder over `qn`.
                    return NormalTy::RecVar(*qn);
                }
                // SAFETY: an alias is the type object of what it stands for,
                // which the executable's data holds and never frees.
                let def = unsafe { &qn.as_ref().ty };
                expanding.insert(*qn);
                let body = Self::from_ty(def, expanding);
                expanding.remove(qn);
                if body.mentions_rec_var(qn) {
                    NormalTy::Mu {
                        binder: *qn,
                        body: Box::new(body),
                    }
                } else {
                    body
                }
            }
        }
    }

    fn from_tys(
        tys: &[RealizedTy],
        expanding: &mut HashSet<NonNull<TypeObject>>,
    ) -> Vec<NormalTy<Named>> {
        tys.iter().map(|t| Self::from_ty(t, expanding)).collect()
    }

    /// Whether this type contains a back-reference to μ-variable `var`.
    fn mentions_rec_var(&self, var: &NonNull<TypeObject>) -> bool {
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
            NormalTy::Union(members) | NormalTy::Tuple(members) => {
                members.iter().any(|m| m.mentions_rec_var(var))
            }
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
            NormalTy::Int
            | NormalTy::Bigint
            | NormalTy::Float
            | NormalTy::String
            | NormalTy::Bool
            | NormalTy::Null
            | NormalTy::Uint8Array
            | NormalTy::Media(_)
            | NormalTy::RustType
            | NormalTy::Type
            | NormalTy::Resource
            | NormalTy::PromptAst
            | NormalTy::Literal(_)
            | NormalTy::Enum(_)
            | NormalTy::EnumVariant(_, _)
            | NormalTy::Never
            | NormalTy::Unknown => false,
        }
    }

    /// Render a named-phase term as a [`RealizedTy`], with every binder dropped and every
    /// back-reference spelled as its alias name — the historical `normalize`
    /// output shape. This fills the *interim* [`MuDisplay::rendered`] payload in
    /// [`Self::resolve_binders`]: it is what fact-oracle calls
    /// (`implements_interface`, `interface_requires`) observe when bottom-up
    /// absorption compares a closed μ member, keeping their view identical to the
    /// pre-μ-canonicalization behavior. The automaton replaces it with the
    /// canonical named-cut rendering.
    fn legacy_render(&self) -> RealizedTy {
        match self {
            NormalTy::Int => RealizedTy::Int,
            NormalTy::Bigint => RealizedTy::Bigint,
            NormalTy::Float => RealizedTy::Float,
            NormalTy::String => RealizedTy::String,
            NormalTy::Bool => RealizedTy::Bool,
            NormalTy::Null => RealizedTy::Null,
            NormalTy::Uint8Array => RealizedTy::Uint8Array,
            NormalTy::Media(kind) => RealizedTy::Media(*kind),
            NormalTy::RustType => RealizedTy::RustType,
            NormalTy::Type => RealizedTy::Type,
            NormalTy::Resource => RealizedTy::Resource,
            NormalTy::PromptAst => RealizedTy::PromptAst,
            NormalTy::Unknown => RealizedTy::Unknown,
            NormalTy::Never => RealizedTy::Never,
            NormalTy::Literal(lit) => RealizedTy::Literal(lit.clone(), super::Freshness::Regular),
            NormalTy::Class(qn, args) => {
                RealizedTy::Class(*qn, args.iter().map(NormalTy::legacy_render).collect())
            }
            NormalTy::Interface(qn, args, bindings) => RealizedTy::Interface(
                *qn,
                args.iter().map(NormalTy::legacy_render).collect(),
                bindings
                    .iter()
                    .map(|(name, ty)| AssocBinding {
                        member: *name,
                        ty: ty.legacy_render(),
                    })
                    .collect(),
            ),
            NormalTy::Enum(qn) => RealizedTy::Enum(*qn),
            NormalTy::EnumVariant(_, v) => RealizedTy::EnumVariant(*v),
            NormalTy::List(inner) => RealizedTy::List(CowBox::new(inner.legacy_render())),
            NormalTy::Map { key, value } => RealizedTy::Map {
                key: CowBox::new(key.legacy_render()),
                value: CowBox::new(value.legacy_render()),
            },
            NormalTy::Union(members) => {
                RealizedTy::Union(members.iter().map(NormalTy::legacy_render).collect())
            }
            NormalTy::Tuple(elements) => {
                RealizedTy::Tuple(elements.iter().map(NormalTy::legacy_render).collect())
            }
            NormalTy::Function {
                params,
                ret,
                throws,
            } => RealizedTy::Function {
                params: params
                    .iter()
                    .map(|p| FunctionParamTy {
                        name: p.name.clone(),
                        ty: p.ty.legacy_render(),
                        mode: p.mode,
                    })
                    .collect(),
                ret: CowBox::new(ret.legacy_render()),
                throws: CowBox::new(throws.legacy_render()),
            },
            NormalTy::Future(value, error) => RealizedTy::Future(
                CowBox::new(value.legacy_render()),
                CowBox::new(error.legacy_render()),
            ),
            // The binder has no surface syntax (its body renders in place); a
            // back-reference is spelled as its alias name — which in the named
            // phase the variable itself carries.
            NormalTy::Mu { body, .. } => body.legacy_render(),
            NormalTy::RecVar(qn) => RealizedTy::TypeAlias(*qn),
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
    fn resolve_binders(self, stack: &mut Vec<NonNull<TypeObject>>, saw_mu: &mut bool) -> NormalTy {
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
                    name: Some(binder),
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
            NormalTy::Tuple(elements) => NormalTy::Tuple(
                elements
                    .into_iter()
                    .map(|e| e.resolve_binders(stack, saw_mu))
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
            NormalTy::Int => NormalTy::Int,
            NormalTy::Bigint => NormalTy::Bigint,
            NormalTy::Float => NormalTy::Float,
            NormalTy::String => NormalTy::String,
            NormalTy::Bool => NormalTy::Bool,
            NormalTy::Null => NormalTy::Null,
            NormalTy::Uint8Array => NormalTy::Uint8Array,
            NormalTy::Media(kind) => NormalTy::Media(kind),
            NormalTy::RustType => NormalTy::RustType,
            NormalTy::Type => NormalTy::Type,
            NormalTy::Resource => NormalTy::Resource,
            NormalTy::PromptAst => NormalTy::PromptAst,
            NormalTy::Literal(lit) => NormalTy::Literal(lit),
            NormalTy::Enum(qn) => NormalTy::Enum(qn),
            NormalTy::EnumVariant(qn, v) => NormalTy::EnumVariant(qn, v),
            NormalTy::Never => NormalTy::Never,
            NormalTy::Unknown => NormalTy::Unknown,
        }
    }
}

impl NormalTy {
    // ── canonicalization ───────────────────────────────────────────────────

    /// Rewrite to a unique canonical form: children canonicalized bottom-up,
    /// unions reduced by the full set algebra. `saw_mu` is the recursive-type
    /// flag from [`NormalTy::resolve_binders`]; when clear, the μ-related guards
    /// below vanish from the hot path.
    fn canonicalize<C: TypeContext>(
        self,
        ctx: &C,
        saw_mu: bool,
        assumptions: &mut HashSet<(NormalTy, NormalTy)>,
    ) -> NormalTy {
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
                bindings.sort_by_key(|(member, _)| *member);
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
            NormalTy::Tuple(elements) => {
                let elements: Vec<_> = elements
                    .into_iter()
                    .map(|e| e.canonicalize(ctx, saw_mu, assumptions))
                    .collect();
                if elements.iter().any(|e| matches!(e, NormalTy::Never)) {
                    NormalTy::Never
                } else {
                    NormalTy::Tuple(elements)
                }
            }
            leaf @ (NormalTy::Int
            | NormalTy::Bigint
            | NormalTy::Float
            | NormalTy::String
            | NormalTy::Bool
            | NormalTy::Null
            | NormalTy::Uint8Array
            | NormalTy::Media(_)
            | NormalTy::RustType
            | NormalTy::Type
            | NormalTy::Resource
            | NormalTy::PromptAst
            | NormalTy::Literal(_)
            | NormalTy::Enum(_)
            | NormalTy::EnumVariant(..)
            | NormalTy::RecVar(_)
            | NormalTy::Never
            | NormalTy::Unknown) => leaf,
        }
    }

    // ── subtyping ──────────────────────────────────────────────────────────

    /// Invariant-position compatibility for a generic type argument: the two
    /// types must be mutual subtypes (i.e. equivalent). Generic constructors —
    /// classes, lists, maps, futures, interface-existentials — are invariant in
    /// BAML (type arguments are real instance data), so `T<A>` relates to `T<B>`
    /// only when every `A`/`B` pair is compatible here.
    ///
    /// The `unknown` top type is a real,
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
    /// (`C <: I`, `A <: B` via requires). There are no
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
        if matches!(sup, NormalTy::Unknown) {
            return true;
        }
        // `unknown` (top) is a subtype of nothing else (reflexivity handled above).
        if matches!(self, NormalTy::Unknown) {
            return false;
        }

        // ── Termination argument ──────────────────────────────────────────
        // The co-inductive assumption set exists *only* to terminate cycles,
        // and a cycle can only arise through an arm that *expands* (regenerates)
        // a type rather than descending into a strictly-smaller subterm:
        //   * μ-unfolding — `unfold` can reproduce the same pair (`(_, Mu)` on
        //     the right, `(Mu, _)` on the left).
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
        let expanding = matches!(self, NormalTy::Mu { .. }) || matches!(sup, NormalTy::Mu { .. });
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

            // `reflect.AnyClass` is compiler-derived for class values. The stdlib
            // blanket impl supplies default-method dispatch, but cannot define
            // membership by itself without also admitting primitives and
            // containers. Keep this before the general impl-registry arm so
            // static coercion and the VM's shared runtime type matcher agree.
            (sub, NormalTy::Interface(qn, _, _))
                if is_any_class(qn, ctx) && !matches!(sub, NormalTy::Interface(..)) =>
            {
                matches!(sub, NormalTy::Class(..))
            }

            // BEP-062: `reflect.AnyFunction` is a compiler builtin implemented by
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
                let pin = |name: AssocIndex| {
                    bindings
                        .iter()
                        .find_map(|(n, ty)| (*n == name).then_some(ty))
                };
                pin(ANY_FUNCTION_RETURNS).is_none_or(|r| ret.is_subtype_of(r, ctx, assumptions))
                    && pin(ANY_FUNCTION_THROWS)
                        .is_none_or(|t| throws.is_subtype_of(t, ctx, assumptions))
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
                        None => NormalTy::Unknown.is_subtype_of(sup_pin, ctx, assumptions),
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
            (NormalTy::List(a), NormalTy::List(b)) => a.invariant_compatible(b, ctx, assumptions),
            (NormalTy::Map { key: k1, value: v1 }, NormalTy::Map { key: k2, value: v2 }) => {
                k1.invariant_compatible(k2, ctx, assumptions)
                    && v1.invariant_compatible(v2, ctx, assumptions)
            }

            (NormalTy::Tuple(a), NormalTy::Tuple(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .zip(b.iter())
                        .all(|(x, y)| x.invariant_compatible(y, ctx, assumptions))
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

    // ── conversion out: NormalTy → RealizedTy ───────────────────

    /// Render a **closed** canonical form back as a [`RealizedTy`]. Surface syntax has no
    /// μ-binder, so a μ-subterm renders as its precomputed [`MuDisplay`] payload
    /// (recursion spelled via alias names) — this never descends into a μ body,
    /// which is why a `RecVar` (always under its binder in a closed term) is
    /// unreachable here.
    fn into_ty(self) -> RealizedTy {
        match self {
            NormalTy::Int => RealizedTy::Int,
            NormalTy::Bigint => RealizedTy::Bigint,
            NormalTy::Float => RealizedTy::Float,
            NormalTy::String => RealizedTy::String,
            NormalTy::Bool => RealizedTy::Bool,
            NormalTy::Null => RealizedTy::Null,
            NormalTy::Uint8Array => RealizedTy::Uint8Array,
            NormalTy::Media(kind) => RealizedTy::Media(kind),
            NormalTy::RustType => RealizedTy::RustType,
            NormalTy::Type => RealizedTy::Type,
            NormalTy::Resource => RealizedTy::Resource,
            NormalTy::PromptAst => RealizedTy::PromptAst,
            NormalTy::Unknown => RealizedTy::Unknown,
            NormalTy::Never => RealizedTy::Never,
            NormalTy::Literal(lit) => RealizedTy::Literal(lit, super::Freshness::Regular),
            NormalTy::Class(qn, args) => RealizedTy::Class(qn, Self::into_tys(args)),
            NormalTy::Interface(qn, args, bindings) => RealizedTy::Interface(
                qn,
                Self::into_tys(args),
                bindings
                    .into_iter()
                    .map(|(name, ty)| AssocBinding {
                        member: name,
                        ty: ty.into_ty(),
                    })
                    .collect(),
            ),
            NormalTy::Enum(qn) => RealizedTy::Enum(qn),
            NormalTy::EnumVariant(_, v) => RealizedTy::EnumVariant(v),
            NormalTy::List(inner) => RealizedTy::List(CowBox::new(inner.into_ty())),
            NormalTy::Map { key, value } => RealizedTy::Map {
                key: CowBox::new(key.into_ty()),
                value: CowBox::new(value.into_ty()),
            },
            NormalTy::Union(members) => RealizedTy::Union(Self::into_tys(members)),
            NormalTy::Tuple(elements) => RealizedTy::Tuple(Self::into_tys(elements)),
            NormalTy::Function {
                params,
                ret,
                throws,
            } => RealizedTy::Function {
                params: params
                    .into_iter()
                    .map(|p| FunctionParamTy {
                        name: p.name,
                        ty: p.ty.into_ty(),
                        mode: p.mode,
                    })
                    .collect(),
                ret: CowBox::new(ret.into_ty()),
                throws: CowBox::new(throws.into_ty()),
            },
            NormalTy::Future(value, error) => {
                RealizedTy::Future(CowBox::new(value.into_ty()), CowBox::new(error.into_ty()))
            }
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
        }
    }
}

impl NormalTy {
    fn into_tys(tys: Vec<NormalTy>) -> CowSlice<RealizedTy> {
        tys.into_iter().map(NormalTy::into_ty).collect()
    }

    /// The interface constraint denoted by a `NormalTy::Interface`'s parts —
    /// its name, generic input arguments, and associated-type bindings, converted
    /// back to `RealizedTy`. This is the precise interface shape handed to the
    /// [`TypeContext`] membership (`implements_interface`) and requires
    /// (`interface_requires`) oracles, so they never have to re-destructure a
    /// loose `RealizedTy` to recover it. The parts are always closed here: the subtype
    /// arms that build a constraint fire only after μ-unfolding the operand.
    fn interface_constraint(
        name: &NonNull<InterfaceObject>,
        generics: &[NormalTy],
        bindings: &[(AssocIndex, NormalTy)],
    ) -> InterfaceBound<RealizedTy> {
        InterfaceBound {
            interface: *name,
            args: generics.iter().cloned().map(NormalTy::into_ty).collect(),
            assoc: bindings
                .iter()
                .map(|(name, ty)| AssocBinding {
                    member: *name,
                    ty: ty.clone().into_ty(),
                })
                .collect(),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// UNION CANONICALIZATION
// ═══════════════════════════════════════════════════════════════════════════

impl NormalTy {
    /// Reduce a union of already-canonical members to canonical form: flatten,
    /// remove `never`, absorb under `unknown`, collapse complete enums, absorb
    /// subtype-members, then sort/dedup and unwrap singletons.
    fn canonicalize_union<C: TypeContext>(
        members: Vec<NormalTy>,
        ctx: &C,
        saw_mu: bool,
        assumptions: &mut HashSet<(NormalTy, NormalTy)>,
    ) -> NormalTy {
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
        if flat.iter().any(|m| matches!(m, NormalTy::Unknown)) {
            return NormalTy::Unknown;
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
    fn collapse_complete_enums<C: TypeContext>(members: &mut Vec<NormalTy>, ctx: &C) {
        // Distinct enums that have at least one variant present.
        let mut enums: Vec<NonNull<ClassObject>> = members
            .iter()
            .filter_map(|m| match m {
                NormalTy::EnumVariant(e, _) => Some(*e),
                _ => None,
            })
            .collect();
        enums.sort();
        enums.dedup();

        for e in enums {
            let Some(all) = ctx.enum_variants(&e) else {
                continue; // unknown enum → no collapse (fail-safe)
            };
            let present: HashSet<&NonNull<EnumVariantObject>> = members
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
        fn spine_has_rec(t: &NormalTy, depth: u32) -> bool {
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
            NormalTy::Union(members) | NormalTy::Tuple(members) => {
                members.iter().any(NormalTy::has_unguarded_mu)
            }
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
            NormalTy::Int
            | NormalTy::Bigint
            | NormalTy::Float
            | NormalTy::String
            | NormalTy::Bool
            | NormalTy::Null
            | NormalTy::Uint8Array
            | NormalTy::Media(_)
            | NormalTy::RustType
            | NormalTy::Type
            | NormalTy::Resource
            | NormalTy::PromptAst
            | NormalTy::Literal(_)
            | NormalTy::Enum(_)
            | NormalTy::EnumVariant(_, _)
            | NormalTy::Never
            | NormalTy::Unknown => false,
        }
    }

    /// Replace the complete pair of bool literals with `bool`
    /// (`true | false == bool`, TYPE_SYSTEM.md §Subtyping Cases) — the bool
    /// analogue of enum completeness, context-free because the variant family is
    /// closed and its equality unoverridable.
    fn collapse_complete_bools(members: &mut Vec<NormalTy>) {
        let has = |members: &[NormalTy], b: bool| {
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
    /// `T | I == I`.
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
    fn absorb_subtypes<C: TypeContext>(
        members: &[NormalTy],
        ctx: &C,
        saw_mu: bool,
        assumptions: &mut HashSet<(NormalTy, NormalTy)>,
    ) -> Vec<NormalTy> {
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
            if open[i] {
                continue;
            }
            for j in 0..n {
                if i == j || !keep[j] || open[j] {
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

impl NormalTy {
    /// One unfold step of a closed canonical μ: `μX.body ↦ body[μX.body/X]`,
    /// i.e. every `RecVar` bound by *this* binder is replaced with the whole μ-term.
    ///
    /// Because `self` is closed (the public canonical-form invariant), the
    /// replacement is closed too, so grafting it under deeper binders can neither
    /// capture nor be captured — no de Bruijn shifting is needed, and the result
    /// is again closed with a unique (α-canonical) representation, keeping the
    /// derived-`==` probes of the co-inductive assumption set exact.
    fn unfold(&self) -> NormalTy {
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
            NormalTy::Union(members) | NormalTy::Tuple(members) => {
                members.iter().any(|m| m.has_free_rec_var(depth))
            }
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
            NormalTy::Int
            | NormalTy::Bigint
            | NormalTy::Float
            | NormalTy::String
            | NormalTy::Bool
            | NormalTy::Null
            | NormalTy::Uint8Array
            | NormalTy::Media(_)
            | NormalTy::RustType
            | NormalTy::Type
            | NormalTy::Resource
            | NormalTy::PromptAst
            | NormalTy::Literal(_)
            | NormalTy::Enum(_)
            | NormalTy::EnumVariant(_, _)
            | NormalTy::Never
            | NormalTy::Unknown => false,
        }
    }

    /// Replace every `RecVar` bound by the binder `depth` levels out with
    /// `replacement` (which must be closed — see [`Self::unfold`]).
    fn replace_rec_var(&self, depth: u32, replacement: &NormalTy) -> NormalTy {
        match self {
            NormalTy::RecVar(i) if *i == depth => replacement.clone(),
            NormalTy::Mu { binder, body } => NormalTy::Mu {
                binder: binder.clone(),
                body: Box::new(body.replace_rec_var(depth + 1, replacement)),
            },
            NormalTy::Class(qn, args) => NormalTy::Class(
                *qn,
                args.iter()
                    .map(|a| a.replace_rec_var(depth, replacement))
                    .collect(),
            ),
            NormalTy::Interface(qn, args, bindings) => NormalTy::Interface(
                *qn,
                args.iter()
                    .map(|a| a.replace_rec_var(depth, replacement))
                    .collect(),
                bindings
                    .iter()
                    .map(|(n, t)| (*n, t.replace_rec_var(depth, replacement)))
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
            NormalTy::Tuple(elements) => NormalTy::Tuple(
                elements
                    .iter()
                    .map(|e| e.replace_rec_var(depth, replacement))
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
            // Leaves and non-matching indices are untouched.
            NormalTy::RecVar(_)
            | NormalTy::Int
            | NormalTy::Bigint
            | NormalTy::Float
            | NormalTy::String
            | NormalTy::Bool
            | NormalTy::Null
            | NormalTy::Uint8Array
            | NormalTy::Media(_)
            | NormalTy::RustType
            | NormalTy::Type
            | NormalTy::Resource
            | NormalTy::PromptAst
            | NormalTy::Literal(_)
            | NormalTy::Enum(_)
            | NormalTy::EnumVariant(..)
            | NormalTy::Never
            | NormalTy::Unknown => self.clone(),
        }
    }
}

impl<P: MuPhase> NormalParam<P> {
    fn is_required(&self) -> bool {
        matches!(self.mode, FunctionParamMode::Required)
    }
}

impl NormalParam {
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

mod mu;

#[cfg(test)]
mod tests;
