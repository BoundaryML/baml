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
    ast::{FunctionBodyDef, FunctionDef, FunctionDefaults, Param, SpannedTypeExpr, TypeExpr},
    lower_cst::{synthesize_llm_builtin_call, synthesize_llm_parse_call},
};

type CompanionExpander = fn(&FunctionDef) -> Option<FunctionDef>;

const COMPANIONS: &[CompanionExpander] = &[
    llm_render_prompt,
    llm_build_request,
    llm_build_request_stream,
    llm_parse,
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

fn llm_parse(parent: &FunctionDef) -> Option<FunctionDef> {
    // Only LLM functions get parse companion.
    if !matches!(&parent.declarative_meta, Some(DeclarativeMeta::Llm(_))) {
        return None;
    }

    let name = Name::new(format!("{}$parse", parent.name));

    // Parse takes a single `json: string` parameter instead of the parent's params.
    let json_param = Param {
        name: Name::new("json"),
        type_expr: Some(SpannedTypeExpr {
            expr: TypeExpr::String { attrs: vec![] },
            span: parent.span,
        }),
        default: None,
        span: parent.span,
        name_span: parent.name_span,
    };

    // Return type is the same as the parent function.
    let return_type = parent.return_type.clone();

    let (body, source_map) = synthesize_llm_parse_call(parent.name.as_str(), parent.span);

    Some(FunctionDef {
        name,
        generic_params: parent.generic_params.clone(),
        params: vec![json_param],
        defaults: FunctionDefaults::empty(),
        return_type,
        throws: None,
        body: Some(FunctionBodyDef::Expr(body, source_map)),
        declarative_meta: None,
        origin: crate::ast::FunctionOrigin::Companion,
        attributes: vec![],
        docstring: parent.docstring.clone(),
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
    let return_type = SpannedTypeExpr {
        expr: TypeExpr::Path {
            segments: return_type_path.iter().map(Name::new).collect(),
            generic_args: vec![],
            attrs: vec![],
        },
        span: parent.span,
    };
    let param_names: Vec<Name> = parent.params.iter().map(|p| p.name.clone()).collect();
    // Extract client name from parent's LLM declarative meta.
    let client_name = match &parent.declarative_meta {
        Some(DeclarativeMeta::Llm(llm)) => llm.client.as_ref().map(|n| n.as_str().to_string()),
        _ => None,
    };
    let (body, source_map) = synthesize_llm_builtin_call(
        target,
        parent.name.as_str(),
        &param_names,
        client_name.as_deref(),
        Vec::new(),
        parent.span,
    );
    FunctionDef {
        name,
        generic_params: parent.generic_params.clone(),
        params: parent.params.clone(),
        defaults: parent.defaults.clone(),
        return_type: Some(return_type),
        throws: None,
        body: Some(FunctionBodyDef::Expr(body, source_map)),
        declarative_meta: None,
        origin: crate::ast::FunctionOrigin::Companion,
        attributes: vec![],
        docstring: parent.docstring.clone(),
        span: parent.span,
        name_span: parent.name_span,
    }
}
