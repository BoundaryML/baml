//! Generic type variable binding and substitution.
//!
//! When the type checker encounters `arr.at(0)` where `arr: int[]`, it needs
//! to know that `at` returns `int` (not `T`). This module provides the
//! binding and substitution machinery.
//!
//! ## How it works
//!
//! 1. The caller looks up the builtin class (e.g. `Array`) from the `"baml"`
//!    package and extracts its `generic_params` (e.g. `["T"]`).
//! 2. It provides the concrete type arguments (e.g. `[Ty::Primitive(Int)]`).
//! 3. `bind_type_vars` zips them together: `{T → int}`.
//! 4. For each method parameter/return type, `lower_type_expr_with_generics`
//!    is called: if the `TypeExpr` is a `Path(["T"])` that matches a bound
//!    variable, it returns the bound concrete type directly; otherwise it
//!    falls through to normal `lower_type_expr` and then applies
//!    `substitute_ty` to replace any residual type-variable references.

use baml_base::Name;
use baml_compiler2_ast::TypeExpr;
use rustc_hash::FxHashMap;

use crate::{
    infer_context::TirTypeError,
    lower_type_expr::{DiagSink, LoweringCtx, lower_type_expr_in_ns_into},
    ty::{FunctionParamMode, FunctionParamTy, Ty},
};

// ── Type variable binding ─────────────────────────────────────────────────────

/// Bind type variables from generic params to concrete type arguments.
///
/// Example: `bind_type_vars(&["T"], &[Ty::Primitive(Int)])` → `{"T" → Int}`
///
/// If there are more params than args (or vice versa), the extra entries are
/// silently ignored — callers are responsible for providing matching lengths.
pub fn bind_type_vars(generic_params: &[Name], concrete_args: &[Ty]) -> FxHashMap<Name, Ty> {
    let mut bindings = FxHashMap::default();
    for (param, arg) in generic_params.iter().zip(concrete_args.iter()) {
        bindings.insert(param.clone(), arg.clone());
    }
    bindings
}

// ── Type substitution ─────────────────────────────────────────────────────────

/// Substitute type variables in a `Ty` using the provided bindings.
///
/// Recursively walks the type, replacing any `Ty::TypeVar` present in
/// `bindings`. This is used both for callable generic instantiation and for
/// interface implementation rule instantiation, so it must preserve the full
/// TIR shape rather than only class/member-signature types.
pub fn substitute_ty(ty: &Ty, bindings: &FxHashMap<Name, Ty>) -> Ty {
    if bindings.is_empty() {
        return ty.clone();
    }
    match ty {
        Ty::TypeVar(name) => bindings.get(name).cloned().unwrap_or_else(|| ty.clone()),
        Ty::List(inner) => Ty::List(Box::new(substitute_ty(inner, bindings))),
        Ty::EvolvingList(inner) => Ty::EvolvingList(Box::new(substitute_ty(inner, bindings))),
        Ty::Map(k, v) => Ty::Map(
            Box::new(substitute_ty(k, bindings)),
            Box::new(substitute_ty(v, bindings)),
        ),
        Ty::EvolvingMap(k, v) => Ty::EvolvingMap(
            Box::new(substitute_ty(k, bindings)),
            Box::new(substitute_ty(v, bindings)),
        ),
        Ty::Optional(inner) => Ty::Optional(Box::new(substitute_ty(inner, bindings))),
        Ty::Future(value, error) => Ty::Future(
            Box::new(substitute_ty(value, bindings)),
            Box::new(substitute_ty(error, bindings)),
        ),
        Ty::Union(members) => {
            Ty::Union(members.iter().map(|m| substitute_ty(m, bindings)).collect())
        }
        Ty::Function {
            generic_params,
            generic_param_bounds,
            params,
            ret,
            throws,
        } => {
            let mut nested_bindings = bindings.clone();
            for generic_param in generic_params {
                nested_bindings.remove(generic_param);
            }
            Ty::Function {
                generic_params: generic_params.clone(),
                generic_param_bounds: generic_param_bounds
                    .iter()
                    .map(|bound| bound.as_ref().map(|ty| substitute_ty(ty, &nested_bindings)))
                    .collect(),
                params: params
                    .iter()
                    .map(|param| FunctionParamTy {
                        name: param.name.clone(),
                        ty: substitute_ty(&param.ty, &nested_bindings),
                        mode: param.mode,
                    })
                    .collect(),
                ret: Box::new(substitute_ty(ret, &nested_bindings)),
                throws: Box::new(substitute_ty(throws, &nested_bindings)),
            }
        }
        Ty::Class(name, type_args) => {
            let substituted_args: Vec<Ty> = type_args
                .iter()
                .map(|t| substitute_ty(t, bindings))
                .collect();
            Ty::Class(name.clone(), substituted_args)
        }
        Ty::Interface(name, type_args) => {
            let substituted_args: Vec<Ty> = type_args
                .iter()
                .map(|t| substitute_ty(t, bindings))
                .collect();
            Ty::Interface(name.clone(), substituted_args)
        }
        // All other types are leaves (primitives, enums, etc.) — pass through.
        _ => ty.clone(),
    }
}

