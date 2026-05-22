//! `SemanticIndexBuilder` — walks `Vec<ast::Item>` and builds `FileSemanticIndex`.
//!
//! Allocates scopes in DFS pre-order with `TextRange`, builds the `ItemTree`,
//! collects `FileSymbolContributions`, and records expression→scope mappings
//! with per-scope `ScopeBindings`.
//!
//! Scope chain: Project → Package → Namespace* → File → Items.

use std::sync::Arc;

use baml_base::{Name, SourceFile};
use baml_compiler_diagnostics::diagnostic::DiagnosticId;
use baml_compiler2_ast::{self as ast, LoweringDiagnostic};
use rustc_hash::FxHashMap;
use text_size::{TextRange, TextSize};

use crate::{
    contributions::{Contribution, Definition, DefinitionKind, FileSymbolContributions},
    diagnostic::{Hir2Diagnostic, MemberSite},
    file_package::file_package,
    ids::{FunctionMarker, LocalItemId},
    item_tree::ItemTree,
    loc::{
        ClassLoc, ClientLoc, EnumLoc, FunctionLoc, GeneratorLoc, LetLoc, RetryPolicyLoc,
        TemplateStringLoc, TestLoc, TypeAliasLoc,
    },
    scope::{FileScopeId, Scope, ScopeId, ScopeKind},
    semantic_index::{
        BindingId, DefinitionSite, FileSemanticIndex, LocalBinding, PathResolution, ScopeBindings,
        SemanticIndexExtra, visible_binding_at_in_scopes,
    },
};

#[derive(Debug, Clone)]
struct PathRootReference {
    name: Name,
    use_scope: FileScopeId,
    use_offset: TextSize,
    owner_lambda: Option<FileScopeId>,
}

