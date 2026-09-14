//! Per-function and per-let body queries.
//!
//! Reads from the `ItemTree` (full AST data) — no CST re-parsing needed.
//! The semantic body (no spans) and the source map (spans) are split into
//! separate queries for Salsa early-cutoff.

use std::sync::Arc;

use baml_compiler2_ast::{AstSourceMap, BuiltinKind, ExprBody, FunctionBodyDef};

use crate::loc::{FunctionLoc, LetLoc};

/// Semantic function body — either an expression body, a builtin, or missing.
///
/// No spans — those live in the companion `AstSourceMap` returned by
/// `function_body_source_map`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunctionBody {
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

/// Semantic let-binding body — the initializer expression, or missing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LetBody {
    /// Expression initializer (semantic arena, no spans).
    Expr(ExprBody),
    /// Initializer was omitted or could not be parsed.
    Missing,
}

/// Salsa query: semantic let-binding body (no source map).
///
/// Downstream type-checking queries depend on this and will NOT re-run on
/// whitespace-only file changes (the `ExprBody` arena is span-free).
#[salsa::tracked]
pub fn let_body<'db>(db: &'db dyn crate::Db, let_binding: LetLoc<'db>) -> Arc<LetBody> {
    let file = let_binding.file(db);
    let item_tree = crate::file_item_tree(db, file);
    let let_data = &item_tree[let_binding.id(db)];

    let body = match &let_data.initializer {
        Some((expr_body, _source_map)) => LetBody::Expr(expr_body.clone()),
        None => LetBody::Missing,
    };

    Arc::new(body)
}

/// Salsa query: let-binding body source map (spans only).
///
/// Re-runs on any file change, but because downstream type queries only depend
/// on `let_body`, they are unaffected by whitespace-only changes.
#[salsa::tracked]
pub fn let_body_source_map<'db>(
    db: &'db dyn crate::Db,
    let_binding: LetLoc<'db>,
) -> Option<AstSourceMap> {
    let file = let_binding.file(db);
    let item_tree = crate::file_item_tree(db, file);
    let let_data = &item_tree[let_binding.id(db)];

    let_data
        .initializer
        .as_ref()
        .map(|(_body, source_map)| source_map.clone())
}

/// A definition that owns an expression body - the key type for body queries
/// and (in `baml_compiler2_hir_ty`) inference, mirroring rust-analyzer's
/// `DefWithBodyId`.
///
/// Lambdas are deliberately not members: a lambda's body lives in its owner's
/// arena and is typed by the owner's inference pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update)]
pub enum BodyOwnerId<'db> {
    Function(FunctionLoc<'db>),
    Let(LetLoc<'db>),
    /// A function's parameter-default expressions - ONE owner per
    /// function because all its defaults share one arena
    /// (`FunctionDefaults.exprs`). Small expression contexts as ordinary
    /// body owners is rust-analyzer's shape (`DefWithBodyId::VariantId`
    /// for enum discriminants, `AnonConstId` in the new solver).
    ParameterDefaults(FunctionLoc<'db>),
}

impl<'db> BodyOwnerId<'db> {
    pub fn file(self, db: &'db dyn crate::Db) -> baml_base::SourceFile {
        match self {
            BodyOwnerId::Function(function) => function.file(db),
            BodyOwnerId::Let(let_binding) => let_binding.file(db),
            BodyOwnerId::ParameterDefaults(function) => function.file(db),
        }
    }
}

impl<'db> From<FunctionLoc<'db>> for BodyOwnerId<'db> {
    fn from(function: FunctionLoc<'db>) -> BodyOwnerId<'db> {
        BodyOwnerId::Function(function)
    }
}

impl<'db> From<LetLoc<'db>> for BodyOwnerId<'db> {
    fn from(let_binding: LetLoc<'db>) -> BodyOwnerId<'db> {
        BodyOwnerId::Let(let_binding)
    }
}

/// A body owner's body, borrowing the per-kind query results without cloning
/// the arenas out of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnerBody {
    Function(Arc<FunctionBody>),
    Let(Arc<LetBody>),
    ParameterDefaults(Arc<crate::signature::FunctionParameterDefaults>),
}

impl OwnerBody {
    /// The expression body, if the owner has one (`None` for builtin and
    /// missing bodies).
    pub fn expr_body(&self) -> Option<&ExprBody> {
        match self {
            OwnerBody::Function(body) => match body.as_ref() {
                FunctionBody::Expr(expr_body) => Some(expr_body),
                FunctionBody::Builtin(_) | FunctionBody::Missing => None,
            },
            OwnerBody::Let(body) => match body.as_ref() {
                LetBody::Expr(expr_body) => Some(expr_body),
                LetBody::Missing => None,
            },
            OwnerBody::ParameterDefaults(defaults) => Some(&defaults.defaults.exprs),
        }
    }
}
