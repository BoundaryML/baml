//! `definition_at` — go-to-definition at a cursor position.
//!
//! This is a regular function (not a Salsa query). It uses the Rowan CST to
//! find the token under the cursor, extracts its text as a name, calls
//! `resolve_name_at` to resolve the name in scope, and then maps the
//! `ResolvedName` to a `Location` (source file + text range).
//!
//! ## Resolution cases
//!
//! - `ResolvedName::Item(def)` — top-level item (class, function, enum, …).
//!   File and name span come from `definition_span(db, def)` in `utils.rs`.
//!
//! - `ResolvedName::Builtin(def)` — same as `Item`, but in the builtin file.
//!
//! - `ResolvedName::Local { definition_site: Some(Statement(stmt_id)) }` — a
//!   `let` binding. Resolve via `function_body_source_map` to get the
//!   statement's source range.
//!
//! - `ResolvedName::Local { definition_site: Some(Parameter(idx)) }` — a
//!   function parameter. Resolve via `function_signature_source_map` to get
//!   the parameter's source range.
//!
//! - `ResolvedName::Unknown` or missing `definition_site` — return `None`.

use baml_base::{Name, SourceFile};
use baml_compiler_syntax::SyntaxKind;
use baml_compiler2_hir::semantic_index::DefinitionSite;
use text_size::{TextRange, TextSize};

use crate::{Db, utils};

// ── Location ─────────────────────────────────────────────────────────────────

/// A source location returned by `definition_at`.
///
/// Contains the target source file and the byte range of the name token at the
/// definition site. The LSP layer converts `file` to a URI and `range` to an
/// LSP `Range`.
#[derive(Clone, PartialEq, Eq)]
pub struct Location {
    /// The file where the symbol is defined.
    pub file: SourceFile,
    /// Byte range of the name token (not the full item body).
    pub range: TextRange,
}

// ── definition_at ─────────────────────────────────────────────────────────────

/// Find the definition of the symbol at `offset` in `file`.
///
/// Regular function (not cached). The expensive work (`file_semantic_index`,
/// `resolve_name_at`) is internally Salsa-cached.
///
/// Returns `None` if the cursor is not on an identifier, or if the name
/// cannot be resolved.
pub fn definition_at(db: &dyn Db, file: SourceFile, offset: TextSize) -> Option<Location> {
    // ── Step 1: find the token at the cursor ─────────────────────────────────
    let token = utils::find_token_at_offset(db, file, offset)?;

    // Only WORD tokens can be names that resolve to definitions.
    if token.kind() != SyntaxKind::WORD {
        return None;
    }

    let name_text = token.text();
    let name = Name::new(name_text);

    // ── Step 2: resolve the name in scope ─────────────────────────────────────
    let resolved = baml_compiler2_tir::resolve::resolve_name_at(db, file, offset, &name);

    // ── Step 3: map ResolvedName to a Location ────────────────────────────────
    match resolved {
        baml_compiler2_tir::resolve::ResolvedName::Item(def)
        | baml_compiler2_tir::resolve::ResolvedName::Builtin(def) => {
            // Top-level item — look up the contribution's name_span.
            let (def_file, range) = utils::definition_span(db, def)?;
            Some(Location {
                file: def_file,
                range,
            })
        }

        baml_compiler2_tir::resolve::ResolvedName::Local {
            definition_site: Some(site),
            ..
        } => local_definition_location(db, file, offset, site),

        baml_compiler2_tir::resolve::ResolvedName::Local {
            definition_site: None,
            ..
        }
        | baml_compiler2_tir::resolve::ResolvedName::Unknown => {
            // Fallback: try member resolution (field access, enum variant, constructor field, method)
            resolve_member_at(db, file, offset, name_text)
        }
    }
}

// ── local_definition_location ─────────────────────────────────────────────────

