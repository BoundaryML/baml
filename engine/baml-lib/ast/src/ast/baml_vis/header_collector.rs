//! Header collection for Markdown-style section annotations.
//!
//! Overview
//! - Builds a `HeaderIndex` from the AST, preserving source order per scope and
//!   computing parent/child relationships implied by Markdown header levels.
//!
//! AST support for header annotations
//! - Expression statements: `Stmt::Expression` now carries an `ExprStmt` which includes
//!   `annotations: Vec<Arc<Header>>` and a `span`. This lets free-standing headers that
//!   immediately precede an expression statement (e.g., "### Before If") exist as
//!   first-class, ordered items in the surrounding block scope.
//! - Final expression headers: `ExpressionBlock` retains `expr_headers` for headers that
//!   immediately precede the optional trailing expression in a block.

use std::{collections::HashMap, sync::Arc};

use indexmap::IndexMap;
use internal_baml_diagnostics::Span;

use crate::ast::{
    traits::{WithIdentifier, WithName},
    Ast, ClassConstructorField, Expression, ExpressionBlock, Field, Header, Stmt, Top,
};

/// Alias for external header identifiers for public consumption
type HeaderId = String;

/// Dense internal header id for compact storage in maps/edges
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Hid(pub u32);

/// A simple numeric identifier for a logical header scope (any block: function, for-loop body, expr block, etc.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScopeId(pub u32);

/// Classification of what kind of statement a header labels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HeaderLabelKind {
    None,
    Expression,
    If,
    For,
    Function,
}

/// Minimal header representation for rendering and navigation
#[derive(Debug, Clone)]
pub struct RenderableHeader {
    pub hid: Hid,
    pub id: String,
    pub title: Arc<str>,
    /// Normalized within its scope so the first header is level 1
    pub level: u8,
    pub span: Span,
    pub scope: ScopeId,
    /// Markdown parent within the same scope
    pub parent_id: Option<HeaderId>,
    /// What kind of statement this header labels
    pub label_kind: HeaderLabelKind,
}

/// Collected index of headers with simple querying APIs
#[derive(Debug, Default, Clone)]
pub struct HeaderIndex {
    /// All headers in source order per scope, flattened
    pub headers: Vec<RenderableHeader>,

    /// Header indexes in source order per scope.
    /// Uses [`IndexMap`] instead of BamlMap to always guarantee iteration order = insertion order
    /// = source order.
    /// See [`graph`](crate::ast::baml_vis::graph) module, where we add
    by_scope: IndexMap<ScopeId, Vec<usize>>,
    /// Mapping of internal Hid -> names of functions called by the labeled expression
    pub header_calls: HashMap<Hid, Vec<String>>, // hid -> [callee_name]
    hid_to_idx: Vec<usize>,
    /// Internal edges and children keyed by Hid
    nested_edges_hid: Vec<(Hid, Hid)>,
}

impl HeaderIndex {
    /// Iterate headers in a scope without allocation
    pub fn headers_in_scope_iter(
        &self,
        scope: ScopeId,
    ) -> impl DoubleEndedIterator<Item = &RenderableHeader> {
        self.by_scope
            .get(&scope)
            .into_iter()
            .flat_map(|idxs| idxs.iter().map(|i| &self.headers[*i]))
    }

    /// Iterates all scopes. Order of iteration is guaranteed to be
    /// consistent with source order.
    pub fn scopes<'iter>(&'iter self) -> impl Iterator<Item = ScopeId> + 'iter {
        self.by_scope.keys().copied()
    }

    /// O(1) access to a header by its Hid via internal index
    pub fn get_by_hid(&self, hid: Hid) -> Option<&RenderableHeader> {
        let idx = *self.hid_to_idx.get(hid.0 as usize)?;
        if idx == usize::MAX {
            return None;
        }
        self.headers.get(idx)
    }

    /// Iterate nested edges as Hid pairs
    pub fn nested_edges_hid_iter(&self) -> impl Iterator<Item = &(Hid, Hid)> {
        self.nested_edges_hid.iter()
    }
}