// ── Combined lowering with generic substitution ───────────────────────────────

/// Lower a `TypeExpr` to `Ty` with type variable substitution applied.
///
/// For complex type expressions (e.g. `T[]`, `map<K, V>`, `V?`), first lowers
/// normally then substitutes type variables in the result. For single-segment
/// paths that directly name a type variable (e.g. `T`, `K`, `V`), intercepts
/// before lowering to avoid the "unresolved type" diagnostic that `lower_type_expr`
/// would otherwise emit.
///
/// Diagnostics from the lowering step (for non-variable paths that genuinely
/// don't exist) are collected into `diagnostics`.
///
/// `ns_context` is the defining file's namespace within its package (e.g. `["llm"]`
/// for `<builtin>/baml/llm/llm.baml`); unqualified type paths resolve there first.
pub fn lower_type_expr_with_generics(
    db: &dyn crate::Db,
    expr: &TypeExpr,
    package_items: &baml_compiler2_hir::package::PackageItems<'_>,
    ns_context: &[Name],
    bindings: &FxHashMap<Name, Ty>,
    diagnostics: &mut Vec<TirTypeError>,
) -> Ty {
    let mut push = |e| diagnostics.push(e);
    let ctx = LoweringCtx {
        db,
        package_items,
        ns_context,
    };
    lower_with_generics(ctx, expr, bindings, &mut push)
}

/// Sink-based variant of [`lower_type_expr_with_generics`]: forwards each
/// diagnostic to `sink` in source-walk order instead of collecting into a `Vec`.
pub fn lower_type_expr_with_generics_into(
    db: &dyn crate::Db,
    expr: &TypeExpr,
    package_items: &baml_compiler2_hir::package::PackageItems<'_>,
    ns_context: &[Name],
    bindings: &FxHashMap<Name, Ty>,
    sink: DiagSink<'_>,
) -> Ty {
    let ctx = LoweringCtx {
        db,
        package_items,
        ns_context,
    };
    lower_with_generics(ctx, expr, bindings, sink)
}

