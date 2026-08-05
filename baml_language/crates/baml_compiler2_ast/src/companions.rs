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
    ai_task,
    llm_render_prompt,
    llm_build_request,
    llm_build_request_stream,
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
    // Client-sourced declarations retain the established request-inspection
    // companions for compatibility. Normal execution still goes through Task.
    if !matches!(&parent.declarative_meta, Some(DeclarativeMeta::Llm(llm)) if llm.client.is_some())
    {
        return None;
    }
    Some(make_llm_companion(
        parent,
        "render_prompt",
        &["baml", "llm", "PromptAst"],
    ))
}

fn llm_build_request(parent: &FunctionDef) -> Option<FunctionDef> {
    if !matches!(&parent.declarative_meta, Some(DeclarativeMeta::Llm(llm)) if llm.client.is_some())
    {
        return None;
    }
    Some(make_llm_companion(
        parent,
        "build_request",
        &["baml", "http", "Request"],
    ))
}

fn llm_build_request_stream(parent: &FunctionDef) -> Option<FunctionDef> {
    if !matches!(&parent.declarative_meta, Some(DeclarativeMeta::Llm(llm)) if llm.client.is_some())
    {
        return None;
    }
    Some(make_llm_companion(
        parent,
        "build_request_stream",
        &["baml", "http", "Request"],
    ))
}

fn ai_task(parent: &FunctionDef) -> Option<FunctionDef> {
    let Some(DeclarativeMeta::Llm(_)) = &parent.declarative_meta else {
        return None;
    };
    let (body, source_map) = llm_companion_body(parent, "task")?;
    let output = parent.return_type.clone()?;
    let return_type = (TypeExprKind::Path {
        segments: vec![Name::new("ai"), Name::new("Task")],
        generic_args: vec![output],
        associated_type_bindings: vec![],
        attrs: vec![],
    })
    .at(parent.span);

    Some(FunctionDef {
        name: Name::new(format!("{}$task", parent.name)),
        generic_params: parent.generic_params.clone(),
        params: parent.params.clone(),
        defaults: parent.defaults.clone(),
        return_type: Some(return_type),
        throws: None,
        body: Some(FunctionBodyDef::Expr(body, source_map)),
        declarative_meta: None,
        metadata: crate::ast::FunctionMetadata::user_facing(crate::ast::FunctionOrigin::Companion),
        attributes: vec![],
        docstring: parent.docstring.clone(),
        is_tagged_template_tag: false,
        span: parent.span,
        name_span: parent.name_span,
    })
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
    // Client-sourced declarations retain the established local parser
    // companion for compatibility. Parsing is not part of model execution.
    if !matches!(&parent.declarative_meta, Some(DeclarativeMeta::Llm(llm)) if llm.client.is_some())
    {
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
        params,
        defaults: parent.defaults.clone(),
        return_type,
        throws: None,
        body: Some(FunctionBodyDef::Expr(body, source_map)),
        declarative_meta: None,
        metadata: crate::ast::FunctionMetadata::user_facing(crate::ast::FunctionOrigin::Companion),
        attributes: vec![],
        docstring: parent.docstring.clone(),
        is_tagged_template_tag: parent.is_tagged_template_tag,
        span: parent.span,
        name_span: parent.name_span,
    })
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
    // Backtick-prompt parents stash a closure-carrying companion body while the
    // CST is available. Raw Jinja prompts use the registered-template fallback.
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
        params: parent.params.clone(),
        defaults: parent.defaults.clone(),
        return_type: Some(return_type),
        throws: None,
        body: Some(FunctionBodyDef::Expr(body, source_map)),
        declarative_meta: None,
        metadata: crate::ast::FunctionMetadata::user_facing(crate::ast::FunctionOrigin::Companion),
        attributes: vec![],
        docstring: parent.docstring.clone(),
        is_tagged_template_tag: parent.is_tagged_template_tag,
        span: parent.span,
        name_span: parent.name_span,
    }
}

/// The pre-lowered, closure-carrying companion body for `target` that a
/// backtick-prompt parent stashed during CST lowering (see
/// [`LlmBodyDef::companion_bodies`](crate::ast::LlmBodyDef::companion_bodies)),
/// or `None` for a raw Jinja prompt (which renders via the 3-arg builtin).
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
