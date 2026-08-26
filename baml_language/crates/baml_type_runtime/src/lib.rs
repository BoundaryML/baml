//! Shared generic type-variable inference and union normalization.
//!
//! These are the pure `Ty`-walking primitives behind generic-call inference.
//! They live in this dedicated crate — which depends only on the type
//! vocabulary (`baml_type`), neither the compiler frontend nor the BEX engine —
//! so that:
//!
//! - the compiler (`baml_compiler2_hir_ty`) uses them at *compile time* over
//!   typed expressions, re-exporting them so its callers are unchanged; and
//! - the runtime engine (`bex_engine`) uses them at the *inbound boundary* over
//!   types synthesized from argument values, by widening its
//!   [`baml_type::RuntimeTy`] up to [`Ty`] (`From`), running the shared unifier,
//!   and narrowing each resulting binding back down (`TryFrom`).
//!
//! Keeping one algorithm here removes the hand-maintained `RuntimeTy` port the
//! runtime engine previously carried, without a runtime → compiler dependency
//! edge, and lets a crate that needs only the `Ty`/`RuntimeTy` *definitions*
//! depend on `baml_type` without pulling in inference (see
//! `02a-inbound-inference-generics.md` and `01c-inbound-inference-reuse.md`).
//!
//! Only the *pure* helpers belong here. Anything needing the compiler database,
//! `TypeExpr`, or `TirTypeError` (e.g. `lower_type_expr_with_generics`,
//! `erase_unresolved_typevars`) stays compiler-side.

#[cfg(test)]
use baml_type::Name;
#[expect(
    deprecated,
    reason = "fact-free by necessity: inference runs at the host entry boundary (no VM exists yet) and shares its lattice with compile-time inference — both sides must gain real fact contexts in lockstep"
)]
use baml_type::normalize::NoFacts;
use baml_type::{FunctionParamTy, ParamTy, Ty, TyAttr, normalize::TypeContext};
use rustc_hash::FxHashMap;

// ── Inference options ─────────────────────────────────────────────────────────

/// Knobs that distinguish the compile-time and runtime inference variants. The
/// structural recursion is identical; only the leaf decisions differ.
#[derive(Clone, Copy)]
struct InferOpts<'a> {
    /// Bind a `TypeVar` formal even when the actual is itself a `TypeVar`.
    /// Compile-time callable-summary paths opt in; ordinary inference does not
    /// (a `TypeVar` actual carries no concrete information).
    allow_typevar_actuals: bool,
    /// A *rigid* type variable that must never be bound from an argument — the
    /// pinned `Self` of an interface method call. `None` = no rigid variable.
    rigid: Option<&'a ParamTy>,
}

impl InferOpts<'_> {
    const COMPILE_TIME: Self = InferOpts {
        allow_typevar_actuals: false,
        rigid: None,
    };
}

// ── Variance ────────────────────────────────────────────────────────────────

/// The variance of the position a `TypeVar` occurrence is reached through, which
/// determines how repeat bindings of the *same* variable must be combined
/// (`02d`/`02e`):
///
/// | variance      | reached via                                | combine repeats by |
/// |---------------|--------------------------------------------|---------------------|
/// | `Covariant`   | bare arg, function return / throws         | join (lower bound)  |
/// | `Contravariant` | a function **parameter** position        | meet (upper bound)  |
/// | `Invariant`   | a `List`/`Map`/`Class`/`Future` argument   | rigid equality      |
///
/// The shared unifier descends the formal type tracking this; the leaf records
/// each `(variance, actual)` so the [solver](InferenceConstraints::solve) can
/// detect a position whose occurrences have no consistent solution and *reject*
/// the call, instead of fabricating a union the way an unconditional join does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Variance {
    Covariant,
    Contravariant,
    Invariant,
}

impl Variance {
    /// Compose the current variance with the variance of a position we are about
    /// to descend into. Invariance is absorbing (anything under an invariant
    /// constructor is invariant); descending into a function parameter flips
    /// covariant ↔ contravariant, so a nested `((T) -> _) -> _` makes `T`
    /// doubly-contravariant, i.e. covariant again.
    fn compose(self, inner: Variance) -> Variance {
        match (self, inner) {
            (Variance::Invariant, _) | (_, Variance::Invariant) => Variance::Invariant,
            (Variance::Covariant, v) => v,
            (Variance::Contravariant, Variance::Covariant) => Variance::Contravariant,
            (Variance::Contravariant, Variance::Contravariant) => Variance::Covariant,
        }
    }
}

// ── Constraint accumulation ───────────────────────────────────────────────────

/// An inference failure: a `TypeVar` whose recorded occurrences admit no
/// consistent binding (the `02d`/`02e` reject cases). Runtime-only for now —
/// the message is surfaced as an engine `TypeMismatch`, not (yet) a compiler
/// diagnostic (see `03c-impl-guide`).
#[derive(Clone, Debug)]
pub struct InferError {
    pub var: ParamTy,
    pub message: String,
}

/// The per-`TypeVar` occurrences gathered while walking one or more
/// formal/actual pairs, each tagged with the [`Variance`] of the position it was
/// reached through, in encounter order. Drive it across every argument of a call
/// (one [`record`](Self::record) per arg), then [`solve`](Self::solve) once so
/// conflicting occurrences across *different* arguments are caught.
#[derive(Default)]
pub struct InferenceConstraints {
    vars: FxHashMap<ParamTy, Vec<(Variance, Ty)>>,
}

impl InferenceConstraints {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record the occurrences of every `TypeVar` in `formal` against `actual`,
    /// starting from a covariant root. Uses the runtime leaf decisions (a
    /// `TypeVar` actual / `Unknown` carries no information).
    pub fn record(&mut self, formal: &Ty, actual: &Ty) {
        collect(
            formal,
            actual,
            Variance::Covariant,
            &mut self.vars,
            InferOpts::COMPILE_TIME,
        );
    }

    fn record_with(&mut self, formal: &Ty, actual: &Ty, opts: InferOpts<'_>) {
        collect(formal, actual, Variance::Covariant, &mut self.vars, opts);
    }

    /// Best-effort bindings, **ignoring variance** — every occurrence of a var
    /// is union-joined, exactly as the unifier did before variance tracking.
    /// This is the compile-time path: it preserves today's behavior and leans on
    /// the variance-aware downstream subtyping checks to reject the unsound
    /// joins. The runtime path uses [`solve`](Self::solve) instead.
    fn solve_best_effort(&self) -> FxHashMap<ParamTy, Ty> {
        let mut out = FxHashMap::default();
        for (name, occ) in &self.vars {
            let mut acc: Option<Ty> = None;
            for (_, ty) in occ {
                acc = Some(match acc {
                    None => ty.clone(),
                    Some(prev) => union_ty(&prev, ty),
                });
            }
            if let Some(ty) = acc {
                out.insert(name.clone(), ty);
            }
        }
        out
    }

