//! Shared generic type-variable inference and union normalization.
//!
//! These are the pure `Ty`-walking primitives behind generic-call inference.
//! They live in this dedicated crate — which depends only on the type
//! vocabulary (`baml_type`), neither the compiler frontend nor the BEX engine —
//! so that:
//!
//! - the TIR (`baml_compiler2_tir::generics`) uses them at *compile time* over
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
//! `erase_unresolved_typevars`) stays in `baml_compiler2_tir::generics`.

use rustc_hash::FxHashMap;

use baml_type::{Name, Ty, TyAttr};

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
    rigid: Option<&'a Name>,
    /// Treat `BuiltinUnknown` (the well-formed top type) as carrying no
    /// information — skip it as an actual, exactly as the compiler-only
    /// `Unknown` is skipped. Set by runtime value-inference: a host value erased
    /// to the top type must not bind a `TypeVar` (00b3 Case 2 uses `rust_type`,
    /// never `unknown`). Compile-time inference leaves this off so a deliberate
    /// `unknown`-typed argument can still bind a type variable.
    skip_top: bool,
    /// Zip `Class` formal/actual type args without requiring the type *names* to
    /// match. Set by runtime value-inference: the synthesized instance type is
    /// ground truth and may carry a differently-qualified name than the declared
    /// parameter, so the names are ignored and the wire args are trusted
    /// (mirrors the engine's self-receiver `collect_type_var_bindings`).
    class_name_agnostic: bool,
}

impl InferOpts<'_> {
    const COMPILE_TIME: Self = InferOpts {
        allow_typevar_actuals: false,
        rigid: None,
        skip_top: false,
        class_name_agnostic: false,
    };
}

// ── Type variable inference ────────────────────────────────────────────────

