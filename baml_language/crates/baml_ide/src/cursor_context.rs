//! Playground cursor context: what entity the cursor is on, for playground
//! graph navigation (`cursorPosition` → `cursorContext` on the wire).
//!
//! Salvaged from the pre-rework `ProjectDatabase` methods (see `cfg.rs` for
//! the transformation rules). The wire shape ([`CursorContext`]) is consumed
//! by the TypeScript playground (`worker-protocol.ts`); field names are the
//! serde-camelCased contract.

use std::path::PathBuf;

use baml_base::SourceFile;

/// Context about what the cursor is pointing at, for playground navigation.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorContext {
    pub function_name: Option<String>,
    pub is_workflow: bool,
    pub workflow_memberships: Vec<String>,
    /// Raw `ExprId` index — matched against `node.metadata.sourceExpr` on the TS side.
    /// NOT a CFG `NodeId`. The TS side scans the cached graph for a node whose
    /// sourceExpr matches this value.
    pub source_expr_id: Option<u32>,
    /// Ordered list of expression IDs containing the cursor, from most
    /// specific (smallest span) to least specific (largest span). The TS
    /// side tries each one in order, highlighting the first that matches a
    /// CFG node. This gives "closest ancestor" behavior — e.g. cursor on a
    /// variable inside a call highlights the call; cursor on `if` keyword
    /// highlights the branch group.
    #[serde(default)]
    pub source_expr_candidates: Vec<u32>,
    /// Function body that owns `source_expr_id` / `source_expr_candidates`.
    ///
    /// This differs from `function_name` at call sites: the token resolves to
    /// the callee, but the expression span belongs to the caller.
    #[serde(default)]
    pub source_expr_function_name: Option<String>,
    pub test_name: Option<String>,
    /// Byte offset of the cursor position in the source file.
    /// Used for cursor ↔ event matching via span containment.
    #[serde(default)]
    pub cursor_offset: Option<u32>,
}

fn func_origin_rank(origin: baml_compiler2_ast::ast::FunctionOrigin) -> u8 {
    use baml_compiler2_ast::ast::FunctionOrigin;
    match origin {
        FunctionOrigin::UserDefined => 0,
        FunctionOrigin::Companion => 1,
        FunctionOrigin::Internal => 2,
        FunctionOrigin::AutoDerive => 3,
    }
}

pub fn playground_cursor_context(
    db: &dyn baml_compiler2_ppir::Db,
    files: &[SourceFile],
    file_path: &str,
    byte_offset: u32,
) -> CursorContext {
    use baml_db::baml_compiler_syntax::SyntaxKind;

    let empty = CursorContext {
        function_name: None,
        is_workflow: false,
        workflow_memberships: vec![],
        source_expr_id: None,
        source_expr_candidates: vec![],
        source_expr_function_name: None,
        test_name: None,
        cursor_offset: Some(byte_offset),
    };

    // 1. Find the SourceFile matching file_path
    let Some(source_file) = find_source_file(db, files, file_path) else {
        return empty;
    };

    let offset = text_size::TextSize::from(byte_offset);

    // 2. Find CST token at offset
    let Some(token) = crate::syntax::find_token_at_offset(db, source_file, offset) else {
        return empty;
    };

    // 3. Check if cursor is inside a HEADER_COMMENT node — these need
    //    special handling since their tokens don't resolve via name lookup.
    if let Some(parent) = token.parent() {
        if parent
            .ancestors()
            .any(|n| n.kind() == SyntaxKind::HEADER_COMMENT)
        {
            return cursor_context_positional(db, files, source_file, offset);
        }
    }

    // 4. For WORD tokens, try name resolution first (handles function
    //    definitions, call sites, local variables).
    if token.kind() == SyntaxKind::WORD {
        let name = baml_base::Name::from(token.text().to_string());

        let resolved =
            baml_compiler2_ppir::resolve::resolve_name_at(db, source_file, offset, &name);

        match resolved {
            baml_compiler2_ppir::resolve::ResolvedName::Item(def)
            | baml_compiler2_ppir::resolve::ResolvedName::Builtin(def) => {
                use baml_compiler2_hir::contributions::Definition;
                match &def {
                    Definition::Function(_) => {
                        return cursor_context_for_definition(db, files, source_file, offset, def);
                    }
                    _ => {
                        // Non-function items (class, enum, etc.) used inside
                        // a function body — fall through to positional so we
                        // can still highlight the enclosing graph node.
                    }
                }
            }
            baml_compiler2_ppir::resolve::ResolvedName::Local { .. } => {
                return cursor_context_for_local(db, files, source_file, offset);
            }
            baml_compiler2_ppir::resolve::ResolvedName::Unknown => {
                // Fall through to positional fallback below
            }
        }
    }

    // 5. Positional fallback for non-WORD tokens (keywords like `if`,
    //    `match`, `return`, operators, punctuation), unresolved WORDs,
    //    and non-function item references (class names in return types, etc.).
    cursor_context_positional(db, files, source_file, offset)
}