    /// Variance-aware resolution (`02d`/`02e`). For each `TypeVar`, partition its
    /// occurrences into covariant **lower bounds** (combined by join),
    /// contravariant **upper bounds** (combined by meet), and invariant
    /// **equality** constraints (which must be rigidly equal). Require
    /// `join(lowers) <: T <: meet(uppers)` and every equality member mutually
    /// equal and consistent with the bounds; otherwise the var has no solution
    /// and the whole inference fails. On success, returns the resolved bindings.
    pub fn solve(&self) -> Result<FxHashMap<ParamTy, Ty>, InferError> {
        let mut out = FxHashMap::default();
        for (name, occ) in &self.vars {
            if let Some(ty) = solve_var(name, occ)? {
                out.insert(name.clone(), ty);
            }
        }
        Ok(out)
    }
}

/// Resolve one `TypeVar`'s recorded occurrences to a single binding, or fail.
#[expect(
    deprecated,
    reason = "fact-free by necessity: this solver runs at the host entry boundary (no VM exists yet) and shares its lattice with compile-time inference — both sides must gain real fact contexts in lockstep"
)]
fn solve_var(param: &ParamTy, occ: &[(Variance, Ty)]) -> Result<Option<Ty>, InferError> {
    let mut lowers: Vec<&Ty> = Vec::new();
    let mut uppers: Vec<&Ty> = Vec::new();
    let mut equals: Vec<&Ty> = Vec::new();
    for (variance, ty) in occ {
        match variance {
            Variance::Covariant => lowers.push(ty),
            Variance::Contravariant => uppers.push(ty),
            Variance::Invariant => equals.push(ty),
        }
    }

    let fail = |msg: String| {
        Err(InferError {
            var: param.clone(),
            message: msg,
        })
    };

    // Invariant occurrences must be rigidly equal to one another (canonical
    // equivalence: coercion-free, union-order-insensitive).
    let rigid: Option<Ty> = match equals.split_first() {
        None => None,
        Some((first, rest)) => {
            for other in rest {
                if !NoFacts.equivalent(first, other) {
                    return fail(format!(
                        "`{param}` would have to be both `{first}` and `{other}` at the same \
                         time. Because `{param}` appears inside a list, map, or class type, it \
                         has to be exactly the same type in every argument."
                    ));
                }
            }
            Some((*first).clone())
        }
    };

    let lower = join_all(&lowers);
    let upper = meet_all(param, &uppers)?;

    match rigid {
        Some(eq) => {
            // T == eq; every lower must be <: eq and eq <: every upper.
            if let Some(l) = &lower
                && !NoFacts.is_subtype(l, &eq)
            {
                return fail(format!(
                    "`{param}` would have to be both `{eq}` and `{l}` at the same time: one \
                     argument fixes `{param}` to `{eq}` (where it appears inside a list, \
                     map, or class type), while another supplies a `{l}`."
                ));
            }
            for u in &uppers {
                if !NoFacts.is_subtype(&eq, u) {
                    return fail(format!(
                        "one argument fixes `{param}` to `{eq}` (where it appears inside a list, \
                         map, or class type), but a function argument only accepts `{u}` for \
                         `{param}`, and a `{eq}` is not a `{u}`."
                    ));
                }
            }
            Ok(Some(eq))
        }
        None => match (lower, upper) {
            (Some(l), Some(u)) => {
                if !NoFacts.is_subtype(&l, &u) {
                    return fail(format!(
                        "`{param}` can't satisfy every argument at once: one argument supplies a \
                         `{l}` for `{param}`, while a function argument only accepts `{u}` for \
                         `{param}`, and a `{l}` is not a `{u}`."
                    ));
                }
                Ok(Some(l))
            }
            (Some(l), None) => Ok(Some(l)),
            // Contravariant-only: bind to the meet of the upper bounds. An empty
            // meet (`Never`) means the parameter positions disagree irreconcilably.
            (None, Some(u)) => Ok(Some(u)),
            (None, None) => Ok(None),
        },
    }
}

/// Join a set of lower bounds into a single type (their union), or `None` if empty.
fn join_all(tys: &[&Ty]) -> Option<Ty> {
    let mut acc: Option<Ty> = None;
    for ty in tys {
        acc = Some(match acc {
            None => (*ty).clone(),
            Some(prev) => union_ty(&prev, ty),
        });
    }
    acc
}

/// Meet a set of upper bounds. Returns `None` if empty. Fails if the meet
/// collapses to `Never` (irreconcilable contravariant occurrences, e.g. a `T`
/// required to be `<: int` *and* `<: string`).
fn meet_all(param: &ParamTy, tys: &[&Ty]) -> Result<Option<Ty>, InferError> {
    let mut acc: Option<Ty> = None;
    for ty in tys {
        acc = Some(match acc {
            None => (*ty).clone(),
            Some(prev) => meet_ty(&prev, ty),
        });
    }
    if let Some(m) = &acc
        && matches!(m, Ty::Never { .. })
    {
        return Err(InferError {
            var: param.clone(),
            message: format!(
                "`{param}` can't satisfy every argument at once: two function arguments \
                 accept incompatible types for `{param}`, with no type in common."
            ),
        });
    }
    Ok(acc)
}

/// The meet (greatest lower bound) of two types, using the canonical coercion-free
/// subtype relation. For comparable types it is the narrower one; for unrelated
/// types it is `Never` (no common subtype) — which the solver reads as an
/// irreconcilable conflict.
#[expect(
    deprecated,
    reason = "fact-free by necessity — see the `NoFacts` import note"
)]
fn meet_ty(a: &Ty, b: &Ty) -> Ty {
    if NoFacts.is_subtype(a, b) {
        a.clone()
    } else if NoFacts.is_subtype(b, a) {
        b.clone()
    } else {
        Ty::Never {
            attr: TyAttr::default(),
        }
    }
}

// ── Type variable inference ────────────────────────────────────────────────

