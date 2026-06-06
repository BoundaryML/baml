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
//! 2. It provides the concrete type arguments (e.g. `[Ty::Primitive(Int, TyAttr::default())]`).
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
    lower_type_expr::lower_type_expr_in_ns,
    ty::{FunctionParamMode, FunctionParamTy, Ty, TyAttr},
};

// ── Type variable binding ─────────────────────────────────────────────────────

/// Bind type variables from generic params to concrete type arguments.
///
/// Example: `bind_type_vars(&["T"], &[Ty::Primitive(Int, TyAttr::default())])` → `{"T" → Int}`
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
        Ty::TypeVar(name, _) => bindings.get(name).cloned().unwrap_or_else(|| ty.clone()),
        Ty::List(inner, attr) => Ty::List(Box::new(substitute_ty(inner, bindings)), attr.clone()),
        Ty::EvolvingList(inner, attr) => {
            Ty::EvolvingList(Box::new(substitute_ty(inner, bindings)), attr.clone())
        }
        Ty::Map(k, v, attr) => Ty::Map(
            Box::new(substitute_ty(k, bindings)),
            Box::new(substitute_ty(v, bindings)),
            attr.clone(),
        ),
        Ty::EvolvingMap(k, v, attr) => Ty::EvolvingMap(
            Box::new(substitute_ty(k, bindings)),
            Box::new(substitute_ty(v, bindings)),
            attr.clone(),
        ),
        Ty::Future(value, error, attr) => Ty::Future(
            Box::new(substitute_ty(value, bindings)),
            Box::new(substitute_ty(error, bindings)),
            attr.clone(),
        ),
        Ty::AssociatedTypeProjection {
            base,
            interface,
            member,
            attr,
        } => Ty::AssociatedTypeProjection {
            base: Box::new(substitute_ty(base, bindings)),
            interface: interface
                .as_ref()
                .map(|interface| Box::new(substitute_ty(interface, bindings))),
            member: member.clone(),
            attr: attr.clone(),
        },
        Ty::Union(members, attr) => Ty::Union(
            members.iter().map(|m| substitute_ty(m, bindings)).collect(),
            attr.clone(),
        ),
        Ty::Function {
            generic_params,
            generic_param_bounds,
            params,
            ret,
            throws,
            attr,
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
                attr: attr.clone(),
            }
        }
        Ty::Class(name, type_args, attr) => {
            let substituted_args: Vec<Ty> = type_args
                .iter()
                .map(|t| substitute_ty(t, bindings))
                .collect();
            Ty::Class(name.clone(), substituted_args, attr.clone())
        }
        Ty::Interface(name, type_args, associated_bindings, attr) => {
            let substituted_args: Vec<Ty> = type_args
                .iter()
                .map(|t| substitute_ty(t, bindings))
                .collect();
            let substituted_bindings = associated_bindings
                .iter()
                .map(|(name, ty)| (name.clone(), substitute_ty(ty, bindings)))
                .collect();
            Ty::Interface(
                name.clone(),
                substituted_args,
                substituted_bindings,
                attr.clone(),
            )
        }
        // All other types are leaves (primitives, enums, etc.) — pass through.
        _ => ty.clone(),
    }
}

// ── TypeExpr-level substitution ───────────────────────────────────────────────