/// Infer type variable bindings by walking formal and actual types in parallel.
///
/// When `formal` is `Ty::TypeVar("T", _)` and `actual` is `Ty::Int { .. }`,
/// records `T → int` in `bindings`. For structural types, recurses into
/// matching structures. Conflicting inferences are merged via [`union_ty`].
fn infer_bindings_inner(
    formal: &Ty,
    actual: &Ty,
    bindings: &mut FxHashMap<Name, Ty>,
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
            // An `Unknown` actual carries NO information: binding it (or
            // unioning it into an existing binding) only poisons the result —
            // e.g. an expected return of `SpawnParams<unknown, unknown>`
            // driving phase-0 must not turn a param-bound `T = int` into
            // `int | unknown`.
            if matches!(actual_ty, Ty::Unknown { .. }) {
                return;
            }
            // Runtime value-inference additionally treats the top type as
            // no-information (a host value erased to `unknown` binds nothing).
            if opts.skip_top && matches!(actual_ty, Ty::BuiltinUnknown { .. }) {
                return;
            }
            bindings
                .entry(name.clone())
                .and_modify(|existing| *existing = union_ty(existing, actual_ty))
                .or_insert_with(|| actual_ty.clone());
        }
        (Ty::List(f, _), Ty::List(a, _)) => {
            infer_bindings_inner(f, a, bindings, opts);
        }
        (
            Ty::Map {
                key: fk, value: fv, ..
            },
            Ty::Map {
                key: ak, value: av, ..
            },
        ) => {
            infer_bindings_inner(fk, ak, bindings, opts);
            infer_bindings_inner(fv, av, bindings, opts);
        }
        (Ty::Union(_, _), _) if nullable_non_null_part(formal).is_some() => {
            let formal_inner = nullable_non_null_part(formal).expect("checked above");
            let actual_inner = nullable_non_null_part(actual).unwrap_or_else(|| actual.clone());
            infer_bindings_inner(&formal_inner, &actual_inner, bindings, opts);
        }
        // Equal-length union ↔ union positional zip. This is only sound when the
        // formal carries NO *direct* `TypeVar` member: it matches structurally-
        // parallel unions like `List<T> | int` ↔ `List<int> | int` (the `T` is
        // nested inside a member, so the residual arm below would not see it).
        // When the formal HAS a direct `TypeVar` member, positional zip is
        // unsound — it binds by accidental member ordering (`T | int` ↔
        // `int | string` would bind `T = int` instead of routing the unmatched
        // `string` atom to `T`). Defer those to the residual/ambiguity arm below.
        (Ty::Union(f_members, _), Ty::Union(a_members, _))
            if f_members.len() == a_members.len()
                && !f_members.iter().any(|m| matches!(m, Ty::TypeVar(_, _))) =>
        {
            for (formal_member, actual_member) in f_members.iter().zip(a_members.iter()) {
                infer_bindings_inner(formal_member, actual_member, bindings, opts);
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
                // arm, which honors `rigid`/`skip_top`/`allow_typevar_actuals`.
                if !residual.is_empty() {
                    let residual_ty = normalize_union_members(residual, TyAttr::default());
                    infer_bindings_inner(tv, &residual_ty, bindings, opts);
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
            for (fp, ap) in fp.iter().zip(ap.iter()) {
                infer_bindings_inner(&fp.ty, &ap.ty, bindings, opts);
            }
            infer_bindings_inner(fr, ar, bindings, opts);
            infer_bindings_inner(fth, ath, bindings, opts);
        }
        (Ty::Class(fn_name, f_args, _), Ty::Class(an_name, a_args, _))
            if opts.class_name_agnostic || fn_name == an_name =>
        {
            for (ft, at) in f_args.iter().zip(a_args.iter()) {
                infer_bindings_inner(ft, at, bindings, opts);
            }
        }
        // `Future<T, E>` is its own variant — descend into both params so the
        // future combinators can infer `<T, E>` from a `Future<T, E>[]` arg.
        (Ty::Future(f_value, f_error, _), Ty::Future(a_value, a_error, _)) => {
            infer_bindings_inner(f_value, a_value, bindings, opts);
            infer_bindings_inner(f_error, a_error, bindings, opts);
        }
        // A heterogeneous future array — e.g. `[spawn { 1 }, spawn { 2 }]` —
        // types as `(Future<A, EA> | Future<B, EB>)[]` because `Future` is
        // invariant. Match the `Future<T, E>` formal against each union member
        // so `T`/`E` bind to the union of the member value/error types (the
        // TypeVar arm merges the per-member bindings via `union_ty`).
        (Ty::Future(_, _, _), Ty::Union(members, _)) => {
            for member in members {
                infer_bindings_inner(formal, member, bindings, opts);
            }
        }
        (
            Ty::Interface(fn_name, f_args, f_assoc, _),
            Ty::Interface(an_name, a_args, a_assoc, _),
        ) if fn_name == an_name => {
            for (ft, at) in f_args.iter().zip(a_args.iter()) {
                infer_bindings_inner(ft, at, bindings, opts);
            }
            for (formal_name, formal_ty) in f_assoc {
                if let Some((_, actual_ty)) = a_assoc
                    .iter()
                    .find(|(actual_name, _)| actual_name == formal_name)
                {
                    infer_bindings_inner(formal_ty, actual_ty, bindings, opts);
                }
            }
        }
        // Builtin container bridging: Array<T> ↔ List(T), Map<K,V> ↔ Map(K,V)
        // This enables UFCS calls like `Array.length(arr)` where the formal self
        // type is Class(Array, [T]) and the actual is List(int).
        (Ty::Class(class_name, f_args, _), Ty::List(actual_inner, _))
            if class_name.is_builtin_root_type("Array") && f_args.len() == 1 =>
        {
            infer_bindings_inner(&f_args[0], actual_inner, bindings, opts);
        }
        (
            Ty::Class(class_name, f_args, _),
            Ty::Map {
                key: actual_key,
                value: actual_val,
                ..
            },
        ) if class_name.is_builtin_root_type("Map") && f_args.len() == 2 => {
            infer_bindings_inner(&f_args[0], actual_key, bindings, opts);
            infer_bindings_inner(&f_args[1], actual_val, bindings, opts);
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
        _ => Some(Ty::Union(non_null, attr.clone())),
    }
}

/// Decompose an actual type into its union atoms: a `Union` contributes each of
/// its members (one level); anything else is a single atom. Used by the
/// union-with-`TypeVar`-member inference arm to subtract concrete siblings atom
/// by atom.
fn union_atoms(actual: &Ty) -> Vec<Ty> {
    match actual {
        Ty::Union(members, _) => members.clone(),
        other => vec![other.clone()],
    }
}

/// Whether a concrete (non-`TypeVar`) union-formal member already explains an
/// actual `atom` — i.e. the atom is a *coercion-free* subtype of the sibling.
/// Uses [`Ty::is_subtype_of`], which is intentionally free of numeric widening
/// (`int` is NOT covered by a `float` sibling) and admits only same-
/// representation widenings (literal → primitive, union membership). This keeps
/// the subtraction consistent with the TIR's runtime-tag-identity match
/// dispatch (`builder.rs::atoms_overlap`).
fn covers(concrete: &Ty, atom: &Ty) -> bool {
    atom.is_subtype_of(concrete)
}

pub fn infer_bindings(formal: &Ty, actual: &Ty, bindings: &mut FxHashMap<Name, Ty>) {
    infer_bindings_inner(formal, actual, bindings, InferOpts::COMPILE_TIME);
}

pub fn infer_bindings_allow_typevars(formal: &Ty, actual: &Ty, bindings: &mut FxHashMap<Name, Ty>) {
    infer_bindings_inner(
        formal,
        actual,
        bindings,
        InferOpts {
            allow_typevar_actuals: true,
            ..InferOpts::COMPILE_TIME
        },
    );
}

/// Like [`infer_bindings`] but treats `rigid` (when `Some`) as a rigid type
/// variable that is never bound from an argument — the pinned `Self` of an
/// interface method call. Every other variable infers exactly as before.
pub fn infer_bindings_rigid_self(
    formal: &Ty,
    actual: &Ty,
    bindings: &mut FxHashMap<Name, Ty>,
    rigid: Option<&Name>,
) {
    infer_bindings_inner(
        formal,
        actual,
        bindings,
        InferOpts {
            rigid,
            ..InferOpts::COMPILE_TIME
        },
    );
}

/// The runtime value-inference variant (01a/01b): solve a generic call's
/// `TypeVar`s from types synthesized from argument *values*. Differs from
/// [`infer_bindings`] only at the leaves — it treats the top type
/// `BuiltinUnknown` as carrying no information and zips `Class` args without a
/// name guard (the wire instance type is ground truth). The runtime engine
/// reaches this by widening its `RuntimeTy` inputs to `Ty` and narrowing the
/// resulting bindings back.
pub fn infer_value_bindings(formal: &Ty, actual: &Ty, bindings: &mut FxHashMap<Name, Ty>) {
    infer_bindings_inner(
        formal,
        actual,
        bindings,
        InferOpts {
            skip_top: true,
            class_name_agnostic: true,
            ..InferOpts::COMPILE_TIME
        },
    );
}

// ── Union normalization ─────────────────────────────────────────────────────

/// Combine two types into a union, deduplicating members.
///
/// Used when the same type variable is inferred from multiple arguments
/// (e.g., `deep_equals(myInt, myString)` → `T` gets `int` then `string`).
pub fn union_ty(a: &Ty, b: &Ty) -> Ty {
    normalize_union_members([a.clone(), b.clone()], TyAttr::default())
}

/// Flatten nested unions, drop `Never`, deduplicate; collapse a single survivor
/// to a bare type and an empty result to `Never`.
pub fn normalize_union_members(members: impl IntoIterator<Item = Ty>, attr: TyAttr) -> Ty {
    let mut normalized = Vec::new();
    for member in members {
        match member {
            Ty::Never { .. } => {}
            Ty::Union(inner, _) => {
                for inner_member in inner {
                    if !matches!(inner_member, Ty::Never { .. })
                        && !normalized.contains(&inner_member)
                    {
                        normalized.push(inner_member);
                    }
                }
            }
            other if !normalized.contains(&other) => normalized.push(other),
            _ => {}
        }
    }

    match normalized.len() {
        0 => Ty::Never { attr },
        1 => normalized.pop().expect("length checked"),
        _ => {
            // TODO(TyAttr): This union is synthesized from multiple input types — there's no
            // single "original attr" to preserve. If inputs carry different attrs, which one
            // wins? May need a merge/lattice operation on TyAttr, or default may be correct if
            // attrs describe declaration sites rather than computed types.
            Ty::Union(normalized, attr)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use baml_type::Ty;

    fn a() -> TyAttr {
        TyAttr::default()
    }

    fn tv(name: &str) -> Ty {
        Ty::TypeVar(Name::new(name), a())
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
        assert_eq!(b.get(&Name::new("T")), Some(&int()));
    }

    #[test]
    fn union_with_concrete_sibling_routes_actual_to_typevar() {
        // `T | string | null` vs `int` ⇒ T = int (G5 reversal).
        let formal = Ty::Union(vec![tv("T"), string(), null()], a());
        let mut b = FxHashMap::default();
        infer_bindings(&formal, &int(), &mut b);
        assert_eq!(b.get(&Name::new("T")), Some(&int()));
    }

    #[test]
    fn union_concrete_sibling_absorbs_actual_leaves_typevar_unbound() {
        // `T | string | null` vs `string` ⇒ the string sibling absorbs it; T unbound.
        let formal = Ty::Union(vec![tv("T"), string(), null()], a());
        let mut b = FxHashMap::default();
        infer_bindings(&formal, &string(), &mut b);
        assert!(!b.contains_key(&Name::new("T")));
    }

    #[test]
    fn union_null_actual_binds_typevar_to_null() {
        // `T | string | null` vs `null` ⇒ T = null (null not absorbed by string sibling).
        let formal = Ty::Union(vec![tv("T"), string(), null()], a());
        let mut b = FxHashMap::default();
        infer_bindings(&formal, &null(), &mut b);
        assert_eq!(b.get(&Name::new("T")), Some(&null()));
    }

    #[test]
    fn multi_typevar_union_is_ambiguous_binds_nothing() {
        // `T | U | string` vs `int` ⇒ no principled split ⇒ both unbound.
        let formal = Ty::Union(vec![tv("T"), tv("U"), string()], a());
        let mut b = FxHashMap::default();
        infer_bindings(&formal, &int(), &mut b);
        assert!(!b.contains_key(&Name::new("T")));
        assert!(!b.contains_key(&Name::new("U")));
    }

    fn boolt() -> Ty {
        Ty::Bool { attr: a() }
    }

    #[test]
    fn equal_len_union_actual_routes_residual_to_typevar() {
        // Regression: `T | int` vs `int | string` (both len 2). The equal-length
        // positional-zip arm must NOT fire (it would bind T = int by member
        // ordering). The residual arm routes the unmatched `string` to T.
        let formal = Ty::Union(vec![tv("T"), int()], a());
        let actual = Ty::Union(vec![int(), string()], a());
        let mut b = FxHashMap::default();
        infer_bindings(&formal, &actual, &mut b);
        assert_eq!(b.get(&Name::new("T")), Some(&string()));
    }

    #[test]
    fn nullable_union_actual_routes_residual_to_typevar() {
        // Regression: `T | int | null` vs `int | string`. After the nullable arm
        // peels `null`, the recursion lands on `T | int` vs `int | string` and
        // must route `string` to T, not positionally bind T = int.
        let formal = Ty::Union(vec![tv("T"), int(), null()], a());
        let actual = Ty::Union(vec![int(), string()], a());
        let mut b = FxHashMap::default();
        infer_bindings(&formal, &actual, &mut b);
        assert_eq!(b.get(&Name::new("T")), Some(&string()));
    }

    #[test]
    fn multi_typevar_equal_len_union_actual_binds_nothing() {
        // Regression: `T | U | int` vs `int | string | bool` (both len 3). The
        // equal-length zip must not pre-empt the multi-TypeVar ambiguity guard;
        // >1 TypeVar member has no principled split ⇒ both stay unbound.
        let formal = Ty::Union(vec![tv("T"), tv("U"), int()], a());
        let actual = Ty::Union(vec![int(), string(), boolt()], a());
        let mut b = FxHashMap::default();
        infer_bindings(&formal, &actual, &mut b);
        assert!(!b.contains_key(&Name::new("T")));
        assert!(!b.contains_key(&Name::new("U")));
    }

    #[test]
    fn equal_len_union_without_direct_typevar_still_zips() {
        // The equal-length positional-zip arm is preserved for unions whose
        // TypeVar is *nested* inside a member: `List<T> | int` vs
        // `List<int> | int` must still bind T = int (the residual arm only sees
        // direct TypeVar members, so without the zip arm T would stay unbound).
        let list_tv = Ty::List(Box::new(tv("T")), a());
        let list_int = Ty::List(Box::new(int()), a());
        let formal = Ty::Union(vec![list_tv, int()], a());
        let actual = Ty::Union(vec![list_int, int()], a());
        let mut b = FxHashMap::default();
        infer_bindings(&formal, &actual, &mut b);
        assert_eq!(b.get(&Name::new("T")), Some(&int()));
    }

    #[test]
    fn float_sibling_does_not_absorb_int_actual() {
        // Coercion-free: `int` is NOT covered by a `float` sibling, so T = int.
        let float = Ty::Float { attr: a() };
        let formal = Ty::Union(vec![tv("T"), float], a());
        let mut b = FxHashMap::default();
        infer_bindings(&formal, &int(), &mut b);
        assert_eq!(b.get(&Name::new("T")), Some(&int()));
    }
}
