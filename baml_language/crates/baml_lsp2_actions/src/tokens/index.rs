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

use std::{collections::HashMap, sync::Arc};

use baml_base::SourceFile;
use baml_compiler2_ast::{Expr, ExprBody, ExprId};
use baml_compiler2_hir::scope::{ScopeId, ScopeKind};
use baml_compiler2_tir::{
    inference::{ScopeInference, infer_scope_types, scope_body, scope_inference_owner},
    resolve::{ResolvedName, resolve_name_at, resolve_namespace_prefix, resolve_path_at},
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

/// On-demand classification of a single name token: which scope's inference
/// owns the expression at `range`, indexed lazily and cached per scope.
///
/// This is the rust-analyzer `Semantics::resolve(token)` model — a viewport /
/// range request only pays for the scopes it actually touches, instead of
/// resolving every scope in the file up front.
pub(super) fn resolve_token_class(
    db: &dyn Db,
    file: SourceFile,
    range: TextRange,
) -> Option<(SemanticTokenType, ModifierSet)> {
    let sem_index = baml_compiler2_hir::file_semantic_index(db, file);
    // Walk innermost scope -> ancestors. The token's expression may be indexed
    // by an enclosing inference-bearing scope rather than the innermost one
    // (e.g. a `test ... with <runner>` clause, or a value spanning nested
    // scopes), so probe up the chain — each `scope_resolution_index` is cached.
    let mut fsi = sem_index.scope_at_offset(range.start(), None);
    loop {
        let scope_id = sem_index.scope_ids[fsi.index() as usize];
        // Normalize to the inference-owner scope so sibling block/template
        // scopes that share an owner body hit one Salsa cache entry instead of
        // each rebuilding the same `scope_resolution_index` under a distinct key
        // (the `build` whole-file path already keys by the owner scope id).
        let owner = scope_inference_owner(db, scope_id);
        if let Some(class) = scope_resolution_index(db, owner).get(&range).copied() {
            return Some(class);
        }
        fsi = sem_index.scopes[fsi.index() as usize].parent?;
    }
}

/// The resolution index for one inference-bearing scope's body — span -> (token
/// type, modifiers) for every classifiable name occurrence in it. Salsa-cached
/// per `ScopeId` (rust-analyzer's body-granularity memoization): the first token
/// resolved in a scope pays for indexing its whole body; the rest are lookups.
#[salsa::tracked(returns(clone))]
pub(super) fn scope_resolution_index(db: &dyn Db, scope_id: ScopeId<'_>) -> Arc<ResolutionIndex> {
    let mut index = ResolutionIndex::new();
    // `scope_body` resolves `scope_id` to its inference owner (a Function /
    // Lambda / Let), so a token inside a `spawn`/block scope indexes — and is
    // found in — its owning body's index.
    if let Some(body) = scope_body(db, scope_id) {
        let inference = infer_scope_types(db, body.scope);
        index_function(
            db,
            scope_id.file(db),
            &body.expr_body,
            &body.source_map,
            inference,
            &mut index,
        );
    }
    Arc::new(index)
}

/// Whole-file resolution index — the merge of every inference-bearing scope's
/// (per-scope, salsa-cached) index. A full-document request classifies every
/// token anyway, so it builds the merge; editing one scope only invalidates
/// that scope's `scope_resolution_index`, not the whole file. A range request
/// instead resolves on demand via [`resolve_token_class`].
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
        for (&span, &class) in scope_resolution_index(db, sem_index.scope_ids[i]).iter() {
            record(&mut index, span, class);
        }
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
    for (expr_id, expr) in expr_body.exprs.iter() {
        match expr {
            Expr::Path(segments) if !segments.is_empty() => {
                let seg_span = source_map.path_segment_span(expr_id, 0);
                // For a generic-applied callee (`id<int>(...)`), segment 0's span
                // covers the whole `id<int>`; narrow it to the root identifier so
                // it matches the WORD token the walker emits.
                let root_span = TextRange::at(seg_span.start(), TextSize::of(segments[0].as_str()));
                let resolved_root = resolve_name_at(db, file, root_span.start(), &segments[0]);
                match classify::classify_resolved(&resolved_root) {
                    Some(class) => record(index, root_span, class),
                    // A primitive type as a path root (`string.from(...)`) — the
                    // same `defaultLibrary` Type as in type position.
                    None if classify::classify_primitive(segments[0].as_str()).is_some() => {
                        if let Some(class) = classify::classify_primitive(segments[0].as_str()) {
                            record(index, root_span, class);
                        }
                    }
                    // Root of a dotted path: a namespace ONLY if it actually
                    // resolves as one (a real package/namespace), never guessed —
                    // a typo'd prefix stays neutral.
                    None if segments.len() > 1 => {
                        if let Some(builtin) = resolve_namespace_prefix(db, file, &segments[0..1]) {
                            record(index, root_span, classify::namespace_class(builtin));
                        }
                    }
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
            // A namespace ONLY if the prefix actually resolves as one — never
            // guessed, so a typo'd segment stays neutral instead of bogus-namespace.
            None => {
                if let Some(builtin) = resolve_namespace_prefix(db, file, &segments[0..=k]) {
                    record(index, span, classify::namespace_class(builtin));
                }
            }
        }
    }
}