/// Check if a `TypeExpr` is a single-segment path that matches a bound type variable.
///
/// Returns `Some(bound_ty)` if the expression is `Path(["T"])` and `"T"` is in
/// `bindings`. Returns `None` if it's not a type variable reference.
///
/// This is called at the `TypeExpr` level, before `lower_type_expr`, so we can
/// intercept `T` references that would otherwise produce `Ty::Unknown`.
fn substitute_type_expr(expr: &TypeExpr, bindings: &FxHashMap<Name, Ty>) -> Option<Ty> {
    match expr {
        TypeExpr::Path {
            segments,
            generic_args,
            associated_type_bindings,
            ..
        } if segments.len() == 2
            && segments[0].as_str() == "Self"
            && generic_args.is_empty()
            && associated_type_bindings.is_empty() =>
        {
            bindings.get(&segments[1]).cloned()
        }
        TypeExpr::Path {
            segments,
            generic_args,
            associated_type_bindings,
            ..
        } if segments.len() == 1
            && generic_args.is_empty()
            && associated_type_bindings.is_empty() =>
        {
            bindings.get(&segments[0]).cloned()
        }
        _ => None,
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
    // Fast path: empty bindings — no substitution needed.
    if bindings.is_empty() {
        return lower_type_expr_in_ns(db, expr, package_items, ns_context, &[], diagnostics);
    }

    // Intercept single-segment paths that are type variables.
    if let Some(ty) = substitute_type_expr(expr, bindings) {
        return ty;
    }

    // For composite types (List, Map, Optional, Union), recurse with substitution
    // rather than lowering first then substituting, so that type-variable references
    // in nested positions are also intercepted before triggering "unresolved type".
    match expr {
        // `T?` is sugar for `T | null` — lower it directly to a nullable union.
        TypeExpr::Optional { inner, .. } => Ty::nullable(lower_type_expr_with_generics(
            db,
            inner,
            package_items,
            ns_context,
            bindings,
            diagnostics,
        )),
        TypeExpr::List { inner, .. } => Ty::List(
            Box::new(lower_type_expr_with_generics(
                db,
                inner,
                package_items,
                ns_context,
                bindings,
                diagnostics,
            )),
            TyAttr::default(),
        ),
        TypeExpr::Map { key, value, .. } => Ty::Map(
            Box::new(lower_type_expr_with_generics(
                db,
                key,
                package_items,
                ns_context,
                bindings,
                diagnostics,
            )),
            Box::new(lower_type_expr_with_generics(
                db,
                value,
                package_items,
                ns_context,
                bindings,
                diagnostics,
            )),
            TyAttr::default(),
        ),
        TypeExpr::Union {
            variants: members, ..
        } => Ty::Union(
            members
                .iter()
                .map(|m| {
                    lower_type_expr_with_generics(
                        db,
                        m,
                        package_items,
                        ns_context,
                        bindings,
                        diagnostics,
                    )
                })
                .collect(),
            TyAttr::default(),
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
                nested_bindings
                    .insert(param.clone(), Ty::TypeVar(param.clone(), TyAttr::default()));
            }
            Ty::Function {
                generic_params: generic_params.clone(),
                generic_param_bounds: generic_param_bounds
                    .iter()
                    .map(|bound| {
                        bound.as_ref().map(|bound| {
                            lower_type_expr_with_generics(
                                db,
                                bound,
                                package_items,
                                ns_context,
                                &nested_bindings,
                                diagnostics,
                            )
                        })
                    })
                    .collect(),
                params: params
                    .iter()
                    .map(|p| FunctionParamTy {
                        name: p.name.clone(),
                        ty: lower_type_expr_with_generics(
                            db,
                            &p.ty,
                            package_items,
                            ns_context,
                            &nested_bindings,
                            diagnostics,
                        ),
                        mode: if p.optional {
                            FunctionParamMode::Optional
                        } else {
                            FunctionParamMode::Required
                        },
                    })
                    .collect(),
                ret: Box::new(lower_type_expr_with_generics(
                    db,
                    ret,
                    package_items,
                    ns_context,
                    &nested_bindings,
                    diagnostics,
                )),
                throws: Box::new(
                    throws
                        .as_deref()
                        .map(|throws| {
                            lower_type_expr_with_generics(
                                db,
                                throws,
                                package_items,
                                ns_context,
                                &nested_bindings,
                                diagnostics,
                            )
                        })
                        .unwrap_or(Ty::Never {
                            attr: TyAttr::default(),
                        }),
                ),
                attr: TyAttr::default(),
            }
        }
        TypeExpr::AssociatedTypeProjection {
            base,
            interface,
            member,
            ..
        } => Ty::AssociatedTypeProjection {
            base: Box::new(lower_type_expr_with_generics(
                db,
                base,
                package_items,
                ns_context,
                bindings,
                diagnostics,
            )),
            interface: interface.as_ref().map(|interface| {
                Box::new(lower_type_expr_with_generics(
                    db,
                    interface,
                    package_items,
                    ns_context,
                    bindings,
                    diagnostics,
                ))
            }),
            member: member.clone(),
            attr: TyAttr::default(),
        },
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
            let ty = lower_type_expr_in_ns(
                db,
                other,
                package_items,
                ns_context,
                &binding_keys,
                diagnostics,
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
        Ty::TypeVar(_, _) => true,
        Ty::AssociatedTypeProjection {
            base, interface, ..
        } => {
            contains_typevar(base)
                || interface
                    .as_ref()
                    .is_some_and(|interface| contains_typevar(interface))
        }
        Ty::List(inner, _) | Ty::EvolvingList(inner, _) => contains_typevar(inner),
        Ty::Map(k, v, _) | Ty::EvolvingMap(k, v, _) => contains_typevar(k) || contains_typevar(v),
        Ty::Union(tys, _) => tys.iter().any(contains_typevar),
        Ty::Future(value, error, _) => contains_typevar(value) || contains_typevar(error),
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
        Ty::Class(_, type_args, _) => type_args.iter().any(contains_typevar),
        Ty::Interface(_, type_args, associated_bindings, _) => {
            type_args.iter().any(contains_typevar)
                || associated_bindings
                    .iter()
                    .any(|(_, ty)| contains_typevar(ty))
        }
        _ => false,
    }
}

/// Whether `name` is an *inferable* type var for a value call: either one of the
/// callee's declared `generic_params`, or a synthetic effect-polymorphism param
/// (`__effect_param_N`). Anything else is a *rigid ambient* var (the caller's
/// `T` in an instantiation value `foo<T>`), which must be matched structurally
/// rather than inferred. Used by call-site inference's binding-retention filter.
pub fn is_value_call_inferable(name: &Name, generic_params: &[Name]) -> bool {
    generic_params.contains(name) || crate::ty::is_synthetic_effect_param(name)
}

/// Like [`contains_typevar`], but ignores type variables whose name appears in
/// `rigid`. Used at a generic call site to tell the callee's still-unbound
/// inference variables apart from the *enclosing* function's generic params:
/// the latter are rigid (fixed, concrete-enough) types within the current body,
/// so a callback parameter like `Future<T, E>` — where `T`/`E` are the caller's
/// own generics — can drive bidirectional checking of an unannotated lambda
/// parameter instead of falling back to synthesis (which would reject the bare
/// param). A typevar that is NOT rigid is one the callee must still infer, so a
/// param mentioning it genuinely cannot give the lambda a concrete shape.
pub fn contains_non_rigid_typevar(ty: &Ty, rigid: &[Name]) -> bool {
    contains_typevar_where(ty, &|name| !rigid.iter().any(|r| r == name))
}

/// Returns `true` if `ty` contains any type variable for which `pred` returns
/// `true`. A general form of [`contains_typevar`] used by call validation to
/// distinguish *rigid* type variables (the pinned `Self`, caller-scope generic
/// params) — which must be checked — from genuinely-uninferred ones (callee
/// generics, free inference/effect vars) — which are deferred.
pub fn contains_typevar_where(ty: &Ty, pred: &dyn Fn(&Name) -> bool) -> bool {
    match ty {
        Ty::TypeVar(name, _) => pred(name),
        Ty::List(inner, _) | Ty::EvolvingList(inner, _) => contains_typevar_where(inner, pred),
        Ty::Map(k, v, _) | Ty::EvolvingMap(k, v, _) => {
            contains_typevar_where(k, pred) || contains_typevar_where(v, pred)
        }
        Ty::Union(tys, _) => tys.iter().any(|t| contains_typevar_where(t, pred)),
        Ty::Future(value, error, _) => {
            contains_typevar_where(value, pred) || contains_typevar_where(error, pred)
        }
        Ty::Function {
            generic_params,
            generic_param_bounds,
            params,
            ret,
            throws,
            ..
        } => {
            // A function type's own generic params are local binders: a type var
            // bound here is not the caller-scope/rigid var the outer `pred`
            // reasons about, so shadow them out before recursing into its body.
            let shadowed = |name: &Name| !generic_params.iter().any(|g| g == name) && pred(name);
            let shadowed: &dyn Fn(&Name) -> bool = &shadowed;
            generic_param_bounds.iter().any(|bound| {
                bound
                    .as_ref()
                    .is_some_and(|b| contains_typevar_where(b, shadowed))
            }) || params
                .iter()
                .any(|param| contains_typevar_where(&param.ty, shadowed))
                || contains_typevar_where(ret, shadowed)
                || contains_typevar_where(throws, shadowed)
        }
        Ty::Class(_, type_args, _) => type_args.iter().any(|t| contains_typevar_where(t, pred)),
        Ty::Interface(_, type_args, associated_bindings, _) => {
            type_args.iter().any(|t| contains_typevar_where(t, pred))
                || associated_bindings
                    .iter()
                    .any(|(_, ty)| contains_typevar_where(ty, pred))
        }
        // Mirror `contains_typevar`: a type variable can hide in the projection
        // base (e.g. `T::Item`) or the qualifying interface. Without this arm
        // the deferral check (`defers_typevar`) is blind to an associated-type
        // parameter and may check it against an under-determined type.
        Ty::AssociatedTypeProjection {
            base, interface, ..
        } => {
            contains_typevar_where(base, pred)
                || interface
                    .as_ref()
                    .is_some_and(|interface| contains_typevar_where(interface, pred))
        }
        _ => false,
    }
}

/// Infer type variable bindings by walking formal and actual types in parallel.
///
/// When `formal` is `Ty::TypeVar("T", TyAttr::default())` and `actual` is `Ty::Primitive(Int, TyAttr::default())`,
/// records `T → int` in `bindings`. For structural types, recurses into
/// matching structures. Conflicting inferences are merged via `union_ty`.
fn infer_bindings_inner(
    formal: &Ty,
    actual: &Ty,
    bindings: &mut FxHashMap<Name, Ty>,
    allow_typevar_actuals: bool,
    // A *rigid* type variable that must never be bound from an argument — the
    // pinned `Self` of an interface method call (mirrors rustc's `ty::Param`,
    // which unification never instantiates). `None` = no rigid variable (the
    // historical behavior). This is the only thing that distinguishes a
    // Self-pinned call from any other; ordinary calls pass `None`, so their
    // inference is completely unchanged.
    rigid: Option<&Name>,
) {
    match (formal, actual) {
        (Ty::TypeVar(name, _), actual_ty) => {
            if rigid == Some(name) {
                return;
            }
            // Skip TypeVar-to-TypeVar bindings by default — they usually provide
            // no information for ordinary call inference. Some higher-order
            // callable-summary paths opt into preserving them explicitly.
            if !allow_typevar_actuals && matches!(actual_ty, Ty::TypeVar(_, _)) {
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
            bindings
                .entry(name.clone())
                .and_modify(|existing| *existing = union_ty(existing, actual_ty))
                .or_insert_with(|| actual_ty.clone());
        }
        (Ty::List(f, _), Ty::List(a, _)) => {
            infer_bindings_inner(f, a, bindings, allow_typevar_actuals, rigid);
        }
        (Ty::Map(fk, fv, _), Ty::Map(ak, av, _)) => {
            infer_bindings_inner(fk, ak, bindings, allow_typevar_actuals, rigid);
            infer_bindings_inner(fv, av, bindings, allow_typevar_actuals, rigid);
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
                infer_bindings_inner(&fp.ty, &ap.ty, bindings, allow_typevar_actuals, rigid);
            }
            infer_bindings_inner(fr, ar, bindings, allow_typevar_actuals, rigid);
            infer_bindings_inner(fth, ath, bindings, allow_typevar_actuals, rigid);
        }
        (Ty::Class(fn_name, f_args, _), Ty::Class(an_name, a_args, _))
        | (Ty::Interface(fn_name, f_args, _, _), Ty::Interface(an_name, a_args, _, _))
            if fn_name == an_name =>
        {
            for (ft, at) in f_args.iter().zip(a_args.iter()) {
                infer_bindings_inner(ft, at, bindings, allow_typevar_actuals, rigid);
            }
        }
        // `Future<T, E>` is its own variant — descend into both params so the
        // future combinators can infer `<T, E>` from a `Future<T, E>[]` arg.
        (Ty::Future(f_value, f_error, _), Ty::Future(a_value, a_error, _)) => {
            infer_bindings_inner(f_value, a_value, bindings, allow_typevar_actuals, rigid);
            infer_bindings_inner(f_error, a_error, bindings, allow_typevar_actuals, rigid);
        }
        // A heterogeneous future array — e.g. `[spawn { 1 }, spawn { 2 }]` —
        // types as `(Future<A, EA> | Future<B, EB>)[]` because `Future` is
        // invariant. Match the `Future<T, E>` formal against each union member
        // so `T`/`E` bind to the union of the member value/error types (the
        // TypeVar arm merges the per-member bindings via `union_ty`).
        (Ty::Future(_, _, _), Ty::Union(members, _)) => {
            for member in members {
                infer_bindings_inner(formal, member, bindings, allow_typevar_actuals, rigid);
            }
        }
        // Builtin container bridging: Array<T> ↔ List(T), Map<K,V> ↔ Map(K,V)
        // This enables UFCS calls like `Array.length(arr)` where the formal self
        // type is Class(Array, [T]) and the actual is List(int).
        (Ty::Class(class_name, f_args, _), Ty::List(actual_inner, _))
            if class_name.is_builtin_root_type("Array") && f_args.len() == 1 =>
        {
            infer_bindings_inner(
                &f_args[0],
                actual_inner,
                bindings,
                allow_typevar_actuals,
                rigid,
            );
        }
        (Ty::Class(class_name, f_args, _), Ty::Map(actual_key, actual_val, _))
            if class_name.is_builtin_root_type("Map") && f_args.len() == 2 =>
        {
            infer_bindings_inner(
                &f_args[0],
                actual_key,
                bindings,
                allow_typevar_actuals,
                rigid,
            );
            infer_bindings_inner(
                &f_args[1],
                actual_val,
                bindings,
                allow_typevar_actuals,
                rigid,
            );
        }
        _ => {} // Concrete types: nothing to infer
    }
}

pub fn infer_bindings(formal: &Ty, actual: &Ty, bindings: &mut FxHashMap<Name, Ty>) {
    infer_bindings_inner(formal, actual, bindings, false, None);
}

pub fn infer_bindings_allow_typevars(formal: &Ty, actual: &Ty, bindings: &mut FxHashMap<Name, Ty>) {
    infer_bindings_inner(formal, actual, bindings, true, None);
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
    infer_bindings_inner(formal, actual, bindings, false, rigid);
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
        Ty::Union(tys, _) => members.extend(tys.iter().cloned()),
        other => members.push(other.clone()),
    }
    match b {
        Ty::Union(tys, _) => {
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
        Ty::Union(members, TyAttr::default())
    }
}

/// Replace any remaining `Ty::TypeVar` with `Ty::Unknown` and emit diagnostics.
///
/// Called after call-site inference to ensure no type variables escape to
/// VIR/runtime. Each erased `TypeVar` produces a `CannotInferTypeParameter`
/// diagnostic.
#[allow(clippy::only_used_in_recursion)] // diagnostics param kept for future use
/// Replace every `Ty::TypeVar` for which `pred` returns `true` with
/// `Ty::Never`, recursing structurally. Used to drop UNBOUND callee generics
/// from an instantiated throws type: an unconstrained type variable there
/// means "nothing nameable is thrown", and `Never` vanishes from throw-fact
/// sets (`flatten_ty_to_facts` skips it), while a raw `TypeVar` fact would
/// poison the enclosing effective-throws surface.
pub fn erase_typevars_where(ty: &Ty, pred: &dyn Fn(&Name) -> bool) -> Ty {
    match ty {
        Ty::TypeVar(name, attr) => {
            if pred(name) {
                Ty::Never { attr: attr.clone() }
            } else {
                ty.clone()
            }
        }
        Ty::List(inner, attr) => {
            Ty::List(Box::new(erase_typevars_where(inner, pred)), attr.clone())
        }
        Ty::Map(k, v, attr) => Ty::Map(
            Box::new(erase_typevars_where(k, pred)),
            Box::new(erase_typevars_where(v, pred)),
            attr.clone(),
        ),
        Ty::Union(members, attr) => Ty::Union(
            members
                .iter()
                .map(|m| erase_typevars_where(m, pred))
                .collect(),
            attr.clone(),
        ),
        Ty::EvolvingList(inner, attr) => {
            Ty::EvolvingList(Box::new(erase_typevars_where(inner, pred)), attr.clone())
        }
        Ty::EvolvingMap(k, v, attr) => Ty::EvolvingMap(
            Box::new(erase_typevars_where(k, pred)),
            Box::new(erase_typevars_where(v, pred)),
            attr.clone(),
        ),
        Ty::Class(name, type_args, attr) => Ty::Class(
            name.clone(),
            type_args
                .iter()
                .map(|t| erase_typevars_where(t, pred))
                .collect(),
            attr.clone(),
        ),
        Ty::Interface(name, type_args, associated_bindings, attr) => Ty::Interface(
            name.clone(),
            type_args
                .iter()
                .map(|t| erase_typevars_where(t, pred))
                .collect(),
            associated_bindings
                .iter()
                .map(|(n, t)| (n.clone(), erase_typevars_where(t, pred)))
                .collect(),
            attr.clone(),
        ),
        Ty::AssociatedTypeProjection {
            base,
            interface,
            member,
            attr,
        } => Ty::AssociatedTypeProjection {
            base: Box::new(erase_typevars_where(base, pred)),
            interface: interface
                .as_ref()
                .map(|interface| Box::new(erase_typevars_where(interface, pred))),
            member: member.clone(),
            attr: attr.clone(),
        },
        Ty::Future(value, error, attr) => Ty::Future(
            Box::new(erase_typevars_where(value, pred)),
            Box::new(erase_typevars_where(error, pred)),
            attr.clone(),
        ),
        Ty::Function {
            generic_params,
            generic_param_bounds,
            params,
            ret,
            throws,
            attr,
        } => {
            // A function type's own generic params are local binders —
            // shadow them out of the predicate before recursing.
            let shadowed = |name: &Name| !generic_params.iter().any(|g| g == name) && pred(name);
            let shadowed: &dyn Fn(&Name) -> bool = &shadowed;
            Ty::Function {
                generic_params: generic_params.clone(),
                generic_param_bounds: generic_param_bounds
                    .iter()
                    .map(|b| b.as_ref().map(|t| erase_typevars_where(t, shadowed)))
                    .collect(),
                params: params
                    .iter()
                    .map(|p| FunctionParamTy {
                        name: p.name.clone(),
                        ty: erase_typevars_where(&p.ty, shadowed),
                        mode: p.mode,
                    })
                    .collect(),
                ret: Box::new(erase_typevars_where(ret, shadowed)),
                throws: Box::new(erase_typevars_where(throws, shadowed)),
                attr: attr.clone(),
            }
        }
        // Leaves (primitives, enums, etc.) — pass through.
        _ => ty.clone(),
    }
}

#[allow(clippy::only_used_in_recursion)]
pub fn erase_unresolved_typevars(
    ty: &Ty,
    diagnostics: &mut Vec<crate::infer_context::TirTypeError>,
) -> Ty {
    match ty {
        Ty::TypeVar(_, _) => {
            // Preserve TypeVars — they represent the enclosing function's generic
            // parameter and will be resolved at the outer call site.
            ty.clone()
        }
        Ty::List(inner, attr) => Ty::List(
            Box::new(erase_unresolved_typevars(inner, diagnostics)),
            attr.clone(),
        ),
        Ty::Map(k, v, attr) => Ty::Map(
            Box::new(erase_unresolved_typevars(k, diagnostics)),
            Box::new(erase_unresolved_typevars(v, diagnostics)),
            attr.clone(),
        ),
        Ty::AssociatedTypeProjection {
            base,
            interface,
            member,
            attr,
        } => Ty::AssociatedTypeProjection {
            base: Box::new(erase_unresolved_typevars(base, diagnostics)),
            interface: interface
                .as_ref()
                .map(|interface| Box::new(erase_unresolved_typevars(interface, diagnostics))),
            member: member.clone(),
            attr: attr.clone(),
        },
        Ty::Function {
            generic_params,
            generic_param_bounds,
            params,
            ret,
            throws,
            attr,
        } => Ty::Function {
            generic_params: generic_params.clone(),
            generic_param_bounds: generic_param_bounds
                .iter()
                .map(|bound| {
                    bound
                        .as_ref()
                        .map(|ty| erase_unresolved_typevars(ty, diagnostics))
                })
                .collect(),
            params: params
                .iter()
                .map(|param| FunctionParamTy {
                    name: param.name.clone(),
                    ty: erase_unresolved_typevars(&param.ty, diagnostics),
                    mode: param.mode,
                })
                .collect(),
            ret: Box::new(erase_unresolved_typevars(ret, diagnostics)),
            throws: Box::new(erase_unresolved_typevars(throws, diagnostics)),
            attr: attr.clone(),
        },
        Ty::Union(tys, attr) => Ty::Union(
            tys.iter()
                .map(|t| erase_unresolved_typevars(t, diagnostics))
                .collect(),
            attr.clone(),
        ),
        Ty::Class(name, type_args, attr) => Ty::Class(
            name.clone(),
            type_args
                .iter()
                .map(|t| erase_unresolved_typevars(t, diagnostics))
                .collect(),
            attr.clone(),
        ),
        Ty::Future(value, error, attr) => Ty::Future(
            Box::new(erase_unresolved_typevars(value, diagnostics)),
            Box::new(erase_unresolved_typevars(error, diagnostics)),
            attr.clone(),
        ),
        other => other.clone(),
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
        Ty::TypeVar(name, attr) if should_erase(name) => Ty::BuiltinUnknown { attr: attr.clone() },
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
        Ty::EvolvingList(inner, attr) => Ty::EvolvingList(
            Box::new(erase_typevars_matching(inner, should_erase)),
            attr.clone(),
        ),
        Ty::Map(key, value, attr) => Ty::Map(
            Box::new(erase_typevars_matching(key, should_erase)),
            Box::new(erase_typevars_matching(value, should_erase)),
            attr.clone(),
        ),
        Ty::EvolvingMap(key, value, attr) => Ty::EvolvingMap(
            Box::new(erase_typevars_matching(key, should_erase)),
            Box::new(erase_typevars_matching(value, should_erase)),
            attr.clone(),
        ),
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
            interface: interface
                .as_ref()
                .map(|interface| Box::new(erase_typevars_matching(interface, should_erase))),
            member: member.clone(),
            attr: attr.clone(),
        },
        Ty::Function {
            generic_params,
            generic_param_bounds,
            params,
            ret,
            throws,
            attr,
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
            attr: attr.clone(),
        },
        _ => ty.clone(),
    }
}
