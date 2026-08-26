//! `SemanticIndexBuilder` — walks `Vec<ast::Item>` and builds `FileSemanticIndex`.
//!
//! Allocates scopes in DFS pre-order with `TextRange`, builds the `ItemTree`,
//! collects `FileSymbolContributions`, and records expression→scope mappings
//! with per-scope `ScopeBindings`.
//!
//! Scope chain: Project → Package → Namespace* → File → Items.

use std::sync::Arc;

/// Known type-level attribute names (not field attrs, which are
/// `disambiguate::FIELD_ATTR_NAMES`'s business). Public so completion can
/// enumerate exactly what this validation accepts.
pub const KNOWN_TYPE_ATTRS: &[&str] = &["stream.done", "stream.must_exist", "stream.with_state"];

use baml_base::{Name, SourceFile};
use baml_compiler_diagnostics::{diagnostic::DiagnosticId, runtime_type::SerializedKeyContainer};
use baml_compiler2_ast::{self as ast, LoweringDiagnostic};
use rustc_hash::{FxHashMap, FxHashSet};
use text_size::{TextRange, TextSize};

use crate::{
    contributions::{Contribution, Definition, DefinitionKind, FileSymbolContributions},
    diagnostic::{Hir2Diagnostic, MemberSite},
    file_package::file_package,
    ids::{FunctionMarker, LocalItemId},
    item_tree::{ImplBlock, ImplSubject, InterfaceFieldLink},
    loc::{
        ClassLoc, ClientLoc, EnumLoc, FunctionLoc, InterfaceLoc, LetLoc, RetryPolicyLoc,
        TemplateStringLoc, TestLoc, TypeAliasLoc,
    },
    scope::{FileScopeId, ItemScopeOwner, Scope, ScopeId, ScopeKind},
    semantic_index::{
        BindingId, DefinitionSite, ExprMetadataKey, ExprMetadataScope, FileSemanticIndex,
        LocalBinding, PathResolution, ScopeBindings, SemanticIndexExtra,
        visible_binding_at_in_scopes,
    },
};

#[derive(Debug, Clone)]
struct PathRootReference {
    name: Name,
    use_scope: FileScopeId,
    use_offset: TextSize,
    owner_lambda: Option<FileScopeId>,
}

#[derive(Default)]
struct PatternNames {
    /// Per introduced name: its source range and the `Pattern::Bind` node
    /// that introduces it.
    names: FxHashMap<Name, (TextRange, ast::PatId)>,
    duplicates: FxHashSet<Name>,
}

/// The head name used to seed an impl's stable `ImplId` from a target
/// `TypeExpr`: the last path segment (interface/class name), or a coarse shape
/// tag for non-path for-targets (primitives, list/map/union). This only needs
/// to be position-independent and reasonably collision-distributed — the
/// `LocalItemId` collision index disambiguates anything that shares a head.
fn impl_head_name(te: &ast::TypeExpr) -> Name {
    match &te.kind {
        ast::TypeExprKind::Path { segments, .. } => segments
            .last()
            .cloned()
            .unwrap_or_else(|| Name::new("#path")),
        _ => Name::new("#nonpath"),
    }
}

