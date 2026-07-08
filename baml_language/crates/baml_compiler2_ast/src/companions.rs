//! Companion function expansion.
//!
//! A companion is a compiler-generated function derived from a parent function.
//! Each expander inspects a `FunctionDef` and optionally returns a companion
//! `FunctionDef`. Companions are complete, self-contained AST items that flow
//! through HIR → TIR → MIR → emit with zero special-casing.
//!
//! Adding a new companion = writing one `fn(&FunctionDef) -> Option<FunctionDef>`
//! and appending it to `COMPANIONS`.

use baml_base::Name;

use crate::{
    DeclarativeMeta,
    ast::{FunctionBodyDef, FunctionDef, Param, TypeExpr, TypeExprKind},
    lower_cst::{synthesize_llm_builtin_call, synthesize_llm_parse_call},
};

type CompanionExpander = fn(&FunctionDef) -> Option<FunctionDef>;

// NOTE: the `$stream` and `$parse_stream` companions are synthesized at PPIR
// level (`baml_compiler2_ppir::ppir_expansion_items`), not here — their
// bodies need the stream-expanded return type, which only PPIR can compute.
// `$parse` (below) is defined here but likewise *invoked* from PPIR, with the
// stream-expanded type passed in.
const COMPANIONS: &[CompanionExpander] = &[
    llm_render_prompt,
    llm_build_request,
    llm_build_request_stream,
    llm_drive_with,
    llm_drive_run_tools,
    llm_drive_live,
];

/// Run all companion expanders on the given function.
/// Works identically for top-level functions and class methods.
pub(crate) fn expand_companions(func: &FunctionDef) -> Vec<FunctionDef> {
    COMPANIONS
        .iter()
        .filter_map(|expand| expand(func))
        .collect()
}

fn llm_render_prompt(parent: &FunctionDef) -> Option<FunctionDef> {
    // Only LLM functions get render_prompt / build_request companions.
    if !matches!(&parent.declarative_meta, Some(DeclarativeMeta::Llm(_))) {
        return None;
    }
    Some(make_llm_companion(
        parent,
        "render_prompt",
        &["baml", "llm", "PromptAst"],
    ))
}

fn llm_build_request(parent: &FunctionDef) -> Option<FunctionDef> {
    // Only LLM functions get render_prompt / build_request companions.
    if !matches!(&parent.declarative_meta, Some(DeclarativeMeta::Llm(_))) {
        return None;
    }
    Some(make_llm_companion(
        parent,
        "build_request",
        &["baml", "http", "Request"],
    ))
}

fn llm_build_request_stream(parent: &FunctionDef) -> Option<FunctionDef> {
    if !matches!(&parent.declarative_meta, Some(DeclarativeMeta::Llm(_))) {
        return None;
    }
    Some(make_llm_companion(
        parent,
        "build_request_stream",
        &["baml", "http", "Request"],
    ))
}

/// Build the `$parse` companion for an LLM function.
///
/// Unlike the expanders in `COMPANIONS`, this is invoked from PPIR
/// (`ppir_expansion_items`), not at CST-lowering time: the companion body
/// passes `<STREAM_EXPANDED, ORIGINAL>` as explicit type args
/// (`baml.llm.parse<...>(json)`), and the stream-expanded return type is
/// only computable with PPIR context (package items, block attrs, alias
/// bodies) — hence the extra `type_args` parameter.
pub fn llm_parse(parent: &FunctionDef, type_args: Vec<TypeExpr>) -> Option<FunctionDef> {
    // Only LLM functions get a parse companion.
    if !matches!(&parent.declarative_meta, Some(DeclarativeMeta::Llm(_))) {
        return None;
    }

    let name = Name::new(format!("{}$parse", parent.name));

    // Parse takes a `json: string` parameter instead of the parent's prompt
    // params, but keeps the LLM client's default override for API consistency.
    let json_param = Param {
        name: Name::new("json"),
        type_expr: Some((TypeExprKind::String { attrs: vec![] }).at(parent.span)),
        default: None,
        span: parent.span,
        name_span: parent.name_span,
    };

    // Return type is the same as the parent function.
    let return_type = parent.return_type.clone();

    let (body, source_map) = synthesize_llm_parse_call(type_args, parent.span);
    let mut params = vec![json_param];
    if let Some(client_param) = parent.params.iter().find(|p| p.name.as_str() == "client") {
        params.push(client_param.clone());
    }

    Some(FunctionDef {
        name,
        generic_params: parent.generic_params.clone(),
        generic_param_bounds: parent.generic_param_bounds.clone(),
        params,
        defaults: parent.defaults.clone(),
        return_type,
        throws: None,
        body: Some(FunctionBodyDef::Expr(body, source_map)),
        declarative_meta: None,
        origin: crate::ast::FunctionOrigin::Companion,
        attributes: vec![],
        docstring: parent.docstring.clone(),
        is_tagged_template_tag: parent.is_tagged_template_tag,
        llm_companion_suffix: None,
        span: parent.span,
        name_span: parent.name_span,
    })
}

