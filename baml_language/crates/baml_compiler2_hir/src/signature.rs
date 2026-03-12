//! Per-function signature queries.
//!
//! Reads from the `ItemTree` (full AST data stored in Phase 1) — no CST access
//! needed. The semantic data (`TypeExpr`, no spans) and the source map (spans
//! only) are split into separate queries for Salsa early-cutoff: whitespace
//! changes re-run the source map query but NOT the signature query.

use std::sync::Arc;

use baml_compiler2_ast::TypeExpr;
use text_size::TextRange;

use crate::loc::FunctionLoc;

/// Compiler2 function signature — param names + unresolved `TypeExpr`.
///
/// No spans — those live in `SignatureSourceMap`.
/// `TypeExpr` is already span-free (spans live in `SpannedTypeExpr` at the
/// AST layer and are split out here).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionSignature {
    pub name: baml_base::Name,
    /// Parameter names paired with their unresolved type expressions.
    pub params: Vec<(baml_base::Name, TypeExpr)>,
    /// Return type (None if omitted).
    pub return_type: Option<TypeExpr>,
    /// Declared throws contract type (None if omitted).
    pub throws: Option<TypeExpr>,
}

/// Parallel span storage for a signature.
///
/// Kept separate from `FunctionSignature` so that whitespace-only source
/// changes only invalidate `function_signature_source_map`, not
/// `function_signature`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureSourceMap {
    /// One span per parameter, parallel to `FunctionSignature::params`.
    pub param_spans: Vec<TextRange>,
    /// Span of the return type annotation, if present.
    pub return_type_span: Option<TextRange>,
    /// Span of the throws type annotation, if present.
    pub throws_type_span: Option<TextRange>,
}

/// Shared implementation — reads from the `ItemTree` (full AST data),
/// splits into semantic (`TypeExpr`, no spans) + source map (spans only).
fn function_signature_with_source_map<'db>(
    db: &'db dyn crate::Db,
    function: FunctionLoc<'db>,
) -> (Arc<FunctionSignature>, SignatureSourceMap) {
    let file = function.file(db);
    let item_tree = crate::raw_file_item_tree(db, file);
    let func_data = &item_tree[function.id(db)];

    // Build semantic signature — strip spans, keep TypeExpr
    let params: Vec<_> = func_data
        .params
        .iter()
        .map(|p| {
            let type_expr = p
                .type_expr
                .as_ref()
                .map(|te| te.expr.clone())
                .unwrap_or(TypeExpr::Unknown);
            (p.name.clone(), type_expr)
        })
        .collect();

    let return_type = func_data.return_type.as_ref().map(|te| te.expr.clone());

    let sig = Arc::new(FunctionSignature {
        name: func_data.name.clone(),
        params,
        return_type,
        throws: func_data.throws.as_ref().map(|te| te.expr.clone()),
    });

    // Build source map — spans only (separate for early-cutoff)
    let source_map = SignatureSourceMap {
        param_spans: func_data.params.iter().map(|p| p.span).collect(),
        return_type_span: func_data.return_type.as_ref().map(|te| te.span),
        throws_type_span: func_data.throws.as_ref().map(|te| te.span),
    };

    (sig, source_map)
}

/// Salsa query: semantic function signature (no spans).
///
/// Cached independently of the source map. Downstream type-checking queries
/// depend on this and will NOT re-run on whitespace-only file changes.
#[salsa::tracked]
pub fn function_signature<'db>(
    db: &'db dyn crate::Db,
    function: FunctionLoc<'db>,
) -> Arc<FunctionSignature> {
    let (signature, _) = function_signature_with_source_map(db, function);
    signature
}

/// Salsa query: function signature source map (spans only).
///
/// Re-runs on any file change (including whitespace), but because downstream
/// type queries only depend on `function_signature`, they are unaffected.
#[salsa::tracked]
pub fn function_signature_source_map<'db>(
    db: &'db dyn crate::Db,
    function: FunctionLoc<'db>,
) -> SignatureSourceMap {
    let (_, source_map) = function_signature_with_source_map(db, function);
    source_map
}
