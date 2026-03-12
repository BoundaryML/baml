//! `SemanticIndexBuilder` — walks `Vec<ast::Item>` and builds `FileSemanticIndex`.
//!
//! Allocates scopes in DFS pre-order with `TextRange`, builds the `ItemTree`,
//! collects `FileSymbolContributions`, and records expression→scope mappings
//! with per-scope `ScopeBindings`.
//!
//! Scope chain: Project → Package → Namespace* → File → Items.

use std::sync::Arc;

use baml_base::{Name, SourceFile};
use baml_compiler2_ast as ast;
use rustc_hash::FxHashMap;
use text_size::TextRange;

use crate::{
    contributions::{Contribution, Definition, DefinitionKind, FileSymbolContributions},
    diagnostic::{Hir2Diagnostic, MemberSite},
    file_package::file_package,
    ids::{FunctionMarker, LocalItemId},
    item_tree::ItemTree,
    loc::{
        ClassLoc, ClientLoc, EnumLoc, FunctionLoc, GeneratorLoc, RetryPolicyLoc, TemplateStringLoc,
        TestLoc, TypeAliasLoc,
    },
    scope::{FileScopeId, Scope, ScopeId, ScopeKind},
    semantic_index::{DefinitionSite, FileSemanticIndex, ScopeBindings, SemanticIndexExtra},
};

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

    item_tree: ItemTree,
    type_contributions: Vec<(Name, Contribution<'db>)>,
    value_contributions: Vec<(Name, Contribution<'db>)>,
    diagnostics: Vec<Hir2Diagnostic>,
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
            item_tree: ItemTree::new(),
            type_contributions: Vec::new(),
            value_contributions: Vec::new(),
            diagnostics: Vec::new(),
        }
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

        // Pop: File, Namespace*, Package, Project
        self.pop_scope(); // File
        for _ in &pkg_info.namespace_path {
            self.pop_scope(); // Namespace*
        }
        self.pop_scope(); // Package
        self.pop_scope(); // Project

        // Sort expr_scopes for binary search
        self.expr_scopes.sort_by_key(|(id, _)| *id);

        // Pre-intern ScopeIds for each FileScopeId
        let scope_ids: Vec<ScopeId<'db>> = (0..self.scopes.len())
            .map(|i| {
                #[allow(clippy::cast_possible_truncation)]
                ScopeId::new(self.db, self.file, FileScopeId::new(i as u32))
            })
            .collect();

        let extra = if self.diagnostics.is_empty() {
            None
        } else {
            Some(Box::new(SemanticIndexExtra {
                diagnostics: self.diagnostics,
            }))
        };

        FileSemanticIndex {
            scopes: self.scopes,
            expr_scopes: self.expr_scopes,
            scope_bindings: self.scope_bindings,
            scope_ids,
            item_tree: Arc::new(self.item_tree),
            symbol_contributions: Arc::new(FileSymbolContributions {
                types: self.type_contributions,
                values: self.value_contributions,
            }),
            extra,
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

    /// Walk an `ExprBody` arena, recording each expression in the current scope.
    /// Block expressions with let-bindings push a Block scope.
    fn walk_expr_body(&mut self, body: &ast::ExprBody, source_map: &ast::AstSourceMap) {
        for (expr_id, expr) in body.exprs.iter() {
            self.record_expr_scope(expr_id);
            let _ = expr;
        }
        // Collect let-bindings, detecting duplicates within the scope.
        let mut seen: FxHashMap<Name, Vec<MemberSite>> = FxHashMap::default();
        for (stmt_id, stmt) in body.stmts.iter() {
            if let ast::Stmt::Let { pattern, .. } = stmt {
                let scope_id = self.current_scope_id();
                if let ast::Pattern::Binding(name) = &body.patterns[*pattern] {
                    let name_range = source_map.pattern_span(*pattern);

                    seen.entry(name.clone()).or_default().push(MemberSite {
                        range: name_range,
                        kind: DefinitionKind::Binding,
                    });

                    self.scope_bindings[scope_id.index() as usize]
                        .bindings
                        .push((name.clone(), DefinitionSite::Statement(stmt_id), name_range));
                }
            }
        }

        self.emit_duplicate_diagnostics(seen);
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

        for (idx, param) in f.params.iter().enumerate() {
            self.scope_bindings[scope_id.index() as usize]
                .params
                .push((param.name.clone(), idx));
        }

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
        let local_id = self.item_tree.alloc_client(&c.name);
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
        let local_id = self.item_tree.alloc_test(&t.name);
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
        let local_id = self.item_tree.alloc_generator(&g.name);
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
        let local_id = self.item_tree.alloc_template_string(&ts.name);
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
        let local_id = self.item_tree.alloc_retry_policy(&rp.name);
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
}