// ── Capability-driver companions (DCP §1.3, Phase C) ────────────────────────
//
// `Foo$with` / `Foo$run_tools` / `Foo$live` delegate to the stdlib capability
// drivers (`baml.ai.drive_*`, the `//baml:llm_companion(<suffix>)`-marked
// functions). The body is uniformly:
//
//   baml.ai.drive_<suffix><T, …>(client, Foo$render_prompt(args…, client = client), extra…)
//
// Calling the sibling `$render_prompt` companion keeps this mode-agnostic
// (backtick closures vs legacy Jinja are its problem, not ours). The stdlib
// suffix table here mirrors the baked capability registry — user-package
// suffixes generate in Phase D via the HIR registry.

/// A `TypeExpr` path with no generics.
fn ty_path(segments: &[&str], span: text_size::TextRange) -> TypeExpr {
    (TypeExprKind::Path {
        segments: segments.iter().map(Name::new).collect(),
        generic_args: vec![],
        associated_type_bindings: vec![],
        attrs: vec![],
    })
    .at(span)
}

/// Also consumed by PPIR (`ppir_expansion_items`) to generate companions for
/// USER-package drivers from the capability registry (Phase D) — hence pub.
pub struct DriveCompanionSpec {
    pub suffix: Name,
    /// Path segments of the driver function (e.g. `["baml","ai","drive_with"]`).
    pub driver: Vec<Name>,
    /// Extra generic params appended to the companion (e.g. `V`, `E2`).
    pub extra_generics: Vec<Name>,
    /// Extra params appended after the parent's user params (name, type).
    pub extra_params: Vec<(Name, TypeExpr)>,
    /// Explicit type args for the driver call (in the driver's declared order).
    pub driver_type_args: Vec<TypeExpr>,
    pub return_type: TypeExpr,
}

pub fn make_drive_companion(parent: &FunctionDef, spec: DriveCompanionSpec) -> Option<FunctionDef> {
    use la_arena::Arena;

    use crate::ast::{AstSourceMap, CallArg, Expr, ExprBody};

    if !matches!(&parent.declarative_meta, Some(DeclarativeMeta::Llm(_))) {
        return None;
    }
    // The injected client param is the negotiation seam; without it there is
    // no provider to drive.
    if !parent.params.iter().any(|p| p.name.as_str() == "client") {
        return None;
    }
    let span = parent.span;

    // Body: drive_<suffix><…>(client, Foo$render_prompt(user args…, client = client), extras…)
    let mut exprs = Arena::new();
    let mut expr_spans = Arena::new();
    let mut alloc = |expr: Expr| -> crate::ast::ExprId {
        let id = exprs.alloc(expr);
        expr_spans.alloc(span);
        id
    };

    let client_ref = alloc(Expr::Path(vec![Name::new("client")]));
    let render_callee = alloc(Expr::Path(vec![Name::new(format!(
        "{}$render_prompt",
        parent.name
    ))]));
    // All user args ride BY NAME: defaulted parameters reject positional
    // passing (E0005), and named works uniformly for the rest.
    let mut render_args: Vec<CallArg> = parent
        .params
        .iter()
        .filter(|p| p.name.as_str() != "client")
        .map(|p| {
            let arg = alloc(Expr::Path(vec![p.name.clone()]));
            CallArg::named(p.name.clone(), arg)
        })
        .collect();
    let client_for_render = alloc(Expr::Path(vec![Name::new("client")]));
    render_args.push(CallArg::named("client", client_for_render));
    let rendered = alloc(Expr::Call {
        callee: render_callee,
        type_args: vec![],
        args: render_args,
    });

    let driver_callee = alloc(Expr::Path(spec.driver.clone()));
    let mut driver_args = vec![
        CallArg::positional(client_ref),
        CallArg::positional(rendered),
    ];
    for (extra_name, _) in &spec.extra_params {
        let arg = alloc(Expr::Path(vec![extra_name.clone()]));
        driver_args.push(CallArg::positional(arg));
    }
    let call = alloc(Expr::Call {
        callee: driver_callee,
        type_args: spec.driver_type_args,
        args: driver_args,
    });

    let body = ExprBody {
        exprs,
        stmts: Arena::new(),
        patterns: Arena::new(),
        match_arms: Arena::new(),
        catch_arms: Arena::new(),
        type_annotations: Arena::new(),
        root_expr: Some(call),
    };

    // Params: required user params, then the required extras, then the
    // parent's DEFAULTED user params, then the client param. Required extras
    // can't follow a defaulted param ("required parameter cannot appear after
    // a defaulted parameter"), and the body passes user args by name, so this
    // reordering is invisible to the call.
    let mut params: Vec<Param> = parent
        .params
        .iter()
        .filter(|p| p.name.as_str() != "client" && p.default.is_none())
        .cloned()
        .collect();
    for (extra_name, extra_ty) in spec.extra_params {
        params.push(Param {
            name: extra_name,
            type_expr: Some(extra_ty),
            default: None,
            span,
            name_span: parent.name_span,
        });
    }
    params.extend(
        parent
            .params
            .iter()
            .filter(|p| p.name.as_str() != "client" && p.default.is_some())
            .cloned(),
    );
    if let Some(client_param) = parent.params.iter().find(|p| p.name.as_str() == "client") {
        params.push(client_param.clone());
    }

    let mut generic_params = parent.generic_params.clone();
    let mut generic_param_bounds = parent.generic_param_bounds.clone();
    for g in spec.extra_generics {
        generic_params.push(g);
        generic_param_bounds.push(None);
    }

    Some(FunctionDef {
        name: Name::new(format!("{}${}", parent.name, spec.suffix)),
        generic_params,
        generic_param_bounds,
        params,
        defaults: parent.defaults.clone(),
        return_type: Some(spec.return_type),
        throws: None,
        body: Some(FunctionBodyDef::Expr(
            body,
            AstSourceMap {
                expr_spans,
                ..Default::default()
            },
        )),
        declarative_meta: None,
        origin: crate::ast::FunctionOrigin::Companion,
        attributes: vec![],
        docstring: None,
        is_tagged_template_tag: false,
        llm_companion_suffix: None,
        span,
        name_span: parent.name_span,
    })
}