/// Resolve a local variable's definition site to a `Location`.
///
/// Finds the enclosing function by matching the scope range against `item_tree`
/// functions, then uses the appropriate source map to get the span.
fn local_definition_location(
    db: &dyn Db,
    file: SourceFile,
    at_offset: TextSize,
    site: DefinitionSite,
) -> Option<Location> {
    let index = baml_compiler2_hir::file_semantic_index(db, file);
    let item_tree = baml_compiler2_hir::file_item_tree(db, file);

    // Find the enclosing Function scope to locate the function in the item tree.
    let scope_id = index.scope_at_offset(at_offset, None);
    let enclosing_func_scope = index
        .ancestor_scopes(scope_id)
        .into_iter()
        .find(|ancestor_id| {
            matches!(
                index.scopes[ancestor_id.index() as usize].kind,
                baml_compiler2_hir::scope::ScopeKind::Function
            )
        })?;

    let func_scope_range = index.scopes[enclosing_func_scope.index() as usize].range;

    // Find the function in the item tree by matching the scope range.
    let (func_local_id, _) = item_tree
        .functions
        .iter()
        .find(|(_, f)| f.span == func_scope_range)?;

    let func_loc = baml_compiler2_hir::loc::FunctionLoc::new(db, file, *func_local_id);

    match site {
        DefinitionSite::Statement(stmt_id) => {
            // Get the source map to convert StmtId → TextRange.
            let source_map = baml_compiler2_hir::body::function_body_source_map(db, func_loc)?;
            let range = source_map.stmt_span(stmt_id);
            Some(Location { file, range })
        }
        DefinitionSite::Parameter(param_idx) => {
            // Get the signature source map to find parameter spans.
            let sig_map =
                baml_compiler2_hir::signature::function_signature_source_map(db, func_loc);
            let range = sig_map.param_spans.get(param_idx).copied()?;
            Some(Location { file, range })
        }
        DefinitionSite::PatternBinding(pat_id) => {
            // Navigate to the pattern binding.
            let source_map = baml_compiler2_hir::body::function_body_source_map(db, func_loc)?;
            let range = source_map.pattern_span(pat_id);
            Some(Location { file, range })
        }
    }
}

// ── resolve_member_at ─────────────────────────────────────────────────────────

/// Fallback resolution for member access positions where `resolve_name_at`
/// returns `Unknown` — field access, enum variants, constructor fields, methods.
fn resolve_member_at(
    db: &dyn Db,
    file: SourceFile,
    offset: TextSize,
    token_text: &str,
) -> Option<Location> {
    let index = baml_compiler2_hir::file_semantic_index(db, file);
    let item_tree = baml_compiler2_hir::file_item_tree(db, file);
    let scope_id = index.scope_at_offset(offset, None);
    let scope = &index.scopes[scope_id.index() as usize];

    match scope.kind {
        // ── Cursor on a field/variant definition ──────────────────────
        baml_compiler2_hir::scope::ScopeKind::Class => {
            resolve_field_definition(db, file, &item_tree, scope.name.as_ref(), token_text)
        }
        baml_compiler2_hir::scope::ScopeKind::Enum => {
            resolve_variant_definition(db, file, &item_tree, scope.name.as_ref(), token_text)
        }

        // ── Cursor in a function body ─────────────────────────────────
        _ => {
            // Walk ancestors to find enclosing function scope
            let enclosing_func_scope =
                index
                    .ancestor_scopes(scope_id)
                    .into_iter()
                    .find(|ancestor_id| {
                        matches!(
                            index.scopes[ancestor_id.index() as usize].kind,
                            baml_compiler2_hir::scope::ScopeKind::Function
                        )
                    })?;

            let func_scope = &index.scopes[enclosing_func_scope.index() as usize];

            // Find the matching function in the item tree
            let (func_local_id, _) = item_tree.functions.iter().find(|(_, f)| {
                f.span == func_scope.range && func_scope.name.as_ref() == Some(&f.name)
            })?;
            let func_loc = baml_compiler2_hir::loc::FunctionLoc::new(db, file, *func_local_id);

            // Get inference, body, source map
            let func_scope_id = index.scope_ids[enclosing_func_scope.index() as usize];
            let inference = baml_compiler2_tir::inference::infer_scope_types(db, func_scope_id);
            let body = baml_compiler2_hir::body::function_body(db, func_loc);
            let baml_compiler2_hir::body::FunctionBody::Expr(expr_body) = body.as_ref() else {
                return None;
            };
            let source_map = baml_compiler2_hir::body::function_body_source_map(db, func_loc)?;

            // Try MemberAccess path first
            if let Some(loc) = resolve_field_access_at(
                db,
                file,
                offset,
                token_text,
                expr_body,
                &source_map,
                inference,
            ) {
                return Some(loc);
            }

            // Try Object constructor field path
            resolve_constructor_field_at(
                db,
                file,
                offset,
                token_text,
                expr_body,
                &source_map,
                inference,
            )
        }
    }
}

