use baml_base::Name;
use baml_compiler2_ast::{Expr, ExprBody, TypeExpr};
use baml_compiler2_hir::{package::PackageItems, signature::FunctionSignature};

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
    diagnostics: &mut Vec<TirTypeError>,
) -> LoweredCallableBoundary {
    let mut direct_callback_effect_vars = Vec::new();
    let mut all_synthetic_effect_vars = Vec::new();

    let params: Vec<(Option<Name>, Ty)> = sig
        .params
        .iter()
        .map(|(param_name, param_ty_expr)| {
            let ty = if param_name.as_str() == "self"
                && matches!(param_ty_expr, TypeExpr::Unknown { .. })
            {
                self_param_ty.cloned().unwrap_or(Ty::Unknown {
                    attr: TyAttr::default(),
                })
            } else {
                let mut param_effect_vars = Vec::new();
                let lowered = lower_type_expr_with_fn_context(
                    db,
                    param_ty_expr,
                    package_items,
                    ns_context,
                    generic_params,
                    diagnostics,
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
                lowered
            };
            (Some(param_name.clone()), ty)
        })
        .collect();

    let effective_generic_params: Vec<Name> = generic_params
        .iter()
        .cloned()
        .chain(all_synthetic_effect_vars.iter().cloned())
        .collect();

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
                diagnostics,
            )
        })
        .unwrap_or(Ty::Unknown {
            attr: TyAttr::default(),
        });

    let explicit_throws = sig.throws.as_ref().map(|te| {
        lower_type_expr_in_ns(
            db,
            te,
            package_items,
            ns_context,
            generic_params,
            diagnostics,
        )
    });

    LoweredCallableBoundary {
        params,
        ret,
        explicit_throws,
        direct_callback_effect_vars,
    }
}

/// Syntactic scan for direct callback parameter invocation sites.
///
/// A direct callback position is only considered open when the body directly
/// calls the parameter path itself: `f()` or `f?.()`.
pub(crate) fn directly_invoked_callback_params(body: &ExprBody) -> Vec<Name> {
    let mut invoked = Vec::new();
    for (_, expr) in body.exprs.iter() {
        let callee = match expr {
            Expr::Call { callee, .. } | Expr::OptionalCall { callee, .. } => *callee,
            _ => continue,
        };

        if let Expr::Path(segments) = &body.exprs[callee]
            && segments.len() == 1
            && !invoked.contains(&segments[0])
        {
            invoked.push(segments[0].clone());
        }
    }
    invoked
}