/// Internal collector to walk AST and build a HeaderIndex
#[derive(Debug, Default)]
pub struct HeaderCollector {
    scope_counter: u32,
    scope_stack: Vec<ScopeId>,
    /// Raw headers by scope before normalization and in-scope parenting.
    /// [`IndexMap`] to maintain source order.
    raw_by_scope: IndexMap<ScopeId, Vec<RawHeader>>, // source order
    // Accumulated nested edges (Hid -> Hid)
    nested_edges_hid: Vec<(Hid, Hid)>,
    // Mapping during collection: header (by Hid) -> function names called by the labeled expression
    header_fn_calls: HashMap<Hid, Vec<String>>,
    next_hid: u32,
    // Track nearest header so far per active scope for fast parent lookup
    last_hdr_stack: Vec<Option<Hid>>, // parallel to scope_stack
    /// Optional top-level function name filter.
    function_filter: Option<String>,
}

#[derive(Debug, Clone)]
struct RawHeader {
    hid: Hid,
    id: String,
    title: Arc<str>,
    original_level: u8,
    span: Span,
    label_kind: HeaderLabelKind,
}

impl HeaderCollector {
    pub fn collect(ast: &Ast, function_filter: Option<&str>) -> HeaderIndex {
        let mut c = Self {
            function_filter: function_filter.map(|s| s.to_string()),
            ..Self::default()
        };
        c.visit_ast(ast);
        c.build_index()
    }

    fn push_scope(&mut self) -> ScopeId {
        self.scope_counter += 1;
        let id = ScopeId(self.scope_counter);
        self.scope_stack.push(id);
        self.last_hdr_stack.push(None);
        id
    }

    fn pop_scope(&mut self) {
        self.scope_stack.pop();
        self.last_hdr_stack.pop();
    }

    fn current_scope(&self) -> Option<ScopeId> {
        self.scope_stack.last().copied()
    }

    /// Returns the last header Hid in the nearest ancestor scope that has at least one header.
    fn nearest_ancestor_last_hid(&self) -> Option<Hid> {
        if self.last_hdr_stack.len() < 2 {
            return None;
        }
        for depth in (0..self.last_hdr_stack.len() - 1).rev() {
            if let Some(hid) = self.last_hdr_stack[depth] {
                return Some(hid);
            }
        }
        None
    }

    fn add_header(
        &mut self,
        title: impl Into<Arc<str>>,
        level: u8,
        span: Span,
        label_kind: HeaderLabelKind,
    ) -> Option<HeaderId> {
        if let Some(scope) = self.current_scope() {
            let entry = self.raw_by_scope.entry(scope).or_default();
            let title: Arc<str> = title.into();
            let id: HeaderId = format!(
                "{}:{}-{}:{}:{}",
                &*title,
                span.file.path(),
                span.start,
                span.end,
                scope.0
            );
            let is_first_in_scope = entry.is_empty();
            // Allocate internal id
            let hid = {
                let hid = Hid(self.next_hid);
                self.next_hid += 1;
                hid
            };
            entry.push(RawHeader {
                hid,
                id: id.clone(),
                title: title.clone(),
                original_level: level,
                span,
                label_kind,
            });

            // If first header in this scope, connect to nearest ancestor header (if any)
            if is_first_in_scope {
                if let Some(parent_hid) = self.nearest_ancestor_last_hid() {
                    self.nested_edges_hid.push((parent_hid, hid));
                }
            }

            // Update current scope's last header
            if let Some(last_slot) = self.last_hdr_stack.last_mut() {
                *last_slot = Some(hid);
            }

            // Return the id that was just created
            return Some(id);
        }
        None
    }

    /// Add a set of headers (annotations) into the current scope with a fixed
    /// label kind. Returns the created header HIDs in source order.
    fn add_headers_for_annotations(
        &mut self,
        headers: &[Arc<Header>],
        label_kind: HeaderLabelKind,
    ) -> Vec<Hid> {
        let mut header_hids: Vec<Hid> = Vec::new();
        if headers.is_empty() {
            return header_hids;
        }
        // Only the deepest-level header inherits the expression's label kind;
        // outer headers act as structural containers and keep a neutral label.
        let deepest_level = headers.iter().map(|h| h.level).max().unwrap_or(1);
        for h in headers {
            let effective_label = if h.level == deepest_level {
                label_kind
            } else {
                HeaderLabelKind::None
            };
            let hid = self.add_header(h.title.clone(), h.level, h.span.clone(), effective_label);
            if hid.is_some() {
                if let Some(scope) = self.current_scope() {
                    if let Some(last) = self.raw_by_scope.get(&scope).and_then(|v| v.last()) {
                        header_hids.push(last.hid);
                    }
                }
            }
        }
        header_hids
    }