fn lower_with_generics(
    ctx: LoweringCtx<'_>,
    expr: &TypeExpr,
    bindings: &FxHashMap<Name, Ty>,
    sink: DiagSink<'_>,
) -> Ty {
    // Fast path: empty bindings — no substitution needed.
    if bindings.is_empty() {
        return lower_type_expr_in_ns_into(
            ctx.db,
            expr,
            ctx.package_items,
            ctx.ns_context,
            &[],
            sink,
        );
    }

    // Intercept single-segment paths that are type variables.
    if let TypeExpr::Path { segments, .. } = expr {
        if segments.len() == 1 {
            if let Some(ty) = bindings.get(&segments[0]).cloned() {
                return ty;
            }
        }
    }

    // For composite types (List, Map, Optional, Union), recurse with substitution
    // rather than lowering first then substituting, so that type-variable references
    // in nested positions are also intercepted before triggering "unresolved type".
    match expr {
        TypeExpr::Optional { inner, .. } => {
            Ty::Optional(Box::new(lower_with_generics(ctx, inner, bindings, sink)))
        }
        TypeExpr::List { inner, .. } => {
            Ty::List(Box::new(lower_with_generics(ctx, inner, bindings, sink)))
        }
        TypeExpr::Map { key, value, .. } => Ty::Map(
            Box::new(lower_with_generics(ctx, key, bindings, sink)),
            Box::new(lower_with_generics(ctx, value, bindings, sink)),
        ),
        TypeExpr::Union {
            variants: members, ..
        } => Ty::Union(
            members
                .iter()
                .map(|m| lower_with_generics(ctx, m, bindings, sink))
                .collect(),
        ),
        TypeExpr::Function {
            generic_params,
            generic_param_bounds,
            params,
            ret,
            throws,
            ..
        } => {
            let mut nested_bindings = bindings.clone();
            for param in generic_params {
                nested_bindings.insert(param.clone(), Ty::TypeVar(param.clone()));
            }
            Ty::Function {
                generic_params: generic_params.clone(),
                generic_param_bounds: generic_param_bounds
                    .iter()
                    .map(|bound| {
                        bound
                            .as_ref()
                            .map(|bound| lower_with_generics(ctx, bound, &nested_bindings, sink))
                    })
                    .collect(),
                params: params
                    .iter()
                    .map(|p| FunctionParamTy {
                        name: p.name.clone(),
                        ty: lower_with_generics(ctx, &p.ty, &nested_bindings, sink),
                        mode: if p.optional {
                            FunctionParamMode::Optional
                        } else {
                            FunctionParamMode::Required
                        },
                    })
                    .collect(),
                ret: Box::new(lower_with_generics(ctx, ret, &nested_bindings, sink)),
                throws: Box::new(
                    throws
                        .as_deref()
                        .map(|throws| lower_with_generics(ctx, throws, &nested_bindings, sink))
                        .unwrap_or(Ty::Never),
                ),
            }
        }
        // For all other type expressions (primitives, multi-segment paths, etc.),
        // lower normally and then substitute in the result.
        //
        // We pass the binding keys as `generic_params` so that nested type variable
        // references inside path generic args (e.g. `StreamCache<T, S>`) are preserved
        // as `Ty::TypeVar` by `lower_type_expr_in_ns` rather than triggering "unresolved
        // type" diagnostics. `substitute_ty` then replaces those TypeVars with the
        // concrete bound types.
        other => {
            let binding_keys: Vec<Name> = bindings.keys().cloned().collect();
            let ty = lower_type_expr_in_ns_into(
                ctx.db,
                other,
                ctx.package_items,
                ctx.ns_context,
                &binding_keys,
                sink,
            );
            substitute_ty(&ty, bindings)
        }
    }
}

// ── Method parameter adjustment ───────────────────────────────────────────────

/// Skip the `self` parameter in a method's parameter list.
///
/// When `arr.length()` is a method call, the resolved `Ty::Function` includes
/// `self` as the first parameter (from the `.baml` stub declaration). The call
/// site already bound `arr` as the receiver — it should not count as an
/// explicit argument.
///
/// Returns the slice of params after `self`, or the full slice if `self` is
/// not the first parameter name.
pub fn skip_self_param(params: &[FunctionParamTy]) -> &[FunctionParamTy] {
    match params.first() {
        Some(param)
            if param
                .name
                .as_ref()
                .is_some_and(|name| name.as_str() == "self") =>
        {
            &params[1..]
        }
        _ => params,
    }
}

// ── Type variable utilities ────────────────────────────────────────────────