pub struct SemanticIndexBuilder<'db> {
    db: &'db dyn crate::Db,
    file: SourceFile,

    scopes: Vec<Scope>,
    scope_bindings: Vec<ScopeBindings>,
    /// Stack of currently-open scope IDs.
    scope_stack: Vec<FileScopeId>,
    /// Depth of class scopes we're inside (> 0 means methods shouldn't
    /// contribute to top-level symbols — they belong to the class scope).
    class_depth: u32,

    /// Expression → scope mappings, sorted by `ExprId` at the end.
    expr_scopes: Vec<(ast::ExprId, FileScopeId)>,

    /// Path root resolutions for multi-segment `Path` expressions.
    /// Collected during `walk_expr_body`, sorted by `ExprId` at the end.
    path_resolutions: Vec<(ast::ExprId, PathResolution)>,
    /// Path-root references collected while walking source order. Unlike
    /// `expr_scopes`, this carries the scope and innermost lambda context at
    /// collection time so capture analysis does not rely on arena-local `ExprId`s.
    path_root_references: Vec<PathRootReference>,
    lambda_stack: Vec<FileScopeId>,

    item_tree: ItemTree,
    item_tree_source_map: crate::item_tree::ItemTreeSourceMap,
    type_contributions: Vec<(Name, Contribution<'db>)>,
    value_contributions: Vec<(Name, Contribution<'db>)>,
    diagnostics: Vec<Hir2Diagnostic>,
    lowering_diagnostics: Vec<LoweringDiagnostic>,
    env_var_refs: Vec<baml_compiler2_ast::EnvVarRef>,
}

impl<'db> SemanticIndexBuilder<'db> {
    pub fn new(db: &'db dyn crate::Db, file: SourceFile) -> Self {
        Self {
            db,
            file,
            scopes: Vec::new(),
            scope_bindings: Vec::new(),
            scope_stack: Vec::new(),
            class_depth: 0,
            expr_scopes: Vec::new(),
            path_resolutions: Vec::new(),
            path_root_references: Vec::new(),
            lambda_stack: Vec::new(),
            item_tree: ItemTree::new(),
            item_tree_source_map: crate::item_tree::ItemTreeSourceMap::default(),
            type_contributions: Vec::new(),
            value_contributions: Vec::new(),
            diagnostics: Vec::new(),
            lowering_diagnostics: Vec::new(),
            env_var_refs: Vec::new(),
        }
    }

    /// Set lowering diagnostics produced during CST → AST lowering.
    #[must_use]
    pub fn with_lowering_diagnostics(mut self, diags: Vec<LoweringDiagnostic>) -> Self {
        self.lowering_diagnostics = diags;
        self
    }

    /// Set env var references collected during CST → AST lowering.
    #[must_use]
    pub fn with_env_var_refs(mut self, refs: Vec<baml_compiler2_ast::EnvVarRef>) -> Self {
        self.env_var_refs = refs;
        self
    }

    /// Build the `FileSemanticIndex` from a list of AST items.
    ///
    /// `file_range` is the full text range of the file (used for
    /// Project/Package/Namespace/File scopes).
    pub fn build(mut self, items: &[ast::Item], file_range: TextRange) -> FileSemanticIndex<'db> {
        let pkg_info = file_package(self.db, self.file);

        // Build scope chain: Project → Package → Namespace* → File
        self.push_scope(ScopeKind::Project, None, file_range);
        self.push_scope(
            ScopeKind::Package,
            Some(pkg_info.package.clone()),
            file_range,
        );
        for ns in &pkg_info.namespace_path {
            self.push_scope(ScopeKind::Namespace, Some(ns.clone()), file_range);
        }
        let file_name = self
            .file
            .path(self.db)
            .file_name()
            .map(|n| Name::new(n.to_string_lossy()));
        self.push_scope(ScopeKind::File, file_name, file_range);

        // Walk AST items
        for item in items {
            self.lower_item(item);
        }
        self.validate_phase1_builtin_contracts(items);

        // Pop: File, Namespace*, Package, Project
        self.pop_scope(); // File
        for _ in &pkg_info.namespace_path {
            self.pop_scope(); // Namespace*
        }
        self.pop_scope(); // Package
        self.pop_scope(); // Project

        // Sort expr_scopes for binary search
        self.expr_scopes.sort_by_key(|(id, _)| *id);

        // Sort path_resolutions for binary search
        self.path_resolutions.sort_by_key(|(id, _)| *id);

        // Pre-intern ScopeIds for each FileScopeId
        let scope_ids: Vec<ScopeId<'db>> = (0..self.scopes.len())
            .map(|i| {
                #[allow(clippy::cast_possible_truncation)]
                ScopeId::new(self.db, self.file, FileScopeId::new(i as u32))
            })
            .collect();

        let extra = if self.diagnostics.is_empty() && self.lowering_diagnostics.is_empty() {
            None
        } else {
            Some(Box::new(SemanticIndexExtra {
                diagnostics: self.diagnostics,
                lowering_diagnostics: self.lowering_diagnostics,
            }))
        };

        FileSemanticIndex {
            scopes: self.scopes,
            expr_scopes: self.expr_scopes,
            scope_bindings: self.scope_bindings,
            scope_ids,
            item_tree: Arc::new(self.item_tree),
            item_tree_source_map: Arc::new(self.item_tree_source_map),
            symbol_contributions: Arc::new(FileSymbolContributions {
                types: self.type_contributions,
                values: self.value_contributions,
            }),
            extra,
            path_resolutions: self.path_resolutions,
            env_var_refs: self.env_var_refs,
        }
    }

    // ── Scope management ────────────────────────────────────────────────────

    fn push_scope(&mut self, kind: ScopeKind, name: Option<Name>, range: TextRange) {
        #[allow(clippy::cast_possible_truncation)]
        let id = FileScopeId::new(self.scopes.len() as u32);
        let parent = self.scope_stack.last().copied();
        self.scopes.push(Scope {
            parent,
            kind,
            name,
            range,
            descendants: id.next()..id.next(), // empty initially; filled on pop
        });
        self.scope_bindings.push(ScopeBindings::new());
        self.scope_stack.push(id);
    }

    fn pop_scope(&mut self) {
        let popped = self.scope_stack.pop().expect("scope stack underflow");
        #[allow(clippy::cast_possible_truncation)]
        let children_end = FileScopeId::new(self.scopes.len() as u32);
        self.scopes[popped.index() as usize].descendants.end = children_end;
    }

    fn current_scope_id(&self) -> FileScopeId {
        *self.scope_stack.last().expect("no current scope")
    }

    // ── Expression recording ─────────────────────────────────────────────────

    /// Record that an expression belongs to the current scope.
    fn record_expr_scope(&mut self, expr_id: ast::ExprId) {
        self.expr_scopes.push((expr_id, self.current_scope_id()));
    }

    /// Build a dotted scope path from the current scope stack, e.g. `Foo.Bar`.
    /// Skips structural scopes (Project, Package, Namespace, File).
    fn current_scope_path(&self) -> Option<Name> {
        let parts: Vec<&str> = self
            .scope_stack
            .iter()
            .filter_map(|id| {
                let scope = &self.scopes[id.index() as usize];
                match scope.kind {
                    ScopeKind::Project
                    | ScopeKind::Package
                    | ScopeKind::Namespace
                    | ScopeKind::File => None,
                    _ => scope.name.as_ref().map(Name::as_str),
                }
            })
            .collect();
        if parts.is_empty() {
            None
        } else {
            Some(Name::new(parts.join(".")))
        }
    }

    /// Emit a `DuplicateDefinition` diagnostic for any parameter name that
    /// appears more than once in a function or lambda signature. Applies
    /// uniformly to positional and defaulted parameters — both share the
    /// same `params` vector, and any name collision among them would make
    /// later references to the name ambiguous within the body.
    fn emit_duplicate_param_diagnostics(&mut self, params: &[ast::Param]) {
        let mut seen: FxHashMap<Name, Vec<MemberSite>> = FxHashMap::default();
        for param in params {
            seen.entry(param.name.clone())
                .or_default()
                .push(MemberSite {
                    range: param.name_span,
                    kind: DefinitionKind::Parameter,
                });
        }
        self.emit_duplicate_diagnostics(seen);
    }

    /// Emit `DuplicateDefinition` diagnostics for any name with more than one site.
    fn emit_duplicate_diagnostics(&mut self, seen: FxHashMap<Name, Vec<MemberSite>>) {
        let scope = self.current_scope_path();
        for (name, sites) in seen {
            if sites.len() > 1 {
                self.diagnostics.push(Hir2Diagnostic::DuplicateDefinition {
                    name,
                    scope: scope.clone(),
                    sites,
                });
            }
        }
    }

    /// Walk an `ExprBody` arena in source order, recording expression ownership
    /// and local bindings in the lexical scope that owns each expression.
    fn walk_expr_body(&mut self, body: &ast::ExprBody, source_map: &ast::AstSourceMap) {
        if let Some(root_expr) = body.root_expr {
            self.walk_expr(root_expr, body, source_map, false);
        }
    }

    /// Walk an expression, recording its `FileScopeId` and (for `Block`s)
    /// optionally pushing a `ScopeKind::Block` scope around the contents.
    ///
    /// `push_block_scope`: pass `true` for nested expressions; pass `false`
    /// when walking the root body of a function/lambda (the function/lambda
    /// scope is already on the stack — pushing another `Block` scope would
    /// double-wrap the body).
    fn walk_expr(
        &mut self,
        expr_id: ast::ExprId,
        body: &ast::ExprBody,
        source_map: &ast::AstSourceMap,
        push_block_scope: bool,
    ) {
        match &body.exprs[expr_id] {
            ast::Expr::Block { stmts, tail_expr } => {
                if push_block_scope {
                    self.push_scope(ScopeKind::Block, None, source_map.expr_span(expr_id));
                }
                self.record_expr_scope(expr_id);
                self.walk_block_contents(stmts, *tail_expr, body, source_map);
                if push_block_scope {
                    self.pop_scope();
                }
            }
            ast::Expr::Lambda(func_def) => {
                self.record_expr_scope(expr_id);
                self.walk_lambda_expr(expr_id, func_def, source_map);
            }
            _ => {
                self.record_expr_scope(expr_id);
                self.walk_expr_children(expr_id, body, source_map);
            }
        }
    }

    fn walk_block_contents(
        &mut self,
        stmts: &[ast::StmtId],
        tail_expr: Option<ast::ExprId>,
        body: &ast::ExprBody,
        source_map: &ast::AstSourceMap,
    ) {
        for &stmt_id in stmts {
            self.walk_stmt(stmt_id, body, source_map);
        }
        if let Some(tail_expr) = tail_expr {
            self.walk_expr(tail_expr, body, source_map, true);
        }
    }

    fn walk_stmt(
        &mut self,
        stmt_id: ast::StmtId,
        body: &ast::ExprBody,
        source_map: &ast::AstSourceMap,
    ) {
        match &body.stmts[stmt_id] {
            ast::Stmt::Expr(expr) => self.walk_expr(*expr, body, source_map, true),
            ast::Stmt::Let {
                pattern,
                initializer,
                ..
            } => {
                if let Some(initializer) = initializer {
                    self.walk_expr(*initializer, body, source_map, true);
                }
                self.register_local_pattern(
                    *pattern,
                    DefinitionSite::Statement(stmt_id),
                    body,
                    source_map,
                    source_map.stmt_span(stmt_id).end(),
                );
            }
            ast::Stmt::For {
                binding,
                collection,
                body: loop_body,
            } => {
                self.walk_expr(*collection, body, source_map, true);
                self.push_scope(ScopeKind::Block, None, source_map.stmt_span(stmt_id));
                self.register_local_pattern(
                    *binding,
                    DefinitionSite::Statement(stmt_id),
                    body,
                    source_map,
                    source_map.pattern_span(*binding).start(),
                );
                self.walk_expr(*loop_body, body, source_map, true);
                self.pop_scope();
            }
            ast::Stmt::While {
                condition,
                body: loop_body,
                after,
                ..
            } => {
                self.walk_expr(*condition, body, source_map, true);
                // Push a Block scope around the body and the C-style for
                // `after` step, mirroring `Stmt::For`. While the body is
                // itself an `Expr::Block` (which pushes its own scope), the
                // wrapping scope here gives the while-statement its own
                // identity in the scope tree, so downstream consumers (LSP
                // find-references, capture analysis, MIR `binding_locals`
                // lookup) can anchor on the while-statement boundary
                // symmetrically with for-statements.
                //
                // The `after` step (set by C-style `for (init; cond; after)`
                // desugaring) runs at the same level as the body, not inside
                // it — it must be able to see the surrounding-scope locals
                // declared by the for-init, so it stays within this wrapping
                // scope but outside the body's own block scope.
                self.push_scope(ScopeKind::Block, None, source_map.stmt_span(stmt_id));
                self.walk_expr(*loop_body, body, source_map, true);
                if let Some(after) = after {
                    self.walk_stmt(*after, body, source_map);
                }
                self.pop_scope();
            }
            ast::Stmt::Return(expr) => {
                if let Some(expr) = expr {
                    self.walk_expr(*expr, body, source_map, true);
                }
            }
            ast::Stmt::Throw { value } => {
                self.walk_expr(*value, body, source_map, true);
            }
            ast::Stmt::Assign { target, value } => {
                self.walk_expr(*target, body, source_map, true);
                self.walk_expr(*value, body, source_map, true);
            }
            ast::Stmt::AssignOp { target, value, .. } => {
                self.walk_expr(*target, body, source_map, true);
                self.walk_expr(*value, body, source_map, true);
            }
            ast::Stmt::Break
            | ast::Stmt::Continue
            | ast::Stmt::Missing
            | ast::Stmt::HeaderComment { .. } => {}
        }
    }

    fn walk_expr_children(
        &mut self,
        expr_id: ast::ExprId,
        body: &ast::ExprBody,
        source_map: &ast::AstSourceMap,
    ) {
        match &body.exprs[expr_id] {
            ast::Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.walk_expr(*condition, body, source_map, true);
                self.walk_expr(*then_branch, body, source_map, true);
                if let Some(else_branch) = else_branch {
                    self.walk_expr(*else_branch, body, source_map, true);
                }
            }
            ast::Expr::Match {
                scrutinee, arms, ..
            } => {
                self.walk_expr(*scrutinee, body, source_map, true);
                for &arm_id in arms {
                    self.walk_match_arm(arm_id, body, source_map);
                }
            }
            ast::Expr::Is { scrutinee, .. } => {
                // `<expr> is <pattern>` is a one-shot pattern test that yields
                // `bool`. Pattern bindings do NOT escape into the surrounding
                // scope (use `match` / `let` if you need that). Type
                // references inside the pattern are resolved later by TIR.
                self.walk_expr(*scrutinee, body, source_map, true);
            }
            ast::Expr::Catch { base, clauses } => {
                self.walk_expr(*base, body, source_map, true);
                for clause in clauses {
                    self.walk_catch_clause(
                        clause,
                        body,
                        source_map,
                        Self::catch_clause_scope_span(clause, source_map),
                    );
                }
            }
            ast::Expr::Throw { value } => {
                self.walk_expr(*value, body, source_map, true);
            }
            ast::Expr::Spawn {
                name,
                body: spawn_body,
            } => {
                if let Some(name) = name {
                    self.walk_expr(*name, body, source_map, true);
                }
                self.walk_expr(*spawn_body, body, source_map, true);
            }
            ast::Expr::Await { future } => {
                self.walk_expr(*future, body, source_map, true);
            }
            ast::Expr::Binary { lhs, rhs, .. } => {
                self.walk_expr(*lhs, body, source_map, true);
                self.walk_expr(*rhs, body, source_map, true);
            }
            ast::Expr::Unary { expr, .. } | ast::Expr::OptionalChain { expr } => {
                self.walk_expr(*expr, body, source_map, true);
            }
            ast::Expr::Call { callee, args, .. } | ast::Expr::OptionalCall { callee, args } => {
                self.walk_expr(*callee, body, source_map, true);
                for arg in args {
                    self.walk_expr(arg.expr, body, source_map, true);
                }
            }
            ast::Expr::Object {
                fields, spreads, ..
            } => {
                for (_, field_expr) in fields {
                    self.walk_expr(*field_expr, body, source_map, true);
                }
                for spread in spreads {
                    self.walk_expr(spread.expr, body, source_map, true);
                }
            }
            ast::Expr::Array { elements } => {
                for &element in elements {
                    self.walk_expr(element, body, source_map, true);
                }
            }
            ast::Expr::Map { entries } => {
                for &(key, value) in entries {
                    self.walk_expr(key, body, source_map, true);
                    self.walk_expr(value, body, source_map, true);
                }
            }
            ast::Expr::MemberAccess { base, .. } | ast::Expr::OptionalMemberAccess { base, .. } => {
                self.walk_expr(*base, body, source_map, true);
            }
            ast::Expr::Index { base, index } | ast::Expr::OptionalIndex { base, index } => {
                self.walk_expr(*base, body, source_map, true);
                self.walk_expr(*index, body, source_map, true);
            }
            ast::Expr::Path(segments) => {
                if let Some(root) = segments.first() {
                    let use_scope = self.current_scope_id();
                    let use_offset = source_map.expr_span(expr_id).start();
                    self.record_path_root_reference(root, use_scope, use_offset);
                    if segments.len() >= 2 {
                        self.classify_path_expr(expr_id, segments, use_scope, use_offset);
                    }
                }
            }
            ast::Expr::Literal(_)
            | ast::Expr::ByteStringLiteral(_)
            | ast::Expr::Null
            | ast::Expr::Block { .. }
            | ast::Expr::Lambda(_)
            | ast::Expr::Missing => {}
        }
    }

    fn register_local_pattern(
        &mut self,
        pat_id: ast::PatId,
        site: DefinitionSite,
        body: &ast::ExprBody,
        source_map: &ast::AstSourceMap,
        visible_from: TextSize,
    ) {
        // Walk the pattern structurally. `collect_pattern_names` returns the
        // set of names introduced and emits diagnostics for duplicate names
        // and Or-alternative mismatches as it goes.
        let names =
            Self::collect_pattern_names(&body.patterns, pat_id, source_map, &mut self.diagnostics);

        for (name, name_range) in names {
            let scope_id = self.current_scope_id();
            self.scope_bindings[scope_id.index() as usize]
                .bindings
                .push(LocalBinding {
                    name,
                    site,
                    pattern: pat_id,
                    name_range,
                    visible_from,
                });
        }
    }

    /// Recursively walk a pattern and return the set of names it introduces
    /// into scope, paired with the source range of each binding's first
    /// occurrence. Emits diagnostics in two situations:
    ///
    /// 1. **Duplicate names within a single pattern.** Within `Class { a, a }`
    ///    or a chain like `let Foo { x }: let x = ...`, the same name binding
    ///    appears twice — illegal.
    ///
    /// 2. **`Or` alternatives that don't bind the same names.** Each `Or`
    ///    alternative is its own scope, so duplicates *across* alternatives
    ///    are fine. But if alternatives bind *different* names, the arm body
    ///    would only sometimes see a given name — illegal.
    fn collect_pattern_names(
        patterns: &la_arena::Arena<ast::Pattern>,
        pat_id: ast::PatId,
        source_map: &ast::AstSourceMap,
        diagnostics: &mut Vec<Hir2Diagnostic>,
    ) -> FxHashMap<Name, TextRange> {
        match &patterns[pat_id] {
            ast::Pattern::Wildcard | ast::Pattern::Type(_) => FxHashMap::default(),
            ast::Pattern::Bind { name, subpat } => {
                let mut m = FxHashMap::default();
                m.insert(name.clone(), source_map.pattern_span(pat_id));
                if let Some(sp) = subpat {
                    let inner = Self::collect_pattern_names(patterns, *sp, source_map, diagnostics);
                    Self::merge_with_dup_check(&mut m, inner, diagnostics);
                }
                m
            }
            ast::Pattern::Class { fields, .. } => {
                let mut m: FxHashMap<Name, TextRange> = FxHashMap::default();
                let mut seen_fields: FxHashMap<Name, Vec<TextRange>> = FxHashMap::default();
                for f in fields {
                    seen_fields
                        .entry(f.field.clone())
                        .or_default()
                        .push(f.field_span);
                    let inner =
                        Self::collect_pattern_names(patterns, f.pat, source_map, diagnostics);
                    Self::merge_with_dup_check(&mut m, inner, diagnostics);
                }
                for (name, sites) in seen_fields {
                    if sites.len() > 1 {
                        diagnostics.push(Hir2Diagnostic::DuplicatePatternField { name, sites });
                    }
                }
                m
            }
            ast::Pattern::Array {
                prefix,
                rest,
                suffix,
                ascription: _,
            } => {
                let mut m: FxHashMap<Name, TextRange> = FxHashMap::default();
                for id in prefix {
                    let inner = Self::collect_pattern_names(patterns, *id, source_map, diagnostics);
                    Self::merge_with_dup_check(&mut m, inner, diagnostics);
                }
                if let Some(rest) = rest
                    && let Some(id) = rest.pat
                {
                    let inner = Self::collect_pattern_names(patterns, id, source_map, diagnostics);
                    Self::merge_with_dup_check(&mut m, inner, diagnostics);
                }
                for id in suffix {
                    let inner = Self::collect_pattern_names(patterns, *id, source_map, diagnostics);
                    Self::merge_with_dup_check(&mut m, inner, diagnostics);
                }
                m
            }
            ast::Pattern::Or(parts) => {
                // Each alternative is its own branch. Collect them
                // independently; duplicates are checked per-branch (already
                // done by the recursive call), and across-branch parity is
                // checked here.
                let branch_sets: Vec<FxHashMap<Name, TextRange>> = parts
                    .iter()
                    .map(|id| Self::collect_pattern_names(patterns, *id, source_map, diagnostics))
                    .collect();

                let mut has_mismatch = false;
                if let Some(first) = branch_sets.first() {
                    let first_names: std::collections::BTreeSet<&Name> = first.keys().collect();
                    let mut mismatched: std::collections::BTreeSet<Name> =
                        std::collections::BTreeSet::new();
                    for branch in &branch_sets[1..] {
                        let branch_names: std::collections::BTreeSet<&Name> =
                            branch.keys().collect();
                        for n in first_names.symmetric_difference(&branch_names) {
                            mismatched.insert((*n).clone());
                        }
                    }
                    if !mismatched.is_empty() {
                        has_mismatch = true;
                        // Tighten the span to cover just the Or's branches
                        // rather than the whole containing pattern (which can
                        // pull in surrounding trivia and span multiple lines).
                        let or_span = match (parts.first(), parts.last()) {
                            (Some(first), Some(last)) => {
                                let first_range = source_map.pattern_span(*first);
                                let last_range = source_map.pattern_span(*last);
                                TextRange::new(first_range.start(), last_range.end())
                            }
                            _ => source_map.pattern_span(pat_id),
                        };
                        diagnostics.push(Hir2Diagnostic::OrPatternBindingMismatch {
                            or_span,
                            mismatched_names: mismatched.into_iter().collect(),
                        });
                    }
                }

                if has_mismatch {
                    // On mismatch, suppress the Or's bindings entirely so
                    // downstream scopes don't depend on branch order
                    // (`let x | _` vs `_ | let x` would otherwise behave
                    // differently). The primary diagnostic above captures
                    // the error.
                    FxHashMap::default()
                } else {
                    // Every branch introduces the same set, so the first
                    // branch's contribution is representative.
                    branch_sets.into_iter().next().unwrap_or_default()
                }
            }
        }
    }

    /// Merge `source` into `target`. Any name already present in `target`
    /// produces a `DuplicatePatternBinding` diagnostic.
    fn merge_with_dup_check(
        target: &mut FxHashMap<Name, TextRange>,
        source: FxHashMap<Name, TextRange>,
        diagnostics: &mut Vec<Hir2Diagnostic>,
    ) {
        for (name, range) in source {
            if let Some(prev) = target.get(&name) {
                diagnostics.push(Hir2Diagnostic::DuplicatePatternBinding {
                    name: name.clone(),
                    sites: vec![*prev, range],
                });
            } else {
                target.insert(name, range);
            }
        }
    }

    fn walk_match_arm(
        &mut self,
        arm_id: ast::MatchArmId,
        body: &ast::ExprBody,
        source_map: &ast::AstSourceMap,
    ) {
        let arm = &body.match_arms[arm_id];
        self.push_scope(ScopeKind::MatchArm, None, source_map.match_arm_span(arm_id));
        let visible_from = source_map.pattern_span(arm.pattern).start();
        self.register_local_pattern(
            arm.pattern,
            DefinitionSite::PatternBinding(arm.pattern),
            body,
            source_map,
            visible_from,
        );
        if let Some(guard) = arm.guard {
            self.walk_expr(guard, body, source_map, true);
        }
        self.walk_expr(arm.body, body, source_map, true);
        self.pop_scope();
    }

    fn walk_catch_clause(
        &mut self,
        clause: &ast::CatchClause,
        body: &ast::ExprBody,
        source_map: &ast::AstSourceMap,
        catch_span: TextRange,
    ) {
        self.push_scope(ScopeKind::CatchClause, None, catch_span);
        let binding_visible_from = source_map.pattern_span(clause.binding).start();
        self.register_local_pattern(
            clause.binding,
            DefinitionSite::PatternBinding(clause.binding),
            body,
            source_map,
            binding_visible_from,
        );
        if let Some(st_pat) = clause.stack_trace_binding {
            let st_visible_from = source_map.pattern_span(st_pat).start();
            self.register_local_pattern(
                st_pat,
                DefinitionSite::PatternBinding(st_pat),
                body,
                source_map,
                st_visible_from,
            );
        }
        for &arm_id in &clause.arms {
            self.walk_catch_arm(arm_id, body, source_map);
        }
        self.pop_scope();
    }

    fn walk_catch_arm(
        &mut self,
        arm_id: ast::CatchArmId,
        body: &ast::ExprBody,
        source_map: &ast::AstSourceMap,
    ) {
        let arm = &body.catch_arms[arm_id];
        self.push_scope(ScopeKind::CatchArm, None, source_map.catch_arm_span(arm_id));
        let visible_from = source_map.pattern_span(arm.pattern).start();
        self.register_local_pattern(
            arm.pattern,
            DefinitionSite::PatternBinding(arm.pattern),
            body,
            source_map,
            visible_from,
        );
        self.walk_expr(arm.body, body, source_map, true);
        self.pop_scope();
    }

    fn catch_clause_scope_span(
        clause: &ast::CatchClause,
        source_map: &ast::AstSourceMap,
    ) -> TextRange {
        let binding_span = source_map.pattern_span(clause.binding);
        let mut start = binding_span.start();
        let mut end = binding_span.end();

        if let Some(stack_trace_binding) = clause.stack_trace_binding {
            let span = source_map.pattern_span(stack_trace_binding);
            start = start.min(span.start());
            end = end.max(span.end());
        }

        for &arm_id in &clause.arms {
            let span = source_map.catch_arm_span(arm_id);
            start = start.min(span.start());
            end = end.max(span.end());
        }

        TextRange::new(start, end)
    }

    fn walk_lambda_expr(
        &mut self,
        expr_id: ast::ExprId,
        func_def: &ast::FunctionDef,
        source_map: &ast::AstSourceMap,
    ) {
        self.push_scope(ScopeKind::Lambda, None, source_map.expr_span(expr_id));
        let scope_id = self.current_scope_id();
        for (idx, param) in func_def.params.iter().enumerate() {
            self.scope_bindings[scope_id.index() as usize]
                .params
                .push((param.name.clone(), idx));
        }
        self.emit_duplicate_param_diagnostics(&func_def.params);
        self.lambda_stack.push(scope_id);
        for param in &func_def.params {
            if let Some(default) = param.default {
                self.walk_expr(
                    default.expr(),
                    &func_def.defaults.exprs,
                    &func_def.defaults.source_map,
                    true,
                );
            }
        }
        if let Some(ast::FunctionBodyDef::Expr(lambda_body, lambda_source_map)) = &func_def.body {
            self.walk_expr_body(lambda_body, lambda_source_map);
            self.analyze_lambda_captures(scope_id, lambda_body, lambda_source_map);
        }
        self.lambda_stack.pop();
        self.pop_scope();
    }

    fn analyze_lambda_captures(
        &mut self,
        lambda_scope: FileScopeId,
        _lambda_body: &ast::ExprBody,
        _lambda_source_map: &ast::AstSourceMap,
    ) {
        let lambda_idx = lambda_scope.index() as usize;
        let mut captures: Vec<(Name, BindingId)> = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for reference in self
            .path_root_references
            .iter()
            .filter(|reference| reference.owner_lambda == Some(lambda_scope))
        {
            if let Some(binding_id) =
                self.visible_binding_at(reference.use_scope, reference.use_offset, &reference.name)
            {
                if !self.scope_is_descendant_or_self(binding_id.scope, lambda_scope)
                    && seen.insert(binding_id)
                {
                    captures.push((reference.name.clone(), binding_id));
                    self.scope_bindings[binding_id.scope.index() as usize]
                        .captured_bindings
                        .insert(binding_id);
                }
            }
        }

        self.scope_bindings[lambda_idx].captures = captures;
    }

    fn visible_binding_at(
        &self,
        scope_id: FileScopeId,
        at_offset: TextSize,
        name: &Name,
    ) -> Option<BindingId> {
        visible_binding_at_in_scopes(
            &self.scopes,
            &self.scope_bindings,
            scope_id,
            at_offset,
            name,
        )
    }

    fn scope_is_descendant_or_self(&self, scope_id: FileScopeId, ancestor_id: FileScopeId) -> bool {
        let mut current = Some(scope_id);
        while let Some(id) = current {
            if id == ancestor_id {
                return true;
            }
            current = self.scopes[id.index() as usize].parent;
        }
        false
    }

    fn classify_path_expr(
        &mut self,
        expr_id: ast::ExprId,
        segments: &[Name],
        use_scope: FileScopeId,
        use_offset: TextSize,
    ) {
        if segments.len() < 2 {
            return;
        }
        let root = &segments[0];
        let resolution = if self
            .visible_binding_at(use_scope, use_offset, root)
            .is_some()
        {
            PathResolution::Local { name: root.clone() }
        } else {
            PathResolution::Unknown
        };
        self.path_resolutions.push((expr_id, resolution));
    }

    fn record_path_root_reference(
        &mut self,
        root: &Name,
        use_scope: FileScopeId,
        use_offset: TextSize,
    ) {
        self.path_root_references.push(PathRootReference {
            name: root.clone(),
            use_scope,
            use_offset,
            owner_lambda: self.lambda_stack.last().copied(),
        });
    }

    // ── Item lowering ────────────────────────────────────────────────────────

    fn lower_item(&mut self, item: &ast::Item) {
        match item {
            ast::Item::Function(f) => {
                self.lower_function(f);
            }
            ast::Item::Class(c) => self.lower_class(c),
            ast::Item::Enum(e) => self.lower_enum(e),
            ast::Item::TypeAlias(ta) => self.lower_type_alias(ta),
            ast::Item::Client(c) => self.lower_client(c),
            ast::Item::Test(t) => self.lower_test(t),
            ast::Item::Generator(g) => self.lower_generator(g),
            ast::Item::TemplateString(ts) => self.lower_template_string(ts),
            ast::Item::RetryPolicy(rp) => self.lower_retry_policy(rp),
            ast::Item::Let(l) => self.lower_let(l),
        }
    }

    fn lower_function(&mut self, f: &ast::FunctionDef) -> LocalItemId<FunctionMarker> {
        let local_id = self.item_tree.alloc_function(f);
        ItemTree::collect_function_span(&mut self.item_tree_source_map, local_id, f);
        let loc = FunctionLoc::new(self.db, self.file, local_id);

        // Only contribute as a top-level symbol if not inside a class.
        // Methods belong to the class scope, not the package namespace.
        if self.class_depth == 0 {
            self.value_contributions.push((
                f.name.clone(),
                Contribution {
                    name_span: f.name_span,
                    definition: Definition::Function(loc),
                },
            ));
        }

        self.push_scope(ScopeKind::Function, Some(f.name.clone()), f.span);
        let scope_id = self.current_scope_id();

        for (idx, param) in f.params.iter().enumerate() {
            self.scope_bindings[scope_id.index() as usize]
                .params
                .push((param.name.clone(), idx));
        }
        self.emit_duplicate_param_diagnostics(&f.params);
        for param in &f.params {
            if let Some(default) = param.default {
                self.walk_expr(
                    default.expr(),
                    &f.defaults.exprs,
                    &f.defaults.source_map,
                    true,
                );
            }
        }

        if let Some(ast::FunctionBodyDef::Expr(ref body, ref source_map)) = f.body {
            self.walk_expr_body(body, source_map);
        }

        self.pop_scope();
        local_id
    }

    fn lower_class(&mut self, c: &ast::ClassDef) {
        let local_id = self.item_tree.alloc_class(c);
        ItemTree::collect_class_spans(&mut self.item_tree_source_map, local_id, c);
        let loc = ClassLoc::new(self.db, self.file, local_id);
        self.type_contributions.push((
            c.name.clone(),
            Contribution {
                name_span: c.name_span,
                definition: Definition::Class(loc),
            },
        ));

        self.push_scope(ScopeKind::Class, Some(c.name.clone()), c.span);

        // Unified per-scope duplicate detection: all members (fields, methods)
        // share one name-map so cross-kind collisions are also caught.
        let mut seen: FxHashMap<Name, Vec<MemberSite>> = FxHashMap::default();

        for field in &c.fields {
            seen.entry(field.name.clone())
                .or_default()
                .push(MemberSite {
                    range: field.name_span,
                    kind: DefinitionKind::Field,
                });
        }
        for method in &c.methods {
            seen.entry(method.name.clone())
                .or_default()
                .push(MemberSite {
                    range: method.name_span,
                    kind: DefinitionKind::Method,
                });
        }

        self.emit_duplicate_diagnostics(seen);

        // Walk class methods — inside class scope, so methods won't be
        // contributed as top-level symbols.
        self.class_depth += 1;
        let method_ids: Vec<_> = c.methods.iter().map(|m| self.lower_function(m)).collect();
        self.class_depth -= 1;

        self.item_tree.set_class_methods(local_id, method_ids);
        self.pop_scope();
    }

    fn lower_enum(&mut self, e: &ast::EnumDef) {
        let local_id = self.item_tree.alloc_enum(e);
        ItemTree::collect_enum_spans(&mut self.item_tree_source_map, local_id, e);
        let loc = EnumLoc::new(self.db, self.file, local_id);
        self.type_contributions.push((
            e.name.clone(),
            Contribution {
                name_span: e.name_span,
                definition: Definition::Enum(loc),
            },
        ));

        self.push_scope(ScopeKind::Enum, Some(e.name.clone()), e.span);

        let mut seen: FxHashMap<Name, Vec<MemberSite>> = FxHashMap::default();
        for variant in &e.variants {
            seen.entry(variant.name.clone())
                .or_default()
                .push(MemberSite {
                    range: variant.name_span,
                    kind: DefinitionKind::Variant,
                });
        }

        self.emit_duplicate_diagnostics(seen);

        self.pop_scope();
    }

    fn lower_type_alias(&mut self, ta: &ast::TypeAliasDef) {
        let local_id = self.item_tree.alloc_type_alias(ta);
        let loc = TypeAliasLoc::new(self.db, self.file, local_id);
        self.type_contributions.push((
            ta.name.clone(),
            Contribution {
                name_span: ta.name_span,
                definition: Definition::TypeAlias(loc),
            },
        ));

        self.push_scope(ScopeKind::TypeAlias, Some(ta.name.clone()), ta.span);
        self.pop_scope();
    }

    fn lower_client(&mut self, c: &ast::ClientDef) {
        let local_id = self.item_tree.alloc_client(c);
        let loc = ClientLoc::new(self.db, self.file, local_id);
        self.value_contributions.push((
            c.name.clone(),
            Contribution {
                name_span: c.name_span,
                definition: Definition::Client(loc),
            },
        ));

        self.push_scope(ScopeKind::Item, Some(c.name.clone()), c.span);
        self.pop_scope();
    }

    fn lower_test(&mut self, t: &ast::TestDef) {
        let local_id = self.item_tree.alloc_test(t);
        let loc = TestLoc::new(self.db, self.file, local_id);
        self.value_contributions.push((
            t.name.clone(),
            Contribution {
                name_span: t.name_span,
                definition: Definition::Test(loc),
            },
        ));

        self.push_scope(ScopeKind::Item, Some(t.name.clone()), t.span);
        self.pop_scope();
    }

    fn lower_generator(&mut self, g: &ast::GeneratorDef) {
        let local_id = self.item_tree.alloc_generator(g);
        ItemTree::collect_generator_spans(&mut self.item_tree_source_map, local_id, g);
        let loc = GeneratorLoc::new(self.db, self.file, local_id);
        self.value_contributions.push((
            g.name.clone(),
            Contribution {
                name_span: g.name_span,
                definition: Definition::Generator(loc),
            },
        ));

        self.push_scope(ScopeKind::Item, Some(g.name.clone()), g.span);
        self.pop_scope();
    }

    fn lower_template_string(&mut self, ts: &ast::TemplateStringDef) {
        let local_id = self.item_tree.alloc_template_string(ts);
        let loc = TemplateStringLoc::new(self.db, self.file, local_id);
        self.value_contributions.push((
            ts.name.clone(),
            Contribution {
                name_span: ts.name_span,
                definition: Definition::TemplateString(loc),
            },
        ));

        self.push_scope(ScopeKind::Function, Some(ts.name.clone()), ts.span);
        self.pop_scope();
    }

    fn lower_retry_policy(&mut self, rp: &ast::RetryPolicyDef) {
        let local_id = self.item_tree.alloc_retry_policy(rp);
        let loc = RetryPolicyLoc::new(self.db, self.file, local_id);
        self.value_contributions.push((
            rp.name.clone(),
            Contribution {
                name_span: rp.name_span,
                definition: Definition::RetryPolicy(loc),
            },
        ));

        self.push_scope(ScopeKind::Item, Some(rp.name.clone()), rp.span);
        self.pop_scope();
    }

    fn lower_let(&mut self, l: &ast::LetDef) {
        let local_id = self.item_tree.alloc_let(l);
        let loc = LetLoc::new(self.db, self.file, local_id);
        self.value_contributions.push((
            l.name.clone(),
            Contribution {
                name_span: l.name_span,
                definition: Definition::Let(loc),
            },
        ));

        self.push_scope(ScopeKind::Let, Some(l.name.clone()), l.span);
        if let Some((ref body, ref source_map)) = l.initializer {
            self.walk_expr_body(body, source_map);
        }
        self.pop_scope();
    }

    fn validate_phase1_builtin_contracts(&mut self, items: &[ast::Item]) {
        let is_builtin_file = self
            .file
            .path(self.db)
            .to_string_lossy()
            .starts_with("<builtin>/");
        for item in items {
            self.validate_item_phase1(item, is_builtin_file);
        }
    }

    fn validate_item_phase1(&mut self, item: &ast::Item, is_builtin_file: bool) {
        match item {
            ast::Item::Function(function) => {
                self.validate_function_phase1(function, is_builtin_file, "function");
            }
            ast::Item::Class(class) => {
                self.validate_internal_attributes(
                    &class.attributes,
                    is_builtin_file,
                    "class",
                    false,
                );
                self.validate_schema_attributes(&class.attributes);
                for field in &class.fields {
                    if let Some(type_expr) = &field.type_expr {
                        self.validate_type_expr_phase1(
                            &type_expr.expr,
                            type_expr.span,
                            is_builtin_file,
                        );
                    }
                    self.validate_internal_attributes(
                        &field.attributes,
                        is_builtin_file,
                        "class field",
                        false,
                    );
                    self.validate_schema_attributes(&field.attributes);
                }
                for method in &class.methods {
                    self.validate_function_phase1(method, is_builtin_file, "method");
                }
            }
            ast::Item::Enum(enm) => {
                self.validate_schema_attributes(&enm.attributes);
                for variant in &enm.variants {
                    self.validate_schema_attributes(&variant.attributes);
                }
            }
            ast::Item::TypeAlias(alias) => {
                if let Some(type_expr) = &alias.type_expr {
                    self.validate_type_expr_phase1(
                        &type_expr.expr,
                        type_expr.span,
                        is_builtin_file,
                    );
                }
            }
            _ => {}
        }
    }

    fn validate_function_phase1(
        &mut self,
        function: &ast::FunctionDef,
        is_builtin_file: bool,
        context: &'static str,
    ) {
        let is_host_bound = matches!(function.body, Some(ast::FunctionBodyDef::Builtin(_)));
        self.validate_internal_attributes(
            &function.attributes,
            is_builtin_file,
            context,
            is_host_bound,
        );

        for param in &function.params {
            if let Some(type_expr) = &param.type_expr {
                self.validate_type_expr_phase1(&type_expr.expr, type_expr.span, is_builtin_file);
            }
        }
        if let Some(type_expr) = &function.return_type {
            self.validate_type_expr_phase1(&type_expr.expr, type_expr.span, is_builtin_file);
        }
        if let Some(type_expr) = &function.throws {
            self.validate_type_expr_phase1(&type_expr.expr, type_expr.span, is_builtin_file);
        }

        if let Some(ast::FunctionBodyDef::Builtin(kind)) = function.body {
            if !is_builtin_file {
                let feature = match kind {
                    ast::BuiltinKind::Vm => "$rust_function",
                    ast::BuiltinKind::Io => "$rust_io_function",
                    ast::BuiltinKind::Intrinsic => "$compiler_intrinsic",
                };
                self.diagnostics.push(Hir2Diagnostic::BuiltinOnlySyntax {
                    feature: feature.to_string(),
                    span: function.span,
                });
                return;
            }

            if let Some(throws) = &function.throws {
                let mut invalid = Vec::new();
                Self::collect_invalid_builtin_throw_types(
                    &throws.expr,
                    &function.generic_params,
                    &mut invalid,
                );
                if !invalid.is_empty() {
                    self.diagnostics.push(Hir2Diagnostic::DiagnosticMessage {
                        diagnostic_id: DiagnosticId::ThrowsContractViolation,
                        message: format!(
                            "Host-bound builtin `{}` may only declare `throws` using builtin error types and function generic vars; invalid entries: {}",
                            function.name,
                            invalid.join(", ")
                        ),
                        span: throws.span,
                    });
                }
            }
        }
    }

    fn validate_internal_attributes(
        &mut self,
        attributes: &[ast::RawAttribute],
        is_builtin_file: bool,
        context: &'static str,
        is_host_bound: bool,
    ) {
        for attr in attributes {
            let name = attr.name.as_str();
            if !name.starts_with("internal.") {
                continue;
            }

            if !is_builtin_file {
                self.diagnostics.push(Hir2Diagnostic::BuiltinOnlySyntax {
                    feature: format!("@@{name}"),
                    span: attr.span,
                });
                continue;
            }

            match name {
                "internal.opaque" => {
                    if context != "class" {
                        self.diagnostics
                            .push(Hir2Diagnostic::InvalidAttributeContext {
                                attr_name: attr.name.clone(),
                                context,
                                allowed_contexts: "builtin classes",
                                span: attr.span,
                            });
                    }
                }
                "internal.uses" => {
                    if !matches!(context, "function" | "method") || !is_host_bound {
                        self.diagnostics
                            .push(Hir2Diagnostic::InvalidAttributeContext {
                                attr_name: attr.name.clone(),
                                context,
                                allowed_contexts: "host-bound builtin functions and methods",
                                span: attr.span,
                            });
                        continue;
                    }
                    if attr.args.len() != 1 {
                        self.diagnostics.push(Hir2Diagnostic::DiagnosticMessage {
                            diagnostic_id: DiagnosticId::InvalidAttributeArg,
                            message: format!(
                                "Attribute `@@{name}` expects exactly one argument: `vm` or `engine_ctx`"
                            ),
                            span: attr.span,
                        });
                        continue;
                    }
                    let value = attr.args[0].value.as_str();
                    if value != "vm" && value != "engine_ctx" {
                        self.diagnostics.push(Hir2Diagnostic::DiagnosticMessage {
                            diagnostic_id: DiagnosticId::InvalidAttributeArg,
                            message: format!(
                                "Attribute `@@{name}` only accepts `vm` or `engine_ctx`, got `{value}`"
                            ),
                            span: attr.args[0].span,
                        });
                    }
                }
                "internal.panics" => {
                    if !matches!(context, "function" | "method") || !is_host_bound {
                        self.diagnostics
                            .push(Hir2Diagnostic::InvalidAttributeContext {
                                attr_name: attr.name.clone(),
                                context,
                                allowed_contexts: "host-bound builtin functions and methods",
                                span: attr.span,
                            });
                        continue;
                    }
                    for arg in &attr.args {
                        let value = arg.value.as_str();
                        if value != "HostPanic" && value != "baml.errors.HostPanic" {
                            self.diagnostics.push(Hir2Diagnostic::DiagnosticMessage {
                                diagnostic_id: DiagnosticId::InvalidAttributeArg,
                                message: format!(
                                    "Attribute `@@{name}` may only reference known builtin panic types; got `{value}`"
                                ),
                                span: arg.span,
                            });
                        }
                    }
                }
                _ => {
                    self.diagnostics
                        .push(Hir2Diagnostic::UnknownInternalAttribute {
                            attr_name: attr.name.clone(),
                            span: attr.span,
                            valid_attributes: vec![
                                "internal.opaque",
                                "internal.uses",
                                "internal.panics",
                            ],
                        });
                }
            }
        }
    }

    /// Validate `@description`, `@alias`, and `@skip` attribute usage.
    ///
    /// - `description` / `alias`: exactly 1 argument, must be a string literal
    /// - `skip`: exactly 0 arguments
    ///
    /// Unknown attributes are silently passed through (e.g. `@stream.*` for PPIR).
    fn validate_schema_attributes(&mut self, attributes: &[ast::RawAttribute]) {
        for attr in attributes {
            match attr.name.as_str() {
                "description" | "alias" => {
                    let attr_name = attr.name.as_str();
                    if attr.args.len() != 1 {
                        self.diagnostics.push(Hir2Diagnostic::DiagnosticMessage {
                            diagnostic_id: DiagnosticId::InvalidAttributeArg,
                            message: format!("`@{attr_name}` expects exactly one string argument"),
                            span: attr.span,
                        });
                        continue;
                    }
                    let value = attr.args[0].value.as_str();
                    if !is_string_literal(value) {
                        self.diagnostics.push(Hir2Diagnostic::DiagnosticMessage {
                            diagnostic_id: DiagnosticId::InvalidAttributeArg,
                            message: format!(
                                "`@{attr_name}` argument must be a string literal, got `{value}`"
                            ),
                            span: attr.args[0].span,
                        });
                    }
                }
                "skip" => {
                    if !attr.args.is_empty() {
                        self.diagnostics.push(Hir2Diagnostic::DiagnosticMessage {
                            diagnostic_id: DiagnosticId::UnexpectedAttributeArg,
                            message: "`@skip` does not take any arguments".to_string(),
                            span: attr.span,
                        });
                    }
                }
                _ => {
                    // Unknown attributes passed through silently (e.g. @stream.*)
                }
            }
        }
    }

    fn validate_type_expr_phase1(
        &mut self,
        type_expr: &ast::TypeExpr,
        span: TextRange,
        is_builtin_file: bool,
    ) {
        if is_builtin_file {
            return;
        }

        if Self::type_expr_contains_rust(type_expr) {
            self.diagnostics.push(Hir2Diagnostic::BuiltinOnlySyntax {
                feature: "$rust_type".to_string(),
                span,
            });
        }

        Self::collect_unknown_type_attrs(type_expr, &mut self.diagnostics);
    }

    /// Known type-level attribute names (not field attrs, which are handled by
    /// `disambiguate::validate_field_attrs`).
    const KNOWN_TYPE_ATTRS: &'static [&'static str] =
        &["stream.done", "stream.must_exist", "stream.with_state"];

    fn collect_unknown_type_attrs(
        type_expr: &ast::TypeExpr,
        diagnostics: &mut Vec<Hir2Diagnostic>,
    ) {
        for attr in type_expr.attrs() {
            let name = attr.name.as_str();
            if !ast::is_field_attr(name) && !Self::KNOWN_TYPE_ATTRS.contains(&name) {
                diagnostics.push(Hir2Diagnostic::UnknownTypeAttribute {
                    attr_name: attr.name.clone(),
                    span: attr.span,
                });
            }
        }

        match type_expr {
            ast::TypeExpr::Optional { inner, .. } | ast::TypeExpr::List { inner, .. } => {
                Self::collect_unknown_type_attrs(inner, diagnostics);
            }
            ast::TypeExpr::Map { key, value, .. } => {
                Self::collect_unknown_type_attrs(key, diagnostics);
                Self::collect_unknown_type_attrs(value, diagnostics);
            }
            ast::TypeExpr::Union { variants, .. } => {
                for v in variants {
                    Self::collect_unknown_type_attrs(v, diagnostics);
                }
            }
            ast::TypeExpr::Function {
                params,
                ret,
                throws,
                ..
            } => {
                for p in params {
                    Self::collect_unknown_type_attrs(&p.ty, diagnostics);
                }
                Self::collect_unknown_type_attrs(ret, diagnostics);
                if let Some(throws) = throws {
                    Self::collect_unknown_type_attrs(throws, diagnostics);
                }
            }
            _ => {}
        }
    }

    fn type_expr_contains_rust(type_expr: &ast::TypeExpr) -> bool {
        match type_expr {
            ast::TypeExpr::Rust { .. } => true,
            ast::TypeExpr::Optional { inner, .. } | ast::TypeExpr::List { inner, .. } => {
                Self::type_expr_contains_rust(inner)
            }
            ast::TypeExpr::Map { key, value, .. } => {
                Self::type_expr_contains_rust(key) || Self::type_expr_contains_rust(value)
            }
            ast::TypeExpr::Union { variants, .. } => {
                variants.iter().any(Self::type_expr_contains_rust)
            }
            ast::TypeExpr::Function {
                params,
                ret,
                throws,
                ..
            } => {
                params
                    .iter()
                    .any(|param| Self::type_expr_contains_rust(&param.ty))
                    || Self::type_expr_contains_rust(ret)
                    || throws
                        .as_ref()
                        .is_some_and(|throws| Self::type_expr_contains_rust(throws))
            }
            _ => false,
        }
    }

    fn collect_invalid_builtin_throw_types(
        type_expr: &ast::TypeExpr,
        allowed_generic_params: &[Name],
        invalid: &mut Vec<String>,
    ) {
        match type_expr {
            ast::TypeExpr::Path {
                segments,
                generic_args,
                ..
            } => {
                // Allow `baml.errors.*`, `root.errors.*`, and `baml.json.*` (fully qualified).
                // `baml.json.JsonParseError` / `baml.json.JsonDecodeError` /
                // `baml.json.JsonSerializationError` are stdlib error types just like
                // `baml.errors.*` ones; they need the same exemption.
                let is_builtin_error = segments.len() >= 3
                    && (segments[0].as_str() == "baml" || segments[0].as_str() == "root")
                    && (segments[1].as_str() == "errors" || segments[1].as_str() == "json");
                // Allow single-segment class names (e.g. `JsonParseError`) in
                // builtin files — the class is resolvable in the current namespace
                // and TIR will type-check it.  This allows builtin functions to
                // declare `throws` for classes defined in the same stdlib namespace
                // without requiring the full `baml.json.JsonParseError` path.
                let is_builtin_class_ref = segments.len() == 1
                    && generic_args.is_empty()
                    && segments[0]
                        .as_str()
                        .chars()
                        .next()
                        .is_some_and(char::is_uppercase);
                let is_allowed_generic = segments.len() == 1
                    && generic_args.is_empty()
                    && !is_builtin_class_ref
                    && allowed_generic_params
                        .iter()
                        .any(|name| name == &segments[0]);
                if !is_builtin_error && !is_builtin_class_ref && !is_allowed_generic {
                    invalid.push(Self::render_type_expr(type_expr));
                }
            }
            ast::TypeExpr::Union { variants, .. } => {
                for ty in variants {
                    Self::collect_invalid_builtin_throw_types(ty, allowed_generic_params, invalid);
                }
            }
            _ => invalid.push(Self::render_type_expr(type_expr)),
        }
    }

    fn render_type_expr(type_expr: &ast::TypeExpr) -> String {
        match type_expr {
            ast::TypeExpr::Path { segments, .. } => segments
                .iter()
                .map(Name::as_str)
                .collect::<Vec<_>>()
                .join("."),
            ast::TypeExpr::Int { .. } => "int".to_string(),
            ast::TypeExpr::Float { .. } => "float".to_string(),
            ast::TypeExpr::String { .. } => "string".to_string(),
            ast::TypeExpr::Bool { .. } => "bool".to_string(),
            ast::TypeExpr::Null { .. } => "null".to_string(),
            ast::TypeExpr::Never { .. } => "never".to_string(),
            ast::TypeExpr::Void { .. } => "void".to_string(),
            ast::TypeExpr::Uint8Array { .. } => "uint8array".to_string(),
            ast::TypeExpr::Media { kind, .. } => kind.to_string(),
            ast::TypeExpr::Optional { inner, .. } => format!("{}?", Self::render_type_expr(inner)),
            ast::TypeExpr::List { inner, .. } => format!("{}[]", Self::render_type_expr(inner)),
            ast::TypeExpr::Map { key, value, .. } => format!(
                "map<{}, {}>",
                Self::render_type_expr(key),
                Self::render_type_expr(value)
            ),
            ast::TypeExpr::Union { variants, .. } => variants
                .iter()
                .map(Self::render_type_expr)
                .collect::<Vec<_>>()
                .join(" | "),
            ast::TypeExpr::Literal { value, .. } => value.to_string(),
            ast::TypeExpr::Function {
                params,
                ret,
                throws,
                ..
            } => {
                let throws = throws
                    .as_deref()
                    .map(Self::render_type_expr)
                    .map(|throws| format!(" throws {throws}"))
                    .unwrap_or_default();
                format!(
                    "({}) -> {}{}",
                    params
                        .iter()
                        .map(|param| match &param.name {
                            Some(name) => {
                                format!("{}: {}", name, Self::render_type_expr(&param.ty))
                            }
                            None => Self::render_type_expr(&param.ty),
                        })
                        .collect::<Vec<_>>()
                        .join(", "),
                    Self::render_type_expr(ret),
                    throws
                )
            }
            ast::TypeExpr::BuiltinUnknown { .. } => "unknown".to_string(),
            ast::TypeExpr::Type { .. } => "type".to_string(),
            ast::TypeExpr::Rust { .. } => "$rust_type".to_string(),
            ast::TypeExpr::Error { .. } => "<error>".to_string(),
            ast::TypeExpr::Unknown { .. } => "<unknown>".to_string(),
        }
    }
}

/// Check if a raw attribute argument value is a valid string literal.
///
/// Accepts double-quoted (`"text"`), single-quoted (`'text'`), and raw strings
/// (`#"text"#`, `##"text"##`, etc.).
fn is_string_literal(value: &str) -> bool {
    // Double-quoted
    if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
        return true;
    }
    // Single-quoted
    if value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2 {
        return true;
    }
    // Raw string: #"text"#, ##"text"##, etc.
    let hashes = value.bytes().take_while(|&b| b == b'#').count();
    if hashes > 0 && value.len() >= hashes * 2 + 2 {
        let rest = &value[hashes..];
        let closing = format!("\"{}", &value[..hashes]);
        return rest.starts_with('"') && rest.ends_with(&closing);
    }
    false
}