    /// Determine how to label headers for a given expression.
    fn label_kind_for_expr(expr: &Expression) -> HeaderLabelKind {
        match expr {
            Expression::If(_, _, _, _) => HeaderLabelKind::If,
            _ => HeaderLabelKind::Expression,
        }
    }

    /// Attribute top-level call names in `expr` to all `header_ids`.
    fn attribute_calls_to_headers(&mut self, header_hids: &[Hid], expr: &Expression) {
        let top_calls = collect_top_level_calls(expr);
        if top_calls.is_empty() {
            return;
        }
        for hid in header_hids {
            self.header_fn_calls
                .entry(*hid)
                .or_default()
                .extend(top_calls.iter().cloned());
        }
    }

    fn visit_ast(&mut self, ast: &Ast) {
        for top in &ast.tops {
            if self.should_visit_top(top) {
                self.visit_top(top);
            }
        }
    }

    fn should_visit_top(&self, top: &Top) -> bool {
        let Some(filter) = self.function_filter.as_deref() else {
            return true;
        };

        match top {
            Top::Function(block)
            | Top::Client(block)
            | Top::Generator(block)
            | Top::TestCase(block)
            | Top::RetryPolicy(block) => block.identifier().name() == filter,
            Top::ExprFn(expr_fn) => expr_fn.name.name() == filter,
            _ => false,
        }
    }

    fn visit_top(&mut self, top: &Top) {
        match top {
            Top::Function(block)
            | Top::Client(block)
            | Top::Generator(block)
            | Top::TestCase(block)
            | Top::RetryPolicy(block) => {
                let _ = self.push_scope();
                // Block-level headers label this scope
                let _ =
                    self.add_headers_for_annotations(&block.annotations, HeaderLabelKind::Function);
                for field in &block.fields {
                    self.visit_field_expression(field);
                }
                self.pop_scope();
            }
            Top::ExprFn(expr_fn) => {
                let _ = self.push_scope();
                let _ = self
                    .add_headers_for_annotations(&expr_fn.annotations, HeaderLabelKind::Function);
                // no synthetic defaults
                self.visit_expression_block(&expr_fn.body);
                self.pop_scope();
            }
            _ => {}
        }
    }

    fn visit_field_expression(&mut self, field: &Field<Expression>) {
        if let Some(expr) = &field.expr {
            self.visit_expression(expr);
        }
    }

    fn visit_expression(&mut self, expr: &Expression) {
        match expr {
            Expression::ExprBlock(block, _block_span) => {
                self.visit_expression_block(block);
            }
            Expression::If(_cond, then_expr, else_expr, _span) => {
                self.visit_expression(then_expr);
                if let Some(else_expr) = else_expr {
                    self.visit_expression(else_expr);
                }
            }
            Expression::Lambda(_args, body, _) => {
                self.visit_expression_block(body);
            }
            Expression::Array(exprs, _) => {
                for e in exprs {
                    self.visit_expression(e);
                }
            }
            Expression::Map(map, _) => {
                for (k, v) in map {
                    self.visit_expression(k);
                    self.visit_expression(v);
                }
            }
            Expression::ClassConstructor(cons, _) => {
                for f in &cons.fields {
                    match f {
                        ClassConstructorField::Named(_, e) => self.visit_expression(e),
                        ClassConstructorField::Spread(e) => self.visit_expression(e),
                    }
                }
            }
            Expression::UnaryOperation { expr, .. } => self.visit_expression(expr),
            Expression::BoolValue(_, _) => {}
            Expression::NumericValue(_, _) => {}
            Expression::Identifier(_) => {}
            Expression::StringValue(_, _) => {}
            Expression::RawStringValue(_) => {}
            Expression::JinjaExpressionValue(_, _) => {}
            Expression::App(app) => {
                for arg in &app.args {
                    self.visit_expression(arg);
                }
            }
            Expression::ArrayAccess(expression, index, _) => {
                self.visit_expression(expression);
                self.visit_expression(index);
            }
            Expression::FieldAccess(expression, _, _) => {
                self.visit_expression(expression);
            }
            Expression::MethodCall { receiver, args, .. } => {
                self.visit_expression(receiver);
                for arg in args {
                    self.visit_expression(arg);
                }
            }
            Expression::BinaryOperation { left, right, .. } => {
                self.visit_expression(left);
                self.visit_expression(right);
            }
            Expression::Paren(expression, _) => self.visit_expression(expression),
        }
    }