/// Build cursor context purely from position — no name resolution.
/// Used for keywords, operators, punctuation, header comments, and
/// any token that doesn't resolve through the name-lookup path.
fn cursor_context_positional(
    db: &dyn baml_compiler2_ppir::Db,
    files: &[SourceFile],
    source_file: SourceFile,
    offset: text_size::TextSize,
) -> CursorContext {
    let (func_name, is_workflow) = match find_enclosing_function(db, source_file, offset) {
        Some((name, workflow)) => (Some(name), workflow),
        None => (None, false),
    };

    let workflow_memberships = func_name
        .as_ref()
        .map(|n| find_workflow_memberships(db, files, n))
        .unwrap_or_default();

    let (source_expr_id, source_expr_candidates) = find_source_expr_ids_at(db, source_file, offset);

    CursorContext {
        function_name: func_name.clone(),
        is_workflow,
        workflow_memberships,
        source_expr_id,
        source_expr_candidates,
        source_expr_function_name: func_name,
        test_name: None,
        cursor_offset: Some(u32::from(offset)),
    }
}

/// Build cursor context when the cursor resolved to a top-level Definition.
fn cursor_context_for_definition(
    db: &dyn baml_compiler2_ppir::Db,
    files: &[SourceFile],
    source_file: SourceFile,
    offset: text_size::TextSize,
    def: baml_compiler2_hir::contributions::Definition<'_>,
) -> CursorContext {
    use baml_compiler2_hir::contributions::Definition;

    match def {
        Definition::Function(func_loc) => {
            let sig = baml_compiler2_ppir::function_signature(db, func_loc);
            let body = baml_compiler2_ppir::function_body(db, func_loc);
            let is_workflow = matches!(
                body.as_ref(),
                baml_compiler2_hir::body::FunctionBody::Expr(_)
            );

            let func_name =
                crate::symbols::playground_function_name_for_file(db, func_loc.file(db), &sig.name);
            let workflow_memberships = find_workflow_memberships(db, files, &func_name);

            // Find source_expr_id if cursor is inside a function body
            let (source_expr_id, source_expr_candidates) =
                find_source_expr_ids_at(db, source_file, offset);
            let source_expr_function_name =
                find_enclosing_function(db, source_file, offset).map(|(name, _)| name);

            CursorContext {
                function_name: Some(func_name),
                is_workflow,
                workflow_memberships,
                source_expr_id,
                source_expr_candidates,
                source_expr_function_name,
                test_name: None,
                cursor_offset: Some(u32::from(offset)),
            }
        }
        _ => {
            // For classes, enums, etc. - no meaningful playground navigation
            CursorContext {
                function_name: None,
                is_workflow: false,
                workflow_memberships: vec![],
                source_expr_id: None,
                source_expr_candidates: vec![],
                source_expr_function_name: None,
                test_name: None,
                cursor_offset: Some(u32::from(offset)),
            }
        }
    }
}