/// Walk formal and actual types in parallel, recording each `TypeVar`
/// occurrence with the [`Variance`] of the position it was reached through.
///
/// When `formal` is `Ty::TypeVar("T", _)` and `actual` is `Ty::Int { .. }`,
/// records `(variance, int)` for `T`. For structural types, recurses into
/// matching structures, composing the current variance with the position's own
/// variance (function parameters flip, container arguments go invariant). The
/// solver later combines the recorded occurrences per their variance.
fn collect(
    formal: &Ty,
    actual: &Ty,
    variance: Variance,
    vars: &mut FxHashMap<ParamTy, Vec<(Variance, Ty)>>,
    opts: InferOpts<'_>,
) {
    match (formal, actual) {
        (Ty::TypeVar(name, _), actual_ty) => {
            if opts.rigid == Some(name) {
                return;
            }
            // Skip TypeVar-to-TypeVar bindings by default — they usually provide
            // no information for ordinary call inference. Some higher-order
            // callable-summary paths opt into preserving them explicitly.
            if !opts.allow_typevar_actuals && matches!(actual_ty, Ty::TypeVar(_, _)) {
                return;
            }
            vars.entry(name.clone())
                .or_default()
                .push((variance, actual_ty.clone()));
        }
        // Containers are invariant: descend with `Invariant` so two conflicting
        // occurrences of the same var under a container reject rather than join
        // (`02e`: `pair<T>(a: T[], b: T[])` over `int[]`/`string[]`).
        (Ty::List(f, _), Ty::List(a, _)) => {
            collect(f, a, variance.compose(Variance::Invariant), vars, opts);
        }
        (
            Ty::Map {
                key: fk, value: fv, ..
            },
            Ty::Map {
                key: ak, value: av, ..
            },
        ) => {
            let inv = variance.compose(Variance::Invariant);
            collect(fk, ak, inv, vars, opts);
            collect(fv, av, inv, vars, opts);
        }
        (Ty::Union(_, _), _) if nullable_non_null_part(formal).is_some() => {
            let formal_inner = nullable_non_null_part(formal).expect("checked above");
            let actual_inner = nullable_non_null_part(actual).unwrap_or_else(|| actual.clone());
            collect(&formal_inner, &actual_inner, variance, vars, opts);
        }
        // Equal-length union ↔ union member pairing, for formals with NO
        // *direct* `TypeVar` member (a nested one — `List<T> | int` — is what
        // this arm serves; direct ones defer to the residual/ambiguity arm
        // below). A union denotes a member *set*, and an actual's spelling
        // reaches here in whatever order the source or a canonicalization
        // produced — so each formal member binds against the single actual
        // member its HEAD corresponds to (same class/interface name, same
        // structural constructor), never by position: `Opt<T> | Non` must
        // bind `T := Shape` from `Non | Opt<Shape>` exactly as it does from
        // `Opt<Shape> | Non`. A formal member with zero correspondents is
        // unwitnessed by this argument; one with several has no principled
        // pairing — either way it binds nothing, leaving its vars to other
        // arguments or the caller's unresolved-type-arg error rather than
        // guessing by member order.
        (Ty::Union(f_members, _), Ty::Union(a_members, _))
            if f_members.len() == a_members.len()
                && !f_members.iter().any(|m| matches!(m, Ty::TypeVar(_, _))) =>
        {
            for formal_member in f_members {
                let mut correspondents = a_members
                    .iter()
                    .filter(|a_member| heads_correspond(formal_member, a_member));
                if let (Some(actual_member), None) = (correspondents.next(), correspondents.next())
                {
                    collect(formal_member, actual_member, variance, vars, opts);
                }
            }
        }
        // A union formal carrying a `TypeVar` beside concrete members — e.g.
        // `T | string | null` (after the nullable arm above peels `null`, this
        // catches the `T | string` vs concrete recursion). Route the actual to
        // the single `TypeVar` member after *subtracting* the concrete siblings
        // it already satisfies. This is the `02a` "G5 reversal": union-with-
        // concrete-sibling solving, now in scope for BOTH compile-time and
        // runtime inference, mirroring the pyright/TypeScript "subtract the
        // matched constituents, assign the remainder to the free type variable"
        // algorithm — restricted to a *single* `TypeVar` member, since `>1`
        // (e.g. `T | U | string`) has no principled split and is left unbound.
        (Ty::Union(f_members, _), _)
            if f_members.iter().any(|m| matches!(m, Ty::TypeVar(_, _))) =>
        {
            let tv_members: Vec<&Ty> = f_members
                .iter()
                .filter(|m| matches!(m, Ty::TypeVar(_, _)))
                .collect();
            // Only an unambiguous single `TypeVar` member has a reasonable
            // candidate. More than one ⇒ ambiguous ⇒ bind nothing.
            if let [tv] = tv_members.as_slice() {
                let concrete: Vec<&Ty> = f_members
                    .iter()
                    .filter(|m| !matches!(m, Ty::TypeVar(_, _)))
                    .collect();
                // The residual is every actual atom NOT already explained by a
                // concrete sibling (coercion-free subtype — `int` is NOT
                // absorbed by a `float` sibling; see `covers`).
                let residual: Vec<Ty> = union_atoms(actual)
                    .into_iter()
                    .filter(|atom| !concrete.iter().any(|c| covers(c, atom)))
                    .collect();
                // Empty residual ⇒ the actual is fully explained by concrete
                // siblings (e.g. `tag_or_value("hi")`) ⇒ bind nothing; `T` is
                // unconstrained here and Gate A governs. Otherwise bind the
                // single `TypeVar` to the (union-merged) residual via the leaf
                // arm, which honors `rigid`/`allow_typevar_actuals`.
                if !residual.is_empty() {
                    let residual_ty = normalize_union_members(residual, TyAttr::default());
                    collect(tv, &residual_ty, variance, vars, opts);
                }
            }
        }
        (
            Ty::Function {
                params: fp,
                ret: fr,
                throws: fth,
                ..
            },
            Ty::Function {
                params: ap,
                ret: ar,
                throws: ath,
                ..
            },
        ) => {
            // Parameters are contravariant; return and throws are covariant.
            let param_variance = variance.compose(Variance::Contravariant);
            for (fp, ap) in fp.iter().zip(ap.iter()) {
                collect(&fp.ty, &ap.ty, param_variance, vars, opts);
            }
            collect(fr, ar, variance, vars, opts);
            collect(fth, ath, variance, vars, opts);
        }
        (Ty::Class(fn_name, f_args, _), Ty::Class(an_name, a_args, _)) if fn_name == an_name => {
            let inv = variance.compose(Variance::Invariant);
            for (ft, at) in f_args.iter().zip(a_args.iter()) {
                collect(ft, at, inv, vars, opts);
            }
        }
        // `Future<T, E>` is its own variant — descend into both params so the
        // future combinators can infer `<T, E>` from a `Future<T, E>[]` arg.
        (Ty::Future(f_value, f_error, _), Ty::Future(a_value, a_error, _)) => {
            let inv = variance.compose(Variance::Invariant);
            collect(f_value, a_value, inv, vars, opts);
            collect(f_error, a_error, inv, vars, opts);
        }
        // A heterogeneous future array — e.g. `[spawn { 1 }, spawn { 2 }]` —
        // types as `(Future<A, EA> | Future<B, EB>)[]` because `Future` is
        // invariant. Match the `Future<T, E>` formal against each union member
        // so `T`/`E` bind to the union of the member value/error types. This is
        // the *deliberate* distribution arm (`02e` E6): kept as a confined
        // combinator special case, so it joins (covariant) rather than going
        // through the invariant equality path.
        (Ty::Future(_, _, _), Ty::Union(members, _)) => {
            for member in members {
                collect(formal, member, variance, vars, opts);
            }
        }
        (
            Ty::Interface(fn_name, f_args, f_assoc, _),
            Ty::Interface(an_name, a_args, a_assoc, _),
        ) if fn_name == an_name => {
            let inv = variance.compose(Variance::Invariant);
            for (ft, at) in f_args.iter().zip(a_args.iter()) {
                collect(ft, at, inv, vars, opts);
            }
            for (formal_name, formal_ty) in f_assoc {
                if let Some((_, actual_ty)) = a_assoc
                    .iter()
                    .find(|(actual_name, _)| actual_name == formal_name)
                {
                    collect(formal_ty, actual_ty, inv, vars, opts);
                }
            }
        }
        _ => {} // Concrete types: nothing to infer
    }
}

