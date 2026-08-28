//! Compiler-generated companion functions.
//!
//! The LLM spec recipe is lowered while its prompt CST is still available,
//! then moved into a private ordinary `Fn@spec` function. Once synthesized,
//! the companion follows the same AST -> HIR -> TIR -> MIR path as any other
//! function; `@spec` needs no expression or bytecode special case.

use baml_base::Name;

use crate::{
    DeclarativeMeta,
    ast::{FunctionBodyDef, FunctionDef, Param, TypeExprKind},
};

/// Build every AST-level companion for a function.
///
/// This is shared by top-level functions and class methods. Streaming remains
/// a PPIR companion because its partial output type is not known at this stage.
pub(crate) fn expand_companions(function: &FunctionDef) -> Vec<FunctionDef> {
    llm_spec(function).into_iter().collect()
}

/// The function's authored parameters. The compiler-injected direct-call
/// overrides belong to `Fn`, not to the spec recipe it constructs.
fn own_params(function: &FunctionDef) -> Vec<Param> {
    function
        .params
        .iter()
        .filter(|param| !matches!(param.name.as_str(), "client" | "on_event"))
        .cloned()
        .collect()
}

/// Synthesize private ordinary `Fn@spec(args...) -> ai.FunctionSpec<Out>`.
fn llm_spec(function: &FunctionDef) -> Option<FunctionDef> {
    let DeclarativeMeta::Llm(llm) = function.declarative_meta.as_ref()?;
    let (body, source_map) = llm.spec_body.as_ref()?.clone();
    let output = function.return_type.clone()?;
    let return_type = TypeExprKind::Path {
        segments: vec![Name::new("ai"), Name::new("FunctionSpec")],
        generic_args: vec![output],
        associated_type_bindings: vec![],
        attrs: vec![],
    }
    .at(function.span);

    Some(FunctionDef {
        name: Name::new(format!("{}@spec", function.name)),
        generic_params: function.generic_params.clone(),
        params: own_params(function),
        defaults: function.defaults.clone(),
        return_type: Some(return_type),
        throws: None,
        body: Some(FunctionBodyDef::Expr(body, source_map)),
        declarative_meta: None,
        metadata: crate::ast::FunctionMetadata::language_internal(
            crate::ast::FunctionOrigin::Companion,
        ),
        attributes: vec![],
        docstring: function.docstring.clone(),
        is_tagged_template_tag: function.is_tagged_template_tag,
        span: function.span,
        name_span: function.name_span,
    })
}
