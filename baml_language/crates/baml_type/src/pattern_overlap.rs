//! The `match`/`is` pattern reachability oracle (`pattern_overlap`).
//!
//! Decides whether a pattern's denoted type-set can intersect a scrutinee member's
//! under *some* realization of the in-scope rigid type variables — `Yes`/`Unknown`
//! (possibly reachable: the arm keeps its runtime test) vs `No` (provably dead: a
//! compile error at the caller, like any concrete type mismatch). Built on the
//! shared equality-unification core ([`crate::unify`]). The semantic ground rules:
//!
//! 1. A rigid type variable is *potentially* unifiable with other types, never
//!    necessarily — the sole definite case (the same variable on both sides) is
//!    decided upstream by canonical type equality, not here, and the unifier's
//!    bindings are witnesses of possibility only.
//! 2. Bounds refute, never confirm.
//! 3. Uncertainty (search budget, alias cycles) degrades to possible — the sound
//!    direction, since a possible arm's runtime test can never violate a type
//!    contract, whereas a wrong `No` rejects valid code and a wrong "definite"
//!    would skip a needed test.

use baml_base::TyAttr;

use crate::{
    Interface, ParamTy, Ty, TypeName,
    unify::{
        EnumVariants, MAX_UNIFY_DEPTH, Overlap, TypeBindings, chase_var, contains_bound_typevar,
        expand_alias_head, is_literal_subtype, nf, unify_into, var_under_union,
    },
};

/// In-scope rigid type parameters mapped to their declared interface bounds
/// (`T extends I` conjunctions). Relocated from the compiler's lowering scope -
/// the oracle's `bounds` input and the param-env vocabulary share this shape.
pub type TypeVarBoundsMap = rustc_hash::FxHashMap<ParamTy, Vec<Interface>>;

/// How the oracle answers "does this realized type implement this interface?" —
/// abstracted from the impl registry so the oracle stays db-free (unit-testable);
/// production callers close over
/// the compiler's `get_implements_block`. Consulted only
/// with fully-realized inputs, and only to *refute*: `false` must mean "provably
/// does not implement", which is sound even under dynamic loading — the orphan rule
/// prevents a later-loaded package from adding an impl for an already-loaded
/// (type, interface) pair.
pub type ImplementsOracle<'a> = &'a dyn Fn(&Ty, &Interface) -> bool;

/// Everything [`pattern_overlap`] needs to know about the enclosing scope and program.
pub struct PatternOverlapEnv<'a> {
    /// Every in-scope rigid type parameter (function generics, enclosing-type
    /// generics, and `"Self"` in interface-owned bodies). Shared by name across both
    /// sides — the pattern's `T` and the scrutinee's `T` are the *same* variable (one
    /// scope), unlike coherence's two independently-renamed impls.
    ///
    /// **Caller obligation:** a type variable absent from this set is treated as an
    /// opaque atom that overlaps nothing but itself, so a `No` verdict over a type
    /// mentioning an out-of-scope variable is a judgment about a variable the oracle
    /// cannot see. Callers that act on `No` (dead-arm errors, `Never` narrowing)
    /// must first check that every free type variable of both inputs is in this set
    /// (see `TypeInferenceBuilder::all_type_vars_in_scope`); `Yes`/`Unknown` are
    /// safe regardless.
    pub vars: &'a [ParamTy],
    /// Interface bounds of the rigid params (`T extends I`). Bounds only ever
    /// *refute* (a pinned witness that provably fails a bound → `No`); they never
    /// upgrade a possible overlap to a definite one.
    pub bounds: &'a TypeVarBoundsMap,
    /// Type-alias bodies, pre-folded to `nf`'s canonical union form — build with
    /// the caller's normalized alias map. Raw bodies mis-decide alias-obscured
    /// unions at invariant positions (`Bar<TF>` vs `Bar<bool>` with
    /// `type TF = true | false` would be a wrong `No`).
    pub aliases: &'a std::collections::HashMap<TypeName, Ty>,
    /// Enum schemas for `nf`'s complete-variant folding.
    pub enum_variants: EnumVariants<'a>,
    /// See [`ImplementsOracle`].
    pub implements: ImplementsOracle<'a>,
}

/// Can the type-set denoted by `pat` intersect the set denoted by `member` under
/// *some* realization of the in-scope rigid type variables
/// (`∃σ. σ(pat) ∩ σ(member) ≠ ∅`)?
///
/// The reachability oracle for `match`/`is` arms whose pattern or scrutinee type
/// carries rigid type variables or associated-type projections:
///
/// - `Yes` / `Unknown` — the arm is **possibly** reachable: some realization could
///   give the pattern and the member a common value, so the arm keeps its runtime
///   test. Never a promise: a rigid variable is *potentially* unifiable with another
///   type, never necessarily. The sole definite case — the same variable on both
///   sides — is decided upstream by canonical type equality, not here; the unifier's
///   bindings are witnesses of possibility only.
/// - `No` — **provably** no realization gives them a common value; the arm is
///   statically dead, reported by the caller like any concrete type mismatch.
///
/// Overlap here differs from impl-coherence overlap (`impls_overlap` in
/// `interfaces::coherence`) in what "meet" means at the top level, because it
/// compares *value sets*, not impl subjects: top-level unions meet by intersection
/// (a disjunction over member pairs), a literal meets its base (`1` and `int` share
/// values while remaining distinct types — likewise an enum variant and its enum), a
/// concrete type meets an interface-existential by *membership*, two existentials
/// always possibly meet (a common implementor), and two function types
/// conservatively meet (their value sets nest by structural subtyping). Inside
/// invariant constructor arguments all of that collapses to equality — `Box<1>` and
/// `Box<int>` are disjoint — which is exactly [`unify_into`].
pub fn pattern_overlap(pat: &Ty, member: &Ty, env: &PatternOverlapEnv<'_>) -> Overlap {
    pattern_overlap_at(pat, member, env, 0)
}