    fn collect_statement(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let(let_stmt) => {
                let label_kind = Self::label_kind_for_expr(&let_stmt.expr);
                let stmt_header_ids =
                    self.add_headers_for_annotations(&let_stmt.annotations, label_kind);
                self.attribute_calls_to_headers(&stmt_header_ids, &let_stmt.expr);
                self.visit_expression(&let_stmt.expr);
            }
            Stmt::ForLoop(for_stmt) => {
                let _ =
                    self.add_headers_for_annotations(&for_stmt.annotations, HeaderLabelKind::For);
                self.visit_expression(&for_stmt.iterator);
                self.visit_expression_block(&for_stmt.body);
            }
            Stmt::Expression(es) => {
                let kind = Self::label_kind_for_expr(&es.expr);
                let hids = self.add_headers_for_annotations(&es.annotations, kind);
                self.attribute_calls_to_headers(&hids, &es.expr);
                self.visit_expression(&es.expr);
            }
            Stmt::Semicolon(es) => {
                let kind = Self::label_kind_for_expr(&es.expr);
                let hids = self.add_headers_for_annotations(&es.annotations, kind);
                self.attribute_calls_to_headers(&hids, &es.expr);
                self.visit_expression(&es.expr);
            }
            Stmt::Assign(assign_stmt) => {
                let hids = self.add_headers_for_annotations(
                    &assign_stmt.annotations,
                    HeaderLabelKind::Expression,
                );
                self.attribute_calls_to_headers(&hids, &assign_stmt.expr);
                self.visit_expression(&assign_stmt.expr);
            }
            Stmt::AssignOp(assign_op_stmt) => {
                let hids = self.add_headers_for_annotations(
                    &assign_op_stmt.annotations,
                    HeaderLabelKind::Expression,
                );
                self.attribute_calls_to_headers(&hids, &assign_op_stmt.expr);
                self.visit_expression(&assign_op_stmt.expr);
            }
            Stmt::CForLoop(c_for_stmt) => {
                let _ =
                    self.add_headers_for_annotations(&c_for_stmt.annotations, HeaderLabelKind::For);

                if let Some(init_stmt) = &c_for_stmt.init_stmt {
                    self.collect_statement(init_stmt.as_ref());
                }

                if let Some(condition) = &c_for_stmt.condition {
                    self.visit_expression(condition);
                }

                if let Some(after_stmt) = &c_for_stmt.after_stmt {
                    self.collect_statement(after_stmt.as_ref());
                }

                self.visit_expression_block(&c_for_stmt.body);
            }
            Stmt::WhileLoop(while_stmt) => {
                let _ = self.add_headers_for_annotations(
                    &while_stmt.annotations,
                    HeaderLabelKind::Expression,
                );
                self.visit_expression(&while_stmt.condition);
                self.visit_expression_block(&while_stmt.body);
            }
            Stmt::Break(break_stmt) => {
                let _ = self.add_headers_for_annotations(
                    &break_stmt.annotations,
                    HeaderLabelKind::Expression,
                );
            }
            Stmt::Continue(continue_stmt) => {
                let _ = self.add_headers_for_annotations(
                    &continue_stmt.annotations,
                    HeaderLabelKind::Expression,
                );
            }
            Stmt::Return(return_stmt) => {
                let hids = self.add_headers_for_annotations(
                    &return_stmt.annotations,
                    HeaderLabelKind::Expression,
                );
                self.attribute_calls_to_headers(&hids, &return_stmt.value);
                self.visit_expression(&return_stmt.value);
            }
            Stmt::Assert(assert_stmt) => {
                let hids = self.add_headers_for_annotations(
                    &assert_stmt.annotations,
                    HeaderLabelKind::Expression,
                );
                self.attribute_calls_to_headers(&hids, &assert_stmt.value);
                self.visit_expression(&assert_stmt.value);
            }
            Stmt::WatchOptions(options_stmt) => {
                let hids = self.add_headers_for_annotations(
                    &options_stmt.annotations,
                    HeaderLabelKind::Expression,
                );
                self.attribute_calls_to_headers(&hids, &options_stmt.options_expr);
                self.visit_expression(&options_stmt.options_expr);
            }
            Stmt::WatchNotify(notify_stmt) => {
                let _ = self.add_headers_for_annotations(
                    &notify_stmt.annotations,
                    HeaderLabelKind::Expression,
                );
            }
        }
    }

    fn visit_expression_block(&mut self, block: &ExpressionBlock) {
        let _scope_id = self.push_scope();

        // Visit statements first (preserve source order for MD parenting)
        for stmt in &block.stmts {
            self.collect_statement(stmt);
        }

        // Headers that apply to the final expression belong to this scope and come last
        let label_kind = block
            .expr
            .as_deref()
            .map(Self::label_kind_for_expr)
            .unwrap_or(HeaderLabelKind::Expression);
        let expr_header_ids = self.add_headers_for_annotations(&block.expr_headers, label_kind);

        // Final expr
        if let Some(expr) = &block.expr {
            self.visit_expression(expr);
        }

        // Attribute top-level calls of the final expression to its headers
        if let Some(expr) = &block.expr {
            self.attribute_calls_to_headers(&expr_header_ids, expr);
        }

        self.pop_scope();
    }

    fn build_index(self) -> HeaderIndex {
        let mut index = HeaderIndex {
            headers: Vec::new(),
            by_scope: IndexMap::new(),
            header_calls: HashMap::new(),
            hid_to_idx: Vec::new(),
            nested_edges_hid: Vec::new(),
        };

        // Do not inject implicit headers; only render actual source headers.

        // Build normalized headers and parent relationships per scope
        for (scope, raw_list) in self.raw_by_scope {
            if raw_list.is_empty() {
                continue;
            }
            let min_level = raw_list.iter().map(|r| r.original_level).min().unwrap_or(1);

            // Stack of (header_id, level)
            let mut stack: Vec<(String, u8)> = Vec::new();

            for raw in raw_list {
                let norm_level = raw
                    .original_level
                    .saturating_sub(min_level)
                    .saturating_add(1);
                let id = raw.id.clone();

                // Find markdown parent within scope
                let parent_id = loop {
                    if let Some((parent, plevel)) = stack.last() {
                        if *plevel < norm_level {
                            break Some(parent.clone());
                        } else {
                            stack.pop();
                        }
                    } else {
                        break None;
                    }
                };

                let header = RenderableHeader {
                    hid: raw.hid,
                    id: id.clone(),
                    title: raw.title.clone(),
                    level: norm_level,
                    span: raw.span,
                    scope,
                    parent_id,
                    label_kind: raw.label_kind,
                };

                let idx = index.headers.len();
                // populate internal id maps in same order as raw allocation
                let hid = raw.hid;
                let hid_usize = hid.0 as usize;
                if index.hid_to_idx.len() <= hid_usize {
                    index.hid_to_idx.resize(hid_usize + 1, usize::MAX);
                }
                index.hid_to_idx[hid_usize] = idx;
                index.headers.push(header);
                index.by_scope.entry(scope).or_default().push(idx);

                stack.push((id, norm_level));
            }
        }

        // All HIDs should be populated
        debug_assert!(index.hid_to_idx.iter().all(|&i| i != usize::MAX));

        // Carry over nested edges collected during traversal (Hid)
        index.nested_edges_hid = self.nested_edges_hid;
        // Expose header -> call mapping
        index.header_calls = self.header_fn_calls;
        index
    }
}

/// Collect the top-level function application names for the given expression.
/// Only captures the outermost call(s) that structurally represent the expression,
/// ignoring nested calls within arguments or sub-expressions.
fn collect_top_level_calls(expr: &Expression) -> Vec<String> {
    use crate::ast::WithName;
    match expr {
        // If the expression is a block, the top-level expression is inside it
        Expression::ExprBlock(block, _span) => {
            if let Some(inner) = &block.expr {
                collect_top_level_calls(inner)
            } else {
                Vec::new()
            }
        }
        // For an if-expression, the top-level construct is branching, not a direct call
        Expression::If(_cond, _then_expr, _else_expr, _span) => Vec::new(),
        // For a direct function application, capture its name
        Expression::App(app) => vec![app.name.name().to_string()],
        // For other expressions (values, identifiers, constructors, etc.), no top-level calls
        _ => Vec::new(),
    }
}