/// Check if a type contains any `Ty::TypeVar` anywhere in its structure.
pub fn contains_typevar(ty: &Ty) -> bool {
    match ty {
        Ty::TypeVar(_) => true,
        Ty::List(inner) | Ty::Optional(inner) | Ty::EvolvingList(inner) => contains_typevar(inner),
        Ty::Map(k, v) | Ty::EvolvingMap(k, v) => contains_typevar(k) || contains_typevar(v),
        Ty::Union(tys) => tys.iter().any(contains_typevar),
        Ty::Function {
            generic_param_bounds,
            params,
            ret,
            throws,
            ..
        } => {
            generic_param_bounds
                .iter()
                .any(|bound| bound.as_ref().is_some_and(contains_typevar))
                || params.iter().any(|param| contains_typevar(&param.ty))
                || contains_typevar(ret)
                || contains_typevar(throws)
        }
        Ty::Class(_, type_args) | Ty::Interface(_, type_args) => {
            type_args.iter().any(contains_typevar)
        }
        _ => false,
    }
}

/// Infer type variable bindings by walking formal and actual types in parallel.
///
/// When `formal` is `Ty::TypeVar("T")` and `actual` is `Ty::Primitive(Int)`,
/// records `T → int` in `bindings`. For structural types, recurses into
/// matching structures. Conflicting inferences are merged via `union_ty`.
fn infer_bindings_inner(
    formal: &Ty,
    actual: &Ty,
    bindings: &mut FxHashMap<Name, Ty>,
    allow_typevar_actuals: bool,
) {
    match (formal, actual) {
        (Ty::TypeVar(name), actual_ty) => {
            // Skip TypeVar-to-TypeVar bindings by default — they usually provide
            // no information for ordinary call inference. Some higher-order
            // callable-summary paths opt into preserving them explicitly.
            if !allow_typevar_actuals && matches!(actual_ty, Ty::TypeVar(_)) {
                return;
            }
            bindings
                .entry(name.clone())
                .and_modify(|existing| *existing = union_ty(existing, actual_ty))
                .or_insert_with(|| actual_ty.clone());
        }
        (Ty::List(f), Ty::List(a)) => {
            infer_bindings_inner(f, a, bindings, allow_typevar_actuals);
        }
        (Ty::Map(fk, fv), Ty::Map(ak, av)) => {
            infer_bindings_inner(fk, ak, bindings, allow_typevar_actuals);
            infer_bindings_inner(fv, av, bindings, allow_typevar_actuals);
        }
        (Ty::Optional(f), Ty::Optional(a)) => {
            infer_bindings_inner(f, a, bindings, allow_typevar_actuals);
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
                infer_bindings_inner(&fp.ty, &ap.ty, bindings, allow_typevar_actuals);
            }
            infer_bindings_inner(fr, ar, bindings, allow_typevar_actuals);
            infer_bindings_inner(fth, ath, bindings, allow_typevar_actuals);
        }
        (Ty::Class(fn_name, f_args), Ty::Class(an_name, a_args))
        | (Ty::Interface(fn_name, f_args), Ty::Interface(an_name, a_args))
            if fn_name == an_name =>
        {
            for (ft, at) in f_args.iter().zip(a_args.iter()) {
                infer_bindings_inner(ft, at, bindings, allow_typevar_actuals);
            }
        }
        // Builtin container bridging: Array<T> ↔ List(T), Map<K,V> ↔ Map(K,V)
        // This enables UFCS calls like `Array.length(arr)` where the formal self
        // type is Class(Array, [T]) and the actual is List(int).
        (Ty::Class(class_name, f_args), Ty::List(actual_inner))
            if class_name.is_builtin_root_type("Array") && f_args.len() == 1 =>
        {
            infer_bindings_inner(&f_args[0], actual_inner, bindings, allow_typevar_actuals);
        }
        (Ty::Class(class_name, f_args), Ty::Map(actual_key, actual_val))
            if class_name.is_builtin_root_type("Map") && f_args.len() == 2 =>
        {
            infer_bindings_inner(&f_args[0], actual_key, bindings, allow_typevar_actuals);
            infer_bindings_inner(&f_args[1], actual_val, bindings, allow_typevar_actuals);
        }
        _ => {} // Concrete types: nothing to infer
    }
}

