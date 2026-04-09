use baml_base::Name;
use baml_compiler2_ast::{Expr, ExprBody, TypeExpr};
use baml_compiler2_hir::{package::PackageItems, signature::FunctionSignature};
use rustc_hash::FxHashSet;

use crate::{
    infer_context::TirTypeError,
    lower_type_expr::{
        FnTypeLoweringContext, lower_type_expr_in_ns, lower_type_expr_with_fn_context,
    },
    ty::{Ty, TyAttr},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DirectCallbackEffectVar {
    pub param_name: Name,
    pub effect_var: Name,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoweredCallableBoundary {
    pub params: Vec<(Option<Name>, Ty)>,
    pub ret: Ty,
    pub explicit_throws: Option<Ty>,
    pub direct_callback_effect_vars: Vec<DirectCallbackEffectVar>,
    pub param_diagnostics: Vec<Vec<TirTypeError>>,
    pub ret_diagnostics: Vec<TirTypeError>,
    pub throws_diagnostics: Vec<TirTypeError>,
}

impl LoweredCallableBoundary {
    pub(crate) fn used_direct_callback_effect_vars(&self, body: &ExprBody) -> Vec<Name> {
        let directly_invoked = directly_invoked_callback_params(body);
        self.direct_callback_effect_vars
            .iter()
            .filter(|effect_var| directly_invoked.contains(&effect_var.param_name))
            .map(|effect_var| effect_var.effect_var.clone())
            .collect()
    }
}

/// Lower a named callable boundary (function/method signature) with the typed
/// rethrows rule applied exactly once.
///
/// Only the outermost omitted `throws` on a direct function-typed parameter is
/// effect-polymorphic. Everything else remains closed-by-default.
pub(crate) fn lower_callable_boundary<'db>(
    db: &'db dyn crate::Db,
    package_items: &PackageItems<'db>,
    ns_context: &[Name],
    generic_params: &[Name],
    sig: &FunctionSignature,
    self_param_ty: Option<&Ty>,
) -> LoweredCallableBoundary {
    let mut direct_callback_effect_vars = Vec::new();
    let mut all_synthetic_effect_vars = Vec::new();
    let mut param_diagnostics = Vec::with_capacity(sig.params.len());

    let params: Vec<(Option<Name>, Ty)> = sig
        .params
        .iter()
        .map(|(param_name, param_ty_expr)| {
            if param_name.as_str() == "self" && matches!(param_ty_expr, TypeExpr::Unknown { .. }) {
                param_diagnostics.push(Vec::new());
                return (
                    Some(param_name.clone()),
                    self_param_ty.cloned().unwrap_or(Ty::Unknown {
                        attr: TyAttr::default(),
                    }),
                );
            }

            let mut slot_diagnostics = Vec::new();
            let mut param_effect_vars = Vec::new();
            let lowered = lower_type_expr_with_fn_context(
                db,
                param_ty_expr,
                package_items,
                ns_context,
                generic_params,
                &mut slot_diagnostics,
                &FnTypeLoweringContext::DirectParamRoot {
                    param_name: param_name.clone(),
                },
                &mut param_effect_vars,
            );
            if let Some(effect_var) = param_effect_vars.first() {
                direct_callback_effect_vars.push(DirectCallbackEffectVar {
                    param_name: param_name.clone(),
                    effect_var: effect_var.clone(),
                });
            }
            all_synthetic_effect_vars.extend(param_effect_vars);
            param_diagnostics.push(slot_diagnostics);
            (Some(param_name.clone()), lowered)
        })
        .collect();

    let effective_generic_params: Vec<Name> = generic_params
        .iter()
        .cloned()
        .chain(all_synthetic_effect_vars.iter().cloned())
        .collect();

    let mut ret_diagnostics = Vec::new();
    let ret = sig
        .return_type
        .as_ref()
        .map(|te| {
            lower_type_expr_in_ns(
                db,
                te,
                package_items,
                ns_context,
                &effective_generic_params,
                &mut ret_diagnostics,
            )
        })
        .unwrap_or(Ty::Unknown {
            attr: TyAttr::default(),
        });

    let mut throws_diagnostics = Vec::new();
    let explicit_throws = sig.throws.as_ref().map(|te| {
        lower_type_expr_in_ns(
            db,
            te,
            package_items,
            ns_context,
            generic_params,
            &mut throws_diagnostics,
        )
    });

    LoweredCallableBoundary {
        params,
        ret,
        explicit_throws,
        direct_callback_effect_vars,
        param_diagnostics,
        ret_diagnostics,
        throws_diagnostics,
    }
}

/// Syntactic scan for direct callback parameter invocation sites.
///
/// This is a conservative syntactic check, not a flow-sensitive analysis.
/// A direct callback position is only considered open when the body directly
/// calls the parameter path itself: `f()` or `f?.()`.
///
/// Aliased callback calls like `let g = f; g()` are intentionally not tracked,
/// so the wrapper's outward throws will not include the callback's throws in
/// that case. A future extension could follow simple single-assignment `let`
/// aliases.
pub(crate) fn directly_invoked_callback_params(body: &ExprBody) -> FxHashSet<Name> {
    let mut invoked = FxHashSet::default();
    for (_, expr) in body.exprs.iter() {
        let callee = match expr {
            Expr::Call { callee, .. } | Expr::OptionalCall { callee, .. } => *callee,
            _ => continue,
        };

        if let Expr::Path(segments) = &body.exprs[callee]
            && segments.len() == 1
        {
            invoked.insert(segments[0].clone());
        }
    }
    invoked
}