pub struct SemanticIndexBuilder<'db> {
    db: &'db dyn crate::Db,
    file: SourceFile,

    scopes: Vec<Scope>,
    scope_bindings: Vec<ScopeBindings>,
    /// Owning item -> its scope. Inverse of `Scope::owner`.
    item_scopes: FxHashMap<ItemScopeOwner, FileScopeId>,
    /// Stack of currently-open scope IDs.
    scope_stack: Vec<FileScopeId>,
    /// Depth of class scopes we're inside (> 0 means methods shouldn't
    /// contribute to top-level symbols — they belong to the class scope).
    class_depth: u32,

    /// Expression to lexical scope mappings, sorted by arena-safe key at the end.
    expr_scopes: Vec<(ExprMetadataKey, FileScopeId)>,
    /// Lambda expression -> the `Lambda` scope it opened (span-free join).
    lambda_scopes: Vec<(ExprMetadataKey, FileScopeId)>,

    /// Path root resolutions, sorted by arena-safe expression key at the end.
    path_resolutions: Vec<(ExprMetadataKey, PathResolution)>,
    /// Arena namespace active while walking an expression body or defaults.
    expr_metadata_scope_stack: Vec<ExprMetadataScope>,
    /// Path-root references collected while walking source order. Unlike
    /// `expr_scopes`, this carries the scope and innermost lambda context at
    /// collection time so capture analysis does not rely on arena-local `ExprId`s.
    path_root_references: Vec<PathRootReference>,
    lambda_stack: Vec<FileScopeId>,

    item_tree: crate::item_tree::builder::ItemTreeBuilder,
    type_contributions: Vec<(Name, Contribution<'db>)>,
    value_contributions: Vec<(Name, Contribution<'db>)>,
    diagnostics: Vec<Hir2Diagnostic>,
    lowering_diagnostics: Vec<LoweringDiagnostic>,
    invalid_pattern_bindings: FxHashMap<(FileScopeId, ast::PatId), FxHashSet<Name>>,
    invalid_pattern_binding_scopes: FxHashMap<(TextRange, ast::PatId), FileScopeId>,
    env_var_refs: Vec<baml_compiler2_ast::EnvVarRef>,
}

impl<'db> SemanticIndexBuilder<'db> {
    pub fn new(db: &'db dyn crate::Db, file: SourceFile) -> Self {
        Self {
            db,
            file,
            scopes: Vec::new(),
            scope_bindings: Vec::new(),
            item_scopes: FxHashMap::default(),
            scope_stack: Vec::new(),
            class_depth: 0,
            expr_scopes: Vec::new(),
            lambda_scopes: Vec::new(),
            path_resolutions: Vec::new(),
            expr_metadata_scope_stack: Vec::new(),
            path_root_references: Vec::new(),
            lambda_stack: Vec::new(),
            item_tree: crate::item_tree::builder::ItemTreeBuilder::new(),
            type_contributions: Vec::new(),
            value_contributions: Vec::new(),
            diagnostics: Vec::new(),
            lowering_diagnostics: Vec::new(),
            invalid_pattern_bindings: FxHashMap::default(),
            invalid_pattern_binding_scopes: FxHashMap::default(),
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
        self.expr_scopes.sort_by_key(|(key, _)| *key);
        self.lambda_scopes.sort_by_key(|(key, _)| *key);

        // Sort path_resolutions for binary search
        self.path_resolutions.sort_by_key(|(key, _)| *key);

        // Pre-intern ScopeIds for each FileScopeId
        let scope_ids: Vec<ScopeId<'db>> = (0..self.scopes.len())
            .map(|i| {
                #[allow(clippy::cast_possible_truncation)]
                ScopeId::new(self.db, self.file, FileScopeId::new(i as u32))
            })
            .collect();

        let extra = if self.diagnostics.is_empty()
            && self.lowering_diagnostics.is_empty()
            && self.invalid_pattern_bindings.is_empty()
        {
            None
        } else {
            Some(Box::new(SemanticIndexExtra {
                diagnostics: self.diagnostics,
                lowering_diagnostics: self.lowering_diagnostics,
                invalid_pattern_bindings: self.invalid_pattern_bindings,
                invalid_pattern_binding_scopes: self.invalid_pattern_binding_scopes,
            }))
        };

        // Drops the collision counter — it has no meaning once the tree is built.
        let (item_tree, item_tree_source_map) = self.item_tree.finish();

        FileSemanticIndex {
            scopes: self.scopes,
            expr_scopes: self.expr_scopes,
            lambda_scopes: self.lambda_scopes,
            scope_bindings: self.scope_bindings,
            scope_ids,
            item_scopes: self.item_scopes,
            item_tree: Arc::new(item_tree),
            item_tree_source_map: Arc::new(item_tree_source_map),
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
            owner: None,
            range,
            descendants: id.next()..id.next(), // empty initially; filled on pop
            is_template_body: false,
        });
        self.scope_bindings.push(ScopeBindings::new());
        self.scope_stack.push(id);
    }

    /// Link a scope to the item it was opened for.
    ///
    /// Recorded here, at the one place that knows both, rather than recovered
    /// later by comparing `item.span == scope.range`.
    fn record_scope_owner(&mut self, scope: FileScopeId, owner: ItemScopeOwner) {
        self.scopes[scope.index() as usize].owner = Some(owner);
        self.item_scopes.insert(owner, scope);
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
    fn current_expr_metadata_key(&self, expr_id: ast::ExprId) -> ExprMetadataKey {
        let scope = *self
            .expr_metadata_scope_stack
            .last()
            .expect("expression walked without an arena namespace");
        ExprMetadataKey::new(scope, expr_id)
    }

    fn record_expr_scope(&mut self, expr_id: ast::ExprId) {
        let key = self.current_expr_metadata_key(expr_id);
        self.expr_scopes.push((key, self.current_scope_id()));
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

    fn walk_type_operands(
        &mut self,
        ty: &ast::TypeExpr,
        body: &ast::ExprBody,
        source_map: &ast::AstSourceMap,
    ) {
        let mut operands = Vec::new();
        ty.unreflect_operands(&mut operands);
        for operand in operands {
            self.walk_expr(operand, body, source_map, true);
        }
    }

    /// Walk an `ExprBody` arena in source order, recording expression ownership
    /// and local bindings in the lexical scope that owns each expression.
    fn walk_expr_body(&mut self, body: &ast::ExprBody, source_map: &ast::AstSourceMap) {
        let metadata_scope = ExprMetadataScope::Body(self.current_scope_id());
        self.expr_metadata_scope_stack.push(metadata_scope);
        if let Some(root_expr) = body.root_expr {
            self.walk_expr(root_expr, body, source_map, false);
        }
        let popped = self.expr_metadata_scope_stack.pop();
        debug_assert_eq!(popped, Some(metadata_scope));
    }

    /// Takes the parameter list and default arena separately so it serves both
    /// declared functions and lambdas, which no longer share a type.
    fn walk_parameter_defaults(&mut self, params: &[ast::Param], defaults: &ast::FunctionDefaults) {
        let metadata_scope = ExprMetadataScope::ParameterDefault(self.current_scope_id());
        self.expr_metadata_scope_stack.push(metadata_scope);
        for param in params {
            if let Some(default) = param.default {
                self.walk_expr(default.expr(), &defaults.exprs, &defaults.source_map, true);
            }
        }
        let popped = self.expr_metadata_scope_stack.pop();
        debug_assert_eq!(popped, Some(metadata_scope));
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
                self.walk_lambda_expr(expr_id, func_def, body, source_map);
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
            ast::Stmt::TypeBinding { value, .. } => {
                self.walk_type_operands(value, body, source_map);
            }
            ast::Stmt::Let {
                pattern,
                initializer,
                else_branch,
                ..
            } => {
                if let Some(initializer) = initializer {
                    self.walk_expr(*initializer, body, source_map, true);
                }
                if let Some(else_expr) = else_branch {
                    // `let … else { … }` — the else block runs in the
                    // enclosing scope BEFORE the pattern's names are
                    // registered, so walk it before `register_local_pattern`.
                    // The block-expr's own push_scope/pop_scope keeps any
                    // inner bindings from leaking out.
                    self.walk_expr(*else_expr, body, source_map, true);
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
            ast::Stmt::WhileLet {
                pattern,
                scrutinee,
                body: loop_body,
            } => {
                // Scrutinee is evaluated in the enclosing scope (re-evaluated
                // each iteration, but lexically outside the loop body) —
                // mirrors `Stmt::While`'s condition and `Stmt::For`'s
                // collection. Walk it BEFORE pushing the body scope so its
                // paths don't falsely resolve to the loop's own bindings.
                self.walk_expr(*scrutinee, body, source_map, true);

                // Push ONE Block scope spanning the whole while-let statement,
                // mirroring `Stmt::While` / `Stmt::For`. The pattern bindings
                // live in THIS scope and are visible to the body (which adds
                // its own nested block scope); they vanish after the loop.
                self.push_scope(ScopeKind::Block, None, source_map.stmt_span(stmt_id));
                self.register_local_pattern(
                    *pattern,
                    DefinitionSite::Statement(stmt_id),
                    body,
                    source_map,
                    source_map.pattern_span(*pattern).start(),
                );
                self.walk_expr(*loop_body, body, source_map, true);
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
            ast::Stmt::Defer { body: defer_body } => {
                // The defer body is an inline `Expr::Block` in this same
                // `ExprBody`; walk it so its references/bindings are recorded.
                // The block's own push_scope/pop_scope contains inner bindings.
                self.walk_expr(*defer_body, body, source_map, true);
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
            ast::Expr::IfLet {
                pattern,
                scrutinee,
                then_branch,
                else_branch,
            } => {
                // Scrutinee is evaluated in the enclosing scope.
                self.walk_expr(*scrutinee, body, source_map, true);

                // Then-branch sees pattern bindings; push a fresh scope and
                // register them before walking. Mirrors `walk_match_arm`.
                let then_span = source_map.expr_span(*then_branch);
                self.push_scope(ScopeKind::MatchArm, None, then_span);
                let visible_from = source_map.pattern_span(*pattern).start();
                self.register_local_pattern(
                    *pattern,
                    DefinitionSite::PatternBinding(*pattern),
                    body,
                    source_map,
                    visible_from,
                );
                self.walk_expr(*then_branch, body, source_map, true);
                self.pop_scope();

                if let Some(else_branch) = else_branch {
                    // Else-branch never sees pattern bindings.
                    self.walk_expr(*else_branch, body, source_map, true);
                }
            }
            ast::Expr::Match {
                scrutinee,
                scrutinee_type,
                arms,
            } => {
                self.walk_expr(*scrutinee, body, source_map, true);
                if let Some(type_id) = scrutinee_type {
                    self.walk_type_operands(&body.type_annotations[*type_id], body, source_map);
                }
                for &arm_id in arms {
                    self.walk_match_arm(arm_id, body, source_map);
                }
            }
            ast::Expr::Is { scrutinee, pattern } => {
                // `<expr> is <pattern>` is a one-shot pattern test that yields
                // `bool`. Pattern bindings do NOT escape into the surrounding
                // scope (use `match` / `let` if you need that). Runtime
                // `unreflect(expr)` operands are ordinary expressions in the
                // enclosing scope and therefore need the normal HIR path walk.
                self.walk_expr(*scrutinee, body, source_map, true);
                self.walk_pattern_expressions(*pattern, body, source_map);
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
            ast::Expr::Return { value } => {
                if let Some(value) = value {
                    self.walk_expr(*value, body, source_map, true);
                }
            }
            ast::Expr::Spawn {
                name,
                with_exprs,
                body: spawn_body,
            } => {
                if let Some(name) = name {
                    self.walk_expr(*name, body, source_map, true);
                }
                for with_expr in with_exprs {
                    self.walk_expr(*with_expr, body, source_map, true);
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
            ast::Expr::Template { tag, .. } => match tag {
                // Tagged (`Custom`): the tag is an ordinary value reference in
                // the ENCLOSING scope, and the template body is its own lambda
                // scope so references to enclosing locals inside `${...}` are
                // computed as captures (BEP-049 §10 — MIR hand-rolls the body
                // closure off these captures).
                ast::TemplateTag::Custom {
                    tag,
                    body: flatten_body,
                } => {
                    self.walk_expr(*tag, body, source_map, true);
                    self.walk_template_lambda_body(expr_id, *flatten_body, body, source_map);
                }
                // Untagged (`Default`): no closure. The template is realized by
                // the desugared `elaborated` concat in the ENCLOSING scope, so
                // we walk that directly — it contains the same `${…}` exprs and
                // any `${for}` bindings (in normal loop scopes), with no capture
                // boundary. The structured `segments` are diagnostics-only.
                ast::TemplateTag::Default { elaborated } => {
                    self.walk_expr(*elaborated, body, source_map, true);
                }
            },
            ast::Expr::Unary { expr, .. } | ast::Expr::OptionalChain { expr } => {
                self.walk_expr(*expr, body, source_map, true);
            }
            ast::Expr::Call {
                callee,
                type_args,
                args,
            } => {
                self.walk_expr(*callee, body, source_map, true);
                for type_arg in type_args {
                    self.walk_type_operands(type_arg, body, source_map);
                }
                for arg in args {
                    self.walk_expr(arg.expr, body, source_map, true);
                }
            }
            ast::Expr::OptionalCall { callee, args } => {
                self.walk_expr(*callee, body, source_map, true);
                for arg in args {
                    self.walk_expr(arg.expr, body, source_map, true);
                }
            }
            ast::Expr::Object {
                type_args,
                fields,
                spreads,
                ..
            } => {
                for type_arg in type_args {
                    self.walk_type_operands(type_arg, body, source_map);
                }
                for field in fields {
                    self.walk_expr(field.value, body, source_map, true);
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
                for entry in entries {
                    self.walk_expr(entry.key, body, source_map, true);
                    self.walk_expr(entry.value, body, source_map, true);
                }
            }
            ast::Expr::MemberAccess { base, .. } | ast::Expr::OptionalMemberAccess { base, .. } => {
                self.walk_expr(*base, body, source_map, true);
            }
            ast::Expr::Upcast { base, target } => {
                self.walk_expr(*base, body, source_map, true);
                self.walk_type_operands(target, body, source_map);
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
                    self.resolve_path_expr(expr_id, root, use_scope, use_offset);
                }
            }
            ast::Expr::GenericApply { base, type_args } => {
                // `foo<int>` references the base callable; walk it so the path
                // root is recorded for name resolution. Type args are types,
                // not value references, so they need no walking here.
                self.walk_expr(*base, body, source_map, true);
                for type_arg in type_args {
                    self.walk_type_operands(type_arg, body, source_map);
                }
            }
            ast::Expr::Literal(_)
            | ast::Expr::ByteStringLiteral(_)
            | ast::Expr::Null
            | ast::Expr::Block { .. }
            | ast::Expr::Lambda(_)
            | ast::Expr::Missing => {}
            ast::Expr::QualifiedPath {
                qself, interface, ..
            } => {
                self.walk_type_operands(qself, body, source_map);
                self.walk_type_operands(interface, body, source_map);
            }
        }
    }

    /// Walk a *tagged* template's segments inside a fresh `ScopeKind::Lambda`
    /// scope spanning the whole template expression (BEP-049 §10). This makes
    /// the body a capture boundary: interpolation references to enclosing
    /// locals are recorded as captures (consumed by MIR when it hand-rolls the
    /// body closure). Untagged templates do NOT use this — they walk segments
    /// inline (no closure, no captures).
    ///
    /// The lambda's parameters (the tag's `body: (...) -> baml.TaggedString`
    /// params) are NOT registered here: the tag is a cross-file item whose
    /// signature cannot be resolved during the semantic-index walk (it would
    /// cycle `file_semantic_index` -> `package_items`), so MIR (and TIR) inject
    /// the params instead. `${for}` bindings still nest in their own block
    /// scopes via `walk_template_segment`.
    fn walk_template_lambda_body(
        &mut self,
        expr_id: ast::ExprId,
        flatten_body: ast::ExprId,
        body: &ast::ExprBody,
        source_map: &ast::AstSourceMap,
    ) {
        // The enclosing real lambda (if any) — captured BEFORE we push the
        // template's synthetic lambda. See the transitive-capture propagation
        // at the end of this function.
        let enclosing_lambda = self.lambda_stack.last().copied();

        // Span-free scope join, keyed by the template expression — the same
        // registration `walk_lambda_expr` does for real lambdas, so type
        // inference can enter this scope without spans.
        let key = self.current_expr_metadata_key(expr_id);
        self.push_scope(ScopeKind::Lambda, None, source_map.expr_span(expr_id));
        let scope_id = self.current_scope_id();
        self.lambda_scopes.push((key, scope_id));
        // Mark this as a synthetic template body: it is a Lambda scope for
        // capture-analysis purposes, but TIR types its body inline in the
        // enclosing scope, so `inference_owner_scope` must climb past it.
        self.scopes[scope_id.index() as usize].is_template_body = true;
        self.lambda_stack.push(scope_id);
        // Walk the desugared flatten block's CONTENTS inline in THIS synthetic
        // Lambda scope: its `${…}` exprs referencing enclosing locals become
        // captures, its synthetic accumulator `let`s register as lambda-locals,
        // and any `${for}` binding nests in a child block scope. The Lambda scope
        // range stays == the template-expr span, which MIR matches to find these
        // captures.
        //
        // `push_block_scope = false` keeps a block body's contents in this scope
        // (no child block scope) while still recording the block's own `ExprId`
        // in `expr_scopes`, so MIR/TIR scope lookups for the synthetic body
        // resolve. For a non-block body this matches the old `walk_expr(.., true)`
        // — `push_block_scope` is consulted only for `Expr::Block`.
        self.walk_expr(flatten_body, body, source_map, false);
        self.analyze_lambda_captures(scope_id, body, source_map);

        // BEP-049 §10 — transitive capture through a *synthetic* lambda. When a
        // tagged template sits inside a user lambda `f`, every var the template
        // body captures from *beyond* `f` must also be captured BY `f`: TIR types
        // the template body inline in `f`'s scope, and MIR forwards the template
        // closure's captures up through `f`. A real nested lambda gets this for
        // free (its references attribute to it AND its captures thread up); the
        // template's references attribute only to *this* synthetic lambda, so we
        // re-record each capture as a reference owned by `f`. Without this, `f`
        // never learns it must capture the var → TIR reports it unresolved
        // (`[E0003]`) and MIR cannot forward it.
        if let Some(enclosing) = enclosing_lambda {
            let at = source_map.expr_span(expr_id).start();
            let captures = self.scope_bindings[scope_id.index() as usize]
                .captures
                .clone();
            for (name, _binding) in captures {
                self.path_root_references.push(PathRootReference {
                    name,
                    use_scope: enclosing,
                    use_offset: at,
                    owner_lambda: Some(enclosing),
                });
            }
        }

        self.lambda_stack.pop();
        self.pop_scope();
    }

    fn register_local_pattern(
        &mut self,
        pat_id: ast::PatId,
        site: DefinitionSite,
        body: &ast::ExprBody,
        source_map: &ast::AstSourceMap,
        visible_from: TextSize,
    ) {
        // Evaluate expression-bearing pattern atoms (currently
        // `unreflect(expr)`) in the scope surrounding the bindings. This runs
        // before any names from this pattern are installed, so a pattern
        // cannot accidentally refer to a binding it is in the act of
        // declaring.
        self.walk_pattern_expressions(pat_id, body, source_map);

        // Walk the pattern structurally. `collect_pattern_names` returns the
        // set of names introduced and emits diagnostics for duplicate names
        // and Or-alternative mismatches as it goes.
        let names =
            Self::collect_pattern_names(&body.patterns, pat_id, source_map, &mut self.diagnostics);

        let scope_id = self.current_scope_id();
        if !names.duplicates.is_empty() {
            self.invalid_pattern_bindings
                .insert((scope_id, pat_id), names.duplicates);
            self.invalid_pattern_binding_scopes
                .insert((source_map.pattern_span(pat_id), pat_id), scope_id);
        }

        for (name, (name_range, bind_pattern)) in names.names {
            self.scope_bindings[scope_id.index() as usize]
                .bindings
                .push(LocalBinding {
                    name,
                    site,
                    pattern: pat_id,
                    bind_pattern,
                    name_range,
                    visible_from,
                });
        }
    }

    fn walk_pattern_expressions(
        &mut self,
        pat_id: ast::PatId,
        body: &ast::ExprBody,
        source_map: &ast::AstSourceMap,
    ) {
        match &body.patterns[pat_id] {
            ast::Pattern::Type(ty) => self.walk_type_operands(ty, body, source_map),
            ast::Pattern::Unreflect(operand) => {
                self.walk_expr(*operand, body, source_map, true);
            }
            ast::Pattern::Bind { subpat, .. } => {
                if let Some(subpat) = subpat {
                    self.walk_pattern_expressions(*subpat, body, source_map);
                }
            }
            ast::Pattern::Class {
                generic_args,
                associated_type_bindings,
                fields,
                ..
            } => {
                for ty in generic_args {
                    self.walk_type_operands(ty, body, source_map);
                }
                for binding in associated_type_bindings {
                    self.walk_type_operands(&binding.ty, body, source_map);
                }
                for field in fields {
                    self.walk_pattern_expressions(field.pat, body, source_map);
                }
            }
            ast::Pattern::Array {
                prefix,
                rest,
                suffix,
                ascription,
            } => {
                if let Some(ty) = ascription {
                    self.walk_type_operands(ty, body, source_map);
                }
                for pattern in prefix {
                    self.walk_pattern_expressions(*pattern, body, source_map);
                }
                if let Some(pattern) = rest.as_ref().and_then(|rest| rest.pat) {
                    self.walk_pattern_expressions(pattern, body, source_map);
                }
                for pattern in suffix {
                    self.walk_pattern_expressions(*pattern, body, source_map);
                }
            }
            ast::Pattern::Or(patterns) => {
                for pattern in patterns {
                    self.walk_pattern_expressions(*pattern, body, source_map);
                }
            }
            ast::Pattern::Wildcard => {}
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
    ) -> PatternNames {
        match &patterns[pat_id] {
            ast::Pattern::Wildcard | ast::Pattern::Type(_) | ast::Pattern::Unreflect(_) => {
                PatternNames::default()
            }
            ast::Pattern::Bind { name, subpat } => {
                let mut result = PatternNames::default();
                result
                    .names
                    .insert(name.clone(), (source_map.pattern_span(pat_id), pat_id));
                if let Some(sp) = subpat {
                    let inner = Self::collect_pattern_names(patterns, *sp, source_map, diagnostics);
                    Self::merge_with_dup_check(&mut result, inner, diagnostics);
                }
                result
            }
            ast::Pattern::Class { fields, .. } => {
                let mut result = PatternNames::default();
                let mut seen_fields: FxHashMap<Name, Vec<TextRange>> = FxHashMap::default();
                for f in fields {
                    seen_fields
                        .entry(f.field.clone())
                        .or_default()
                        .push(f.field_span);
                    let inner =
                        Self::collect_pattern_names(patterns, f.pat, source_map, diagnostics);
                    Self::merge_with_dup_check(&mut result, inner, diagnostics);
                }
                for (name, sites) in seen_fields {
                    if sites.len() > 1 {
                        diagnostics.push(Hir2Diagnostic::DuplicatePatternField { name, sites });
                    }
                }
                result
            }
            ast::Pattern::Array {
                prefix,
                rest,
                suffix,
                ascription: _,
            } => {
                let mut result = PatternNames::default();
                for id in prefix {
                    let inner = Self::collect_pattern_names(patterns, *id, source_map, diagnostics);
                    Self::merge_with_dup_check(&mut result, inner, diagnostics);
                }
                if let Some(rest) = rest
                    && let Some(id) = rest.pat
                {
                    let inner = Self::collect_pattern_names(patterns, id, source_map, diagnostics);
                    Self::merge_with_dup_check(&mut result, inner, diagnostics);
                }
                for id in suffix {
                    let inner = Self::collect_pattern_names(patterns, *id, source_map, diagnostics);
                    Self::merge_with_dup_check(&mut result, inner, diagnostics);
                }
                result
            }
            ast::Pattern::Or(parts) => {
                // Each alternative is its own branch. Collect them
                // independently; duplicates are checked per-branch (already
                // done by the recursive call), and across-branch parity is
                // checked here.
                let branch_sets: Vec<PatternNames> = parts
                    .iter()
                    .map(|id| Self::collect_pattern_names(patterns, *id, source_map, diagnostics))
                    .collect();

                let duplicates = branch_sets
                    .iter()
                    .flat_map(|branch| branch.duplicates.iter().cloned())
                    .collect();
                let mut has_mismatch = false;
                if let Some(first) = branch_sets.first() {
                    let first_names: std::collections::BTreeSet<&Name> =
                        first.names.keys().collect();
                    let mut mismatched: std::collections::BTreeSet<Name> =
                        std::collections::BTreeSet::new();
                    for branch in &branch_sets[1..] {
                        let branch_names: std::collections::BTreeSet<&Name> =
                            branch.names.keys().collect();
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
                    PatternNames {
                        names: FxHashMap::default(),
                        duplicates,
                    }
                } else {
                    // Every branch introduces the same set, so the first
                    // branch's contribution is representative.
                    PatternNames {
                        names: branch_sets
                            .into_iter()
                            .next()
                            .map_or_else(FxHashMap::default, |branch| branch.names),
                        duplicates,
                    }
                }
            }
        }
    }

    /// Merge `source` into `target`. Any name already present in `target`
    /// produces a `DuplicatePatternBinding` diagnostic.
    fn merge_with_dup_check(
        target: &mut PatternNames,
        source: PatternNames,
        diagnostics: &mut Vec<Hir2Diagnostic>,
    ) {
        target.duplicates.extend(source.duplicates);
        for (name, (range, bind_pattern)) in source.names {
            if let Some((prev, _)) = target.names.get(&name) {
                diagnostics.push(Hir2Diagnostic::DuplicatePatternBinding {
                    name: name.clone(),
                    sites: vec![*prev, range],
                });
                target.duplicates.insert(name);
            } else {
                target.names.insert(name, (range, bind_pattern));
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
            DefinitionSite::CatchBinding(clause.binding),
            body,
            source_map,
            binding_visible_from,
        );
        if let Some(st_pat) = clause.stack_trace_binding {
            let st_visible_from = source_map.pattern_span(st_pat).start();
            self.register_local_pattern(
                st_pat,
                DefinitionSite::CatchBinding(st_pat),
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
        lambda: &ast::LambdaDef,
        body: &ast::ExprBody,
        source_map: &ast::AstSourceMap,
    ) {
        let key = self.current_expr_metadata_key(expr_id);
        self.push_scope(ScopeKind::Lambda, None, source_map.expr_span(expr_id));
        let scope_id = self.current_scope_id();
        self.lambda_scopes.push((key, scope_id));
        for (idx, param) in lambda.params.iter().enumerate() {
            self.scope_bindings[scope_id.index() as usize]
                .params
                .push((param.name.clone(), idx));
        }
        self.emit_duplicate_param_diagnostics(&lambda.params);
        self.lambda_stack.push(scope_id);
        self.walk_parameter_defaults(&lambda.params, &lambda.defaults);

        let metadata_scope = ExprMetadataScope::Body(scope_id);
        self.expr_metadata_scope_stack.push(metadata_scope);
        for param in &lambda.params {
            if let Some(ty) = &param.type_expr {
                self.walk_type_operands(ty, body, source_map);
            }
        }
        if let Some(ty) = &lambda.return_type {
            self.walk_type_operands(ty, body, source_map);
        }
        if let Some(ty) = &lambda.throws {
            self.walk_type_operands(ty, body, source_map);
        }
        if let Some(lambda_body) = lambda.body {
            // The body shares this arena, but it still gets its own metadata
            // namespace keyed by the lambda's scope. That keeps HIR agreeing
            // with TIR's `infer_lambda_body` and MIR's `lower_lambda`, both of
            // which look expression metadata up under the lambda scope. A
            // mismatch here does not fail loudly: `path_resolution` simply
            // misses, flow narrowing silently stops inside every lambda, and
            // reconstructed closure signatures silently degrade to `unknown`.
            self.walk_expr(lambda_body, body, source_map, false);
            self.analyze_lambda_captures(scope_id, body, source_map);
        }
        let popped = self.expr_metadata_scope_stack.pop();
        debug_assert_eq!(popped, Some(metadata_scope));
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

    fn resolve_path_expr(
        &mut self,
        expr_id: ast::ExprId,
        root: &Name,
        use_scope: FileScopeId,
        use_offset: TextSize,
    ) {
        let resolution = self
            .visible_binding_at(use_scope, use_offset, root)
            .map_or(PathResolution::Unknown, PathResolution::Local);
        let key = self.current_expr_metadata_key(expr_id);
        self.path_resolutions.push((key, resolution));
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
            ast::Item::TemplateString(ts) => self.lower_template_string(ts),
            ast::Item::RetryPolicy(rp) => self.lower_retry_policy(rp),
            ast::Item::Let(l) => self.lower_let(l),
            ast::Item::Interface(i) => self.lower_interface(i),
            ast::Item::ImplementsFor(imp) => self.lower_implements_for(imp),
        }
    }

    fn lower_function(&mut self, f: &ast::FunctionDef) -> LocalItemId<FunctionMarker> {
        let local_id = self.item_tree.alloc_function(f);
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
        self.record_scope_owner(scope_id, ItemScopeOwner::Function(local_id));

        for (idx, param) in f.params.iter().enumerate() {
            self.scope_bindings[scope_id.index() as usize]
                .params
                .push((param.name.clone(), idx));
        }
        self.emit_duplicate_param_diagnostics(&f.params);
        self.walk_parameter_defaults(&f.params, &f.defaults);

        if let Some(ast::FunctionBodyDef::Expr(ref body, ref source_map)) = f.body {
            self.walk_expr_body(body, source_map);
        }

        self.pop_scope();
        local_id
    }

    fn lower_class(&mut self, c: &ast::ClassDef) {
        let local_id = self.item_tree.alloc_class(c);
        let loc = ClassLoc::new(self.db, self.file, local_id);
        self.type_contributions.push((
            c.name.clone(),
            Contribution {
                name_span: c.name_span,
                definition: Definition::Class(loc),
            },
        ));

        self.push_scope(ScopeKind::Class, Some(c.name.clone()), c.span);
        let class_scope = self.current_scope_id();
        self.record_scope_owner(class_scope, ItemScopeOwner::Class(local_id));

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
            // `to_string` / `to_json` are not magic methods: each must be provided
            // by implementing `baml.ToString` / `baml.ToJson`, never declared
            // directly on the class. (Methods inside `implements I { ... }` blocks
            // live in `c.implements`, not `c.methods`, so an interface impl is fine.)
            if method.name.as_str() == "to_string" {
                self.diagnostics
                    .push(Hir2Diagnostic::ToStringMustImplementInterface {
                        class_name: c.name.clone(),
                        span: method.name_span,
                    });
            }
            if method.name.as_str() == "to_json" {
                self.diagnostics
                    .push(Hir2Diagnostic::ToJsonMustImplementInterface {
                        class_name: c.name.clone(),
                        span: method.name_span,
                    });
            }
            // `from_json` likewise belongs to `baml.FromJson`. The auto-derived
            // structural-default delegate (origin `AutoDerive`) is exempt — it is
            // synthesized, not user-written, and is `baml.FromJson`'s default.
            if method.name.as_str() == "from_json"
                && method.metadata.origin != ast::FunctionOrigin::AutoDerive
            {
                self.diagnostics
                    .push(Hir2Diagnostic::FromJsonMustImplementInterface {
                        class_name: c.name.clone(),
                        span: method.name_span,
                    });
            }
            // BEP-042: `cleanup` is a reserved magic finalizer name. A method
            // named `cleanup` whose signature isn't `cleanup(self) -> void` is
            // malformed (the magic guard only fires for the exact shape).
            if method.name.as_str() == ast::cleanup_guard::CLEANUP_METHOD
                && !ast::cleanup_guard::has_cleanup_shape(method)
            {
                self.diagnostics
                    .push(Hir2Diagnostic::CleanupMagicMethodSignature {
                        class_name: c.name.clone(),
                        span: method.name_span,
                    });
            }
            seen.entry(method.name.clone())
                .or_default()
                .push(MemberSite {
                    range: method.name_span,
                    kind: DefinitionKind::Method,
                });
        }

        self.emit_duplicate_diagnostics(seen);

        // Walk class methods — inside class scope, so methods won't be
        // contributed as top-level symbols. We collapse class-level methods
        // and all `implements I { ... }` method overrides into a single id
        // list so downstream code (which queries `Class::methods`) sees them
        // uniformly. Disambiguation of which interface a method satisfies
        // happens in TIR via `class.implements`.
        self.class_depth += 1;
        let mut method_ids: Vec<_> = c.methods.iter().map(|m| self.lower_function(m)).collect();
        for impl_block in &c.implements {
            let mut block_method_ids = Vec::new();
            for m in &impl_block.methods {
                let fid = self.lower_function(m);
                // BEP-044: remember which interface this method came from so
                // `default.<name>()` calls inside the body can resolve back
                // to the interface's default function.
                self.item_tree.record_method_interface_target(
                    fid,
                    impl_block.target.clone(),
                    impl_block.associated_type_bindings.clone(),
                );
                block_method_ids.push(fid);
            }
            // Dual-write: also record this in-body impl under a stable `ImplId`.
            // An in-body `implements I {}` (and a simple `implement I for C`
            // merged onto the class) is `InClass`; its for-type is the class.
            let iface_head = impl_head_name(&impl_block.target);
            let block = ImplBlock {
                subject: ImplSubject::InClass {
                    class: local_id,
                    out_of_body: impl_block.is_out_of_body,
                },
                interface_target: impl_block.target.clone(),
                field_links: impl_block
                    .field_links
                    .iter()
                    .map(InterfaceFieldLink::from_ast)
                    .collect(),
                associated_type_bindings: impl_block.associated_type_bindings.clone(),
                methods: block_method_ids.clone(),
                span: impl_block.span,
                // In-body `implements` blocks don't carry a docstring today —
                // the AST `ImplementsBlock` has no field for one.
                docstring: None,
            };
            self.item_tree.alloc_impl(&iface_head, &c.name, block);
            method_ids.extend(block_method_ids);
        }
        self.class_depth -= 1;

        self.item_tree.set_class_methods(local_id, method_ids);
        self.pop_scope();
    }

    fn lower_implements_for(&mut self, imp: &ast::ImplementsForDef) {
        self.class_depth += 1;
        // For blanket impls (implements<T> I for C<T>), push a class-like scope
        // so TIR can resolve `self` and type variables in method bodies.
        let has_generic_params = !imp.generic_params.is_empty();
        let mut impl_scope: Option<FileScopeId> = None;
        if has_generic_params {
            // Derive a synthetic scope name from the for_target for `self` resolution.
            // Use the for_target's root name (e.g. "Container" from "Container<T>").
            let scope_name = match &imp.for_target.kind {
                baml_compiler2_ast::TypeExprKind::Path { segments, .. } => {
                    segments.first().cloned()
                }
                _ => None,
            };
            self.push_scope(ScopeKind::Class, scope_name, imp.span);
            impl_scope = Some(self.current_scope_id());
        }
        let mut method_ids = Vec::new();
        for method in &imp.methods {
            let fid = self.lower_function(method);
            self.item_tree.record_method_interface_target(
                fid,
                imp.interface_target.clone(),
                imp.associated_type_bindings.clone(),
            );
            method_ids.push(fid);
        }
        if has_generic_params {
            self.pop_scope();
        }
        self.class_depth -= 1;
        // Record this out-of-body impl under a stable `ImplId` in the unified `impls` store.
        let iface_head = impl_head_name(&imp.interface_target);
        let for_head = impl_head_name(&imp.for_target);
        let generics = imp.generic_params.clone();
        let block = ImplBlock {
            subject: ImplSubject::Free {
                for_target: imp.for_target.clone(),
                generics,
            },
            interface_target: imp.interface_target.clone(),
            field_links: imp
                .field_links
                .iter()
                .map(InterfaceFieldLink::from_ast)
                .collect(),
            associated_type_bindings: imp.associated_type_bindings.clone(),
            methods: method_ids,
            span: imp.span,
            docstring: imp.docstring.clone(),
        };
        let impl_id = self.item_tree.alloc_impl(&iface_head, &for_head, block);
        if let Some(scope) = impl_scope {
            self.record_scope_owner(scope, ItemScopeOwner::Impl(impl_id));
        }
    }

    /// Lower an `interface I { ... }` declaration (BEP-044).
    ///
    /// Mirrors `lower_class`: contributes the interface name to the type
    /// namespace, pushes a member scope, walks default-method bodies so their
    /// expressions get HIR scope coverage, and stores everything in the item
    /// tree via `alloc_interface`.
    fn lower_interface(&mut self, i: &ast::InterfaceDef) {
        // Contribute the interface's name to the type namespace first so
        // that within its own scope (and inside the bodies of its default
        // methods) `Self`-style references resolve back to this interface.
        // The interface's `local_id` is allocated only after the default
        // methods so we can record their `FunctionMarker` ids on the
        // interface.
        self.push_scope(ScopeKind::Class, Some(i.name.clone()), i.span);
        let interface_scope = self.current_scope_id();

        // BEP-044: default methods are lowered inside the interface's
        // `Class`-kind scope so the semantic index reports the interface
        // as their enclosing type. Without that, MIR can't resolve `self`
        // back to the interface and field/method dispatch inside a default
        // body falls through to dynamic map lookup.
        self.class_depth += 1;
        let mut method_ids: Vec<_> = i
            .default_methods
            .iter()
            .map(|m| self.lower_function(m))
            .collect();
        self.class_depth -= 1;
        // Required signatures are the SAME item kind, just bodyless
        // (r-a's shape); no body walk, so no scope coverage needed.
        method_ids.extend(
            i.required_methods
                .iter()
                .map(|m| self.item_tree.alloc_function_signature(m)),
        );

        let local_id = self.item_tree.alloc_interface(i, method_ids);
        self.record_scope_owner(interface_scope, ItemScopeOwner::Interface(local_id));
        let loc = InterfaceLoc::new(self.db, self.file, local_id);
        self.type_contributions.push((
            i.name.clone(),
            Contribution {
                name_span: i.name_span,
                definition: Definition::Interface(loc),
            },
        ));

        // Member duplicate detection — interface fields and method names share
        // a single namespace (just like classes).
        let mut seen: FxHashMap<Name, Vec<MemberSite>> = FxHashMap::default();
        for field in &i.fields {
            seen.entry(field.name.clone())
                .or_default()
                .push(MemberSite {
                    range: field.name_span,
                    kind: DefinitionKind::Field,
                });
        }
        for assoc in &i.associated_types {
            seen.entry(assoc.name.clone())
                .or_default()
                .push(MemberSite {
                    range: assoc.name_span,
                    kind: DefinitionKind::AssociatedType,
                });
        }
        for sig in &i.required_methods {
            seen.entry(sig.name.clone()).or_default().push(MemberSite {
                range: sig.name_span,
                kind: DefinitionKind::Method,
            });
        }
        for m in &i.default_methods {
            seen.entry(m.name.clone()).or_default().push(MemberSite {
                range: m.name_span,
                kind: DefinitionKind::Method,
            });
        }
        self.emit_duplicate_diagnostics(seen);

        self.pop_scope();
    }

    fn lower_enum(&mut self, e: &ast::EnumDef) {
        let local_id = self.item_tree.alloc_enum(e);
        let loc = EnumLoc::new(self.db, self.file, local_id);
        self.type_contributions.push((
            e.name.clone(),
            Contribution {
                name_span: e.name_span,
                definition: Definition::Enum(loc),
            },
        ));

        self.push_scope(ScopeKind::Enum, Some(e.name.clone()), e.span);
        let scope = self.current_scope_id();
        self.record_scope_owner(scope, ItemScopeOwner::Enum(local_id));

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
        let scope = self.current_scope_id();
        self.record_scope_owner(scope, ItemScopeOwner::TypeAlias(local_id));
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
        let scope = self.current_scope_id();
        self.record_scope_owner(scope, ItemScopeOwner::Client(local_id));
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
        let scope = self.current_scope_id();
        self.record_scope_owner(scope, ItemScopeOwner::Test(local_id));
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
        let scope = self.current_scope_id();
        self.record_scope_owner(scope, ItemScopeOwner::TemplateString(local_id));
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
        let scope = self.current_scope_id();
        self.record_scope_owner(scope, ItemScopeOwner::RetryPolicy(local_id));
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
        let scope = self.current_scope_id();
        self.record_scope_owner(scope, ItemScopeOwner::Let(local_id));
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
                    let type_expr = &field.type_expr;
                    self.validate_type_expr_phase1(type_expr, type_expr.span, is_builtin_file);
                    self.validate_internal_attributes(
                        &field.attributes,
                        is_builtin_file,
                        "class field",
                        false,
                    );
                    self.validate_schema_attributes(&field.attributes);
                }
                self.validate_alias_collisions(
                    class
                        .fields
                        .iter()
                        .map(|f| (&f.name, f.name_span, f.attributes.as_slice())),
                    SerializedKeyContainer::Class,
                    is_builtin_file,
                );
                for method in &class.methods {
                    self.validate_function_phase1(method, is_builtin_file, "method");
                }
            }
            ast::Item::Enum(enm) => {
                self.validate_schema_attributes(&enm.attributes);
                for variant in &enm.variants {
                    self.validate_schema_attributes(&variant.attributes);
                }
                self.validate_alias_collisions(
                    enm.variants
                        .iter()
                        .map(|v| (&v.name, v.name_span, v.attributes.as_slice())),
                    SerializedKeyContainer::Enum,
                    is_builtin_file,
                );
            }
            ast::Item::TypeAlias(alias) => {
                if let Some(type_expr) = &alias.type_expr {
                    self.validate_type_expr_phase1(type_expr, type_expr.span, is_builtin_file);
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
                self.validate_type_expr_phase1(type_expr, type_expr.span, is_builtin_file);
            }
        }
        if let Some(type_expr) = &function.return_type {
            self.validate_type_expr_phase1(type_expr, type_expr.span, is_builtin_file);
        }
        if let Some(type_expr) = &function.throws {
            self.validate_type_expr_phase1(type_expr, type_expr.span, is_builtin_file);
        }

        if let Some(ast::FunctionBodyDef::Builtin(kind)) = function.body {
            if !is_builtin_file {
                let feature = match kind {
                    ast::BuiltinKind::Vm => "$rust_function",
                    ast::BuiltinKind::Io => "$rust_io_function",
                    ast::BuiltinKind::Intrinsic => "$compiler_intrinsic",
                    ast::BuiltinKind::AwaitAny => "$await_any",
                };
                self.diagnostics.push(Hir2Diagnostic::BuiltinOnlySyntax {
                    feature: feature.to_string(),
                    span: function.span,
                });
                return;
            }

            if let Some(throws) = &function.throws {
                let mut invalid = Vec::new();
                let generic_param_names: Vec<Name> = function
                    .generic_params
                    .iter()
                    .map(|param| param.name.clone())
                    .collect();
                Self::collect_invalid_builtin_throw_types(
                    throws,
                    &generic_param_names,
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
        // E0014: reject the same single-valued schema attribute appearing more
        // than once on one declaration. `@alias`, `@description`, and `@skip`
        // each take effect at most once — for valued attrs the last write
        // silently wins and the earlier ones are dropped (Linear B-648) — so a
        // repeat is always a mistake. Only these known single-valued attributes
        // are checked; repeatable / pass-through attributes (`@stream.*`, etc.)
        // are intentionally left alone. Occurrences are gathered in first-seen
        // order so the emitted diagnostics are deterministic.
        let mut occurrences: Vec<(&str, Vec<TextRange>)> = Vec::new();
        for attr in attributes {
            let name = attr.name.as_str();
            let Some(spec) = baml_base::schema_attribute_spec(name) else {
                continue;
            };
            if spec.repeatable {
                continue;
            }
            if let Some(entry) = occurrences.iter_mut().find(|(n, _)| *n == name) {
                entry.1.push(attr.span);
            } else {
                occurrences.push((name, vec![attr.span]));
            }
        }
        for (name, sites) in occurrences {
            if sites.len() >= 2 {
                self.diagnostics.push(Hir2Diagnostic::DuplicateAttribute {
                    attr_name: name.to_string(),
                    sites,
                });
            }
        }

        for attr in attributes {
            let Some(spec) = baml_base::schema_attribute_spec(attr.name.as_str()) else {
                // Unknown attributes pass through (e.g. `@stream.*`).
                continue;
            };
            match spec.arguments {
                baml_base::SchemaAttributeArguments::String { .. } => {
                    let attr_name = spec.name;
                    if attr.args.len() != 1 {
                        self.diagnostics.push(Hir2Diagnostic::DiagnosticMessage {
                            diagnostic_id: DiagnosticId::InvalidAttributeArg,
                            message: format!("`@{attr_name}` expects exactly one string argument"),
                            span: attr.span,
                        });
                        continue;
                    }
                    let value = attr.args[0].value.as_str();
                    if !is_string_literal(value) && !is_removed_hash_string(value) {
                        self.diagnostics.push(Hir2Diagnostic::DiagnosticMessage {
                            diagnostic_id: DiagnosticId::InvalidAttributeArg,
                            message: format!(
                                "`@{attr_name}` argument must be a string literal, got `{value}`"
                            ),
                            span: attr.args[0].span,
                        });
                    }
                }
                baml_base::SchemaAttributeArguments::None if !attr.args.is_empty() => {
                    self.diagnostics.push(Hir2Diagnostic::DiagnosticMessage {
                        diagnostic_id: DiagnosticId::UnexpectedAttributeArg,
                        message: format!("`@{}` does not take any arguments", spec.name),
                        span: attr.span,
                    });
                }
                baml_base::SchemaAttributeArguments::None => {}
            }
        }
    }

    /// Reject a class or enum whose members don't all serialize to distinct
    /// JSON keys.
    ///
    /// A member's *effective serialized key* is its `@alias` value if it carries
    /// one, otherwise its declared name. When an `@alias` is present the real
    /// member name is never used for matching (see `bex_sap`'s
    /// `AnnotatedField::key_matches`), so two members with the same effective key
    /// are indistinguishable in the serialized schema: `ctx.output_format()`
    /// renders duplicate keys and only the first can ever be satisfied. This
    /// catches both `a @alias("x")` + `b @alias("x")` and a plain member `x`
    /// colliding with another member's `@alias("x")`.
    ///
    /// Applies uniformly to class fields (an unsatisfiable output schema — a
    /// required shadowed field can never be parsed) and enum variants (two
    /// variants rendered under one label — the model's choice can't be resolved
    /// back to a unique variant).
    ///
    /// Members marked `@skip` are excluded from the schema entirely and so
    /// cannot collide. A pure duplicate *member name* (no aliasing involved) is
    /// left to the existing `DuplicateField` / duplicate-variant (E0012) checks
    /// to avoid double-reporting; this rule only fires when at least two
    /// *distinct* member names share a key.
    fn validate_alias_collisions<'a>(
        &mut self,
        members: impl Iterator<Item = (&'a Name, TextRange, &'a [ast::RawAttribute])>,
        container: SerializedKeyContainer,
        is_builtin_file: bool,
    ) {
        // Builtin stdlib declarations carry no `@alias`, and type-level
        // validation already skips them — stay consistent and avoid surprising
        // the stdlib.
        if is_builtin_file {
            return;
        }

        let mut buckets: FxHashMap<String, Vec<(Name, TextRange)>> = FxHashMap::default();
        for (name, name_span, attributes) in members {
            let mut alias: Option<String> = None;
            let mut skip = false;
            for attr in attributes {
                match attr.name.as_str() {
                    "alias" if attr.args.len() == 1 => {
                        // Last `@alias` wins, mirroring emit's `extract_schema_attrs`.
                        if let Some(value) =
                            ast::parse_string_attr_value(attr.args[0].value.as_str())
                        {
                            alias = Some(value);
                        }
                    }
                    "skip" => skip = true,
                    _ => {}
                }
            }
            if skip {
                continue;
            }
            let key = alias.unwrap_or_else(|| name.as_str().to_string());
            buckets
                .entry(key)
                .or_default()
                .push((name.clone(), name_span));
        }

        for (key, members) in buckets {
            // Only a collision between two *distinct* member names is a new
            // error; repeated identical names are already reported by the
            // duplicate-definition checks.
            let distinct = members.iter().any(|(name, _)| name != &members[0].0);
            if members.len() >= 2 && distinct {
                let sites = members.into_iter().map(|(_, span)| span).collect();
                self.diagnostics.push(Hir2Diagnostic::DuplicateFieldAlias {
                    key,
                    sites,
                    container,
                });
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

    fn collect_unknown_type_attrs(
        type_expr: &ast::TypeExpr,
        diagnostics: &mut Vec<Hir2Diagnostic>,
    ) {
        for attr in type_expr.attrs() {
            let name = attr.name.as_str();
            if !ast::is_field_attr(name) && !KNOWN_TYPE_ATTRS.contains(&name) {
                diagnostics.push(Hir2Diagnostic::UnknownTypeAttribute {
                    attr_name: attr.name.clone(),
                    span: attr.span,
                });
            }
        }

        match &type_expr.kind {
            ast::TypeExprKind::Optional { inner, .. } | ast::TypeExprKind::List { inner, .. } => {
                Self::collect_unknown_type_attrs(inner, diagnostics);
            }
            ast::TypeExprKind::Map { key, value, .. } => {
                Self::collect_unknown_type_attrs(key, diagnostics);
                Self::collect_unknown_type_attrs(value, diagnostics);
            }
            ast::TypeExprKind::Union { variants, .. } => {
                for v in variants {
                    Self::collect_unknown_type_attrs(v, diagnostics);
                }
            }
            ast::TypeExprKind::Function {
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
            ast::TypeExprKind::Path {
                generic_args,
                associated_type_bindings,
                ..
            } => {
                for arg in generic_args {
                    Self::collect_unknown_type_attrs(arg, diagnostics);
                }
                for binding in associated_type_bindings {
                    Self::collect_unknown_type_attrs(&binding.ty, diagnostics);
                }
            }
            ast::TypeExprKind::AssociatedTypeProjection {
                base, interface, ..
            } => {
                Self::collect_unknown_type_attrs(base, diagnostics);
                if let Some(interface) = interface {
                    Self::collect_unknown_type_attrs(interface, diagnostics);
                }
            }
            _ => {}
        }
    }

    fn type_expr_contains_rust(type_expr: &ast::TypeExpr) -> bool {
        match &type_expr.kind {
            ast::TypeExprKind::Rust { .. } => true,
            ast::TypeExprKind::Optional { inner, .. } | ast::TypeExprKind::List { inner, .. } => {
                Self::type_expr_contains_rust(inner)
            }
            ast::TypeExprKind::Map { key, value, .. } => {
                Self::type_expr_contains_rust(key) || Self::type_expr_contains_rust(value)
            }
            ast::TypeExprKind::Union { variants, .. } => {
                variants.iter().any(Self::type_expr_contains_rust)
            }
            ast::TypeExprKind::Function {
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
            ast::TypeExprKind::AssociatedTypeProjection {
                base, interface, ..
            } => {
                Self::type_expr_contains_rust(base)
                    || interface
                        .as_ref()
                        .is_some_and(|interface| Self::type_expr_contains_rust(interface))
            }
            ast::TypeExprKind::Path {
                generic_args,
                associated_type_bindings,
                ..
            } => {
                generic_args.iter().any(Self::type_expr_contains_rust)
                    || associated_type_bindings
                        .iter()
                        .any(|binding| Self::type_expr_contains_rust(&binding.ty))
            }
            _ => false,
        }
    }

    fn collect_invalid_builtin_throw_types(
        type_expr: &ast::TypeExpr,
        allowed_generic_params: &[Name],
        invalid: &mut Vec<String>,
    ) {
        match &type_expr.kind {
            ast::TypeExprKind::Path {
                segments,
                generic_args,
                ..
            } => {
                // Allow `baml.errors.*`, `root.errors.*`, `baml.json.*`, and
                // BEP-066's `reflect.errors.*` (fully qualified).
                // `baml.json.JsonParseError` / `baml.json.JsonDecodeError` /
                // `baml.json.JsonSerializationError` are stdlib error types just like
                // `baml.errors.*` ones; they need the same exemption.
                let is_core_builtin_error = segments.len() >= 3
                    && (segments[0].as_str() == "baml" || segments[0].as_str() == "root")
                    && (segments[1].as_str() == "errors" || segments[1].as_str() == "json");
                let is_reflection_error = segments.len() >= 3
                    && segments[0].as_str() == "reflect"
                    && segments[1].as_str() == "errors";
                let is_builtin_error = is_core_builtin_error || is_reflection_error;
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
                // A projection off one of the function's own generic params —
                // e.g. `T.CompareError` for `<T extends Comparable>`, which parses
                // as a dotted path at this phase. The concrete error is the
                // implementor's associated type, resolved at the call site; the
                // host fn just propagates whatever the dispatched method throws
                // (the declared `throws` is erased for builtins). Lets
                // `_compare_shim` declare `throws T.CompareError` instead of an
                // unconstrained error param that call sites cannot pin.
                let is_generic_param_projection = segments.len() >= 2
                    && generic_args.is_empty()
                    && allowed_generic_params
                        .iter()
                        .any(|name| name == &segments[0]);
                if !is_builtin_error
                    && !is_builtin_class_ref
                    && !is_allowed_generic
                    && !is_generic_param_projection
                {
                    invalid.push(Self::render_type_expr(type_expr));
                }
            }
            ast::TypeExprKind::Union { variants, .. } => {
                for ty in variants {
                    Self::collect_invalid_builtin_throw_types(ty, allowed_generic_params, invalid);
                }
            }
            // A projection off one of the function's own generic params — e.g.
            // `T.CompareError` for `<T extends Comparable>`. The concrete error is
            // the implementor's associated type, resolved at the call site; the
            // host fn just propagates whatever the dispatched method throws (the
            // declared `throws` is erased for builtins), so this is sound. Lets
            // `_compare_shim` declare `throws T.CompareError` rather than an
            // unconstrained error param that call sites cannot pin.
            ast::TypeExprKind::AssociatedTypeProjection { base, .. }
                if matches!(
                    &base.kind,
                    ast::TypeExprKind::Path { segments, generic_args, .. }
                        if generic_args.is_empty()
                            && segments.len() == 1
                            && allowed_generic_params.iter().any(|name| name == &segments[0])
                ) => {}
            // `throws never` and `throws unknown` are the two explicit effect
            // bounds and are both valid for host-bound functions. The latter
            // is needed by continuations that execute user bytecode.
            ast::TypeExprKind::Never { .. } | ast::TypeExprKind::Unknown { .. } => {}
            _ => invalid.push(Self::render_type_expr(type_expr)),
        }
    }

    fn render_type_expr(type_expr: &ast::TypeExpr) -> String {
        match &type_expr.kind {
            ast::TypeExprKind::Unreflect { .. } => "unreflect(…)".to_string(),
            ast::TypeExprKind::Path { segments, .. } => segments
                .iter()
                .map(Name::as_str)
                .collect::<Vec<_>>()
                .join("."),
            ast::TypeExprKind::AssociatedTypeProjection { .. } => type_expr.to_string(),
            ast::TypeExprKind::Int { .. } => "int".to_string(),
            ast::TypeExprKind::Bigint { .. } => "bigint".to_string(),
            ast::TypeExprKind::Float { .. } => "float".to_string(),
            ast::TypeExprKind::String { .. } => "string".to_string(),
            ast::TypeExprKind::Bool { .. } => "bool".to_string(),
            ast::TypeExprKind::Null { .. } => "null".to_string(),
            ast::TypeExprKind::Never { .. } => "never".to_string(),
            ast::TypeExprKind::Void { .. } => "void".to_string(),
            ast::TypeExprKind::Uint8Array { .. } => "uint8array".to_string(),
            ast::TypeExprKind::Media { kind, .. } => kind.to_string(),
            ast::TypeExprKind::Optional { inner, .. } => {
                format!("{}?", Self::render_type_expr(inner))
            }
            ast::TypeExprKind::List { inner, .. } => format!("{}[]", Self::render_type_expr(inner)),
            ast::TypeExprKind::Map { key, value, .. } => format!(
                "map<{}, {}>",
                Self::render_type_expr(key),
                Self::render_type_expr(value)
            ),
            ast::TypeExprKind::Union { variants, .. } => variants
                .iter()
                .map(Self::render_type_expr)
                .collect::<Vec<_>>()
                .join(" | "),
            ast::TypeExprKind::Literal { value, .. } => value.to_string(),
            ast::TypeExprKind::Function {
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
            ast::TypeExprKind::Unknown { .. } => "unknown".to_string(),
            ast::TypeExprKind::Type { .. } => "reflect.Type".to_string(),
            ast::TypeExprKind::Rust { .. } => "$rust_type".to_string(),
            ast::TypeExprKind::Error { .. } => "<error>".to_string(),
            ast::TypeExprKind::Missing { .. } => "<unknown>".to_string(),
            ast::TypeExprKind::Infer { .. } => "_".to_string(),
        }
    }
}

/// Check if an attribute argument value is a valid quoted string literal.
///
/// Accepts double-quoted (`"text"`) and single-quoted (`'text'`) strings.
fn is_string_literal(value: &str) -> bool {
    // Double-quoted
    if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
        return true;
    }
    // Single-quoted
    if value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2 {
        return true;
    }
    false
}

fn is_removed_hash_string(value: &str) -> bool {
    let hashes = value.bytes().take_while(|&b| b == b'#').count();
    if hashes == 0 || value.len() < hashes * 2 + 2 {
        return false;
    }
    let rest = &value[hashes..];
    let closing = format!("\"{}", &value[..hashes]);
    rest.starts_with('"') && rest.ends_with(&closing)
}
