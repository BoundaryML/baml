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
    ast::{FunctionBodyDef, FunctionDef, SpannedTypeExpr, TypeExpr},
    lower_cst::synthesize_llm_builtin_call,
};

type CompanionExpander = fn(&FunctionDef) -> Option<FunctionDef>;

const COMPANIONS: &[CompanionExpander] = &[llm_render_prompt, llm_build_request];

/// Run all companion expanders on the given function.
/// Works identically for top-level functions and class methods.
pub(crate) fn expand_companions(func: &FunctionDef) -> Vec<FunctionDef> {
    COMPANIONS
        .iter()
        .filter_map(|expand| expand(func))
        .collect()
}

fn llm_render_prompt(parent: &FunctionDef) -> Option<FunctionDef> {
    parent.llm_meta.as_ref()?;
    Some(make_llm_companion(
        parent,
        "render_prompt",
        &["baml", "llm", "PromptAst"],
    ))
}

fn llm_build_request(parent: &FunctionDef) -> Option<FunctionDef> {
    parent.llm_meta.as_ref()?;
    Some(make_llm_companion(
        parent,
        "build_request",
        &["baml", "http", "Request"],
    ))
}

fn make_llm_companion(
    parent: &FunctionDef,
    target: &str,
    return_type_path: &[&str],
) -> FunctionDef {
    let name = Name::new(format!("{}${}", parent.name, target));
    let return_type = SpannedTypeExpr {
        expr: TypeExpr::Path(return_type_path.iter().map(|s| Name::new(s)).collect()),
        span: parent.span,
    };
    let param_names: Vec<Name> = parent.params.iter().map(|p| p.name.clone()).collect();
    let (body, source_map) =
        synthesize_llm_builtin_call(target, parent.name.as_str(), &param_names, parent.span);
    FunctionDef {
        name,
        generic_params: parent.generic_params.clone(),
        params: parent.params.clone(),
        return_type: Some(return_type),
        throws: None,
        body: Some(FunctionBodyDef::Expr(body, source_map)),
        llm_meta: None,
        attributes: vec![],
        span: parent.span,
        name_span: parent.name_span,
    }
}