/// Depth-capped worker: normalize, expand the alias head, and decompose top-level
/// unions into a disjunction of member pairs — set intersection distributes over
/// union, and the existential over realizations distributes over the disjunction, so
/// each pair quantifies its own independent `σ`. The depth cap bounds alias-cycle
/// expansion through union members (`expand_alias_head` bounds only head *chains*);
/// hitting it degrades to `Unknown` — possible, the sound direction.
fn pattern_overlap_at(pat: &Ty, member: &Ty, env: &PatternOverlapEnv<'_>, depth: usize) -> Overlap {
    if depth >= MAX_UNIFY_DEPTH {
        return Overlap::Unknown;
    }
    let pat = nf(&expand_alias_head(pat, env.aliases), env.enum_variants);
    let member = nf(&expand_alias_head(member, env.aliases), env.enum_variants);
    let (pats, members) = (union_members(&pat), union_members(&member));
    if let ([p], [m]) = (pats, members) {
        return pattern_pair_overlap(p, m, env);
    }
    let mut result = Overlap::No;
    for p in pats {
        for m in members {
            match pattern_overlap_at(p, m, env, depth + 1) {
                Overlap::Yes => return Overlap::Yes,
                Overlap::Unknown => result = Overlap::Unknown,
                Overlap::No => {}
            }
        }
    }
    result
}

/// The members a type contributes to a top-level union meet: a union's member list,
/// any other type as the singleton `{ty}`.
fn union_members(ty: &Ty) -> &[Ty] {
    match ty {
        Ty::Union(members, _) => members,
        other => std::slice::from_ref(other),
    }
}

/// One decomposed (non-union) pattern/member pair. Equality-unification decides the
/// bulk — same-constructor pairs recurse into invariant argument positions, in-scope
/// variables bind (shared by name across both sides), ground pairs compare
/// canonically — and the pairs equality wrongly calls disjoint are rescued by the
/// top-level meet ([`pattern_atom_meet`]). A surviving overlap is then checked
/// against the rigid params' interface bounds, which can refute it to `No`.
fn pattern_pair_overlap(pat: &Ty, member: &Ty, env: &PatternOverlapEnv<'_>) -> Overlap {
    debug_assert!(
        !matches!(pat, Ty::Union(..)) && !matches!(member, Ty::Union(..)),
        "pair sides are decomposed union members, never unions themselves"
    );
    // `never` denotes the empty set: it overlaps nothing — not even itself (equality
    // unification would call two `never`s the same type, the wrong question here).
    if matches!(pat, Ty::Never { .. }) || matches!(member, Ty::Never { .. }) {
        return Overlap::No;
    }
    // `unknown` is the top type: at the pair's top level the question is value-set
    // intersection, and every inhabited type shares values with it — for a rigid
    // pattern var, every realization does (`σ(T) ∩ unknown ≠ ∅`), so nothing is
    // pinned and bounds cannot refute. Deciding this by equality-unification
    // instead would bind `var := unknown` and then wrongly bound-refute a bounded
    // var (`unknown` implements nothing). Distinct from `unknown` in an invariant
    // *argument* position, where equality IS the question and `unify_into` keeps
    // deciding it (binding an opposing var, comparing ground pairs exactly).
    if matches!(pat, Ty::BuiltinUnknown { .. }) || matches!(member, Ty::BuiltinUnknown { .. }) {
        return Overlap::Yes;
    }
    // A bare in-scope rigid var meeting an interface existential is likewise
    // membership at the pair's top level: `σ(T)` ranges over concrete types and
    // `⟦I⟧` spans every implementor, so a common concrete instance is always
    // possible in the open world (a later-loaded package can introduce one — the
    // meet's existential reasoning) and never refutable through the var's own
    // bounds (the same new type can implement both interfaces). Equality
    // unification would instead pin `var := I` as a ground atom and wrongly
    // bound-refute it (a bounded var must realize to a *concrete* type — the
    // rule that stays correct for invariant argument positions, where equality
    // is the question and `unify_into` keeps deciding).
    let bare_rigid = |t: &Ty| matches!(t, Ty::TypeVar(n, _) if env.vars.contains(n));
    if (bare_rigid(pat) && matches!(member, Ty::Interface(..)))
        || (matches!(pat, Ty::Interface(..)) && bare_rigid(member))
    {
        return Overlap::Yes;
    }
    let mut bindings = TypeBindings::default();
    let mut result = unify_into(pat, member, env.vars, env.aliases, &mut bindings);
    if result == Overlap::No {
        result = pattern_atom_meet(pat, member, env);
        // A meet rescue proves possibility structurally, without a witness
        // substitution; bindings recorded by the *failed* equality attempt are not
        // conditions on it and must not feed bound refutation.
        bindings.clear();
    }
    if result != Overlap::No && pattern_bounds_refute(pat, member, &bindings, env) {
        return Overlap::No;
    }
    result
}