fn nullable_non_null_part(ty: &Ty) -> Option<Ty> {
    let Ty::Union(members, attr) = ty else {
        return None;
    };
    if !members.iter().any(Ty::is_null) {
        return None;
    }
    let non_null: Vec<Ty> = members
        .iter()
        .filter(|member| !member.is_null())
        .cloned()
        .collect();
    match non_null.as_slice() {
        [] => None,
        [single] => Some(single.clone()),
        _ => Some(Ty::Union(non_null.into(), attr.clone())),
    }
}

/// Whether a formal union member and an actual union member denote the same
/// head constructor — the pairing key for order-insensitive union ↔ union
/// inference. Nominal heads must name the same type; structural heads pair by
/// constructor alone. Heads `collect` cannot descend into (primitives,
/// literals, enums, variants — its catch-all no-op) never pair: collecting
/// them binds nothing, so pairing one would only steal the slot from a
/// genuine correspondent.
fn heads_correspond(formal: &Ty, actual: &Ty) -> bool {
    match (formal, actual) {
        (Ty::Class(f, ..), Ty::Class(a, ..)) | (Ty::Interface(f, ..), Ty::Interface(a, ..)) => {
            f == a
        }
        (Ty::List(..), Ty::List(..))
        | (Ty::Map { .. }, Ty::Map { .. })
        | (Ty::Function { .. }, Ty::Function { .. })
        | (Ty::Future(..), Ty::Future(..)) => true,
        _ => false,
    }
}

/// Decompose an actual type into its union atoms: a `Union` contributes each of
/// its members (one level); anything else is a single atom. Used by the
/// union-with-`TypeVar`-member inference arm to subtract concrete siblings atom
/// by atom.
fn union_atoms(actual: &Ty) -> Box<[Ty]> {
    match actual {
        Ty::Union(members, _) => members.clone(),
        other => Box::new([other.clone()]),
    }
}

/// Whether a concrete (non-`TypeVar`) union-formal member already explains an
/// actual `atom` — i.e. the atom is a *coercion-free* subtype of the sibling.
/// The canonical subtype relation is intentionally free of numeric widening
/// (`int` is NOT covered by a `float` sibling) and admits only same-
/// representation widenings (literal → primitive, union membership). This keeps
/// the subtraction consistent with the TIR's runtime-tag-identity match
/// dispatch (`builder.rs::atoms_overlap`).
#[expect(
    deprecated,
    reason = "fact-free by necessity — see the `NoFacts` import note"
)]
fn covers(concrete: &Ty, atom: &Ty) -> bool {
    NoFacts.is_subtype(atom, concrete)
}

/// Merge a fresh best-effort solve into an existing bindings map, unioning with
/// any binding already present — preserving the cross-call accumulation the old
/// `&mut`-threaded unifier provided (callers invoke it once per argument).
fn merge_best_effort(bindings: &mut FxHashMap<ParamTy, Ty>, cons: &InferenceConstraints) {
    for (name, ty) in cons.solve_best_effort() {
        bindings
            .entry(name)
            .and_modify(|existing| *existing = union_ty(existing, &ty))
            .or_insert(ty);
    }
}

pub fn infer_bindings(formal: &Ty, actual: &Ty, bindings: &mut FxHashMap<ParamTy, Ty>) {
    let mut cons = InferenceConstraints::new();
    cons.record_with(formal, actual, InferOpts::COMPILE_TIME);
    merge_best_effort(bindings, &cons);
}

pub fn infer_bindings_allow_typevars(
    formal: &Ty,
    actual: &Ty,
    bindings: &mut FxHashMap<ParamTy, Ty>,
) {
    let mut cons = InferenceConstraints::new();
    cons.record_with(
        formal,
        actual,
        InferOpts {
            allow_typevar_actuals: true,
            ..InferOpts::COMPILE_TIME
        },
    );
    merge_best_effort(bindings, &cons);
}

/// Like [`infer_bindings`] but treats `rigid` (when `Some`) as a rigid type
/// variable that is never bound from an argument — the pinned `Self` of an
/// interface method call. Every other variable infers exactly as before.
pub fn infer_bindings_rigid_self(
    formal: &Ty,
    actual: &Ty,
    bindings: &mut FxHashMap<ParamTy, Ty>,
    rigid: Option<&ParamTy>,
) {
    let mut cons = InferenceConstraints::new();
    cons.record_with(
        formal,
        actual,
        InferOpts {
            rigid,
            ..InferOpts::COMPILE_TIME
        },
    );
    merge_best_effort(bindings, &cons);
}

/// The runtime value-inference variant (01a/01b): solve a generic call's
/// `TypeVar`s from types synthesized from argument *values*. Uses the same leaf
/// decisions as [`infer_bindings`] — a `Class` arm binds only when the formal
/// and actual name the same class, and the top type carries no special-case
/// skip. The runtime engine reaches this by widening its `RuntimeTy` inputs to
/// `Ty` and narrowing the resulting bindings back.
///
/// This is the *best-effort* (variance-ignoring) merge, kept for callers that
/// solve one argument at a time. Callers that want the variance-aware reject
/// (`02d`/`02e`) should accumulate an [`InferenceConstraints`] across all
/// arguments and call [`InferenceConstraints::solve`].
pub fn infer_value_bindings(formal: &Ty, actual: &Ty, bindings: &mut FxHashMap<ParamTy, Ty>) {
    let mut cons = InferenceConstraints::new();
    cons.record(formal, actual);
    merge_best_effort(bindings, &cons);
}

