//! Check declaration hooks using the ordinary call checker in the target's
//! generic environment. This pass never inserts an executable hook call.

use baml_base::Name;
use baml_compiler2_ast::{CallArg, Expr, ExprBody};
use baml_compiler2_hir::{
    body::BodyOwnerId,
    contributions::Definition,
    item_data::MethodOwner,
    loc::{DeclRef, FunctionLoc},
};
use baml_type::interned::{InferTy, Ty};
use rustc_hash::FxHashMap;

use super::{InferenceContext, owner_declared_bounds};
use crate::{
    diagnostics::{
        DiagnosticLocation, DiagnosticSeverity, RelatedLocation, RelatedNote, TirDiagnostic,
        TirTypeError,
    },
    lower::{function_generic_frame, function_signature, lower_ctx_for_file},
    render::Spell,
};

/// Checked invocation plan shared by diagnostics and executable lowering.
#[derive(Debug, Clone, PartialEq)]
pub struct TraceHookPlan<'db> {
    pub function: crate::extern_loc::FunctionRef<'db>,
    pub type_args: Vec<baml_type::Ty>,
    pub arguments: Vec<HookArgument>,
    pub return_ty: baml_type::Ty,
}

/// Parameter slots in the ordinary checked hook call, in declaration order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookArgument {
    /// Resolved target parameter slot, including a method's receiver.
    Target(usize),
    /// Detached call-local tracing settings.
    Settings,
    /// An omitted optional slot; its default executes in the hook's frame.
    Default,
}

pub fn declaration_diagnostics<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    function: FunctionLoc<'db>,
) -> Vec<TirDiagnostic<'db>> {
    checked_declaration(db, function).0
}

pub fn declaration_plan<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    function: FunctionLoc<'db>,
) -> Option<TraceHookPlan<'db>> {
    checked_declaration(db, function).1
}