/// Cursor is on a class field definition (e.g. `name string` inside a class body).
fn resolve_field_definition(
    db: &dyn Db,
    file: SourceFile,
    item_tree: &baml_compiler2_hir::item_tree::ItemTree,
    class_name: Option<&Name>,
    token_text: &str,
) -> Option<Location> {
    let class_name = class_name?;
    let source_map = baml_compiler2_hir::file_item_tree_source_map(db, file);
    for (class_id, class) in &item_tree.classes {
        if class.name == *class_name {
            let field_spans = source_map.class_field_spans.get(class_id)?;
            for (i, field) in class.fields.iter().enumerate() {
                if field.name.as_str() == token_text {
                    return Some(Location {
                        file,
                        range: field_spans[i],
                    });
                }
            }
        }
    }
    None
}

/// Cursor is on an enum variant definition (e.g. `Active` inside an enum body).
fn resolve_variant_definition(
    db: &dyn Db,
    file: SourceFile,
    item_tree: &baml_compiler2_hir::item_tree::ItemTree,
    enum_name: Option<&Name>,
    token_text: &str,
) -> Option<Location> {
    let enum_name = enum_name?;
    let source_map = baml_compiler2_hir::file_item_tree_source_map(db, file);
    for (enum_id, enum_def) in &item_tree.enums {
        if enum_def.name == *enum_name {
            let variant_spans = source_map.enum_variant_spans.get(enum_id)?;
            for (i, variant) in enum_def.variants.iter().enumerate() {
                if variant.name.as_str() == token_text {
                    return Some(Location {
                        file,
                        range: variant_spans[i],
                    });
                }
            }
        }
    }
    None
}

/// Cursor is on a `MemberAccess` expression or a multi-segment `Path`
/// (e.g. `p.name`, `Status.Active`, `s.Celebrate()`).
fn resolve_field_access_at(
    db: &dyn Db,
    _file: SourceFile,
    offset: TextSize,
    token_text: &str,
    expr_body: &baml_compiler2_ast::ExprBody,
    source_map: &baml_compiler2_ast::AstSourceMap,
    inference: &baml_compiler2_tir::inference::ScopeInference<'_>,
) -> Option<Location> {
    use baml_compiler2_ast::Expr;
    use baml_compiler2_tir::inference::MemberResolution;

    // Find the best matching expr — either a MemberAccess or a multi-segment Path
    // whose span contains the cursor and whose member name matches the token.
    // Pick smallest (innermost) span for nested chains.
    // For Path nodes, also record which segment index the cursor is on.
    let mut best: Option<(baml_compiler2_ast::ExprId, TextRange, Option<usize>)> = None;
    for (expr_id, expr) in expr_body.exprs.iter() {
        match expr {
            Expr::MemberAccess { member, .. } => {
                if member.as_str() != token_text {
                    continue;
                }
                let span = source_map.expr_span(expr_id);
                if !span.contains(offset) && span.end() != offset {
                    continue;
                }
                if best.is_none_or(|(_, prev_span, _)| span.len() < prev_span.len()) {
                    best = Some((expr_id, span, None));
                }
            }
            Expr::Path(segments) if segments.len() >= 2 => {
                // Check if the cursor is on a non-first segment of this path.
                // Only segments[1..] can be member accesses — segments[0] is the root.
                let segment_idx = segments[1..]
                    .iter()
                    .enumerate()
                    .find(|(i, seg)| {
                        if seg.as_str() != token_text {
                            return false;
                        }
                        let seg_span = source_map.path_segment_span(expr_id, *i + 1);
                        seg_span.contains(offset) || seg_span.end() == offset
                    })
                    .map(|(i, _)| i + 1);
                if segment_idx.is_none() {
                    continue;
                }
                let span = source_map.expr_span(expr_id);
                if best.is_none_or(|(_, prev_span, _)| span.len() < prev_span.len()) {
                    best = Some((expr_id, span, segment_idx));
                }
            }
            _ => {}
        }
    }

    let (expr_id, _, path_seg_idx) = best?;

    // For multi-segment Path nodes, look up the per-segment resolution
    // from path_member_resolutions. For MemberAccess, use the top-level resolution.
    let resolution = if let Some(seg_idx) = path_seg_idx {
        // seg_idx is the index into the full segments array (0-based, with 0 being the root).
        // path_member_resolutions[i] corresponds to segments[i+1], so the member resolution
        // for segment seg_idx is at index seg_idx - 1 in path_member_resolutions.
        inference
            .path_member_resolution(expr_id)
            .and_then(|resolutions| resolutions.get(seg_idx - 1))
            .or_else(|| inference.resolution(expr_id))
    } else {
        inference.resolution(expr_id)
    };

    match resolution? {
        MemberResolution::Field {
            class_loc,
            field_name,
        } => {
            let target_file = class_loc.file(db);
            let target_item_tree = baml_compiler2_hir::file_item_tree(db, target_file);
            let target_source_map = baml_compiler2_hir::file_item_tree_source_map(db, target_file);
            let class = &target_item_tree[class_loc.id(db)];
            let field_idx = class.fields.iter().position(|f| f.name == *field_name)?;
            let field_spans = target_source_map.class_field_spans.get(&class_loc.id(db))?;
            Some(Location {
                file: target_file,
                range: field_spans[field_idx],
            })
        }
        MemberResolution::Variant {
            enum_loc,
            variant_name,
        } => {
            let target_file = enum_loc.file(db);
            let target_item_tree = baml_compiler2_hir::file_item_tree(db, target_file);
            let target_source_map = baml_compiler2_hir::file_item_tree_source_map(db, target_file);
            let enum_def = &target_item_tree[enum_loc.id(db)];
            let variant_idx = enum_def
                .variants
                .iter()
                .position(|v| v.name == *variant_name)?;
            let variant_spans = target_source_map.enum_variant_spans.get(&enum_loc.id(db))?;
            Some(Location {
                file: target_file,
                range: variant_spans[variant_idx],
            })
        }
        MemberResolution::Free { func_loc } => {
            let def = baml_compiler2_hir::contributions::Definition::Function(*func_loc);
            let (def_file, range) = utils::definition_span(db, def)?;
            Some(Location {
                file: def_file,
                range,
            })
        }
        MemberResolution::BoundMethod { func_loc, .. }
        | MemberResolution::UnboundMethod { func_loc, .. } => {
            // Methods are not in FileSymbolContributions — use ItemTreeSourceMap.
            let target_file = func_loc.file(db);
            let target_source_map = baml_compiler2_hir::file_item_tree_source_map(db, target_file);
            let name_range = target_source_map
                .function_name_spans
                .get(&func_loc.id(db))?;
            Some(Location {
                file: target_file,
                range: *name_range,
            })
        }
    }
}