pub fn infer_bindings(formal: &Ty, actual: &Ty, bindings: &mut FxHashMap<Name, Ty>) {
    infer_bindings_inner(formal, actual, bindings, false);
}

pub fn infer_bindings_allow_typevars(formal: &Ty, actual: &Ty, bindings: &mut FxHashMap<Name, Ty>) {
    infer_bindings_inner(formal, actual, bindings, true);
}

/// Combine two types into a union, deduplicating members.
///
/// Used when the same type variable is inferred from multiple arguments
/// (e.g., `deep_equals(myInt, myString)` → `T` gets `int` then `string`).
pub fn union_ty(a: &Ty, b: &Ty) -> Ty {
    if a == b {
        return a.clone();
    }
    let mut members = Vec::new();
    match a {
        Ty::Union(tys) => members.extend(tys.iter().cloned()),
        other => members.push(other.clone()),
    }
    match b {
        Ty::Union(tys) => {
            for t in tys {
                if !members.contains(t) {
                    members.push(t.clone());
                }
            }
        }
        other => {
            if !members.contains(other) {
                members.push(other.clone());
            }
        }
    }
    if members.len() == 1 {
        members.pop().unwrap()
    } else {
        // TODO(TyAttr): This union is synthesized from two input types — there's no single
        // "original attr" to preserve. If both inputs are unions with different attrs, which
        // one wins? May need a merge/lattice operation on TyAttr, or TyAttr::default() may
        // be correct if attrs describe declaration sites rather than computed types.
        Ty::Union(members)
    }
}

/// Replace selected type variables with `unknown` for runtime-facing metadata.
///
/// Bounded generic parameters are compile-time evidence, not concrete runtime
/// type tags. MIR and bytecode metadata both need the same erasure behavior, so
/// keep the recursive shape walk here beside the other generic utilities.
pub fn erase_typevars_matching(ty: &Ty, should_erase: &impl Fn(&Name) -> bool) -> Ty {
    if !contains_typevar(ty) {
        return ty.clone();
    }

    match ty {
        Ty::TypeVar(name) if should_erase(name) => Ty::BuiltinUnknown,
        Ty::Class(qtn, args) => Ty::Class(
            qtn.clone(),
            args.iter()
                .map(|arg| erase_typevars_matching(arg, should_erase))
                .collect(),
        ),
        Ty::Interface(qtn, args) => Ty::Interface(
            qtn.clone(),
            args.iter()
                .map(|arg| erase_typevars_matching(arg, should_erase))
                .collect(),
        ),
        Ty::List(inner) => Ty::List(Box::new(erase_typevars_matching(inner, should_erase))),
        Ty::EvolvingList(inner) => {
            Ty::EvolvingList(Box::new(erase_typevars_matching(inner, should_erase)))
        }
        Ty::Optional(inner) => Ty::Optional(Box::new(erase_typevars_matching(inner, should_erase))),
        Ty::Map(key, value) => Ty::Map(
            Box::new(erase_typevars_matching(key, should_erase)),
            Box::new(erase_typevars_matching(value, should_erase)),
        ),
        Ty::EvolvingMap(key, value) => Ty::EvolvingMap(
            Box::new(erase_typevars_matching(key, should_erase)),
            Box::new(erase_typevars_matching(value, should_erase)),
        ),
        Ty::Union(members) => Ty::Union(
            members
                .iter()
                .map(|member| erase_typevars_matching(member, should_erase))
                .collect(),
        ),
        Ty::Future(value, error) => Ty::Future(
            Box::new(erase_typevars_matching(value, should_erase)),
            Box::new(erase_typevars_matching(error, should_erase)),
        ),
        Ty::Function {
            generic_params,
            generic_param_bounds,
            params,
            ret,
            throws,
        } => Ty::Function {
            generic_params: generic_params.clone(),
            generic_param_bounds: generic_param_bounds
                .iter()
                .map(|bound| {
                    bound
                        .as_ref()
                        .map(|ty| erase_typevars_matching(ty, should_erase))
                })
                .collect(),
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
        },
        _ => ty.clone(),
    }
}