fn checked_declaration<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    function: FunctionLoc<'db>,
) -> (Vec<TirDiagnostic<'db>>, Option<TraceHookPlan<'db>>) {
    let Some(path) = baml_compiler2_hir::item_data::function_trace_hook(db, function) else {
        return (Vec::new(), None);
    };
    let span = baml_compiler2_hir::item_data::function_trace_hook_span(db, function)
        .expect("hook source span");
    let hook_name = path.iter().map(Name::as_str).collect::<Vec<_>>().join(".");
    let package = baml_compiler2_hir::file_package::file_package(db, function.file(db));
    let viewpoint = crate::render::Viewpoint::user_facing(db, package.root);
    let mut target_path = package.namespace_path;
    match baml_compiler2_hir::item_data::method_owner(db, function) {
        Some(MethodOwner::Class(class)) => target_path.push(
            baml_compiler2_hir::item_data::class_data(db, class)
                .name
                .clone(),
        ),
        Some(MethodOwner::Interface(interface)) => target_path.push(
            baml_compiler2_hir::item_data::interface_data(db, interface)
                .name
                .clone(),
        ),
        Some(MethodOwner::Impl(_)) => {
            if let Some(owner) = crate::lower::owner_self_ty(db, function) {
                target_path = vec![Name::new(owner.spell(&viewpoint))];
            }
        }
        None => {}
    }
    target_path.push(
        baml_compiler2_hir::item_data::function_data(db, function)
            .name
            .clone(),
    );
    let target_name = target_path
        .iter()
        .map(Name::as_str)
        .collect::<Vec<_>>()
        .join(".");
    let invalid = |reason: String| TirDiagnostic {
        error: TirTypeError::InvalidTraceHook {
            message: format!(
                "Invalid trace hook `{hook_name}` for function `{target_name}`.\n{reason}"
            ),
        },
        severity: DiagnosticSeverity::Error,
        primary: DiagnosticLocation::Span(span),
        related: Vec::new(),
    };
    let target_body = baml_compiler2_hir::body::function_body(db, function);
    let baml_compiler2_hir::body::FunctionBody::Expr(target_body) = target_body.as_ref() else {
        return (
            vec![invalid(
            "The target has no BAML body.\nhelp: Attach the hook to a function with a BAML body."
                .into(),
        )],
            None,
        );
    };
    let owner = BodyOwnerId::Function(function);
    let signature = function_signature(db, function);
    let bounds = owner_declared_bounds(db, owner);
    let lower = lower_ctx_for_file(db, function.file(db))
        .with_frame(function_generic_frame(db, function))
        .with_bounds(bounds.clone())
        .with_self_ty(crate::lower::owner_self_ty(db, function));
    let options = crate::lower::reject_holes(
        &lower.lower_type_path(&[Name::new("trace"), Name::new("Options")]),
    );
    let settings = crate::lower::reject_holes(
        &lower.lower_type_path(&[Name::new("trace"), Name::new("Settings")]),
    );
    let expected = Ty::from_plain(&baml_type::Ty::Union(
        vec![options, baml_type::Ty::Null].into(),
    ));
    let mut context = InferenceContext::new(
        db,
        baml_compiler2_hir::file_semantic_index(db, function.file(db)),
        function.file(db),
        baml_compiler2_hir::body::body_scope(db, owner).map(|scope| scope.file_scope_id(db)),
        lower,
        vec![],
        None,
        baml_compiler2_hir::body_type_refs::body_type_refs(db, owner),
        bounds,
    );
    context.body_owner = Some(function);
    context.body_owner_id = Some(owner);
    context.declared_throws = Some(Ty::never());
    context.throws_channels[0].expected = Some(Ty::never());

    // Reserve source IDs; these expressions must not accidentally acquire a
    // body-local resolution. Hook lookup is in the declaration's namespace.
    let mut body = ExprBody::default();
    for _ in 0..target_body.exprs.len() {
        body.exprs.alloc(Expr::Missing);
    }
    let callee = body.exprs.alloc(Expr::Path(path.clone()));
    let call = body.exprs.alloc(Expr::Call {
        callee,
        type_args: vec![],
        args: vec![],
    });
    body.root_expr = Some(call);
    context.body_root = Some(call);
    let (hook_ty, bound) = context.infer_callee(&body, call, callee);
    let hook = context
        .result
        .member_resolutions
        .get(&callee)
        .or_else(|| {
            context
                .result
                .path_resolutions
                .get(&callee)
                .and_then(|path| path.segments.last())
                .and_then(|segment| segment.resolution.as_ref())
        })
        .and_then(|resolution| resolution.callable(db));
    let is_llm = baml_compiler2_hir::item_data::function_llm_meta(db, function).is_some();
    let params = signature
        .params
        .iter()
        .filter(|param| !is_llm || !matches!(param.name.as_str(), "client" | "on_event"))
        .collect::<Vec<_>>();
    let callable_without_arguments = if let InferTy::Function {
        params: hook_params,
        ..
    } = hook_ty.kind()
    {
        let hook_params = if bound {
            &hook_params[1..]
        } else {
            hook_params.as_ref()
        };
        let required = hook_params
            .iter()
            .filter(|param| param.mode == baml_type::FunctionParamMode::Required)
            .count();
        if required != 0 && required != params.len() + 1 {
            let mut expected_params = params
                .iter()
                .map(|param| format!("{}: {}", param.name, param.ty.spell(&viewpoint)))
                .collect::<Vec<_>>();
            expected_params.push("settings: trace.Settings".into());
            let mut diagnostic = invalid(format!(
                "The hook has {required} required parameter(s), but this target needs {}.\nExpected hook signature: ({}) -> trace.Options? throws never\nhelp: Accept the target's arguments in declaration order, followed by settings, or use a hook callable without arguments.",
                params.len() + 1,
                expected_params.join(", "),
            ));
            if let Some(DeclRef::Source(hook)) = hook {
                diagnostic.related.push(RelatedNote::new(
                    RelatedLocation::Item(Definition::Function(hook)),
                    "hook declared here",
                ));
            }
            return (vec![diagnostic], None);
        }
        // The ordinary call checker below handles omitted defaults rather
        // than requiring an exactly parameterless type.
        required == 0
    } else {
        false
    };

    // Like tagged-template parameters, these are typed compiler inputs without
    // source bindings. Their names cannot collide with any authored identifier.
    let mut inputs = FxHashMap::default();
    let mut args = Vec::new();
    if !callable_without_arguments {
        for (index, param) in params.iter().enumerate() {
            let name = Name::new(format!("$trace_hook_arg_{index}"));
            inputs.insert(name.clone(), crate::impls::interned_ty(&param.ty));
            args.push(CallArg::positional(
                body.exprs.alloc(Expr::Path(vec![name])),
            ));
        }
        let name = Name::new("$trace_hook_settings");
        inputs.insert(name.clone(), crate::impls::interned_ty(&settings));
        args.push(CallArg::positional(
            body.exprs.alloc(Expr::Path(vec![name])),
        ));
    }
    context.template_params.push(inputs);
    let result = context.check_call_args(&body, call, callee, &hook_ty, bound, &args);
    // Defaults have a separate inference owner and are not part of the
    // callable's body throws type. This synthetic call omits every optional
    // argument, so their effects must also satisfy the hook's `throws never`.
    let mut default_effect = None;
    if let Some(DeclRef::Source(hook)) = hook {
        let defaults = super::infer_body(db, BodyOwnerId::ParameterDefaults(hook));
        let type_args = context
            .result
            .call_plans
            .get(&call)
            .map_or(&[][..], |plan| plan.type_args.as_slice());
        let throws = crate::lower::substitute_params(
            &crate::impls::interned_ty(&defaults.throws),
            type_args,
        );
        context.record_throw(call, &throws);
        // Preserve this instantiated effect through writeback so diagnostics
        // can distinguish a throwing default from the hook body's effects.
        let effect = body.exprs.alloc(Expr::Missing);
        context.result.type_of_expr.insert(effect, throws);
        default_effect = Some(effect);
    }
    context.check_inferred(call, result.clone(), &expected);
    context.result.type_of_expr.insert(call, result);
    let result = context.finish(Some(&body));
    let default_only_throws = matches!(result.type_of_expr.get(&callee), Some(baml_type::Ty::Function { throws, .. }) if **throws == baml_type::Ty::Never)
        && default_effect
            .and_then(|expr| result.type_of_expr.get(&expr))
            .is_some_and(|ty| *ty != baml_type::Ty::Never);
    let diagnostics: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
        .map(|diagnostic| {
            let mut related = Vec::new();
            let reason = match &diagnostic.error {
                TirTypeError::TypeMismatch { expected, got } => {
                    let input_index = match &diagnostic.primary {
                        DiagnosticLocation::Expr(expr) => args.iter().position(|arg| arg.expr == *expr),
                        _ => None,
                    };
                    if let Some(index) = input_index {
                        let hook_index = index + usize::from(bound);
                        let hook_param = match hook_ty.kind() {
                            InferTy::Function { params, .. } => params.get(hook_index).and_then(|param| param.name.as_ref()),
                            _ => None,
                        };
                        let hook_param = hook_param.map_or_else(|| format!("#{}", index + 1), ToString::to_string);
                        if let Some(DeclRef::Source(hook)) = hook {
                            related.push(RelatedNote::new(RelatedLocation::Param(hook, hook_index), format!("hook parameter `{hook_param}` expects `{}`", expected.spell(&viewpoint))));
                        }
                        if let Some(target_param) = params.get(index) {
                            let source_index = signature.params.iter().position(|param| param.name == target_param.name).expect("target parameter");
                            related.push(RelatedNote::new(RelatedLocation::Param(function, source_index), format!("target parameter `{}` has type `{}`", target_param.name, got.spell(&viewpoint))));
                            format!("Target parameter `{}` has type `{}`, but hook parameter `{hook_param}` expects `{}`.\nhelp: Change hook parameter `{hook_param}` to accept `{}` or a compatible broader type.", target_param.name, got.spell(&viewpoint), expected.spell(&viewpoint), got.spell(&viewpoint))
                        } else {
                            format!("The runtime supplies `trace.Settings` as the final argument, but hook parameter `{hook_param}` expects `{}`.\nhelp: Make the final required hook parameter accept `trace.Settings`.", expected.spell(&viewpoint))
                        }
                    } else {
                        format!("The hook returns `{}`, but must return `trace.Options` or `null`.\nhelp: Return tracing options (for example `trace.rich()`) or `null` to keep the current settings.", got.spell(&viewpoint))
                    }
                }
                TirTypeError::ThrowsContractViolation { extra, .. } if default_only_throws => format!("The hook's omitted default expressions may throw `{}`, but trace hooks require `throws never`.\nhelp: Handle these errors inside the default expressions.", extra.spell(&viewpoint)),
                TirTypeError::ThrowsContractViolation { extra, .. } => format!("The hook may throw `{}`, but trace hooks require `throws never`.\nhelp: Handle these errors inside the hook or its default expressions.", extra.spell(&viewpoint)),
                TirTypeError::UnresolvedName { .. } => format!("Cannot resolve `{hook_name}` in the target's declaration scope.\nhelp: Define the hook or use its qualified function name in `/// baml:$trace=...`."),
                TirTypeError::NotCallable { ty } => format!("`{hook_name}` has type `{}` and is not callable.\nhelp: Reference a function returning `trace.Options?` with `throws never`.", ty.spell(&viewpoint)),
                _ => diagnostic.error.render(&viewpoint),
            };
            if related.is_empty() {
                if let Some(DeclRef::Source(hook)) = hook {
                    related.push(RelatedNote::new(RelatedLocation::Item(Definition::Function(hook)), "hook declared here"));
                }
            }
            let mut diagnostic = invalid(reason);
            diagnostic.related = related;
            diagnostic
        })
        .collect();
    let plan = if diagnostics.is_empty() {
        hook.and_then(|function| {
            result.call_plans.get(&call).map(|plan| TraceHookPlan {
                function,
                type_args: plan.type_args.clone(),
                arguments: plan
                    .bindings
                    .iter()
                    .map(|binding| match binding {
                        super::ParamBinding::OmittedDefault { .. } => HookArgument::Default,
                        super::ParamBinding::Provided { arg, .. } => {
                            let index = args
                                .iter()
                                .position(|input| input.expr == *arg)
                                .expect("checked hook argument");
                            params.get(index).map_or(HookArgument::Settings, |param| {
                                HookArgument::Target(
                                    signature
                                        .params
                                        .iter()
                                        .position(|source| source.name == param.name)
                                        .expect("target parameter"),
                                )
                            })
                        }
                    })
                    .collect(),
                return_ty: result.type_of_expr[&call].clone(),
            })
        })
    } else {
        None
    };
    (diagnostics, plan)
}
