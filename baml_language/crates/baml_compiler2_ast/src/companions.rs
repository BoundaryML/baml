//! Companion function expansion.
//!
//! A companion is a compiler-generated function derived from a parent function.
//! Each expander inspects a `FunctionDef` and optionally returns a companion
//! `FunctionDef`. Companions are complete, self-contained AST items that flow
//! through HIR → TIR → MIR → emit with zero special-casing.
//!
//! Adding a new companion = writing one `fn(&FunctionDef) -> Option<FunctionDef>`
//! and appending it to `COMPANIONS`.
//!
//! Every LLM function lives in the single-path ai world and gets four
//! companions: `$spec` (the bound, unrun `ai.FunctionSpec<Out>`), `$render_prompt`
//! (the spec's prompt rendered with the return type's output-format text),
//! `$build_request` (a network-free provider request preview), and `$parse`
//! (a network-free `baml.sap.parse<Out>` of an existing reply).
//! `$stream` is synthesized at PPIR level — its body needs the stream-expanded
//! return type, which only PPIR can compute.

use baml_base::Name;

use crate::{
    DeclarativeMeta,
    ast::{FunctionBodyDef, FunctionDef, Param, TypeExprKind},
    lower_expr_body,
};

type CompanionExpander = fn(&FunctionDef) -> Option<FunctionDef>;

const COMPANIONS: &[CompanionExpander] =
    &[llm_spec, llm_render_prompt, llm_build_request, llm_parse];

/// Run all companion expanders on the given function.
/// Works identically for top-level functions and class methods.
pub(crate) fn expand_companions(func: &FunctionDef) -> Vec<FunctionDef> {
    COMPANIONS
        .iter()
        .filter_map(|expand| expand(func))
        .collect()
}

/// The parent's LLM metadata when its spec desugar succeeded — the gate for
/// every companion. `None` for non-LLM functions and for LLM functions whose
/// prompt/client was unusable (a migration diagnostic already fired; extra
/// companions would only cascade).
fn spec_llm_meta(parent: &FunctionDef) -> Option<&crate::ast::LlmBodyDef> {
    match &parent.declarative_meta {
        Some(DeclarativeMeta::Llm(llm))
            if llm.companion_bodies.iter().any(|(t, _)| t == "spec") =>
        {
            Some(llm)
        }
        _ => None,
    }
}

/// The function's own parameters — the compiler-injected `client` and
/// `on_event` overrides belong to the runner, never to a companion's surface.
fn own_params(parent: &FunctionDef) -> Vec<Param> {
    parent
        .params
        .iter()
        .filter(|p| p.name.as_str() != "client" && p.name.as_str() != "on_event")
        .cloned()
        .collect()
}

fn companion_def(
    parent: &FunctionDef,
    name: Name,
    params: Vec<Param>,
    return_type: Option<crate::ast::TypeExpr>,
    body: (crate::ast::ExprBody, crate::ast::AstSourceMap),
) -> FunctionDef {
    FunctionDef {
        name,
        generic_params: parent.generic_params.clone(),
        params,
        defaults: parent.defaults.clone(),
        return_type,
        throws: None,
        body: Some(FunctionBodyDef::Expr(body.0, body.1)),
        declarative_meta: None,
        metadata: crate::ast::FunctionMetadata::user_facing(crate::ast::FunctionOrigin::Companion),
        attributes: vec![],
        docstring: parent.docstring.clone(),
        is_tagged_template_tag: parent.is_tagged_template_tag,
        span: parent.span,
        name_span: parent.name_span,
    }
}