/// `Foo$with(args…, project, client?) -> CallResult<T, V>` — value + sidecar.
fn llm_drive_with(parent: &FunctionDef) -> Option<FunctionDef> {
    let span = parent.span;
    let ret = parent.return_type.clone()?;
    let project_ty = (TypeExprKind::Function {
        params: vec![crate::ast::FunctionTypeParam {
            name: None,
            optional: false,
            ty: ty_path(&["baml", "ai", "ResponseMeta"], span),
        }],
        ret: Box::new(ty_path(&["V"], span)),
        throws: Some(Box::new(ty_path(&["E2"], span))),
        attrs: vec![],
    })
    .at(span);
    let return_type = (TypeExprKind::Path {
        segments: vec![Name::new("baml"), Name::new("ai"), Name::new("CallResult")],
        generic_args: vec![ret.clone(), ty_path(&["V"], span)],
        associated_type_bindings: vec![],
        attrs: vec![],
    })
    .at(span);
    make_drive_companion(
        parent,
        DriveCompanionSpec {
            suffix: Name::new("with"),
            driver: vec![Name::new("baml"), Name::new("ai"), Name::new("drive_with")],
            extra_generics: vec![Name::new("V"), Name::new("E2")],
            extra_params: vec![(Name::new("project"), project_ty)],
            driver_type_args: vec![ret, ty_path(&["V"], span), ty_path(&["E2"], span)],
            return_type,
        },
    )
}

/// `Foo$run_tools(args…, tools, dispatch, client?) -> T` — the explicit-control
/// agentic form (the primary tool surface is a ToolLoop client on plain `Foo`).
fn llm_drive_run_tools(parent: &FunctionDef) -> Option<FunctionDef> {
    let span = parent.span;
    let ret = parent.return_type.clone()?;
    let tools_ty = (TypeExprKind::List {
        inner: Box::new(ty_path(&["baml", "ai", "Tool"], span)),
        attrs: vec![],
    })
    .at(span);
    let dispatch_ty = (TypeExprKind::Function {
        params: vec![crate::ast::FunctionTypeParam {
            name: None,
            optional: false,
            ty: (TypeExprKind::List {
                inner: Box::new(ty_path(&["baml", "ai", "ToolCall"], span)),
                attrs: vec![],
            })
            .at(span),
        }],
        ret: Box::new(
            (TypeExprKind::List {
                inner: Box::new(ty_path(&["baml", "ai", "ToolResult"], span)),
                attrs: vec![],
            })
            .at(span),
        ),
        throws: Some(Box::new((TypeExprKind::Never { attrs: vec![] }).at(span))),
        attrs: vec![],
    })
    .at(span);
    make_drive_companion(
        parent,
        DriveCompanionSpec {
            suffix: Name::new("run_tools"),
            driver: vec![
                Name::new("baml"),
                Name::new("ai"),
                Name::new("drive_run_tools"),
            ],
            extra_generics: vec![],
            extra_params: vec![
                (Name::new("tools"), tools_ty),
                (Name::new("dispatch"), dispatch_ty),
            ],
            driver_type_args: vec![ret.clone()],
            return_type: ret,
        },
    )
}