// ── Union normalization ─────────────────────────────────────────────────────

/// Combine two types into a union, deduplicating members.
///
/// Used when the same type variable is inferred from multiple arguments
/// (e.g., `pair<T>(myInt, myString)` → `T` gets `int` then `string`).
pub fn union_ty(a: &Ty, b: &Ty) -> Ty {
    normalize_union_members([a.clone(), b.clone()], TyAttr::default())
}

// ── Pure `Ty` walks (substitution, typevar queries, erasure) ──────────────────
//
// Moved from `baml_compiler2_tir::generics` (which re-exports them) during the
// S16 TIR retirement: they are pure walks over the shared vocabulary with no
// compiler-database dependence, exactly this crate's charter.
pub use baml_type::unify::{bind_type_vars, normalize_union_members, substitute_ty};

/// Deep any-node predicate over a type tree: does `pred` hold for `ty` itself
/// or for any type nested inside it? The single traversal behind the
/// `contains_*` family; the arms cover every child position a `Ty` can carry.
/// A function type carries no generic binders of its own (function values are
/// realized), so recursion enters its params/ret/throws with `pred` unchanged;
/// a projection is entered through both its base (`T::Item`) and its
/// qualifying interface's types.
pub fn contains_ty_where(ty: &Ty, pred: &dyn Fn(&Ty) -> bool) -> bool {
    if pred(ty) {
        return true;
    }
    match ty {
        Ty::AssociatedTypeProjection {
            base, interface, ..
        } => contains_ty_where(base, pred) || interface.tys().any(|t| contains_ty_where(t, pred)),
        Ty::List(inner, _) => contains_ty_where(inner, pred),
        Ty::Map {
            key: k, value: v, ..
        } => contains_ty_where(k, pred) || contains_ty_where(v, pred),
        Ty::Union(tys, _) => tys.iter().any(|t| contains_ty_where(t, pred)),
        Ty::Future(value, error, _) => {
            contains_ty_where(value, pred) || contains_ty_where(error, pred)
        }
        Ty::Function {
            params,
            ret,
            throws,
            ..
        } => {
            params
                .iter()
                .any(|param| contains_ty_where(&param.ty, pred))
                || contains_ty_where(ret, pred)
                || contains_ty_where(throws, pred)
        }
        Ty::Class(_, type_args, _) => type_args.iter().any(|t| contains_ty_where(t, pred)),
        Ty::Interface(_, type_args, associated_bindings, _) => {
            type_args.iter().any(|t| contains_ty_where(t, pred))
                || associated_bindings
                    .iter()
                    .any(|(_, ty)| contains_ty_where(ty, pred))
        }
        _ => false,
    }
}

/// Check if a type contains any `Ty::TypeVar` anywhere in its structure.
pub fn contains_typevar(ty: &Ty) -> bool {
    contains_ty_where(ty, &|t| matches!(t, Ty::TypeVar(_, _)))
}

/// Does `ty` carry an error-recovery sentinel (`Ty::Error`)
/// anywhere in its structure? An expression recorded with such a type already
/// failed to compile at its own site; downstream consumers (e.g. call-site
/// generic inference) use this to recognize an already-failed input and avoid
/// cascading a second diagnostic off it.
pub fn contains_error_recovery(ty: &Ty) -> bool {
    contains_ty_where(ty, &|t| matches!(t, Ty::Error { .. }))
}

/// Returns `true` if `ty` contains any type variable for which `pred` returns
/// `true`. A general form of [`contains_typevar`] used by call validation to
/// distinguish *rigid* type variables (the pinned `Self`, caller-scope generic
/// params) — which must be checked — from genuinely-uninferred ones (callee
/// generics, free inference/effect vars) — which are deferred.
pub fn contains_typevar_where(ty: &Ty, pred: &dyn Fn(&ParamTy) -> bool) -> bool {
    contains_ty_where(ty, &|t| matches!(t, Ty::TypeVar(param, _) if pred(param)))
}

/// Replace selected type variables with `unknown` for runtime-facing metadata.
///
/// Bounded generic parameters are compile-time evidence, not concrete runtime
/// type tags. MIR and bytecode metadata both need the same erasure behavior, so
/// keep the recursive shape walk here beside the other generic utilities.
pub fn erase_typevars_matching(ty: &Ty, should_erase: &impl Fn(&ParamTy) -> bool) -> Ty {
    if !contains_typevar(ty) {
        return ty.clone();
    }

    match ty {
        Ty::TypeVar(name, attr) if should_erase(name) => Ty::Unknown { attr: attr.clone() },
        Ty::Class(qtn, args, attr) => Ty::Class(
            qtn.clone(),
            args.iter()
                .map(|arg| erase_typevars_matching(arg, should_erase))
                .collect(),
            attr.clone(),
        ),
        Ty::Interface(qtn, args, associated_bindings, attr) => Ty::Interface(
            qtn.clone(),
            args.iter()
                .map(|arg| erase_typevars_matching(arg, should_erase))
                .collect(),
            associated_bindings
                .iter()
                .map(|(name, ty)| (name.clone(), erase_typevars_matching(ty, should_erase)))
                .collect(),
            attr.clone(),
        ),
        Ty::List(inner, attr) => Ty::List(
            Box::new(erase_typevars_matching(inner, should_erase)),
            attr.clone(),
        ),
        Ty::Map { key, value, attr } => Ty::Map {
            key: Box::new(erase_typevars_matching(key, should_erase)),
            value: Box::new(erase_typevars_matching(value, should_erase)),
            attr: attr.clone(),
        },
        Ty::Union(members, attr) => Ty::Union(
            members
                .iter()
                .map(|member| erase_typevars_matching(member, should_erase))
                .collect(),
            attr.clone(),
        ),
        Ty::Future(value, error, attr) => Ty::Future(
            Box::new(erase_typevars_matching(value, should_erase)),
            Box::new(erase_typevars_matching(error, should_erase)),
            attr.clone(),
        ),
        Ty::AssociatedTypeProjection {
            base,
            interface,
            member,
            attr,
        } => Ty::AssociatedTypeProjection {
            base: Box::new(erase_typevars_matching(base, should_erase)),
            interface: Box::new(interface.map_tys(|t| erase_typevars_matching(t, should_erase))),
            member: member.clone(),
            attr: attr.clone(),
        },
        Ty::Function {
            params,
            ret,
            throws,
            attr,
        } => Ty::Function {
            params: params
                .iter()
                .map(|param| FunctionParamTy {
                    name: param.name.clone(),
                    ty: erase_typevars_matching(&param.ty, should_erase),
                    mode: param.mode,
                })
                .collect(),
            ret: Box::new(erase_typevars_matching(ret, should_erase)),
            throws: Box::new(erase_typevars_matching(throws, should_erase)),
            attr: attr.clone(),
        },
        _ => ty.clone(),
    }
}