/// The top-level meets that value-set intersection has and equality-unification
/// lacks. Only consulted after [`unify_into`] answered `No`, so equal or unifiable
/// pairs never reach it.
fn pattern_atom_meet(pat: &Ty, member: &Ty, env: &PatternOverlapEnv<'_>) -> Overlap {
    // A literal shares its values with its base (`1 ⊂ int`), an enum variant with
    // its enum — symmetric here, since intersection is.
    if is_literal_subtype(pat, member) || is_literal_subtype(member, pat) {
        return Overlap::Yes;
    }
    match (pat, member) {
        // Error sentinels overlap nothing (mirrors `unify_into`): the type already
        // carries its own diagnostic, and callers suppress cascading reports.
        (Ty::Unknown { .. } | Ty::Error { .. } | Ty::Infer { .. }, _)
        | (_, Ty::Unknown { .. } | Ty::Error { .. } | Ty::Infer { .. }) => Overlap::No,
        // `unknown` is the top type: it shares values with every inhabited type
        // (`never` was rejected before unification).
        (Ty::BuiltinUnknown { .. }, _) | (_, Ty::BuiltinUnknown { .. }) => Overlap::Yes,
        // An opaque `$rust_type` could coincide with anything.
        (Ty::RustType { .. }, _) | (_, Ty::RustType { .. }) => Overlap::Yes,
        // A residual projection could stand for any type (defensive mirror of the
        // `unify_into` arm, which normally decides these pairs before the meet).
        (Ty::AssociatedTypeProjection { .. }, _) | (_, Ty::AssociatedTypeProjection { .. }) => {
            Overlap::Yes
        }
        // Two interface-existentials always possibly meet — a common implementor may
        // exist, and never refutably so: a later-loaded package can always introduce
        // a *new* type implementing both (the orphan rule closes the world only for
        // already-loaded (type, interface) pairs). This includes same-name
        // existentials with non-unifiable args: coherence admits
        // `implement I<int> for C` alongside `implement I<string> for C` (disjoint
        // as impls), so the existential sets `I<int>` and `I<string>` share `C`'s
        // values.
        (Ty::Interface(..), Ty::Interface(..)) => Overlap::Yes,
        // Concrete-vs-existential is membership (`C <: I` iff `C` implements `I`):
        // possible, unless both sides are fully realized and the registry proves
        // non-implementation.
        (iface @ Ty::Interface(..), other) | (other, iface @ Ty::Interface(..)) => {
            interface_membership_overlap(iface, other, env)
        }
        // Function types are the structural-subtyping exception (contravariant
        // params, covariant returns/throws), so two non-equal function types can
        // still share values — a value whose concrete function type is a structural
        // subtype of both. Equality already failed above; conservatively possible.
        (Ty::Function { .. }, Ty::Function { .. }) => Overlap::Yes,
        // Unreachable by construction (the caller decomposes unions); fail toward
        // possible, the sound direction.
        (Ty::Union(..), _) | (_, Ty::Union(..)) => Overlap::Unknown,
        // Every remaining pair is genuinely disjoint: distinct concrete constructors
        // denote disjoint value sets, same-constructor pairs already recursed inside
        // `unify_into`, and a type variable outside `env.vars` is an opaque atom. No
        // total wildcard on the left so a new `Ty` variant must be classified here.
        (
            Ty::Class(..)
            | Ty::List(..)
            | Ty::EvolvingList(..)
            | Ty::Map { .. }
            | Ty::EvolvingMap(..)
            | Ty::Future(..)
            | Ty::Int { .. }
            | Ty::Bigint { .. }
            | Ty::Float { .. }
            | Ty::String { .. }
            | Ty::Bool { .. }
            | Ty::Null { .. }
            | Ty::Uint8Array { .. }
            | Ty::Media(..)
            | Ty::Literal(..)
            | Ty::Enum(..)
            | Ty::EnumVariant(..)
            | Ty::Function { .. }
            | Ty::Type { .. }
            | Ty::Resource { .. }
            | Ty::PromptAst { .. }
            | Ty::Void { .. }
            | Ty::TypeAlias(..)
            | Ty::TypeVar(..)
            | Ty::Never { .. },
            _,
        ) => Overlap::No,
    }
}

