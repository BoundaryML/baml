//! IDE-facing lookups over the per-owner inference tables (rust-analyzer's
//! `Semantics` discipline, minimally): map a scope to the body OWNER whose
//! inference covers it, and hand back that owner's `InferenceResult`.
//! Lambdas type inline in their owner's run, so a lambda scope resolves to
//! the enclosing Function/Let owner and its table is a superset of what
//! the scope alone would hold.

use baml_compiler2_ast::{AstSourceMap, Expr as AstExpr, ExprBody, ExprId, LambdaDef};
use baml_compiler2_hir::{
    body::BodyOwnerId,
    scope::{FileScopeId, ScopeId, ScopeKind},
};
use text_size::TextRange;

use crate::infer::InferenceResult;

/// The body + source map of the scope that owns `scope_id`'s expressions,
/// with the scope-local root: a lambda's body expression, or the whole
/// body's root for a function / `let`. Walking `expr_body` flatly instead
/// would visit the entire enclosing function.
pub struct ScopeBody<'db> {
    /// The OWNER whose inference tables cover this scope.
    pub owner: BodyOwnerId<'db>,
    /// The expression this scope is rooted at.
    pub root: Option<ExprId>,
    /// The LAMBDA expression itself when this scope is a lambda scope
    /// (its type in the owner's tables is the lambda's `Function` type).
    pub scope_expr: Option<ExprId>,
    pub expr_body: ExprBody,
    pub source_map: AstSourceMap,
}

fn owner_scope_of(
    index: &baml_compiler2_hir::semantic_index::FileSemanticIndex<'_>,
    mut fsid: FileScopeId,
) -> FileScopeId {
    loop {
        let scope = &index.scopes[fsid.index() as usize];
        if matches!(scope.kind, ScopeKind::Function | ScopeKind::Let) {
            return fsid;
        }
        let Some(parent) = scope.parent else {
            return fsid;
        };
        fsid = parent;
    }
}

/// The body owner whose inference covers `scope_id`'s expressions: the
/// nearest enclosing Function/Let scope's recorded item owner.
pub fn owner_for_scope<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    scope_id: ScopeId<'db>,
) -> Option<BodyOwnerId<'db>> {
    let file = scope_id.file(db);
    let index = baml_compiler2_ppir::file_semantic_index(db, file);
    let owner_fsid = owner_scope_of(index, scope_id.file_scope_id(db));
    let owner_scope_id = index.scope_ids[owner_fsid.index() as usize];
    match baml_compiler2_ppir::item_data::scope_owner(db, owner_scope_id)? {
        baml_compiler2_ppir::item_data::ScopeOwner::Function(function) => {
            Some(BodyOwnerId::Function(function))
        }
        baml_compiler2_ppir::item_data::ScopeOwner::Let(let_binding) => {
            Some(BodyOwnerId::Let(let_binding))
        }
        _ => None,
    }
}

/// The inference covering `scope_id`, keyed off its owner.
pub fn infer_for_scope<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    scope_id: ScopeId<'db>,
) -> Option<&'db InferenceResult<'db>> {
    owner_for_scope(db, scope_id).map(|owner| crate::infer::infer_body(db, owner))
}

/// Resolve `scope_id` to its owning body, with the scope-local root (a
/// lambda scope roots at that lambda's body expression). `None` for
/// non-body scopes and bodiless owners.
pub fn scope_body<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    scope_id: ScopeId<'db>,
) -> Option<ScopeBody<'db>> {
    let file = scope_id.file(db);
    let index = baml_compiler2_ppir::file_semantic_index(db, file);
    let fsid = scope_id.file_scope_id(db);
    let owner_fsid = owner_scope_of(index, fsid);
    let owner_scope_id = index.scope_ids[owner_fsid.index() as usize];
    let owner = match baml_compiler2_ppir::item_data::scope_owner(db, owner_scope_id)? {
        baml_compiler2_ppir::item_data::ScopeOwner::Function(function) => {
            BodyOwnerId::Function(function)
        }
        baml_compiler2_ppir::item_data::ScopeOwner::Let(let_binding) => {
            BodyOwnerId::Let(let_binding)
        }
        _ => return None,
    };
    let expr_body = baml_compiler2_ppir::body(db, owner).expr_body()?.clone();
    let source_map = baml_compiler2_ppir::body_source_map(db, owner)?;
    // A LAMBDA scope (any non-template lambda between `scope_id` and the
    // owner) roots at its own body expression within the shared arena.
    let scope = &index.scopes[fsid.index() as usize];
    let (root, scope_expr) = if scope.kind == ScopeKind::Lambda && !scope.is_template_body {
        match find_lambda_by_span(&expr_body, &source_map, scope.range) {
            Some((lambda, lambda_expr)) => (lambda.body, Some(lambda_expr)),
            None => (None, None),
        }
    } else {
        (expr_body.root_expr, None)
    };
    Some(ScopeBody {
        owner,
        root,
        scope_expr,
        expr_body,
        source_map,
    })
}

fn find_lambda_by_span<'a>(
    body: &'a ExprBody,
    source_map: &AstSourceMap,
    target_span: TextRange,
) -> Option<(&'a LambdaDef, ExprId)> {
    body.exprs.iter().find_map(|(expr_id, expr)| match expr {
        AstExpr::Lambda(lambda) if source_map.expr_span(expr_id) == target_span => {
            Some((&**lambda, expr_id))
        }
        _ => None,
    })
}