/// Build cursor context when the cursor resolved to a local variable.
/// We look up the enclosing function to provide context.
fn cursor_context_for_local(
    db: &dyn baml_compiler2_ppir::Db,
    files: &[SourceFile],
    source_file: SourceFile,
    offset: text_size::TextSize,
) -> CursorContext {
    let (func_name, is_workflow) = match find_enclosing_function(db, source_file, offset) {
        Some((name, workflow)) => (Some(name), workflow),
        None => (None, false),
    };

    let workflow_memberships = func_name
        .as_ref()
        .map(|n| find_workflow_memberships(db, files, n))
        .unwrap_or_default();

    let (source_expr_id, source_expr_candidates) = find_source_expr_ids_at(db, source_file, offset);

    CursorContext {
        function_name: func_name.clone(),
        is_workflow,
        workflow_memberships,
        source_expr_id,
        source_expr_candidates,
        source_expr_function_name: func_name,
        test_name: None,
        cursor_offset: Some(u32::from(offset)),
    }
}

/// Find a [`SourceFile`] by file path (matches by suffix to handle
/// different path formats, e.g. Monaco's relative paths).
pub fn find_source_file(
    db: &dyn baml_compiler2_ppir::Db,
    files: &[SourceFile],
    file_path: &str,
) -> Option<SourceFile> {
    let path = PathBuf::from(file_path);
    // Exact match first.
    if let Some(&sf) = files.iter().find(|&&sf| sf.path(db) == path) {
        return Some(sf);
    }
    // Fallback: match by path suffix.
    files
        .iter()
        .find(|&&sf| {
            let stored = sf.path(db);
            stored.ends_with(file_path) || file_path.ends_with(stored.to_string_lossy().as_ref())
        })
        .copied()
}

/// Find the enclosing function name and whether it's a workflow, given a cursor position.
fn find_enclosing_function(
    db: &dyn baml_compiler2_ppir::Db,
    source_file: SourceFile,
    offset: text_size::TextSize,
) -> Option<(String, bool)> {
    use baml_compiler2_hir::scope::ScopeKind;

    let index = baml_compiler2_ppir::file_semantic_index(db, source_file);
    let scope_id = index.scope_at_offset(offset, None);
    let ancestors = index.ancestor_scopes(scope_id);

    // Find the innermost Function scope
    let func_scope_id = ancestors.iter().find(|&&ancestor_id| {
        let scope = &index.scopes[ancestor_id.index() as usize];
        matches!(scope.kind, ScopeKind::Function)
    })?;

    let func_scope_range = index.scopes[func_scope_id.index() as usize].range;

    // A declarative LLM function and its compiler-generated private spec/stream
    // companions share one source span, hence one scope range — so multiple
    // functions match here. Prefer the user-authored one (origin order).
    let func_loc = baml_compiler2_ppir::item_data::file_functions(db, source_file)
        .iter()
        .copied()
        .filter(|&loc| {
            baml_compiler2_ppir::item_data::function_source_map(db, loc).span == func_scope_range
        })
        .min_by_key(|&loc| {
            func_origin_rank(
                baml_compiler2_ppir::item_data::function_data(db, loc)
                    .metadata
                    .origin,
            )
        })?;
    let sig = baml_compiler2_ppir::function_signature(db, func_loc);
    let body = baml_compiler2_ppir::function_body(db, func_loc);
    let is_workflow = matches!(
        body.as_ref(),
        baml_compiler2_hir::body::FunctionBody::Expr(_)
    );
    Some((
        crate::symbols::playground_function_name_for_file(db, source_file, &sig.name),
        is_workflow,
    ))
}

/// Find workflows that call the given function by scanning all function bodies.
fn find_workflow_memberships(
    db: &dyn baml_compiler2_ppir::Db,
    files: &[SourceFile],
    target_function_name: &str,
) -> Vec<String> {
    let mut memberships = Vec::new();

    for &source_file in files {
        for &func_loc in baml_compiler2_ppir::item_data::file_functions(db, source_file) {
            let func_data = baml_compiler2_ppir::item_data::function_data(db, func_loc);
            let func_name =
                crate::symbols::playground_function_name_for_file(db, source_file, &func_data.name);
            if func_data.name.as_str() == target_function_name || func_name == target_function_name
            {
                continue; // Skip self
            }

            let body = baml_compiler2_ppir::function_body(db, func_loc);

            // Only workflow (Expr) functions can call other functions
            if let baml_compiler2_hir::body::FunctionBody::Expr(expr_body) = body.as_ref() {
                if expr_body_calls_function(expr_body, target_function_name) {
                    memberships.push(func_name);
                }
            }
        }
    }

    memberships
}

