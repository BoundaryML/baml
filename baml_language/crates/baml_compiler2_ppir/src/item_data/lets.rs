use baml_base::Name;
use baml_compiler2_ast as ast;
use baml_compiler2_hir::loc::LetLoc;
use text_size::TextRange;

/// Span-free semantic data for a top-level `let` binding.
///
/// The initializer is deliberately absent: it has its own query
/// (`body::let_body`), so editing an initializer cannot invalidate anything that
/// only depends on the binding itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LetData {
    pub name: Name,
    pub origin: ast::LetOrigin,
}

/// Spans for a `Let`, parallel to [`LetData`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LetSourceMap {
    /// Full source span of the declaration.
    pub span: TextRange,
    /// Span of just the binding's name.
    pub name_span: TextRange,
}

/// Semantic data for one `let` binding. Span-free — see the module docs.
#[salsa::tracked(returns(ref))]
pub fn let_data<'db>(db: &'db dyn crate::Db, binding: LetLoc<'db>) -> LetData {
    lower(db, binding).0
}

/// Spans for one `let` binding. Kept separate from [`let_data`] so that a
/// whitespace-only edit invalidates this but not the semantic data.
#[salsa::tracked(returns(ref))]
pub fn let_source_map<'db>(db: &'db dyn crate::Db, binding: LetLoc<'db>) -> LetSourceMap {
    lower(db, binding).1
}

fn lower<'db>(db: &'db dyn crate::Db, binding: LetLoc<'db>) -> (LetData, LetSourceMap) {
    let item_tree = crate::file_item_tree(db, binding.file(db));
    let data = &item_tree[binding.id(db)];

    (
        LetData {
            name: data.name.clone(),
            origin: data.origin,
        },
        LetSourceMap {
            span: data.span,
            name_span: data.name_span,
        },
    )
}
