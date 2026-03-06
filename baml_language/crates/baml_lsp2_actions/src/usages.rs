//! `usages_at` — find all references to the symbol at a cursor position.
//!
//! This is a regular function (not a Salsa query). It resolves the name at
//! `offset` to determine the target, then scans for all occurrences of that
//! name across the relevant scope:
//!
//! - **Top-level items** (class, function, enum, …): scan all source files'
//!   CSTs for `WORD` tokens that match the target name, confirm each via
//!   `resolve_name_at`, and collect as `Location`s.
//!
//! - **Locals** (let bindings, parameters): search only within the enclosing
//!   function's `ExprBody` for `Expr::Path` nodes that use the same name and
//!   resolve to the same local binding.
//!
//! ## Optimization
//!
//! Before parsing/walking a file's CST, we pre-filter by checking whether the
//! target name string appears anywhere in the file text. This avoids expensive
//! CST work for files that cannot possibly contain a reference.

use baml_base::{Name, SourceFile};
use baml_compiler_syntax::SyntaxKind;
use baml_compiler2_ast::{Expr, ExprBody};
use baml_compiler2_hir::{body::FunctionBody, loc::FunctionLoc, scope::ScopeKind};
use baml_compiler2_tir::resolve::{ResolvedName, resolve_name_at};
use rowan::NodeOrToken;
use text_size::TextSize;

use crate::{Db, definition::Location, utils};

// ── usages_at ─────────────────────────────────────────────────────────────────

/// Find all references to the symbol at `offset` in `file`.
///
/// Regular function (not cached). The expensive work is internally
/// Salsa-cached (`file_semantic_index`, `syntax_tree`, `function_body`, …).
///
/// Returns an empty `Vec` if the cursor is not on an identifier or if the
/// name cannot be resolved.
///
/// The definition site itself is NOT included in the results. Callers that
/// want "peek references + definition" should call `definition_at` separately
/// and decide whether to include it.
pub fn usages_at(db: &dyn Db, file: SourceFile, offset: TextSize) -> Vec<Location> {
    // ── Step 1: find and resolve the token at the cursor ─────────────────────
    let token = match utils::find_token_at_offset(db, file, offset) {
        Some(t) => t,
        None => return Vec::new(),
    };

    if token.kind() != SyntaxKind::WORD {
        return Vec::new();
    }

    let name_text = token.text().to_string();
    let name = Name::new(&name_text);

    let resolved = resolve_name_at(db, file, offset, &name);

    match &resolved {
        ResolvedName::Item(_) | ResolvedName::Builtin(_) => {
            // Top-level item — scan all source files.
            find_top_level_usages(db, file, &name_text, &resolved)
        }
        ResolvedName::Local {
            definition_site: Some(_),
            ..
        } => {
            // Local variable — only search in the enclosing function body.
            find_local_usages(db, file, offset, &name_text, &resolved)
        }
        ResolvedName::Local {
            definition_site: None,
            ..
        }
        | ResolvedName::Unknown => Vec::new(),
    }
}

// ── top-level usages ──────────────────────────────────────────────────────────

/// Scan all source files for references to a top-level item.
///
/// Pre-filters each file by checking if the name string appears in the raw
/// text before walking the CST.
fn find_top_level_usages(
    db: &dyn Db,
    current_file: SourceFile,
    name_text: &str,
    target_resolved: &ResolvedName<'_>,
) -> Vec<Location> {
    // Collect all user source files.
    let source_files = collect_source_files(db, current_file);

    let mut results = Vec::new();

    for sf in source_files {
        // Optimization: skip files that do not contain the name string at all.
        let text = sf.text(db);
        if !text.contains(name_text) {
            continue;
        }

        // Walk the CST for WORD tokens matching the target name.
        let root = baml_compiler_parser::syntax_tree(db, sf);
        for node_or_token in root.descendants_with_tokens() {
            let NodeOrToken::Token(tok) = node_or_token else {
                continue;
            };

            if tok.kind() != SyntaxKind::WORD {
                continue;
            }

            if tok.text() != name_text {
                continue;
            }

            // Confirm this token resolves to the same definition.
            let tok_offset = tok.text_range().start();
            let resolved_here = resolve_name_at(db, sf, tok_offset, &Name::new(name_text));

            if same_item_definition(&resolved_here, target_resolved) {
                results.push(Location {
                    file: sf,
                    range: tok.text_range(),
                });
            }
        }
    }

    results
}

/// Returns `true` when two `ResolvedName` values refer to the same top-level
/// item definition.
fn same_item_definition(a: &ResolvedName<'_>, b: &ResolvedName<'_>) -> bool {
    match (a, b) {
        (ResolvedName::Item(def_a), ResolvedName::Item(def_b)) => def_a == def_b,
        (ResolvedName::Builtin(def_a), ResolvedName::Builtin(def_b)) => def_a == def_b,
        // Allow Item vs Builtin matching in case one side resolved as Builtin
        // and the other as Item (shouldn't happen in practice, but be safe).
        _ => false,
    }
}

// ── local usages ──────────────────────────────────────────────────────────────