#[cfg(test)]
mod tests {
    use baml_type::Ty;

    use super::*;

    fn a() -> TyAttr {
        TyAttr::default()
    }

    fn param(name: &str) -> ParamTy {
        let index = match name {
            "T" => 0,
            "U" => 1,
            "E" => 2,
            _ => 0,
        };
        ParamTy::new(index, Name::new(name))
    }

    fn tv(name: &str) -> Ty {
        Ty::TypeVar(param(name), a())
    }

    fn int() -> Ty {
        Ty::Int { attr: a() }
    }

    fn string() -> Ty {
        Ty::String { attr: a() }
    }

    fn null() -> Ty {
        Ty::Null { attr: a() }
    }

    #[test]
    fn binds_bare_typevar() {
        let mut b = FxHashMap::default();
        infer_bindings(&tv("T"), &int(), &mut b);
        assert_eq!(b.get(&param("T")), Some(&int()));
    }

    #[test]
    fn union_with_concrete_sibling_routes_actual_to_typevar() {
        // `T | string | null` vs `int` ⇒ T = int (G5 reversal).
        let formal = Ty::Union(Box::new([tv("T"), string(), null()]), a());
        let mut b = FxHashMap::default();
        infer_bindings(&formal, &int(), &mut b);
        assert_eq!(b.get(&param("T")), Some(&int()));
    }

    #[test]
    fn union_concrete_sibling_absorbs_actual_leaves_typevar_unbound() {
        // `T | string | null` vs `string` ⇒ the string sibling absorbs it; T unbound.
        let formal = Ty::Union(Box::new([tv("T"), string(), null()]), a());
        let mut b = FxHashMap::default();
        infer_bindings(&formal, &string(), &mut b);
        assert!(!b.contains_key(&param("T")));
    }

    #[test]
    fn union_null_actual_binds_typevar_to_null() {
        // `T | string | null` vs `null` ⇒ T = null (null not absorbed by string sibling).
        let formal = Ty::Union(Box::new([tv("T"), string(), null()]), a());
        let mut b = FxHashMap::default();
        infer_bindings(&formal, &null(), &mut b);
        assert_eq!(b.get(&param("T")), Some(&null()));
    }

    #[test]
    fn multi_typevar_union_is_ambiguous_binds_nothing() {
        // `T | U | string` vs `int` ⇒ no principled split ⇒ both unbound.
        let formal = Ty::Union(Box::new([tv("T"), tv("U"), string()]), a());
        let mut b = FxHashMap::default();
        infer_bindings(&formal, &int(), &mut b);
        assert!(!b.contains_key(&param("T")));
        assert!(!b.contains_key(&param("U")));
    }

    fn boolt() -> Ty {
        Ty::Bool { attr: a() }
    }

    #[test]
    fn equal_len_union_actual_routes_residual_to_typevar() {
        // Regression: `T | int` vs `int | string` (both len 2). The equal-length
        // positional-zip arm must NOT fire (it would bind T = int by member
        // ordering). The residual arm routes the unmatched `string` to T.
        let formal = Ty::Union(Box::new([tv("T"), int()]), a());
        let actual = Ty::Union(Box::new([int(), string()]), a());
        let mut b = FxHashMap::default();
        infer_bindings(&formal, &actual, &mut b);
        assert_eq!(b.get(&param("T")), Some(&string()));
    }

    #[test]
    fn nullable_union_actual_routes_residual_to_typevar() {
        // Regression: `T | int | null` vs `int | string`. After the nullable arm
        // peels `null`, the recursion lands on `T | int` vs `int | string` and
        // must route `string` to T, not positionally bind T = int.
        let formal = Ty::Union(Box::new([tv("T"), int(), null()]), a());
        let actual = Ty::Union(Box::new([int(), string()]), a());
        let mut b = FxHashMap::default();
        infer_bindings(&formal, &actual, &mut b);
        assert_eq!(b.get(&param("T")), Some(&string()));
    }

    #[test]
    fn multi_typevar_equal_len_union_actual_binds_nothing() {
        // Regression: `T | U | int` vs `int | string | bool` (both len 3). The
        // equal-length zip must not pre-empt the multi-TypeVar ambiguity guard;
        // >1 TypeVar member has no principled split ⇒ both stay unbound.
        let formal = Ty::Union(Box::new([tv("T"), tv("U"), int()]), a());
        let actual = Ty::Union(Box::new([int(), string(), boolt()]), a());
        let mut b = FxHashMap::default();
        infer_bindings(&formal, &actual, &mut b);
        assert!(!b.contains_key(&param("T")));
        assert!(!b.contains_key(&param("U")));
    }

    #[test]
    fn equal_len_union_nested_typevar_binds_by_head() {
        // The equal-length union arm serves unions whose TypeVar is *nested*
        // inside a member: `List<T> | int` vs `List<int> | int` binds T = int
        // (the residual arm only sees direct TypeVar members, so without this
        // arm T would stay unbound).
        let list_tv = Ty::List(Box::new(tv("T")), a());
        let list_int = Ty::List(Box::new(int()), a());
        let formal = Ty::Union(Box::new([list_tv, int()]), a());
        let actual = Ty::Union(Box::new([list_int, int()]), a());
        let mut b = FxHashMap::default();
        infer_bindings(&formal, &actual, &mut b);
        assert_eq!(b.get(&param("T")), Some(&int()));
    }

    fn opt_of(arg: Ty) -> Ty {
        Ty::Class(
            baml_type::TypeName::local(Name::new("Opt")),
            Box::new([arg]),
            a(),
        )
    }

    fn non() -> Ty {
        Ty::Class(
            baml_type::TypeName::local(Name::new("Non")),
            Box::new([]),
            a(),
        )
    }