/// Concrete-type-vs-interface-existential membership. Possible (`Yes`) unless every
/// input is fully realized and the registry proves `other` cannot implement the
/// existential's interface — the one membership refutation the open world allows
/// (see [`ImplementsOracle`]). Literals and enum variants are skipped (their
/// membership routes through their base concrete type), staying conservatively
/// possible.
fn interface_membership_overlap(iface: &Ty, other: &Ty, env: &PatternOverlapEnv<'_>) -> Overlap {
    let constraint = iface
        .as_interface()
        .unwrap_or_else(|| unreachable!("caller matched `Ty::Interface`"));
    let realized = |t: &Ty| crate::RealizedTy::try_from(t).is_ok();
    if matches!(other, Ty::Literal(..) | Ty::EnumVariant(..))
        || !realized(other)
        || !constraint.tys().all(realized)
    {
        return Overlap::Yes;
    }
    if (env.implements)(other, &constraint) {
        Overlap::Yes
    } else {
        Overlap::No
    }
}

/// Refute a surviving pair overlap through the rigid params' interface bounds:
/// `true` iff some rigid param was pinned by the unifier to a ground witness that
/// provably cannot realize it. Bounds never *confirm* an overlap — a satisfiable (or
/// undecidable) bound leaves the result as it was.
///
/// Mirrors coherence's `bounds_hold_at_common_instance` structurally, but over the
/// shared (unrenamed) pattern scope, an [`ImplementsOracle`] instead of the impl
/// registry directly, and with one extra refutation: a bounded param must realize to
/// a *concrete* type (`TYPE_SYSTEM.md` "Generics on Functions"), so a witness pinned
/// to a union or an interface-existential fails regardless of the registry. The same
/// principality caveat applies: a param occurring inside a union on either side may
/// have been bound by the covering search to one of several possible witnesses, so
/// disproving its bound against that arbitrary pick would be unsound — skipped.
fn pattern_bounds_refute(
    pat: &Ty,
    member: &Ty,
    bindings: &TypeBindings,
    env: &PatternOverlapEnv<'_>,
) -> bool {
    if bindings.is_empty() {
        return false;
    }
    // Every rigid param resolved through the binding chains — a param's own witness,
    // and the instantiation for sibling-param mentions in bound args.
    let witnesses: TypeBindings = env
        .vars
        .iter()
        .map(|name| {
            (
                name.clone(),
                chase_var(
                    &Ty::TypeVar(name.clone(), TyAttr::default()),
                    env.vars,
                    bindings,
                ),
            )
        })
        .collect();
    for name in env.vars {
        let Some(bounds) = env.bounds.get(name) else {
            continue;
        };
        if bounds.is_empty() {
            continue;
        }
        // Non-principal (union-cover) binding ⇒ the witness is arbitrary; don't
        // disprove.
        if var_under_union(name, pat) || var_under_union(name, member) {
            continue;
        }
        let witness = &witnesses[name];
        // Unpinned, or pinned to something still variable-bearing (which some
        // realization may yet collapse — `U | int` is concrete at `U = int`):
        // undecidable in an open world, assumed satisfiable.
        if contains_bound_typevar(witness, env.vars) {
            continue;
        }
        // Bounded params must realize to concrete types: a ground union or
        // interface-existential witness fails every interface bound structurally.
        if matches!(witness, Ty::Union(..) | Ty::Interface(..)) {
            return true;
        }
        if crate::RealizedTy::try_from(witness).is_err() {
            continue;
        }
        // A single provably-unsatisfied bound kills the overlap. Only a
        // fully-realized instantiated bound may refute; an unresolved bound arg
        // keeps the bound conservatively assumed-to-hold.
        for bound in bounds {
            let bound = bound.map_tys(|t| crate::unify::substitute_ty(t, &witnesses));
            if bound.tys().all(|t| crate::RealizedTy::try_from(t).is_ok())
                && !(env.implements)(witness, &bound)
            {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use baml_base::{Literal, Name};

    use super::*;

    fn param(name: &str) -> ParamTy {
        ParamTy::new(0, Name::new(name))
    }

    fn params(vars: &[&str]) -> Vec<ParamTy> {
        vars.iter().map(|name| param(name)).collect()
    }

    fn interface(name: &str, args: Vec<Ty>) -> Ty {
        Ty::Interface(
            TypeName::local(Name::new(name)),
            args,
            vec![],
            TyAttr::default(),
        )
    }

    fn int_literal(n: i64) -> Ty {
        Ty::Literal(
            Literal::Int(n),
            crate::Freshness::Regular,
            TyAttr::default(),
        )
    }

    fn bool_literal(b: bool) -> Ty {
        Ty::Literal(
            Literal::Bool(b),
            crate::Freshness::Regular,
            TyAttr::default(),
        )
    }

    fn enum_ty(name: &str) -> Ty {
        Ty::Enum(TypeName::local(Name::new(name)), TyAttr::default())
    }

    fn enum_variant(enum_name: &str, variant: &str) -> Ty {
        Ty::EnumVariant(
            TypeName::local(Name::new(enum_name)),
            Name::new(variant),
            TyAttr::default(),
        )
    }

    fn never() -> Ty {
        Ty::Never {
            attr: TyAttr::default(),
        }
    }

    /// Stub enum schema: `Cmp` has variants `Less`, `Equal`, `More`.
    fn stub_enum_variants(qtn: &TypeName) -> Option<Vec<Name>> {
        (qtn.name().as_str() == "Cmp")
            .then(|| vec![Name::new("Less"), Name::new("Equal"), Name::new("More")])
    }

    fn class1(name: &str, arg: Ty) -> Ty {
        Ty::Class(
            TypeName::local(Name::new(name)),
            vec![arg],
            TyAttr::default(),
        )
    }

    fn class2(name: &str, a: Ty, b: Ty) -> Ty {
        Ty::Class(
            TypeName::local(Name::new(name)),
            vec![a, b],
            TyAttr::default(),
        )
    }

    fn type_alias(name: &str) -> Ty {
        Ty::TypeAlias(TypeName::local(Name::new(name)), TyAttr::default())
    }

    /// A nullary interface constraint (the bound / registry-request form, as opposed
    /// to the [`interface`] helper's existential `Ty`).
    fn constraint(name: &str) -> Interface {
        Interface::new(TypeName::local(Name::new(name)), vec![], vec![])
    }

    /// A zero-parameter function type with the given return type (`() -> ret`, never
    /// throwing) — enough to exercise the function-type meets without param plumbing.
    fn fn_returning(ret: Ty) -> Ty {
        Ty::Function {
            params: vec![],
            ret: Box::new(ret),
            throws: Box::new(never()),
            attr: TyAttr::default(),
        }
    }

    fn projection(base: Ty, iface: &str, member: &str) -> Ty {
        Ty::AssociatedTypeProjection {
            base: Box::new(base),
            interface: Box::new(constraint(iface)),
            member: Name::new(member),
            attr: TyAttr::default(),
        }
    }

    /// The oracle with everything configurable; `implements` answers the registry's
    /// membership question for fully-realized (type, interface) pairs.
    fn pattern_overlap_with(
        pat: &Ty,
        member: &Ty,
        vars: &[ParamTy],
        bounds: &TypeVarBoundsMap,
        aliases: &std::collections::HashMap<TypeName, Ty>,
        implements: ImplementsOracle<'_>,
    ) -> Overlap {
        pattern_overlap(
            pat,
            member,
            &PatternOverlapEnv {
                vars,
                bounds,
                aliases,
                enum_variants: &stub_enum_variants,
                implements,
            },
        )
    }

    /// The oracle over an empty program: no aliases, no bounds, and a registry that
    /// refutes every membership question (irrelevant unless the test involves
    /// interface-existentials).
    fn pattern_overlap_plain(pat: &Ty, member: &Ty, vars: &[ParamTy]) -> Overlap {
        pattern_overlap_with(
            pat,
            member,
            vars,
            &TypeVarBoundsMap::default(),
            &std::collections::HashMap::default(),
            &|_, _| false,
        )
    }

    #[test]
    fn pattern_overlap_same_var_is_reflexively_yes() {
        let vars = params(&["T"]);
        let t = Ty::type_var("T");
        assert_eq!(pattern_overlap_plain(&t, &t, &vars), Overlap::Yes);
    }

    #[test]
    fn pattern_overlap_var_vs_concrete_is_possible_both_directions() {
        // An unbounded rigid var can realize to any type — the pattern-side and the
        // member-side (scrutinee) directions are the same question.
        let vars = params(&["T"]);
        let t = Ty::type_var("T");
        assert_eq!(pattern_overlap_plain(&t, &Ty::int(), &vars), Overlap::Yes);
        assert_eq!(pattern_overlap_plain(&Ty::int(), &t, &vars), Overlap::Yes);
    }

    #[test]
    fn pattern_overlap_var_vs_ctor_carrying_var_is_possible() {
        // `T` vs `Box<U>` overlaps at `T := Box<U>` (vars may bind to var-bearing
        // types; possibility, not a witness).
        let vars = params(&["T", "U"]);
        assert_eq!(
            pattern_overlap_plain(&Ty::type_var("T"), &class1("Box", Ty::type_var("U")), &vars),
            Overlap::Yes
        );
    }

    #[test]
    fn pattern_overlap_binding_consistency_across_positions() {
        // `Pair<T, T>` requires one consistent realization: `Pair<int, string>`
        // forces `T := int` then `T := string` — no common instance.
        let vars = params(&["T"]);
        let pat = class2("Pair", Ty::type_var("T"), Ty::type_var("T"));
        assert_eq!(
            pattern_overlap_plain(&pat, &class2("Pair", Ty::int(), Ty::string()), &vars),
            Overlap::No
        );
        assert_eq!(
            pattern_overlap_plain(&pat, &class2("Pair", Ty::int(), Ty::int()), &vars),
            Overlap::Yes
        );
    }

    #[test]
    fn pattern_overlap_vars_are_shared_by_name_across_sides() {
        // Unlike coherence's renamed-apart impls, the pattern and the scrutinee live
        // in one scope: their `T`s are the same variable. `Pair<T, string>` vs
        // `Pair<T, T>` overlaps at `T := string`; `Pair<int, T>` vs `Pair<T, string>`
        // forces `T := int` and `T := string` — disjoint.
        let vars = params(&["T"]);
        let t = || Ty::type_var("T");
        assert_eq!(
            pattern_overlap_plain(
                &class2("Pair", t(), Ty::string()),
                &class2("Pair", t(), t()),
                &vars
            ),
            Overlap::Yes
        );
        assert_eq!(
            pattern_overlap_plain(
                &class2("Pair", Ty::int(), t()),
                &class2("Pair", t(), Ty::string()),
                &vars
            ),
            Overlap::No
        );
    }

    #[test]
    fn pattern_overlap_ground_mismatch_is_no() {
        let vars = params(&["T"]);
        assert_eq!(
            pattern_overlap_plain(&Ty::int(), &Ty::string(), &vars),
            Overlap::No
        );
        assert_eq!(
            pattern_overlap_plain(
                &class1("Box", Ty::int()),
                &class1("Box", Ty::string()),
                &vars
            ),
            Overlap::No
        );
    }

    #[test]
    fn pattern_overlap_literal_meets_base_at_top_level_only() {
        // Top level compares value sets: `1 ⊂ int`, so they share values (both
        // directions). Inside an invariant constructor argument the relation is
        // equality, so `Box<1>` and `Box<int>` are disjoint types.
        let vars = params(&["T"]);
        assert_eq!(
            pattern_overlap_plain(&int_literal(1), &Ty::int(), &vars),
            Overlap::Yes
        );
        assert_eq!(
            pattern_overlap_plain(&Ty::int(), &int_literal(1), &vars),
            Overlap::Yes
        );
        assert_eq!(
            pattern_overlap_plain(
                &class1("Box", int_literal(1)),
                &class1("Box", Ty::int()),
                &vars
            ),
            Overlap::No
        );
    }

    #[test]
    fn pattern_overlap_enum_variant_meets_its_enum_only() {
        let vars = params(&[]);
        assert_eq!(
            pattern_overlap_plain(&enum_variant("Color", "Red"), &enum_ty("Color"), &vars),
            Overlap::Yes
        );
        assert_eq!(
            pattern_overlap_plain(&enum_variant("Color", "Red"), &enum_ty("Other"), &vars),
            Overlap::No
        );
    }

    #[test]
    fn pattern_overlap_union_decomposes_by_intersection() {
        // Top-level unions meet by intersection, not union equality: one overlapping
        // member pair suffices.
        let vars = params(&["T"]);
        assert_eq!(
            pattern_overlap_plain(
                &Ty::union([Ty::type_var("T"), Ty::int()]),
                &Ty::string(),
                &vars
            ),
            Overlap::Yes
        );
        assert_eq!(
            pattern_overlap_plain(&Ty::union([Ty::int(), Ty::bool()]), &Ty::string(), &vars),
            Overlap::No
        );
        assert_eq!(
            pattern_overlap_plain(
                &Ty::string(),
                &Ty::union([Ty::type_var("T"), Ty::int()]),
                &vars
            ),
            Overlap::Yes
        );
    }

    #[test]
    fn pattern_overlap_never_overlaps_nothing() {
        // The empty set has no common value with anything — not even itself
        // (equality-unification would call two `never`s the same type; overlap must
        // not).
        let vars = params(&["T"]);
        assert_eq!(
            pattern_overlap_plain(&never(), &Ty::type_var("T"), &vars),
            Overlap::No
        );
        assert_eq!(
            pattern_overlap_plain(&Ty::type_var("T"), &never(), &vars),
            Overlap::No
        );
        assert_eq!(
            pattern_overlap_plain(&never(), &never(), &vars),
            Overlap::No
        );
    }

    #[test]
    fn pattern_overlap_unknown_top_type_meets_everything() {
        let vars = params(&[]);
        let unknown = Ty::BuiltinUnknown {
            attr: TyAttr::default(),
        };
        assert_eq!(
            pattern_overlap_plain(&unknown, &Ty::int(), &vars),
            Overlap::Yes
        );
    }

    #[test]
    fn pattern_overlap_bounded_var_meets_unknown_member() {
        // A rigid var pattern over an `unknown`-typed scrutinee member is possible
        // regardless of the var's bounds: every realization's value set intersects
        // the top type, so no witness is pinned and bound refutation must not fire
        // (the `other is Self` shape inside an interface default method). In an
        // invariant *argument* position the question is equality instead, and a
        // bounded var genuinely cannot realize to `unknown` — refuted.
        let vars = params(&["T"]);
        let unknown = Ty::BuiltinUnknown {
            attr: TyAttr::default(),
        };
        let mut bounds = TypeVarBoundsMap::default();
        bounds.insert(param("T"), vec![constraint("I")]);
        let aliases = std::collections::HashMap::default();
        assert_eq!(
            pattern_overlap_with(
                &Ty::type_var("T"),
                &unknown,
                &vars,
                &bounds,
                &aliases,
                &|_, _| false,
            ),
            Overlap::Yes
        );
        assert_eq!(
            pattern_overlap_with(
                &class1("Box", Ty::type_var("T")),
                &class1("Box", unknown),
                &vars,
                &bounds,
                &aliases,
                &|_, _| false,
            ),
            Overlap::No
        );
    }

    #[test]
    fn pattern_overlap_bounded_var_meets_existential_member() {
        // A rigid var meeting an interface existential at the top level is
        // membership over concrete implementors — always possible in the open
        // world (a later package can add a type implementing both the
        // existential's interface and the var's bound), in either orientation
        // (the `other is Self` shape with an interface-typed scrutinee). The
        // equality view — a bounded var cannot *realize to* an existential —
        // still refutes in invariant argument positions
        // (`pattern_overlap_bounded_var_cannot_realize_to_union_or_existential`).
        let vars = params(&["T"]);
        let mut bounds = TypeVarBoundsMap::default();
        bounds.insert(param("T"), vec![constraint("I")]);
        let aliases = std::collections::HashMap::default();
        assert_eq!(
            pattern_overlap_with(
                &Ty::type_var("T"),
                &interface("J", vec![]),
                &vars,
                &bounds,
                &aliases,
                &|_, _| false,
            ),
            Overlap::Yes
        );
        assert_eq!(
            pattern_overlap_with(
                &interface("J", vec![]),
                &Ty::type_var("T"),
                &vars,
                &bounds,
                &aliases,
                &|_, _| false,
            ),
            Overlap::Yes
        );
    }

    #[test]
    fn pattern_overlap_error_sentinel_overlaps_nothing() {
        // An errored type carries its own diagnostic; the oracle must not stack an
        // overlap claim (in either direction) on top of it.
        let vars = params(&["T"]);
        let error = Ty::Error {
            attr: TyAttr::default(),
        };
        assert_eq!(
            pattern_overlap_plain(&error, &Ty::type_var("T"), &vars),
            Overlap::No
        );
    }

    #[test]
    fn pattern_overlap_projection_is_conservatively_possible() {
        // A residual projection could stand for any type; identical projections are
        // trivially possible too.
        let vars = params(&["T"]);
        let proj = projection(Ty::type_var("T"), "Iter", "Item");
        assert_eq!(
            pattern_overlap_plain(&proj, &Ty::int(), &vars),
            Overlap::Yes
        );
        assert_eq!(pattern_overlap_plain(&proj, &proj, &vars), Overlap::Yes);
    }

    #[test]
    fn pattern_overlap_bound_refutes_pinned_witness() {
        // `T extends I` vs a member that pins `T := int`: the registry's answer for
        // `int implements I` decides — bounds refute (`No`) but never confirm (a
        // positive answer just leaves the possibility standing).
        let vars = params(&["T"]);
        let mut bounds = TypeVarBoundsMap::default();
        bounds.insert(param("T"), vec![constraint("I")]);
        let aliases = std::collections::HashMap::default();
        assert_eq!(
            pattern_overlap_with(
                &Ty::type_var("T"),
                &Ty::int(),
                &vars,
                &bounds,
                &aliases,
                &|_, _| false,
            ),
            Overlap::No
        );
        assert_eq!(
            pattern_overlap_with(
                &Ty::type_var("T"),
                &Ty::int(),
                &vars,
                &bounds,
                &aliases,
                &|_, _| true,
            ),
            Overlap::Yes
        );
    }

    #[test]
    fn pattern_overlap_bound_on_unpinned_witness_is_assumed_satisfiable() {
        // `T extends I` vs `Box<U>` pins `T := Box<U>` — still variable-bearing, so
        // the bound cannot refute in an open world.
        let vars = params(&["T", "U"]);
        let mut bounds = TypeVarBoundsMap::default();
        bounds.insert(param("T"), vec![constraint("I")]);
        assert_eq!(
            pattern_overlap_with(
                &Ty::type_var("T"),
                &class1("Box", Ty::type_var("U")),
                &vars,
                &bounds,
                &std::collections::HashMap::default(),
                &|_, _| false,
            ),
            Overlap::Yes
        );
    }

    #[test]
    fn pattern_overlap_bounded_var_cannot_realize_to_union_or_existential() {
        // Bounded params must realize to *concrete* types (TYPE_SYSTEM.md, Generics
        // on Functions), so a witness pinned to a union or an interface-existential
        // refutes structurally — even with the registry answering yes to everything.
        let vars = params(&["T"]);
        let mut bounds = TypeVarBoundsMap::default();
        bounds.insert(param("T"), vec![constraint("I")]);
        let aliases = std::collections::HashMap::default();
        let always = |_: &Ty, _: &Interface| true;
        assert_eq!(
            pattern_overlap_with(
                &class1("Box", Ty::type_var("T")),
                &class1("Box", Ty::union([Ty::int(), Ty::string()])),
                &vars,
                &bounds,
                &aliases,
                &always,
            ),
            Overlap::No
        );
        assert_eq!(
            pattern_overlap_with(
                &class1("Box", Ty::type_var("T")),
                &class1("Box", interface("J", vec![])),
                &vars,
                &bounds,
                &aliases,
                &always,
            ),
            Overlap::No
        );
    }

    #[test]
    fn pattern_overlap_union_cover_binding_is_not_refuted() {
        // `Box<T[] | int>` vs `Box<string[] | int>` unifies via the union-covering
        // search, which binds `T` to *one* of several possible witnesses; disproving
        // the bound against that arbitrary pick would be unsound, so refutation is
        // skipped for params occurring under a union (non-principality).
        let vars = params(&["T"]);
        let mut bounds = TypeVarBoundsMap::default();
        bounds.insert(param("T"), vec![constraint("I")]);
        assert_eq!(
            pattern_overlap_with(
                &class1("Box", Ty::union([Ty::list(Ty::type_var("T")), Ty::int()])),
                &class1("Box", Ty::union([Ty::list(Ty::string()), Ty::int()])),
                &vars,
                &bounds,
                &std::collections::HashMap::default(),
                &|_, _| false,
            ),
            Overlap::Yes
        );
    }

    #[test]
    fn pattern_overlap_membership_refutes_only_fully_realized_pairs() {
        // Concrete-vs-existential is membership: the registry may refute a ground
        // class, but a var-bearing side stays possible whatever the registry says.
        let vars = params(&["T"]);
        let bounds = TypeVarBoundsMap::default();
        let aliases = std::collections::HashMap::default();
        let iface = interface("I", vec![]);
        assert_eq!(
            pattern_overlap_with(
                &iface,
                &Ty::class("Foo"),
                &vars,
                &bounds,
                &aliases,
                &|_, _| false
            ),
            Overlap::No
        );
        assert_eq!(
            pattern_overlap_with(
                &iface,
                &Ty::class("Foo"),
                &vars,
                &bounds,
                &aliases,
                &|_, _| true
            ),
            Overlap::Yes
        );
        assert_eq!(
            pattern_overlap_with(
                &iface,
                &class1("Box", Ty::type_var("T")),
                &vars,
                &bounds,
                &aliases,
                &|_, _| false
            ),
            Overlap::Yes
        );
    }

    #[test]
    fn pattern_overlap_existential_pairs_are_always_possible() {
        // Two existentials may always share a future common implementor (the orphan
        // rule closes the world only for already-loaded pairs) — including same-name
        // existentials with different args, since one type may implement both
        // `I<int>` and `I<string>` coherently.
        let vars = params(&[]);
        assert_eq!(
            pattern_overlap_plain(
                &interface("I", vec![Ty::int()]),
                &interface("I", vec![Ty::string()]),
                &vars
            ),
            Overlap::Yes
        );
        assert_eq!(
            pattern_overlap_plain(&interface("I", vec![]), &interface("J", vec![]), &vars),
            Overlap::Yes
        );
    }

    #[test]
    fn pattern_overlap_function_types_meet_conservatively_at_top_level() {
        // Function types are the structural-subtyping exception, so two non-equal
        // function types can share values (a common structural subtype) — but at an
        // invariant constructor argument the relation is equality, so `Box<fn>`s
        // with different component types stay disjoint.
        let vars = params(&[]);
        assert_eq!(
            pattern_overlap_plain(&fn_returning(Ty::int()), &fn_returning(Ty::string()), &vars),
            Overlap::Yes
        );
        assert_eq!(
            pattern_overlap_plain(
                &class1("Box", fn_returning(Ty::int())),
                &class1("Box", fn_returning(Ty::string())),
                &vars
            ),
            Overlap::No
        );
    }

    #[test]
    fn pattern_overlap_alias_under_union_decomposes() {
        // A union member that is itself an alias to a union must decompose through
        // the alias — union *equality* (covering) on the raw pair would wrongly
        // answer `No` for `string` vs `A = int | string`.
        let vars = params(&[]);
        let mut aliases = std::collections::HashMap::default();
        aliases.insert(
            TypeName::local(Name::new("A")),
            Ty::union([Ty::int(), Ty::string()]),
        );
        assert_eq!(
            pattern_overlap_with(
                &Ty::string(),
                &Ty::union([type_alias("A"), Ty::class("Foo")]),
                &vars,
                &TypeVarBoundsMap::default(),
                &aliases,
                &|_, _| false,
            ),
            Overlap::Yes
        );
    }

    #[test]
    fn pattern_overlap_cyclic_alias_degrades_to_unknown() {
        // Mutually-recursive union aliases can neither be proven overlapping nor
        // disjoint by finite expansion; the depth cap degrades to `Unknown`
        // (possible — the sound direction), never a hang or a wrong `No`.
        let vars = params(&[]);
        let mut aliases = std::collections::HashMap::default();
        aliases.insert(
            TypeName::local(Name::new("A")),
            Ty::union([Ty::int(), type_alias("B")]),
        );
        aliases.insert(
            TypeName::local(Name::new("B")),
            Ty::union([Ty::bool(), type_alias("A")]),
        );
        assert_eq!(
            pattern_overlap_with(
                &Ty::string(),
                &type_alias("A"),
                &vars,
                &TypeVarBoundsMap::default(),
                &aliases,
                &|_, _| false,
            ),
            Overlap::Unknown
        );
    }

    #[test]
    fn pattern_overlap_normalizes_inputs_before_comparing() {
        // `nf` folds finite bases inside invariant args (`true | false` → `bool`),
        // so the spelled-out and folded forms are the same type, not a mismatch.
        let vars = params(&[]);
        assert_eq!(
            pattern_overlap_plain(
                &class1("Bar", Ty::union([bool_literal(true), bool_literal(false)])),
                &class1("Bar", Ty::bool()),
                &vars
            ),
            Overlap::Yes
        );
    }
}