/// `Foo$live(args…, io, client?) -> baml.ai.Transcript` — live duplex session.
fn llm_drive_live(parent: &FunctionDef) -> Option<FunctionDef> {
    let span = parent.span;
    let ret = parent.return_type.clone()?;
    make_drive_companion(
        parent,
        DriveCompanionSpec {
            suffix: Name::new("live"),
            driver: vec![Name::new("baml"), Name::new("ai"), Name::new("drive_live")],
            extra_generics: vec![],
            extra_params: vec![(Name::new("io"), ty_path(&["baml", "ai", "Channel"], span))],
            driver_type_args: vec![ret],
            return_type: ty_path(&["baml", "ai", "Transcript"], span),
        },
    )
}

fn make_llm_companion(
    parent: &FunctionDef,
    target: &str,
    return_type_path: &[&str],
) -> FunctionDef {
    let name = Name::new(format!("{}${}", parent.name, target));
    let return_type = (TypeExprKind::Path {
        segments: return_type_path.iter().map(Name::new).collect(),
        generic_args: vec![],
        associated_type_bindings: vec![],
        attrs: vec![],
    })
    .at(parent.span);
    let param_names: Vec<Name> = parent
        .params
        .iter()
        .filter(|p| p.name.as_str() != "client")
        .map(|p| p.name.clone())
        .collect();
    // Extract client name from parent's LLM declarative meta.
    let client_arg_name = match &parent.declarative_meta {
        Some(DeclarativeMeta::Llm(_))
            if parent.params.iter().any(|p| p.name.as_str() == "client") =>
        {
            Some("client")
        }
        Some(DeclarativeMeta::Llm(llm)) => llm.client.as_ref().map(smol_str::SmolStr::as_str),
        _ => None,
    };
    // New-mode (backtick) parents stashed a closure-carrying companion body for
    // this target during CST lowering (the backtick CST isn't reachable here);
    // use it so the companion renders the prompt through its closure — matching
    // execution. Legacy Jinja parents fall back to the plain 3-arg builtin call.
    let (body, source_map) = llm_companion_body(parent, target).unwrap_or_else(|| {
        synthesize_llm_builtin_call(
            target,
            parent.name.as_str(),
            &param_names,
            client_arg_name,
            Vec::new(),
            parent.span,
        )
    });
    FunctionDef {
        name,
        generic_params: parent.generic_params.clone(),
        generic_param_bounds: parent.generic_param_bounds.clone(),
        params: parent.params.clone(),
        defaults: parent.defaults.clone(),
        return_type: Some(return_type),
        throws: None,
        body: Some(FunctionBodyDef::Expr(body, source_map)),
        declarative_meta: None,
        origin: crate::ast::FunctionOrigin::Companion,
        attributes: vec![],
        docstring: parent.docstring.clone(),
        is_tagged_template_tag: parent.is_tagged_template_tag,
        llm_companion_suffix: None,
        span: parent.span,
        name_span: parent.name_span,
    }
}

/// The pre-lowered, closure-carrying companion body for `target` that a new-mode
/// (backtick) LLM parent stashed during CST lowering (see
/// [`LlmBodyDef::companion_bodies`](crate::ast::LlmBodyDef::companion_bodies)),
/// or `None` for a legacy Jinja parent (which renders via the 3-arg builtin).
fn llm_companion_body(
    parent: &FunctionDef,
    target: &str,
) -> Option<(crate::ast::ExprBody, crate::ast::AstSourceMap)> {
    let Some(DeclarativeMeta::Llm(llm)) = &parent.declarative_meta else {
        return None;
    };
    llm.companion_bodies
        .iter()
        .find(|(t, _)| t == target)
        .map(|(_, body)| body.clone())
}