/// Cursor is on a constructor literal field (e.g. `data:` in `Success { data: "..." }`).
fn resolve_constructor_field_at(
    db: &dyn Db,
    _file: SourceFile,
    offset: TextSize,
    token_text: &str,
    expr_body: &baml_compiler2_ast::ExprBody,
    source_map: &baml_compiler2_ast::AstSourceMap,
    inference: &baml_compiler2_tir::inference::ScopeInference<'_>,
) -> Option<Location> {
    use baml_compiler2_ast::Expr;
    use baml_compiler2_tir::ty::Ty;

    for (expr_id, expr) in expr_body.exprs.iter() {
        if let Expr::Object { fields, .. } = expr {
            let span = source_map.expr_span(expr_id);
            if !span.contains(offset) && span.end() != offset {
                continue;
            }

            // Check if cursor token matches any field key name
            let _matching_field = fields
                .iter()
                .find(|(name, _)| name.as_str() == token_text)?;

            // Get the Object's type
            let obj_ty = inference.expression_type(expr_id)?;
            let Ty::Class(qtn, _, _) = obj_ty else {
                return None;
            };

            // Resolve QualifiedTypeName → ClassLoc
            let pkg_id = baml_compiler2_hir::package::PackageId::new(db, qtn.package().clone());
            let pkg_items = baml_compiler2_hir::package::package_items(db, pkg_id);
            let def = pkg_items.lookup_type(qtn.namespace(), qtn.name())?;
            let baml_compiler2_hir::contributions::Definition::Class(class_loc) = def else {
                return None;
            };

            // Look up field name_span in ItemTreeSourceMap
            let target_file = class_loc.file(db);
            let target_item_tree = baml_compiler2_hir::file_item_tree(db, target_file);
            let target_source_map = baml_compiler2_hir::file_item_tree_source_map(db, target_file);
            let class = &target_item_tree[class_loc.id(db)];
            let field_idx = class
                .fields
                .iter()
                .position(|f| f.name.as_str() == token_text)?;
            let field_spans = target_source_map.class_field_spans.get(&class_loc.id(db))?;
            return Some(Location {
                file: target_file,
                range: field_spans[field_idx],
            });
        }
    }
    None
}