/// Search for references to a local variable within the enclosing function's
/// expression body.
///
/// We walk the `ExprBody` (span-free) and use the source map for positions.
/// For each `Expr::Path([name])` that resolves to the same local, we emit a
/// `Location` using the expression's span from the source map.
fn find_local_usages(
    db: &dyn Db,
    file: SourceFile,
    at_offset: TextSize,
    name_text: &str,
    target_resolved: &ResolvedName<'_>,
) -> Vec<Location> {
    let index = baml_compiler2_hir::file_semantic_index(db, file);
    let item_tree = baml_compiler2_hir::file_item_tree(db, file);

    // Find the enclosing Function scope.
    let scope_id = index.scope_at_offset(at_offset);
    let enclosing_func_scope = index
        .ancestor_scopes(scope_id)
        .into_iter()
        .find(|ancestor_id| {
            matches!(
                index.scopes[ancestor_id.index() as usize].kind,
                ScopeKind::Function
            )
        });

    let Some(enclosing_func_scope) = enclosing_func_scope else {
        return Vec::new();
    };

    let func_scope_range = index.scopes[enclosing_func_scope.index() as usize].range;

    // Find the function in the item tree by matching its span.
    let func_entry = item_tree
        .functions
        .iter()
        .find(|(_, f)| f.span == func_scope_range);

    let Some((func_local_id, _)) = func_entry else {
        return Vec::new();
    };

    let func_loc = FunctionLoc::new(db, file, *func_local_id);

    // We need an expression body and source map.
    let body = baml_compiler2_hir::body::function_body(db, func_loc);
    let FunctionBody::Expr(expr_body) = body.as_ref() else {
        return Vec::new();
    };

    let Some(source_map) = baml_compiler2_hir::body::function_body_source_map(db, func_loc) else {
        return Vec::new();
    };

    let name = Name::new(name_text);
    let mut results = Vec::new();

    collect_local_path_usages(
        db,
        file,
        expr_body,
        &name,
        target_resolved,
        &source_map,
        &mut results,
    );

    results
}

/// Walk an `ExprBody` and collect `Expr::Path([name])` occurrences that
/// resolve to the same local as `target_resolved`.
fn collect_local_path_usages(
    db: &dyn Db,
    file: SourceFile,
    expr_body: &ExprBody,
    name: &Name,
    target_resolved: &ResolvedName<'_>,
    source_map: &baml_compiler2_ast::AstSourceMap,
    results: &mut Vec<Location>,
) {
    for (expr_id, expr) in expr_body.exprs.iter() {
        let Expr::Path(segments) = expr else {
            continue;
        };

        // Only single-segment paths can refer to locals.
        if segments.len() != 1 || &segments[0] != name {
            continue;
        }

        // Get the span of this expression from the source map.
        let range = source_map.expr_span(expr_id);
        if range.is_empty() {
            continue;
        }

        // Confirm that this usage resolves to the same local.
        let use_offset = range.start();
        let resolved_here = resolve_name_at(db, file, use_offset, name);

        if same_local_definition(&resolved_here, target_resolved) {
            results.push(Location { file, range });
        }
    }
}

/// Returns `true` when two `ResolvedName::Local` values refer to the same
/// definition site.
fn same_local_definition(a: &ResolvedName<'_>, b: &ResolvedName<'_>) -> bool {
    match (a, b) {
        (
            ResolvedName::Local {
                definition_site: Some(site_a),
                ..
            },
            ResolvedName::Local {
                definition_site: Some(site_b),
                ..
            },
        ) => site_a == site_b,
        _ => false,
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Collect all source files known to the database for the same package as
/// `reference_file`.
///
/// The `Db` trait extends the compiler2 trait chain but does not expose a
/// direct file listing method at the `dyn Db` level. Instead, we enumerate
/// source files by walking `package_items` for the reference file's package —
/// every `Definition` in the package knows its `SourceFile`, so we collect
/// those (deduped).
///
/// We also always include `reference_file` itself, in case it contributes no
/// top-level items (e.g. a file that is only a consumer, not a definer).
fn collect_source_files(db: &dyn Db, reference_file: SourceFile) -> Vec<SourceFile> {
    use baml_compiler2_hir::{
        file_package::file_package,
        package::{PackageId, package_items},
    };

    let pkg_info = file_package(db, reference_file);
    let pkg_id = PackageId::new(db, pkg_info.package.clone());
    let items = package_items(db, pkg_id);

    // `PackageItems.namespaces` maps namespace path -> `NamespaceItems`.
    // `NamespaceItems.types` / `.values` map Name -> Definition.
    // Enumerate all Definitions and collect their unique SourceFiles.
    let mut files: Vec<SourceFile> = Vec::new();

    for ns_items in items.namespaces.values() {
        for def in ns_items.types.values() {
            let f = def.file(db);
            if !files.contains(&f) {
                files.push(f);
            }
        }
        for def in ns_items.values.values() {
            let f = def.file(db);
            if !files.contains(&f) {
                files.push(f);
            }
        }
    }

    // Always include the current file even if it has no contributions.
    if !files.contains(&reference_file) {
        files.push(reference_file);
    }

    files
}
