//! Per-function body queries.
//!
//! Reads from the `ItemTree` (full AST data) — no CST re-parsing needed.
//! The semantic body (no spans) and the source map (spans) are split into
//! separate queries for Salsa early-cutoff.

use std::sync::Arc;

use baml_compiler2_ast::{AstSourceMap, BuiltinKind, ExprBody, FunctionBodyDef};

use crate::loc::FunctionLoc;

/// Semantic LLM function body — client name + prompt text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmBody {
    pub client: Option<baml_base::Name>,
    pub prompt: Option<baml_compiler2_ast::RawPrompt>,
}

/// Semantic function body — either an LLM prompt, an expression body, or missing.
///
/// No spans — those live in the companion `AstSourceMap` returned by
/// `function_body_source_map`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunctionBody {
    /// LLM function body (client + prompt template).
    Llm(LlmBody),
    /// Expression body (semantic arena, no spans).
    Expr(ExprBody),
    /// Rust-bound builtin implementation (`$rust_function` or `$rust_io_function`).
    Builtin(BuiltinKind),
    /// Body was omitted or could not be parsed.
    Missing,
}

/// Salsa query: semantic function body (no source map).
///
/// Downstream type-checking queries depend on this and will NOT re-run on
/// whitespace-only file changes (the `ExprBody` arena is span-free).
#[salsa::tracked]
pub fn function_body<'db>(db: &'db dyn crate::Db, function: FunctionLoc<'db>) -> Arc<FunctionBody> {
    let file = function.file(db);
    let item_tree = crate::file_item_tree(db, file);
    let func_data = &item_tree[function.id(db)];

    let body = match &func_data.body {
        Some(FunctionBodyDef::Expr(expr_body, _source_map)) => {
            FunctionBody::Expr(expr_body.clone())
        }
        Some(FunctionBodyDef::Llm(llm)) => FunctionBody::Llm(LlmBody {
            client: llm.client.clone(),
            prompt: llm.prompt.clone(),
        }),
        Some(FunctionBodyDef::Builtin(kind)) => FunctionBody::Builtin(*kind),
        None => FunctionBody::Missing,
    };

    Arc::new(body)
}

/// Salsa query: function body source map (spans only).
///
/// Re-runs on any file change, but because downstream type queries only depend
/// on `function_body`, they are unaffected by whitespace-only changes.
#[salsa::tracked]
pub fn function_body_source_map<'db>(
    db: &'db dyn crate::Db,
    function: FunctionLoc<'db>,
) -> Option<AstSourceMap> {
    let file = function.file(db);
    let item_tree = crate::file_item_tree(db, file);
    let func_data = &item_tree[function.id(db)];

    match &func_data.body {
        Some(FunctionBodyDef::Expr(_body, source_map)) => Some(source_map.clone()),
        _ => None,
    }
}
