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
//! - **Locals** (let bindings, parameters): search the enclosing function and
//!   nested lambda arenas for `Expr::Path` nodes that resolve to the same local
//!   binding.
//!
//! ## Optimization
//!
//! Before parsing/walking a file's CST, we pre-filter by checking whether the
//! target name string appears anywhere in the file text. This avoids expensive
//! CST work for files that cannot possibly contain a reference.

use baml_base::{Name, SourceFile};
use baml_compiler_syntax::SyntaxKind;
use baml_compiler2_ast::{Expr, ExprBody};
use baml_compiler2_hir::{
    body::FunctionBody,
    scope::{FileScopeId, ScopeKind},
    semantic_index::{
        BindingId, ExprMetadataKey, ExprMetadataScope, FileSemanticIndex, PathResolution,
    },
};
use baml_compiler2_ppir::resolve::{ResolvedName, resolve_name_at};
use rowan::NodeOrToken;
use text_size::{TextRange, TextSize};

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
    let Some(token) = utils::find_token_at_offset(db, file, offset) else {
        return Vec::new();
    };

    if token.kind() != SyntaxKind::WORD {
        return Vec::new();
    }

    let name_text = token.text().to_string();
    let name = Name::new(&name_text);

    if let Some(target_binding) = local_binding_id_at(db, file, offset, &name) {
        return find_local_usages(db, file, offset, &name_text, target_binding);
    }

    let resolved = resolve_name_at(db, file, offset, &name);

    match &resolved {
        ResolvedName::Item(_) | ResolvedName::Builtin(_) => {
            // Top-level item — scan all source files.
            find_top_level_usages(db, file, &name_text, &resolved)
        }
        ResolvedName::Local {
            definition_site: Some(_),
            ..
        } => Vec::new(),
        ResolvedName::Local {
            definition_site: None,
            ..
        }
        | ResolvedName::Unknown => {
            // Try field-definition usages: if cursor is on a class field definition,
            // find all field access and constructor field sites.
            find_field_definition_usages(db, file, offset, &name_text)
        }
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

/// Search for references to a local variable within the enclosing function.
///
/// We walk the `ExprBody` (span-free) and use the source map for positions.
/// For each path whose root resolves to the same local, we emit a `Location`
/// using the root segment's span from the source map.
fn find_local_usages(
    db: &dyn Db,
    file: SourceFile,
    at_offset: TextSize,
    name_text: &str,
    target_binding: BindingId,
) -> Vec<Location> {
    let index = baml_compiler2_hir::file_semantic_index(db, file);
    // Find the enclosing Function scope.
    let scope_id = index.scope_at_offset(at_offset, None);
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

    // The recorded item↔scope link, not a span match (which cannot tell a
    // function from its companions — they share one span). A template-string
    // scope has a non-Function owner and returns early here.
    let owner_scope = index.scope_ids[enclosing_func_scope.index() as usize];
    let Some(baml_compiler2_ppir::item_data::ScopeOwner::Function(func_loc)) =
        baml_compiler2_ppir::item_data::scope_owner(db, owner_scope)
    else {
        return Vec::new();
    };

    // We need an expression body and source map.
    let body = baml_compiler2_hir::body::function_body(db, func_loc);
    let FunctionBody::Expr(expr_body) = body.as_ref() else {
        return Vec::new();
    };

    let Some(source_map) = baml_compiler2_hir::body::function_body_source_map(db, func_loc) else {
        return Vec::new();
    };

    let mut collector = LocalUsageCollector {
        file,
        index,
        name: Name::new(name_text),
        target_binding,
        results: Vec::new(),
    };
    collector.collect(
        enclosing_func_scope,
        expr_body.root_expr,
        expr_body,
        ExprMetadataScope::Body(enclosing_func_scope),
        &source_map,
    );

    // A defaults arena is a *forest* — one root per defaulted parameter, and
    // `root_expr` is always `None` for it (`lower_default_expr_nodes` finishes
    // with `None`). Walk each parameter's default separately.
    let defaults = baml_compiler2_ppir::function_parameter_defaults(db, func_loc);
    for default in defaults.params.iter().flatten() {
        collector.collect(
            enclosing_func_scope,
            Some(default.expr.expr()),
            &defaults.defaults.exprs,
            ExprMetadataScope::ParameterDefault(enclosing_func_scope),
            &defaults.defaults.source_map,
        );
    }

    collector.results
}

struct LocalUsageCollector<'index, 'db> {
    file: SourceFile,
    index: &'index FileSemanticIndex<'db>,
    name: Name,
    target_binding: BindingId,
    results: Vec<Location>,
}

impl LocalUsageCollector<'_, '_> {
    /// Walk the expressions belonging to `owner_scope`, recursing into lambda
    /// bodies under *their* scope.
    ///
    /// Structural rather than a flat arena scan: lambda bodies share this arena
    /// but are recorded under their own metadata namespace, so visiting them
    /// here would look them up under the wrong key and silently find nothing.
    fn collect(
        &mut self,
        owner_scope: FileScopeId,
        root: Option<baml_compiler2_ast::ExprId>,
        expr_body: &ExprBody,
        metadata_scope: ExprMetadataScope,
        source_map: &baml_compiler2_ast::AstSourceMap,
    ) {
        let nodes = root
            .map(|root| expr_body.reachable_excluding_lambdas(root))
            .unwrap_or_default();
        for node in nodes {
            let baml_compiler2_ast::BodyNode::Expr(expr_id) = node else {
                continue;
            };
            let expr = &expr_body.exprs[expr_id];
            match expr {
                Expr::Path(segments) if segments.first() == Some(&self.name) => {
                    let segment_range = source_map.path_segment_span(expr_id, 0);
                    let range =
                        TextRange::at(segment_range.start(), TextSize::of(segments[0].as_str()));
                    if range.is_empty() {
                        continue;
                    }

                    let key = ExprMetadataKey::new(metadata_scope, expr_id);
                    if self.index.path_resolution(key)
                        == Some(PathResolution::Local(self.target_binding))
                    {
                        self.results.push(Location {
                            file: self.file,
                            range,
                        });
                    }
                }
                Expr::Lambda(func_def) => {
                    let span = source_map.expr_span(expr_id);
                    let Some(lambda_scope) = self.index.lambda_scope_for_within(owner_scope, span)
                    else {
                        continue;
                    };

                    // One root per defaulted parameter — see above.
                    for param in &func_def.params {
                        let Some(default) = param.default else {
                            continue;
                        };
                        self.collect(
                            lambda_scope,
                            Some(default.expr()),
                            &func_def.defaults.exprs,
                            ExprMetadataScope::ParameterDefault(lambda_scope),
                            &func_def.defaults.source_map,
                        );
                    }

                    self.collect(
                        lambda_scope,
                        func_def.body,
                        expr_body,
                        ExprMetadataScope::Body(lambda_scope),
                        source_map,
                    );
                }
                _ => {}
            }
        }
    }
}

fn local_binding_id_at(
    db: &dyn Db,
    file: SourceFile,
    offset: TextSize,
    name: &Name,
) -> Option<BindingId> {
    let index = baml_compiler2_hir::file_semantic_index(db, file);
    let scope_id = index.scope_at_offset(offset, None);

    // Declaration tokens are intentionally not visible until after their
    // initializer/statement, so identify them by their recorded name range.
    for ancestor_id in index.ancestor_scopes(scope_id) {
        let bindings = &index.scope_bindings[ancestor_id.index() as usize];
        for (binding_idx, binding) in bindings.bindings.iter().enumerate().rev() {
            if &binding.name == name
                && (binding.name_range.contains(offset) || binding.name_range.start() == offset)
            {
                return Some(BindingId::local(ancestor_id, binding_idx));
            }
        }
    }

    index.visible_binding_at(scope_id, offset, name)
}

// ── field definition usages ───────────────────────────────────────────────────

/// When cursor is on a class field definition, find all usage sites.
///
/// Uses text pre-filter + `ScopeInference` confirmation pattern:
/// 1. Check that cursor is on a class field definition (Class scope).
/// 2. Collect all source files.
/// 3. For each file, scan function scopes for `MemberAccess` and Object constructor
///    expressions that reference the same field of the same class.
fn find_field_definition_usages(
    db: &dyn Db,
    file: SourceFile,
    offset: TextSize,
    field_name_text: &str,
) -> Vec<Location> {
    use baml_type::Ty;

    let index = baml_compiler2_hir::file_semantic_index(db, file);
    let scope_id = index.scope_at_offset(offset, None);
    let scope = &index.scopes[scope_id.index() as usize];

    // Only handle cursor on class field definitions
    if !matches!(scope.kind, ScopeKind::Class) {
        return Vec::new();
    }

    // The class that opened this scope, from the recorded item↔scope link.
    let owner_scope = index.scope_ids[scope_id.index() as usize];
    let Some(baml_compiler2_ppir::item_data::ScopeOwner::Class(class_loc)) =
        baml_compiler2_ppir::item_data::scope_owner(db, owner_scope)
    else {
        return Vec::new();
    };
    let class_data = baml_compiler2_ppir::item_data::class_data(db, class_loc);

    // Verify the cursor is actually on this field
    let field_match = class_data
        .fields
        .iter()
        .any(|f| f.name.as_str() == field_name_text);
    if !field_match {
        return Vec::new();
    }

    // Collect all source files
    let source_files = collect_source_files(db, file);

    let mut results = Vec::new();

    for sf in source_files {
        // Text pre-filter
        let text = sf.text(db);
        if !text.contains(field_name_text) {
            continue;
        }

        let sf_index = baml_compiler2_hir::file_semantic_index(db, sf);

        // Scan each function scope in the file
        for (scope_idx, scope) in sf_index.scopes.iter().enumerate() {
            if !matches!(scope.kind, ScopeKind::Function) {
                continue;
            }

            // The function that opened this scope, via the recorded item↔scope
            // link (template-string scopes have a non-Function owner → skipped).
            let owner_scope = sf_index.scope_ids[scope_idx];
            let Some(baml_compiler2_ppir::item_data::ScopeOwner::Function(func_loc)) =
                baml_compiler2_ppir::item_data::scope_owner(db, owner_scope)
            else {
                continue;
            };
            let body = baml_compiler2_hir::body::function_body(db, func_loc);
            let baml_compiler2_hir::body::FunctionBody::Expr(expr_body) = body.as_ref() else {
                continue;
            };

            let Some(source_map) = baml_compiler2_hir::body::function_body_source_map(db, func_loc)
            else {
                continue;
            };

            #[allow(clippy::cast_possible_truncation)]
            let file_scope_id = baml_compiler2_hir::scope::FileScopeId::new(scope_idx as u32);
            let scope_id_salsa = sf_index.scope_ids[file_scope_id.index() as usize];
            let Some(inference) = baml_compiler2_hir_ty::ide::infer_for_scope(db, scope_id_salsa)
            else {
                continue;
            };

            // MemberAccess sites: scan resolutions for matching Field
            for (expr_id, resolution) in &inference.member_resolutions {
                use baml_compiler2_hir_ty::infer::MemberResolution;
                if let MemberResolution::Field {
                    class: res_class_loc,
                    field: field_name,
                } = resolution
                {
                    if *res_class_loc == class_loc && field_name.as_str() == field_name_text {
                        // Get field name range from the MemberAccess expression span
                        let expr_span = source_map.expr_span(*expr_id);
                        // Extract just the field name portion (after the last dot)
                        let start: usize = expr_span.start().into();
                        let end: usize = expr_span.end().into();
                        if end <= text.len() {
                            let expr_text = &text[start..end];
                            if let Some(dot_offset) = expr_text.rfind(field_name_text) {
                                let field_start = start + dot_offset;
                                let field_end = field_start + field_name_text.len();
                                #[allow(clippy::cast_possible_truncation)]
                                let field_range = TextRange::new(
                                    TextSize::from(field_start as u32),
                                    TextSize::from(field_end as u32),
                                );
                                results.push(Location {
                                    file: sf,
                                    range: field_range,
                                });
                            }
                        }
                    }
                }
            }

            // Multi-segment Path sites: scan path_member_resolutions for matching Field.
            // For `obj.field` which is Path(["obj", "field"]), the field resolution is
            // in path_member_resolutions[expr_id][0] (index into segments[1..]).
            for (expr_id, resolved_path) in &inference.path_resolutions {
                use baml_compiler2_hir_ty::infer::MemberResolution;
                // Look up the Path's segments to find which segment index matched.
                let Some((_, path_expr)) = expr_body.exprs.iter().find(|(id, _)| id == expr_id)
                else {
                    continue;
                };
                let baml_compiler2_ast::Expr::Path(segments) = path_expr else {
                    continue;
                };
                for (seg_idx, step) in resolved_path.segments.iter().enumerate().skip(1) {
                    if let Some(MemberResolution::Field {
                        class: res_class_loc,
                        field: field_name,
                    }) = step.resolution.as_ref()
                    {
                        if *res_class_loc == class_loc && field_name.as_str() == field_name_text {
                            if seg_idx < segments.len() {
                                let seg_span = source_map.path_segment_span(*expr_id, seg_idx);
                                if !seg_span.is_empty() {
                                    results.push(Location {
                                        file: sf,
                                        range: seg_span,
                                    });
                                }
                            }
                        }
                    }
                }
            }

            // Object constructor field sites: scan Object expressions
            for (expr_id, expr) in expr_body.exprs.iter() {
                if let Expr::Object { fields, .. } = expr {
                    // Check if any field key matches
                    let has_matching_key = fields
                        .iter()
                        .any(|(name, _)| name.as_str() == field_name_text);
                    if !has_matching_key {
                        continue;
                    }

                    // Check if the Object type matches our target class
                    let Some(obj_ty) = inference
                        .type_of_expr
                        .get(&expr_id)
                        .map(baml_type::interned::Ty::to_plain)
                    else {
                        continue;
                    };
                    let Ty::Class(ref qtn, _, _) = obj_ty else {
                        continue;
                    };

                    // Resolve QualifiedTypeName to ClassLoc and compare
                    let pkg_id =
                        baml_compiler2_hir::package::PackageId::new(db, qtn.package().clone());
                    let pkg_items = baml_compiler2_hir::package::package_items(db, pkg_id);
                    let Some(def) = pkg_items.lookup_type(qtn.namespace(), qtn.name()) else {
                        continue;
                    };
                    let baml_compiler2_hir::contributions::Definition::Class(obj_class_loc) = def
                    else {
                        continue;
                    };
                    if obj_class_loc != class_loc {
                        continue;
                    }

                    // Find the field key token in the CST
                    let obj_span = source_map.expr_span(expr_id);
                    let root = baml_compiler_parser::syntax_tree(db, sf);
                    for node_or_token in root.descendants_with_tokens() {
                        let rowan::NodeOrToken::Token(tok) = node_or_token else {
                            continue;
                        };
                        if tok.kind() != SyntaxKind::WORD {
                            continue;
                        }
                        if tok.text() != field_name_text {
                            continue;
                        }
                        let tok_range = tok.text_range();
                        if obj_span.contains(tok_range.start()) {
                            results.push(Location {
                                file: sf,
                                range: tok_range,
                            });
                            break; // Only one match per Object expression
                        }
                    }
                }
            }
        }
    }

    results
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
    let pkg_id = PackageId::new(db, pkg_info.package);
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