/// Check if an expression body contains a call to a function with the given name.
fn expr_body_calls_function(body: &baml_compiler2_ast::ExprBody, target_name: &str) -> bool {
    use baml_compiler2_ast::Expr;
    let target_leaf_name = target_name.rsplit('.').next().unwrap_or(target_name);
    for (_id, expr) in body.exprs.iter() {
        if let Expr::Call { callee, .. } = expr {
            // Check if the callee is a Path containing the target name
            if let Expr::Path(segments) = &body.exprs[*callee] {
                let callee_name = segments
                    .iter()
                    .map(AsRef::<str>::as_ref)
                    .collect::<Vec<_>>()
                    .join(".");
                if callee_name == target_name
                    || (segments.len() == 1 && segments[0].as_str() == target_leaf_name)
                {
                    return true;
                }
            }
        }
    }
    false
}

/// O(1) lookup of an [`ExprId`](baml_compiler2_ast::ExprId)'s span in the source map. Returns
/// the raw index and span length as a `(u32, TextSize)` pair for the candidate list.
fn expr_span_entry(
    source_map: &baml_compiler2_ast::AstSourceMap,
    expr_id: baml_compiler2_ast::ExprId,
) -> Option<(u32, text_size::TextSize)> {
    let raw = expr_id.into_raw();
    let idx = raw.into_u32() as usize;
    if idx >= source_map.expr_spans.len() {
        return None;
    }
    let span_idx = la_arena::Idx::<text_size::TextRange>::from_raw(raw);
    let range = &source_map.expr_spans[span_idx];
    Some((raw.into_u32(), range.len()))
}