/// Build the `<Fn>$spec` companion: the function's parameters in, an
/// `ai.FunctionSpec<Out>` out. The body was pre-lowered in `lower_cst` (the
/// CST backtick is unreachable here) and stashed under the `"spec"` key.
fn llm_spec(parent: &FunctionDef) -> Option<FunctionDef> {
    let llm = spec_llm_meta(parent)?;
    let (body, source_map) = llm
        .companion_bodies
        .iter()
        .find(|(t, _)| t == "spec")
        .map(|(_, b)| b.clone())?;
    let out = parent.return_type.clone()?;

    let return_type = (TypeExprKind::Path {
        segments: vec![Name::new("ai"), Name::new("FunctionSpec")],
        generic_args: vec![out],
        associated_type_bindings: vec![],
        attrs: vec![],
    })
    .at(parent.span);

    Some(companion_def(
        parent,
        Name::new(format!("{}$spec", parent.name)),
        own_params(parent),
        Some(return_type),
        (body, source_map),
    ))
}

/// Build the `<Fn>$render_prompt` companion: the spec's prompt rendered with
/// the return type's output-format text as a structural `ai.Prompt`.
fn llm_render_prompt(parent: &FunctionDef) -> Option<FunctionDef> {
    spec_llm_meta(parent)?;
    parent.return_type.as_ref()?;
    let params = own_params(parent);
    let generic_param_names: Vec<Name> = parent
        .generic_params
        .iter()
        .map(|p| p.name.clone())
        .collect();
    let body = lower_expr_body::synthesize_spec_render_prompt_body(
        parent.name.as_str(),
        &params,
        &generic_param_names,
        parent.span,
    );
    let return_type = (TypeExprKind::Path {
        segments: vec![Name::new("ai"), Name::new("Prompt")],
        generic_args: vec![],
        associated_type_bindings: vec![],
        attrs: vec![],
    })
    .at(parent.span);
    Some(companion_def(
        parent,
        Name::new(format!("{}$render_prompt", parent.name)),
        params,
        Some(return_type),
        body,
    ))
}

/// Build the `<Fn>$build_request` companion. It accepts the same parameters as
/// the parent plus its injected `client` override and delegates to the spec's
/// pure `FunctionSpec.build_request` method. This keeps request construction
/// provider-neutral while preserving the parent's default-client semantics.
fn llm_build_request(parent: &FunctionDef) -> Option<FunctionDef> {
    spec_llm_meta(parent)?;
    // Keep the injected `client` (default-client semantics) but not
    // `on_event`: a network-free request preview never runs the agent, so
    // there are no events to listen to.
    let params: Vec<Param> = parent
        .params
        .iter()
        .filter(|p| p.name.as_str() != "on_event")
        .cloned()
        .collect();
    let body = lower_expr_body::synthesize_spec_build_request_body(
        parent.name.as_str(),
        &own_params(parent),
        &parent
            .generic_params
            .iter()
            .map(|p| p.name.clone())
            .collect::<Vec<_>>(),
        parent.span,
    );
    Some(companion_def(
        parent,
        Name::new(format!("{}$build_request", parent.name)),
        params,
        Some(
            (TypeExprKind::Path {
                segments: vec![Name::new("baml"), Name::new("http"), Name::new("Request")],
                generic_args: vec![],
                associated_type_bindings: vec![],
                attrs: vec![],
            })
            .at(parent.span),
        ),
        body,
    ))
}

/// Build the `<Fn>$parse` companion: a network-free parse of an existing
/// reply string into the function's return type.
fn llm_parse(parent: &FunctionDef) -> Option<FunctionDef> {
    spec_llm_meta(parent)?;
    let return_type = parent.return_type.clone()?;
    let json_param = Param {
        name: Name::new("json"),
        type_expr: Some((TypeExprKind::String { attrs: vec![] }).at(parent.span)),
        default: None,
        span: parent.span,
        name_span: parent.name_span,
    };
    let body = lower_expr_body::synthesize_spec_parse_body(Some(return_type.clone()), parent.span);
    Some(companion_def(
        parent,
        Name::new(format!("{}$parse", parent.name)),
        vec![json_param],
        Some(return_type),
        body,
    ))
}
