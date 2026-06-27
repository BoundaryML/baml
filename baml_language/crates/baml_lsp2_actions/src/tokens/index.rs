//! Per-file resolution index for expression-body name occurrences.
//!
//! For every inference-bearing scope (function body, lambda/closure, top-level
//! `let` initializer — reached uniformly via `scope_body`), records the exact
//! source span of each classifiable name occurrence -> (token type, modifiers):
//!
//! - **path roots** (`x`, `foo`, `baml`, `Status`) resolved by offset,
//! - **local-rooted path segments** (`obj.a.b`) from `path_member_resolutions`,
//! - **type/package-rooted path leaves** (`Status.Active`, `baml.env.get`) from
//!   the path expression's `resolution` / `resolve_path_at`, with inner segments
//!   as namespaces,
//! - **member accesses** (`p.name`, `s.Celebrate()`) from `resolution`.
//!
//! Spans come from the AST source map (`path_segment_span`,
//! `member_access_member_span`) — never substring scanning — and kinds come from
//! name resolution — never the `Ty`. Pattern bindings (`let x`, match arms) are
//! handled by the walker from their `BINDING_PATTERN` CST nodes, not here.

use std::collections::HashMap;

use baml_base::SourceFile;
use baml_compiler2_ast::{Expr, ExprBody, ExprId};
use baml_compiler2_hir::scope::ScopeKind;
use baml_compiler2_tir::{
    inference::{ScopeInference, infer_scope_types, scope_body},
    resolve::{ResolvedName, resolve_name_at, resolve_path_at},
};
use text_size::{TextRange, TextSize};

use super::{ModifierSet, SemanticTokenType, classify};
use crate::Db;

/// A classification keyed by the exact source span of a name occurrence.
pub(super) type ResolutionIndex = HashMap<TextRange, (SemanticTokenType, ModifierSet)>;

/// Record `class` at `span` unless the span is empty or already classified
/// (first writer wins, matching document order).
fn record(index: &mut ResolutionIndex, span: TextRange, class: (SemanticTokenType, ModifierSet)) {
    if !span.is_empty() {
        index.entry(span).or_insert(class);
    }
}

/// Build the resolution index over every inference-bearing scope in `file`.
///
/// Iterates each `Function` / `Lambda` / `Let` scope and indexes its body via
/// the uniform [`scope_body`] lookup, so names inside lambdas, `spawn` blocks,
/// and other closures are classified with their own scope's inference — not
/// just top-level function bodies. Synthetic template-string bodies are skipped:
/// their expressions are typed inline in the enclosing scope and indexed there.
pub(super) fn build(db: &dyn Db, file: SourceFile) -> ResolutionIndex {
    let mut index = ResolutionIndex::new();
    let sem_index = baml_compiler2_hir::file_semantic_index(db, file);

    for (i, scope) in sem_index.scopes.iter().enumerate() {
        if scope.is_template_body
            || !matches!(
                scope.kind,
                ScopeKind::Function | ScopeKind::Lambda | ScopeKind::Let
            )
        {
            continue;
        }
        let Some(body) = scope_body(db, sem_index.scope_ids[i]) else {
            continue;
        };
        let inference = infer_scope_types(db, body.scope);
        index_function(db, file, &body.expr_body, &body.source_map, inference, &mut index);
    }

    index
}

/// Index every classifiable name occurrence in one function body.
fn index_function(
    db: &dyn Db,
    file: SourceFile,
    expr_body: &ExprBody,
    source_map: &baml_compiler2_ast::AstSourceMap,
    inference: &ScopeInference<'_>,
    index: &mut ResolutionIndex,
) {
    let ns = (SemanticTokenType::Namespace, ModifierSet::empty());

    for (expr_id, expr) in expr_body.exprs.iter() {
        match expr {
            Expr::Path(segments) if !segments.is_empty() => {
                let seg_span = source_map.path_segment_span(expr_id, 0);
                // For a generic-applied callee (`id<int>(...)`), segment 0's span
                // covers the whole `id<int>`; narrow it to the root identifier so
                // it matches the WORD token the walker emits.
                let root_span =
                    TextRange::at(seg_span.start(), TextSize::of(segments[0].as_str()));
                let resolved_root = resolve_name_at(db, file, root_span.start(), &segments[0]);
                match classify::classify_resolved(&resolved_root) {
                    Some(class) => record(index, root_span, class),
                    // Unresolved root of a dotted path => a package/namespace.
                    None if segments.len() > 1 => record(index, root_span, ns),
                    None => {}
                }

                if segments.len() > 1 {
                    index_path_tail(db, file, expr_id, segments, source_map, inference, index);
                }
            }

            // `a.b` and `a?.b` (null chaining) — classify the member name from
            // the inference. Interface members (casts, `Self` methods) now record
            // a resolution like any other member, so an unresolved one is a real
            // unknown (e.g. a typo) and stays neutral.
            Expr::MemberAccess { .. } | Expr::OptionalMemberAccess { .. } => {
                if let Some(res) = inference.resolution(expr_id) {
                    record(
                        index,
                        source_map.member_access_member_span(expr_id),
                        classify::classify_member(res),
                    );
                }
            }

            _ => {}
        }
    }
}

/// Classify the non-root segments of a multi-segment path.
fn index_path_tail(
    db: &dyn Db,
    file: SourceFile,
    expr_id: ExprId,
    segments: &[baml_base::Name],
    source_map: &baml_compiler2_ast::AstSourceMap,
    inference: &ScopeInference<'_>,
    index: &mut ResolutionIndex,
) {
    // A local/parameter root (`self`, a `let` binding — resolved by offset, a
    // cache hit) means the whole tail is member accesses, never namespace/type
    // segments.
    let root_span = source_map.path_segment_span(expr_id, 0);
    let local_root = matches!(
        resolve_name_at(db, file, root_span.start(), &segments[0]),
        ResolvedName::Local { .. }
    );
    // Per-segment member resolutions for local-rooted paths (`obj.a.b`).
    let members = inference.path_member_resolution(expr_id);
    let leaf = segments.len() - 1;

    for k in 1..segments.len() {
        let span = source_map.path_segment_span(expr_id, k);
        // 1. A recorded field/variant/method resolution wins.
        if let Some(res) = members.and_then(|m| m.get(k - 1)) {
            record(index, span, classify::classify_member(res));
            continue;
        }
        // 2. A type-rooted leaf member (e.g. `Status.Active`) lives in
        //    `resolution`, not `path_member_resolution`.
        if k == leaf && members.is_none() {
            if let Some(res) = inference.resolution(expr_id) {
                record(index, span, classify::classify_member(res));
                continue;
            }
        }
        // 3. A local's members are never namespaces. Real members (including
        //    interface `Self` methods/fields) now carry a resolution handled
        //    above; anything still unresolved here is a genuine unknown (a typo),
        //    so leave it neutral rather than emit a bogus namespace.
        if local_root {
            continue;
        }
        // 4. Package/type-rooted: resolve the prefix so a namespace prefix stays
        //    a namespace while a type segment is classified as one.
        let resolved = resolve_path_at(db, file, span.start(), &segments[0..=k], None);
        match classify::classify_resolved(&resolved) {
            Some(class) => record(index, span, class),
            None => record(index, span, (SemanticTokenType::Namespace, ModifierSet::empty())),
        }
    }
}