    #[test]
    fn union_members_pair_by_head_not_position() {
        // A union denotes a member SET: `Opt<T> | Non` must bind T from a
        // reordered actual spelling exactly as from the declaration-ordered
        // one. Positional zip paired `Opt<T>` with `Non` and bound nothing —
        // the R15 order-sensitivity a canonically sorted binding type exposed.
        let formal = Ty::Union(Box::new([opt_of(tv("T")), non()]), a());
        let reordered = Ty::Union(Box::new([non(), opt_of(int())]), a());
        let mut b = FxHashMap::default();
        infer_bindings(&formal, &reordered, &mut b);
        assert_eq!(b.get(&param("T")), Some(&int()));

        let declared = Ty::Union(Box::new([opt_of(int()), non()]), a());
        let mut b2 = FxHashMap::default();
        infer_bindings(&formal, &declared, &mut b2);
        assert_eq!(b2.get(&param("T")), Some(&int()));
    }

    #[test]
    fn union_member_with_ambiguous_head_correspondent_binds_nothing() {
        // Two same-head actual members have no principled pairing for one
        // formal member — bind nothing rather than guess by order.
        let formal = Ty::Union(Box::new([opt_of(tv("T")), non()]), a());
        let actual = Ty::Union(Box::new([opt_of(int()), opt_of(string())]), a());
        let mut b = FxHashMap::default();
        infer_bindings(&formal, &actual, &mut b);
        assert!(!b.contains_key(&param("T")));
    }

    #[test]
    fn float_sibling_does_not_absorb_int_actual() {
        // Coercion-free: `int` is NOT covered by a `float` sibling, so T = int.
        let float = Ty::Float { attr: a() };
        let formal = Ty::Union(Box::new([tv("T"), float]), a());
        let mut b = FxHashMap::default();
        infer_bindings(&formal, &int(), &mut b);
        assert_eq!(b.get(&param("T")), Some(&int()));
    }

    // ── §J variance soundness (02d / 02e) ─────────────────────────────────────
    //
    // These assert directly on the checked solver (`InferenceConstraints::solve`),
    // which the runtime drives across every argument of a call. The compile-time
    // `infer_bindings` path keeps the best-effort join (covered by the cases
    // above); the variance-aware reject is the *runtime* contract.

    use baml_type::{FunctionParamTy, TypeName};

    fn list(t: Ty) -> Ty {
        Ty::List(Box::new(t), a())
    }

    fn map_str(value: Ty) -> Ty {
        Ty::Map {
            key: Box::new(string()),
            value: Box::new(value),
            attr: a(),
        }
    }

    fn boxed(t: Ty) -> Ty {
        Ty::Class(TypeName::local(Name::new("GenericBox")), Box::new([t]), a())
    }

    fn pair_cls(first: Ty, second: Ty) -> Ty {
        Ty::Class(
            TypeName::local(Name::new("GenericPair")),
            Box::new([first, second]),
            a(),
        )
    }

    fn float() -> Ty {
        Ty::Float { attr: a() }
    }

    fn func0(ret: Ty) -> Ty {
        Ty::Function {
            params: Box::new([]),
            ret: Box::new(ret),
            throws: Box::new(null()),
            attr: a(),
        }
    }

    fn func1(param: Ty, ret: Ty) -> Ty {
        Ty::Function {
            params: Box::new([FunctionParamTy::required(None, param)]),
            ret: Box::new(ret),
            throws: Box::new(null()),
            attr: a(),
        }
    }

    /// Drive the checked solver over a list of `(formal, actual)` argument pairs.
    fn solve_call(args: &[(Ty, Ty)]) -> Result<FxHashMap<ParamTy, Ty>, InferError> {
        let mut cons = InferenceConstraints::new();
        for (formal, actual) in args {
            cons.record(formal, actual);
        }
        cons.solve()
    }