/// Find source expression candidates at the cursor offset.
///
/// Returns `(best, candidates)` where `best` is backward-compatible
/// (the smallest expression) and `candidates` is the full list of
/// containing expression IDs sorted smallest-first. Headers (tagged
/// with the high bit) are inserted at the front when present.
fn find_source_expr_ids_at(
    db: &dyn baml_compiler2_ppir::Db,
    source_file: SourceFile,
    offset: text_size::TextSize,
) -> (Option<u32>, Vec<u32>) {
    use baml_compiler2_hir::scope::ScopeKind;

    let index = baml_compiler2_ppir::file_semantic_index(db, source_file);
    let scope_id = index.scope_at_offset(offset, None);
    let ancestors = index.ancestor_scopes(scope_id);

    let Some(func_scope_id) = ancestors.iter().find(|&&ancestor_id| {
        let scope = &index.scopes[ancestor_id.index() as usize];
        matches!(scope.kind, ScopeKind::Function)
    }) else {
        return (None, vec![]);
    };

    let func_scope_range = index.scopes[func_scope_id.index() as usize].range;
    if let Some(func_loc) = baml_compiler2_ppir::item_data::file_functions(db, source_file)
        .iter()
        .copied()
        .filter(|&loc| {
            baml_compiler2_ppir::item_data::function_source_map(db, loc).span == func_scope_range
        })
        .min_by_key(|&loc| {
            func_origin_rank(
                baml_compiler2_ppir::item_data::function_data(db, loc)
                    .metadata
                    .origin,
            )
        })
    {
        let Some(source_map) = baml_compiler2_ppir::function_body_source_map(db, func_loc) else {
            return (None, vec![]);
        };
        let body = baml_compiler2_ppir::function_body(db, func_loc);
        let expr_body = match body.as_ref() {
            baml_compiler2_hir::body::FunctionBody::Expr(eb) => Some(eb),
            _ => None,
        };

        // Collect ALL expression spans containing the cursor.
        let mut containing: Vec<(u32, text_size::TextSize)> = Vec::new();
        #[allow(clippy::cast_possible_truncation)] // arena indices never exceed u32
        for (idx, (_id, range)) in source_map.expr_spans.iter().enumerate() {
            if range.contains(offset) || range.end() == offset {
                containing.push((idx as u32, range.len()));
            }
        }

        // For each statement span containing the cursor, inject the
        // statement's "graph-relevant expression" into the candidate list.
        // This maps the whole `let x = Call(...)` or `return Obj {...}`
        // line to the expression the graph node uses as source_expr.
        #[allow(clippy::cast_possible_truncation)] // arena indices never exceed u32
        if let Some(eb) = expr_body {
            for (idx, (_id, range)) in source_map.stmt_spans.iter().enumerate() {
                if !(range.contains(offset) || range.end() == offset) {
                    continue;
                }
                let idx_u32 = idx as u32;
                let stmt_id = la_arena::Idx::<baml_compiler2_ast::Stmt>::from_raw(
                    la_arena::RawIdx::from_u32(idx_u32),
                );
                // Look up the expression the graph node uses as source_expr
                // for this statement, and inject it into the candidate list.
                let injected_expr = match &eb.stmts[stmt_id] {
                    baml_compiler2_ast::Stmt::HeaderComment { .. } => {
                        let tagged =
                            baml_compiler2_visualization::control_flow::STMT_SOURCE_EXPR_TAG
                                | idx_u32;
                        Some((tagged, range.len()))
                    }
                    baml_compiler2_ast::Stmt::Let {
                        initializer: Some(init),
                        ..
                    } => expr_span_entry(&source_map, *init),
                    baml_compiler2_ast::Stmt::Return(Some(expr_id)) => {
                        expr_span_entry(&source_map, *expr_id)
                    }
                    baml_compiler2_ast::Stmt::Expr(expr_id) => {
                        expr_span_entry(&source_map, *expr_id)
                    }
                    _ => None,
                };
                if let Some(entry) = injected_expr {
                    containing.push(entry);
                }
            }
        }

        // Region governance for `//#` headers: a header owns the lines from
        // its own line until the next header in the same block, or the end
        // of that block. Clicking anywhere in that region (e.g. inside an
        // `if` that is not itself a rendered node) should be able to select
        // the header node. So find the nearest preceding header whose block
        // still contains the cursor and inject it as the LEAST-specific
        // candidate — a max length sorts it last, behind any real node.
        #[allow(clippy::cast_possible_truncation)] // arena indices never exceed u32
        if let Some(eb) = expr_body {
            let mut governing: Option<(u32, text_size::TextSize)> = None;
            for (idx, (_id, range)) in source_map.stmt_spans.iter().enumerate() {
                let idx_u32 = idx as u32;
                let stmt_id = la_arena::Idx::<baml_compiler2_ast::Stmt>::from_raw(
                    la_arena::RawIdx::from_u32(idx_u32),
                );
                if !matches!(
                    &eb.stmts[stmt_id],
                    baml_compiler2_ast::Stmt::HeaderComment { .. }
                ) {
                    continue;
                }
                // The header must begin at or before the cursor.
                if range.start() > offset {
                    continue;
                }
                // ...and its own block must still contain the cursor —
                // otherwise the header's region ended when that block closed.
                let header_scope = index.scope_at_offset(range.start(), None);
                let header_scope_range = index.scopes[header_scope.index() as usize].range;
                if !(header_scope_range.contains(offset) || header_scope_range.end() == offset) {
                    continue;
                }
                // Nearest preceding header wins (the next header supersedes).
                let take = match governing {
                    None => true,
                    Some((_, start)) => range.start() > start,
                };
                if take {
                    governing = Some((idx_u32, range.start()));
                }
            }
            if let Some((idx_u32, _)) = governing {
                let tagged =
                    baml_compiler2_visualization::control_flow::STMT_SOURCE_EXPR_TAG | idx_u32;
                // Max length → sorts last, so any real expression node under
                // the cursor is still preferred over the governing header.
                containing.push((tagged, func_scope_range.len()));
            }
        }

        // Sort smallest-first and deduplicate so the TS side tries
        // the most specific expression first.
        containing.sort_by_key(|&(_, len)| len);
        let mut seen = std::collections::HashSet::new();
        let candidates: Vec<u32> = containing
            .iter()
            .filter_map(|&(id, _)| if seen.insert(id) { Some(id) } else { None })
            .collect();

        let best = candidates.first().copied();
        return (best, candidates);
    }

    (None, vec![])
}