    fn get<'a>(b: &'a FxHashMap<ParamTy, Ty>, name: &str) -> Option<&'a Ty> {
        b.get(&param(name))
    }

    /// Assert a binding is a union whose members are exactly `expected` (any order).
    fn assert_union_members(ty: &Ty, expected: &[Ty]) {
        let Ty::Union(members, _) = ty else {
            panic!("expected a union, got {ty}");
        };
        assert_eq!(members.len(), expected.len(), "union {ty} arity");
        for want in expected {
            assert!(members.contains(want), "union {ty} missing {want}");
        }
    }

    // 02d — contravariant function parameters.

    #[test]
    fn j1_pipe_covariant_vs_contravariant_rejects() {
        // pipe<T>(produce: () -> T, consume: (T) -> bool):
        //   produce: () -> int   ⇒ int <: T   (covariant return — lower bound)
        //   consume: (string) -> bool ⇒ T <: string (contravariant param — upper)
        // require int <: T <: string ⇒ unsatisfiable ⇒ reject.
        let res = solve_call(&[
            (func0(tv("T")), func0(int())),
            (func1(tv("T"), boolt()), func1(string(), boolt())),
        ]);
        assert!(res.is_err(), "pipe must reject, got {res:?}");
    }

    #[test]
    fn j2_invoke_two_contravariant_meet_to_never_rejects() {
        // invoke<T>(f: (T) -> bool, g: (T) -> bool) with (int)->bool, (string)->bool:
        // two contravariant occurrences meet to int ∧ string = Never ⇒ reject.
        let res = solve_call(&[
            (func1(tv("T"), boolt()), func1(int(), boolt())),
            (func1(tv("T"), boolt()), func1(string(), boolt())),
        ]);
        assert!(res.is_err(), "invoke must reject, got {res:?}");
    }

    #[test]
    fn j3_doubly_contravariant_flips_back_to_covariant() {
        // nested<T>(a: ((T) -> bool) -> bool, b: ((T) -> bool) -> bool): a function
        // *parameter* that is itself a function makes T doubly-contravariant, i.e.
        // covariant again. Two covariant occurrences must JOIN (succeed), not meet:
        //   a: ((int) -> bool) -> bool, b: ((string) -> bool) -> bool ⇒ T = int | string.
        let formal = func1(func1(tv("T"), boolt()), boolt());
        let res = solve_call(&[
            (formal.clone(), func1(func1(int(), boolt()), boolt())),
            (formal, func1(func1(string(), boolt()), boolt())),
        ])
        .expect("doubly-contravariant T is covariant ⇒ join, not reject");
        assert_union_members(get(&res, "T").expect("T bound"), &[int(), string()]);
    }

    // 02e — invariant containers.

    #[test]
    fn j4_pair_invariant_list_conflict_rejects() {
        // pair<T>(a: T[], b: T[]) with int[], string[] ⇒ T==int and T==string ⇒ reject.
        let res = solve_call(&[
            (list(tv("T")), list(int())),
            (list(tv("T")), list(string())),
        ]);
        assert!(
            res.is_err(),
            "pair(int[], string[]) must reject, got {res:?}"
        );
    }

    #[test]
    fn j5_merge_invariant_map_value_conflict_rejects() {
        // merge<T>(a: map<string,T>, b: map<string,T>) with int / string values ⇒ reject.
        let res = solve_call(&[
            (map_str(tv("T")), map_str(int())),
            (map_str(tv("T")), map_str(string())),
        ]);
        assert!(res.is_err(), "merge must reject, got {res:?}");
    }

    #[test]
    fn j6_combine_invariant_class_arg_conflict_rejects() {
        // combine<T>(x: GenericBox<T>, y: GenericBox<T>) with Box<int>, Box<string> ⇒ reject.
        let res = solve_call(&[
            (boxed(tv("T")), boxed(int())),
            (boxed(tv("T")), boxed(string())),
        ]);
        assert!(res.is_err(), "combine must reject, got {res:?}");
    }

    #[test]
    fn j7_glue_invariant_vs_covariant_conflict_rejects() {
        // glue<T>(bare: T, arr: T[]) with int, string[]:
        //   arr ⇒ T == string (invariant), bare ⇒ int <: T (covariant);
        //   int <: string is false ⇒ reject.
        let res = solve_call(&[(tv("T"), int()), (list(tv("T")), list(string()))]);
        assert!(res.is_err(), "glue(int, string[]) must reject, got {res:?}");
    }

    #[test]
    fn j8_apply_each_contravariant_and_invariant_conflict_rejects() {
        // apply_each<T,R>(f: (T) -> R, xs: T[]) with f: (int) -> bool, xs: string[]:
        //   f ⇒ T <: int (contravariant), xs ⇒ T == string (invariant) ⇒ reject.
        let res = solve_call(&[
            (func1(tv("T"), tv("R")), func1(int(), boolt())),
            (list(tv("T")), list(string())),
        ]);
        assert!(res.is_err(), "apply_each must reject, got {res:?}");
    }

    // Regression guards — the fix narrows behavior, so these must STILL succeed.

    #[test]
    fn j9_pair_invariant_agree_binds() {
        // pair<T>(int[], int[]) ⇒ two invariant occurrences that agree ⇒ T = int.
        let res = solve_call(&[(list(tv("T")), list(int())), (list(tv("T")), list(int()))])
            .expect("agreeing invariant occurrences must bind");
        assert_eq!(get(&res, "T"), Some(&int()));
    }

    #[test]
    fn j10_choose_union_outside_container_joins() {
        // choose<T>(a: T, b: T) with int[], string[] ⇒ union OUTSIDE the container
        // (both covariant) ⇒ T = int[] | string[]. Proves the fix keys on position
        // variance, not "arrays are involved."
        let res = solve_call(&[(tv("T"), list(int())), (tv("T"), list(string()))])
            .expect("covariant occurrences must join");
        assert_union_members(
            get(&res, "T").expect("T bound"),
            &[list(int()), list(string())],
        );
    }

    #[test]
    fn j11_glue_invariant_and_covariant_agree_binds() {
        // glue<T>(int, int[]) ⇒ invariant (T==int) + covariant (int <: int) agree ⇒ T = int.
        let res = solve_call(&[(tv("T"), int()), (list(tv("T")), list(int()))])
            .expect("agreeing mixed-variance occurrences must bind");
        assert_eq!(get(&res, "T"), Some(&int()));
    }

    #[test]
    fn g3_single_invariant_occurrence_binds() {
        // read_items<T>(xs: T[]) with int[] ⇒ single invariant occurrence ⇒ T = int.
        let res = solve_call(&[(list(tv("T")), list(int()))]).expect("single occurrence binds");
        assert_eq!(get(&res, "T"), Some(&int()));
    }

    #[test]
    fn g5_plain_covariant_join_unchanged() {
        // choose<T>(5, "a") ⇒ plain covariant join ⇒ T = int | string (the §C path).
        let res =
            solve_call(&[(tv("T"), int()), (tv("T"), string())]).expect("plain covariant join");
        assert_union_members(get(&res, "T").expect("T bound"), &[int(), string()]);
    }

    // ── §B structural solving / §D covariant class-union (checked solver) ─────

    #[test]
    fn b2_second_of_recovers_typevar_from_class_arg() {
        // second_of<T>(p: GenericPair<int, T>) vs GenericPair<int, string> ⇒
        // T = string, recovered from the 2nd class arg (a single invariant
        // occurrence — no conflict).
        let res = solve_call(&[(pair_cls(int(), tv("T")), pair_cls(int(), string()))])
            .expect("class-arg recovery must succeed");
        assert_eq!(get(&res, "T"), Some(&string()));
    }

    #[test]
    fn b5_nested_class_extracts_four_vars() {
        // extract<A,B,C,D>(GenericPair<GenericPair<A,B>, GenericPair<C,D>>) over a
        // fully-concrete nested instance ⇒ all four vars zipped from the nested
        // args (each a single invariant occurrence).
        let formal = pair_cls(pair_cls(tv("A"), tv("B")), pair_cls(tv("C"), tv("D")));
        let actual = pair_cls(pair_cls(int(), string()), pair_cls(boolt(), float()));
        let res = solve_call(&[(formal, actual)]).expect("nested extraction must succeed");
        assert_eq!(get(&res, "A"), Some(&int()));
        assert_eq!(get(&res, "B"), Some(&string()));
        assert_eq!(get(&res, "C"), Some(&boolt()));
        assert_eq!(get(&res, "D"), Some(&float()));
    }

    #[test]
    fn d2_divergent_class_instances_join() {
        // choose<T>(a: T, b: T) with GenericBox<int>, GenericBox<string> ⇒ the two
        // covariant (bare-arg) occurrences join OUTSIDE the class ⇒
        // T = GenericBox<int> | GenericBox<string>. (Contrast j6_combine, where T
        // is INSIDE the box and the same actuals conflict.)
        let res = solve_call(&[(tv("T"), boxed(int())), (tv("T"), boxed(string()))])
            .expect("covariant class join must succeed");
        assert_union_members(
            get(&res, "T").expect("T bound"),
            &[boxed(int()), boxed(string())],
        );
    }

    #[test]
    fn checked_solver_agrees_with_best_effort_on_basic_inference() {
        // make_triple<A,B,C>(a: A, b: B[], c: map<string,C>) — every var single-
        // occurrence, mixed constructors — must solve identically under the checked
        // path (G6 regression guard).
        let res = solve_call(&[
            (tv("A"), int()),
            (list(tv("B")), list(string())),
            (map_str(tv("C")), map_str(boolt())),
        ])
        .expect("structural inference must succeed");
        assert_eq!(get(&res, "A"), Some(&int()));
        assert_eq!(get(&res, "B"), Some(&string()));
        assert_eq!(get(&res, "C"), Some(&boolt()));
    }
}
