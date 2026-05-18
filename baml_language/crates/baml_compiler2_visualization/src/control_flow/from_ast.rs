//! Build a control flow visualization graph directly from a compiler2 AST.
//!
//! This is the AST-based counterpart of `super::build_control_flow_graph` which
//! walks VIR expressions. The advantage of building from the AST is resilience:
//! the compiler2 AST uses `Expr::Missing` / `Stmt::Missing` sentinels for error
//! recovery, so the CFG survives parse and type errors.

use std::fmt::Write;

use baml_compiler2_ast as ast;

use super::{
    ControlFlowGraph, CounterKind, Frame, FrameEntry, GraphAccumulator, Node, NodeId, NodeType,
    PathSegment, encode_segments, slugify,
};

/// Tag bit used to distinguish statement-based source IDs from expression-based ones.
/// The cursor context code in `baml_project::db` uses the same tag when searching
/// `stmt_spans` so both sides agree on the encoding.
pub const STMT_SOURCE_EXPR_TAG: u32 = 1 << 31;

/// Convert a `StmtId` to a `source_expr` value with the high bit set to
/// distinguish it from expression-based IDs.
fn stmt_id_to_source_expr(id: ast::StmtId) -> u32 {
    STMT_SOURCE_EXPR_TAG | id.into_raw().into_u32()
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Build a control flow visualization graph from a compiler2 AST expression body.
pub fn build_control_flow_graph_from_ast(
    function_name: &str,
    body: &ast::ExprBody,
) -> ControlFlowGraph {
    let Some(root_expr) = body.root_expr else {
        // No root expression — return a graph with just the root node.
        let mut graph = GraphAccumulator::default();
        let root_id = graph.allocate_id();
        let root_segment = PathSegment::FunctionRoot { ordinal: 0 };
        let root_key = encode_segments(function_name, std::slice::from_ref(&root_segment));
        graph.add_node(Node::root(root_id, root_key, function_name));
        return graph.finish();
    };

    let mut builder = AstGraphBuilder::new(function_name, body);
    builder.visit_expr(root_expr);
    builder.finish()
}

// ---------------------------------------------------------------------------
// AST graph builder — walks compiler2 AST ExprBody
// ---------------------------------------------------------------------------

struct AstGraphBuilder<'a> {
    body: &'a ast::ExprBody,
    function_name: String,
    graph: GraphAccumulator,
    frames: Vec<Frame>,
}

impl<'a> AstGraphBuilder<'a> {
    fn new(function_name: &str, body: &'a ast::ExprBody) -> Self {
        let mut graph = GraphAccumulator::default();
        let root_id = graph.allocate_id();
        let root_segment = PathSegment::FunctionRoot { ordinal: 0 };
        let root_key = encode_segments(function_name, std::slice::from_ref(&root_segment));
        graph.add_node(Node::root(root_id, root_key, function_name.to_string()));

        Self {
            body,
            function_name: function_name.to_string(),
            graph,
            frames: vec![Frame::new(
                FrameEntry::FunctionRoot,
                root_id,
                Some(root_segment),
            )],
        }
    }

    fn finish(self) -> ControlFlowGraph {
        self.graph.finish()
    }

    // -- Frame helpers (same logic as GraphBuilder) --

    fn current_parent_index(&self) -> usize {
        self.frames
            .len()
            .checked_sub(1)
            .expect("frame stack always contains root")
    }

    fn current_parent_id(&self) -> Option<NodeId> {
        self.frames.last().map(|frame| frame.node_id)
    }

    fn build_log_filter_key(&self, segment: &PathSegment) -> String {
        let mut segments: Vec<PathSegment> = self
            .frames
            .iter()
            .filter_map(|frame| frame.lexical_segment.clone())
            .collect();
        segments.push(segment.clone());
        encode_segments(&self.function_name, &segments)
    }

    fn register_child_with_parent(&mut self, parent_index: usize, node_id: NodeId) {
        let parent_entry = self.frames[parent_index].entry.children_are_linear();
        if !parent_entry {
            return;
        }
        if let Some(prev) = self.frames[parent_index].last_linear_child {
            self.graph.add_edge(prev, node_id);
        }
        self.frames[parent_index].last_linear_child = Some(node_id);
    }

    fn pop_headers_to_level(&mut self, desired_level: u8) {
        while let Some(frame) = self.frames.last() {
            match frame.entry {
                FrameEntry::Header { level } if level > desired_level => {
                    self.frames.pop();
                }
                _ => break,
            }
        }
    }

    fn pop_frames_to(&mut self, len: usize) {
        while self.frames.len() > len {
            self.frames.pop();
        }
    }

    // -- Main dispatch --

    fn visit_expr(&mut self, id: ast::ExprId) {
        let expr = self.body.exprs[id].clone();
        match &expr {
            ast::Expr::Block { stmts, tail_expr } => {
                for stmt_id in stmts {
                    self.visit_stmt(*stmt_id);
                }
                if let Some(tail) = tail_expr {
                    self.visit_expr(*tail);
                }
            }

            ast::Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.visit_if(id, *condition, *then_branch, *else_branch);
            }

            ast::Expr::Match {
                scrutinee, arms, ..
            } => {
                self.visit_match(id, *scrutinee, arms);
            }
            ast::Expr::Catch { base, clauses } => {
                self.visit_expr(*base);
                for clause in clauses {
                    for arm_id in &clause.arms {
                        self.visit_expr(self.body.catch_arms[*arm_id].body);
                    }
                }
            }
            ast::Expr::Throw { value } => {
                self.visit_expr(*value);
            }

            ast::Expr::Call { .. } | ast::Expr::OptionalCall { .. } => {
                // Emit an OtherScope node for function calls so they appear in the graph.
                let label = render_expr_compact_ast(self.body, id);
                self.emit_call_scope(id, &label);
            }

            // All other expressions don't create graph nodes.
            _ => {}
        }
    }

    fn visit_stmt(&mut self, id: ast::StmtId) {
        let stmt = self.body.stmts[id].clone();
        match &stmt {
            ast::Stmt::HeaderComment { name, level } => {
                self.enter_header(name.as_ref(), *level, id);
            }

            ast::Stmt::While {
                condition,
                body,
                origin,
                ..
            } => {
                self.visit_loop(*condition, *body, *origin);
            }

            ast::Stmt::Let {
                initializer: Some(init),
                pattern,
                ..
            } => {
                let init_expr = self.body.exprs[*init].clone();
                let needs_scope =
                    matches!(init_expr, ast::Expr::If { .. } | ast::Expr::Match { .. });
                if needs_scope {
                    // `format_pattern` prefixes top-level `Bind` patterns
                    // with `let ` so they don't collapse with bare path arms;
                    // for non-Bind patterns (`_`, type patterns, class
                    // destructures, …) we still need the keyword to render
                    // the let-statement as a declaration rather than an
                    // assignment.
                    let pat_name = self.format_pattern(*pattern);
                    let label = if pat_name.starts_with("let ") {
                        format!("{pat_name} = ...")
                    } else {
                        format!("let {pat_name} = ...")
                    };
                    self.emit_other_scope(*init, Some(label));
                } else {
                    self.visit_expr(*init);
                }
            }

            ast::Stmt::Let { .. } => {}

            ast::Stmt::Expr(expr_id) => {
                self.visit_expr(*expr_id);
            }
            ast::Stmt::Throw { value } => {
                self.visit_expr(*value);
            }

            ast::Stmt::Return(Some(expr_id)) => {
                let return_expr = self.body.exprs[*expr_id].clone();
                match &return_expr {
                    // Calls already produce their own graph node via visit_expr.
                    ast::Expr::Call { .. } | ast::Expr::OptionalCall { .. } => {
                        self.visit_expr(*expr_id);
                    }
                    // For other return values, emit a leaf node showing the
                    // return path so branching returns are visible in the graph.
                    _ => {
                        let label =
                            format!("return {}", render_expr_compact_ast(self.body, *expr_id));
                        self.emit_return_leaf(*expr_id, &label);
                    }
                }
            }

            ast::Stmt::Assign { value, .. } | ast::Stmt::AssignOp { value, .. } => {
                self.visit_expr(*value);
            }

            // Break, Continue, bare Return, Assert, Missing — no graph nodes.
            _ => {}
        }
    }

    // -- If/else chain flattening --

    fn visit_if(
        &mut self,
        if_expr: ast::ExprId,
        condition: ast::ExprId,
        then_branch: ast::ExprId,
        else_branch: Option<ast::ExprId>,
    ) {
        let parent_depth = self.frames.len();
        let ordinal = {
            let frame = self
                .frames
                .last_mut()
                .expect("frame stack should not be empty");
            frame.next_ordinal(&CounterKind::BranchGroup)
        };
        let label = format!("if ({})", render_expr_compact_ast(self.body, condition));
        let slug = {
            let slug_base = slugify(&label);
            if slug_base.is_empty() {
                format!("if-{ordinal}")
            } else {
                slug_base
            }
        };
        let segment = PathSegment::BranchGroup { slug, ordinal };
        let log_filter_key = self.build_log_filter_key(&segment);
        let node_id = self.graph.allocate_id();
        let parent_id = self.current_parent_id();
        let node = Node::new(
            node_id,
            parent_id,
            log_filter_key,
            label,
            Some(if_expr.into_raw().into_u32()),
            NodeType::BranchGroup,
        );
        self.graph.add_node(node);
        let parent_index = self.current_parent_index();
        self.register_child_with_parent(parent_index, node_id);
        self.frames
            .push(Frame::new(FrameEntry::BranchGroup, node_id, Some(segment)));

        // First arm: "if (condition)"
        let arm_label = format!("if ({})", render_expr_compact_ast(self.body, condition));
        self.visit_branch_arm(arm_label, then_branch);

        // Flatten else-if chains
        let mut current_else = else_branch;
        while let Some(else_id) = current_else {
            let else_expr = self.body.exprs[else_id].clone();
            match else_expr {
                ast::Expr::If {
                    condition: else_cond,
                    then_branch: else_then,
                    else_branch: else_else,
                } => {
                    let arm_label = format!(
                        "else if ({})",
                        render_expr_compact_ast(self.body, else_cond)
                    );
                    self.visit_branch_arm(arm_label, else_then);
                    current_else = else_else;
                }
                _ => {
                    self.visit_branch_arm("else".to_string(), else_id);
                    current_else = None;
                }
            }
        }

        // Synthetic "else" arm if no else branch
        if else_branch.is_none() {
            self.emit_synthetic_branch_arm("else".to_string());
        }

        self.pop_frames_to(parent_depth);
    }

    fn visit_branch_arm(&mut self, label: String, body_expr: ast::ExprId) {
        let parent_depth = self.frames.len();
        let ordinal = {
            let frame = self
                .frames
                .last_mut()
                .expect("branch group frame must exist");
            frame.next_ordinal(&CounterKind::BranchArm)
        };
        let slug_base = slugify(&label);
        let slug = if slug_base.is_empty() {
            format!("branch-arm-{ordinal}")
        } else {
            slug_base
        };
        let segment = PathSegment::BranchArm { slug, ordinal };
        let log_filter_key = self.build_log_filter_key(&segment);
        let node_id = self.graph.allocate_id();
        let parent_id = self.current_parent_id();
        let node = Node::new(
            node_id,
            parent_id,
            log_filter_key,
            label,
            Some(body_expr.into_raw().into_u32()),
            NodeType::BranchArm,
        );
        self.graph.add_node(node);
        let parent_index = self.current_parent_index();
        self.register_child_with_parent(parent_index, node_id);
        self.frames
            .push(Frame::new(FrameEntry::BranchArm, node_id, Some(segment)));
        self.visit_expr(body_expr);
        self.pop_frames_to(parent_depth);
    }

    fn emit_synthetic_branch_arm(&mut self, label: String) {
        let ordinal = {
            let frame = self
                .frames
                .last_mut()
                .expect("branch group frame must exist");
            frame.next_ordinal(&CounterKind::BranchArm)
        };
        let slug_base = slugify(&label);
        let slug = if slug_base.is_empty() {
            format!("branch-arm-{ordinal}")
        } else {
            slug_base
        };
        let segment = PathSegment::BranchArm { slug, ordinal };
        let log_filter_key = self.build_log_filter_key(&segment);
        let node_id = self.graph.allocate_id();
        let parent_id = self.current_parent_id();
        let node = Node::new(
            node_id,
            parent_id,
            log_filter_key,
            label,
            None,
            NodeType::BranchArm,
        );
        self.graph.add_node(node);
        let parent_index = self.current_parent_index();
        self.register_child_with_parent(parent_index, node_id);
    }

    // -- Match expressions --

    fn visit_match(
        &mut self,
        match_expr: ast::ExprId,
        scrutinee: ast::ExprId,
        arms: &[ast::MatchArmId],
    ) {
        let parent_depth = self.frames.len();
        let ordinal = {
            let frame = self
                .frames
                .last_mut()
                .expect("frame stack should not be empty");
            frame.next_ordinal(&CounterKind::BranchGroup)
        };
        let label = format!("match ({})", render_expr_compact_ast(self.body, scrutinee));
        let slug = {
            let slug_base = slugify(&label);
            if slug_base.is_empty() {
                format!("match-{ordinal}")
            } else {
                slug_base
            }
        };
        let segment = PathSegment::BranchGroup { slug, ordinal };
        let log_filter_key = self.build_log_filter_key(&segment);
        let node_id = self.graph.allocate_id();
        let parent_id = self.current_parent_id();
        let node = Node::new(
            node_id,
            parent_id,
            log_filter_key,
            label,
            Some(match_expr.into_raw().into_u32()),
            NodeType::BranchGroup,
        );
        self.graph.add_node(node);
        let parent_index = self.current_parent_index();
        self.register_child_with_parent(parent_index, node_id);
        self.frames
            .push(Frame::new(FrameEntry::BranchGroup, node_id, Some(segment)));

        for arm_id in arms {
            let arm = &self.body.match_arms[*arm_id];
            let arm_label = self.format_pattern(arm.pattern);
            self.visit_branch_arm(arm_label, arm.body);
        }

        self.pop_frames_to(parent_depth);
    }

    // -- While / for loops --

    fn visit_loop(&mut self, condition: ast::ExprId, body: ast::ExprId, origin: ast::LoopOrigin) {
        let parent_depth = self.frames.len();
        let ordinal = {
            let frame = self
                .frames
                .last_mut()
                .expect("frame stack should not be empty");
            frame.next_ordinal(&CounterKind::Loop)
        };
        let keyword = match origin {
            ast::LoopOrigin::While => "while",
            ast::LoopOrigin::For => "for",
        };
        let label = format!(
            "{keyword} ({})",
            render_expr_compact_ast(self.body, condition)
        );
        let slug_base = slugify(&label);
        let slug = if slug_base.is_empty() {
            format!("loop-{ordinal}")
        } else {
            slug_base
        };
        let segment = PathSegment::Loop { slug, ordinal };
        let log_filter_key = self.build_log_filter_key(&segment);
        let node_id = self.graph.allocate_id();
        let parent_id = self.current_parent_id();
        let node = Node::new(
            node_id,
            parent_id,
            log_filter_key,
            label,
            Some(condition.into_raw().into_u32()),
            NodeType::Loop,
        );
        self.graph.add_node(node);
        let parent_index = self.current_parent_index();
        self.register_child_with_parent(parent_index, node_id);
        self.frames
            .push(Frame::new(FrameEntry::Loop, node_id, Some(segment)));
        self.visit_expr(body);
        self.pop_frames_to(parent_depth);
    }

    // -- Headers --

    #[allow(clippy::cast_possible_truncation)]
    fn enter_header(&mut self, title: &str, level: usize, stmt_id: ast::StmtId) {
        let level = (level as u8).max(1);
        self.pop_headers_to_level(level - 1);

        let ordinal = {
            let frame = self
                .frames
                .last_mut()
                .expect("frame stack should not be empty");
            frame.next_ordinal(&CounterKind::Header)
        };

        let mut slug = slugify(title);
        if slug.is_empty() {
            slug = format!("header-{ordinal}");
        }

        let segment = PathSegment::Header { slug, ordinal };
        let log_filter_key = self.build_log_filter_key(&segment);
        let node_id = self.graph.allocate_id();
        let parent_id = self.current_parent_id();
        let source_expr = Some(stmt_id_to_source_expr(stmt_id));
        let node = Node::new(
            node_id,
            parent_id,
            log_filter_key,
            title.to_string(),
            source_expr,
            NodeType::HeaderContextEnter,
        );
        self.graph.add_node(node);

        let parent_index = self.current_parent_index();
        self.register_child_with_parent(parent_index, node_id);
        self.frames.push(Frame::new(
            FrameEntry::Header { level },
            node_id,
            Some(segment),
        ));
    }

    // -- Call scope (leaf node — no recursion into the call's arguments) --

    fn emit_call_scope(&mut self, call_expr: ast::ExprId, label: &str) {
        let callee_name = call_callee_name(self.body, call_expr);
        let ordinal = {
            let frame = self
                .frames
                .last_mut()
                .expect("frame stack should not be empty");
            frame.next_ordinal(&CounterKind::OtherScope)
        };
        let slug_base = slugify(label);
        let slug = if slug_base.is_empty() {
            format!("call-{ordinal}")
        } else {
            slug_base
        };
        let segment = PathSegment::OtherScope { slug, ordinal };
        let log_filter_key = self.build_log_filter_key(&segment);
        let node_id = self.graph.allocate_id();
        let parent_id = self.current_parent_id();
        let node = Node::new(
            node_id,
            parent_id,
            log_filter_key,
            label.to_string(),
            Some(call_expr.into_raw().into_u32()),
            NodeType::OtherScope,
        )
        .with_callee_name(callee_name);
        self.graph.add_node(node);
        let parent_index = self.current_parent_index();
        self.register_child_with_parent(parent_index, node_id);
        // Note: no frame push / recursion — call nodes are leaves.
    }

    // -- Return leaf (leaf node for return statements with non-call values) --

    fn emit_return_leaf(&mut self, return_expr: ast::ExprId, label: &str) {
        let ordinal = {
            let frame = self
                .frames
                .last_mut()
                .expect("frame stack should not be empty");
            frame.next_ordinal(&CounterKind::OtherScope)
        };
        let slug_base = slugify(label);
        let slug = if slug_base.is_empty() {
            format!("return-{ordinal}")
        } else {
            slug_base
        };
        let segment = PathSegment::OtherScope { slug, ordinal };
        let log_filter_key = self.build_log_filter_key(&segment);
        let node_id = self.graph.allocate_id();
        let parent_id = self.current_parent_id();
        let node = Node::new(
            node_id,
            parent_id,
            log_filter_key,
            label.to_string(),
            Some(return_expr.into_raw().into_u32()),
            NodeType::OtherScope,
        );
        self.graph.add_node(node);
        let parent_index = self.current_parent_index();
        self.register_child_with_parent(parent_index, node_id);
    }

    // -- OtherScope --

    fn emit_other_scope(&mut self, inner_expr: ast::ExprId, label: Option<String>) {
        let parent_depth = self.frames.len();
        let ordinal = {
            let frame = self
                .frames
                .last_mut()
                .expect("frame stack should not be empty");
            frame.next_ordinal(&CounterKind::OtherScope)
        };
        let label_ref = label.as_deref().unwrap_or("");
        let slug_base = slugify(label_ref);
        let slug = if slug_base.is_empty() {
            format!("other-scope-{ordinal}")
        } else {
            slug_base
        };
        let segment = PathSegment::OtherScope { slug, ordinal };
        let log_filter_key = self.build_log_filter_key(&segment);
        let node_id = self.graph.allocate_id();
        let parent_id = self.current_parent_id();
        let node_label = label.unwrap_or_default();
        let node = Node::new(
            node_id,
            parent_id,
            log_filter_key,
            node_label,
            Some(inner_expr.into_raw().into_u32()),
            NodeType::OtherScope,
        );
        self.graph.add_node(node);
        let parent_index = self.current_parent_index();
        self.register_child_with_parent(parent_index, node_id);
        self.frames
            .push(Frame::new(FrameEntry::OtherScope, node_id, Some(segment)));
        self.visit_expr(inner_expr);
        self.pop_frames_to(parent_depth);
    }

    // -- Pattern formatting --

    fn format_pattern(&self, pat_id: ast::PatId) -> String {
        let pat = &self.body.patterns[pat_id];
        match pat {
            ast::Pattern::Wildcard => "_".to_string(),
            // Render as `let x` so it doesn't collapse into a path/type label
            // (the new grammar requires the keyword for bindings).
            ast::Pattern::Bind { name, subpat } => match subpat {
                Some(sp) => format!("let {name}: {}", self.format_pattern(*sp)),
                None => format!("let {name}"),
            },
            ast::Pattern::Type(ty) => ty.to_string(),
            ast::Pattern::Class {
                class,
                generic_args,
                fields,
            } => {
                let class_path: Vec<_> = class.iter().map(baml_base::Name::as_str).collect();
                let generic_args = if generic_args.is_empty() {
                    String::new()
                } else {
                    format!(
                        "<{}>",
                        generic_args
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                let field_strs: Vec<_> = fields
                    .iter()
                    .map(|f| format!("{}: {}", f.field, self.format_pattern(f.pat)))
                    .collect();
                format!(
                    "{}{} {{ {} }}",
                    class_path.join("."),
                    generic_args,
                    field_strs.join(", ")
                )
            }
            ast::Pattern::Array {
                prefix,
                rest,
                suffix,
                ascription,
            } => {
                let mut parts: Vec<String> =
                    prefix.iter().map(|p| self.format_pattern(*p)).collect();
                if let Some(rest) = rest {
                    parts.push(match rest.pat {
                        Some(p) => format!("..{}", self.format_pattern(p)),
                        None => "..".to_string(),
                    });
                }
                parts.extend(suffix.iter().map(|p| self.format_pattern(*p)));
                let arr = format!("[{}]", parts.join(", "));
                match ascription {
                    Some(ty) => format!("{arr}: {ty}"),
                    None => arr,
                }
            }
            ast::Pattern::Or(pats) => pats
                .iter()
                .map(|p| self.format_pattern_child(*p, ChildContext::Or))
                .collect::<Vec<_>>()
                .join(" | "),
        }
    }

    /// Format a child pattern, parenthesizing when its variant has lower or
    /// equal precedence than the parent combinator.
    fn format_pattern_child(&self, pat_id: ast::PatId, parent: ChildContext) -> String {
        let s = self.format_pattern(pat_id);
        let needs_parens = matches!(
            (&self.body.patterns[pat_id], parent),
            (ast::Pattern::Or(_), ChildContext::Or)
        );
        if needs_parens { format!("({s})") } else { s }
    }
}

fn call_callee_name(body: &ast::ExprBody, id: ast::ExprId) -> String {
    match &body.exprs[id] {
        ast::Expr::Call { callee, .. } | ast::Expr::OptionalCall { callee, .. } => {
            render_expr_compact_ast(body, *callee)
        }
        _ => render_expr_compact_ast(body, id),
    }
}

#[derive(Clone, Copy)]
enum ChildContext {
    Or,
}

// ---------------------------------------------------------------------------
// Compact expression renderer for AST Expr (for labels)
// ---------------------------------------------------------------------------

fn render_expr_compact_ast(body: &ast::ExprBody, id: ast::ExprId) -> String {
    let expr = &body.exprs[id];
    match expr {
        ast::Expr::Literal(lit) => format_literal_ast(lit),
        ast::Expr::Null => "null".to_string(),
        ast::Expr::Path(segments) => {
            let parts: Vec<_> = segments.iter().map(ToString::to_string).collect();
            parts.join(".")
        }
        ast::Expr::Binary { op, lhs, rhs } => {
            let op_str = match op {
                ast::BinaryOp::Add => "+",
                ast::BinaryOp::Sub => "-",
                ast::BinaryOp::Mul => "*",
                ast::BinaryOp::Div => "/",
                ast::BinaryOp::Mod => "%",
                ast::BinaryOp::Eq => "==",
                ast::BinaryOp::Ne => "!=",
                ast::BinaryOp::Lt => "<",
                ast::BinaryOp::Le => "<=",
                ast::BinaryOp::Gt => ">",
                ast::BinaryOp::Ge => ">=",
                ast::BinaryOp::And => "&&",
                ast::BinaryOp::Or => "||",
                ast::BinaryOp::BitAnd => "&",
                ast::BinaryOp::BitOr => "|",
                ast::BinaryOp::BitXor => "^",
                ast::BinaryOp::Shl => "<<",
                ast::BinaryOp::Shr => ">>",
                ast::BinaryOp::NullCoalesce => "??",
            };
            format!(
                "{} {} {}",
                render_expr_compact_ast(body, *lhs),
                op_str,
                render_expr_compact_ast(body, *rhs)
            )
        }
        ast::Expr::Unary { op, expr } => {
            let op_str = match op {
                ast::UnaryOp::Not => "!",
                ast::UnaryOp::Neg => "-",
            };
            format!("{op_str}{}", render_expr_compact_ast(body, *expr))
        }
        ast::Expr::MemberAccess { base, member } => {
            format!("{}.{member}", render_expr_compact_ast(body, *base))
        }
        ast::Expr::OptionalMemberAccess { base, member } => {
            format!("{}?.{member}", render_expr_compact_ast(body, *base))
        }
        ast::Expr::Index { base, index } => {
            format!(
                "{}[{}]",
                render_expr_compact_ast(body, *base),
                render_expr_compact_ast(body, *index)
            )
        }
        ast::Expr::OptionalIndex { base, index } => {
            format!(
                "{}?.[{}]",
                render_expr_compact_ast(body, *base),
                render_expr_compact_ast(body, *index)
            )
        }
        ast::Expr::Call { callee, args, .. } => {
            let callee_str = render_expr_compact_ast(body, *callee);
            let args_str: Vec<_> = args
                .iter()
                .map(|a| {
                    let expr = render_expr_compact_ast(body, a.expr);
                    match &a.label {
                        Some(label) => format!("{label} = {expr}"),
                        None => expr,
                    }
                })
                .collect();
            format!("{}({})", callee_str, args_str.join(", "))
        }
        ast::Expr::OptionalCall { callee, args } => {
            let callee_str = render_expr_compact_ast(body, *callee);
            let args_str: Vec<_> = args
                .iter()
                .map(|a| {
                    let expr = render_expr_compact_ast(body, a.expr);
                    match &a.label {
                        Some(label) => format!("{label} = {expr}"),
                        None => expr,
                    }
                })
                .collect();
            format!("{}?.({})", callee_str, args_str.join(", "))
        }
        ast::Expr::Throw { value } => format!("throw {}", render_expr_compact_ast(body, *value)),
        ast::Expr::Catch { base, clauses } => {
            let mut out = render_expr_compact_ast(body, *base);
            for clause in clauses {
                let kind = match clause.kind {
                    ast::CatchClauseKind::Catch => "catch",
                    ast::CatchClauseKind::CatchAll => "catch_all",
                    ast::CatchClauseKind::CatchAllPanics => "catch_all_panics",
                };
                write!(out, " {kind}(...)").unwrap();
            }
            out
        }
        ast::Expr::Object {
            type_name, fields, ..
        } => {
            if let Some(name) = type_name {
                format!("{name} {{ ... }}")
            } else if fields.is_empty() {
                "{ }".to_string()
            } else {
                "{ ... }".to_string()
            }
        }
        ast::Expr::Array { elements } => {
            if elements.is_empty() {
                "[]".to_string()
            } else {
                "[...]".to_string()
            }
        }
        _ => "...".to_string(),
    }
}

fn format_literal_ast(lit: &ast::Literal) -> String {
    match lit {
        ast::Literal::Int(n) => n.to_string(),
        ast::Literal::Float(s) => s.clone(),
        ast::Literal::String(s) => format!("{s:?}"),
        ast::Literal::Bool(b) => b.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use baml_base::TypePath;
    use la_arena::Arena;

    use super::*;

    fn make_ast_body(
        build: impl FnOnce(
            &mut Arena<ast::Expr>,
            &mut Arena<ast::Stmt>,
            &mut Arena<ast::Pattern>,
            &mut Arena<ast::MatchArm>,
        ) -> Option<ast::ExprId>,
    ) -> ast::ExprBody {
        let mut exprs = Arena::new();
        let mut stmts = Arena::new();
        let mut patterns = Arena::new();
        let mut match_arms = Arena::new();
        let catch_arms = Arena::new();
        let root_expr = build(&mut exprs, &mut stmts, &mut patterns, &mut match_arms);
        ast::ExprBody {
            exprs,
            stmts,
            patterns,
            match_arms,
            catch_arms,
            type_annotations: Arena::new(),
            root_expr,
        }
    }

    #[test]
    fn empty_function_has_root_only() {
        let body = make_ast_body(|exprs, _, _, _| Some(exprs.alloc(ast::Expr::Null)));
        let graph = build_control_flow_graph_from_ast("MyFunc", &body);
        assert_eq!(graph.nodes.len(), 1);
        assert!(matches!(
            graph.nodes.values().next().unwrap().node_type,
            NodeType::FunctionRoot
        ));
    }

    #[test]
    fn no_root_expr_has_root_only() {
        let body = ast::ExprBody {
            exprs: Arena::new(),
            stmts: Arena::new(),
            patterns: Arena::new(),
            match_arms: Arena::new(),
            catch_arms: Arena::new(),
            type_annotations: Arena::new(),
            root_expr: None,
        };
        let graph = build_control_flow_graph_from_ast("MyFunc", &body);
        assert_eq!(graph.nodes.len(), 1);
        assert!(matches!(
            graph.nodes.values().next().unwrap().node_type,
            NodeType::FunctionRoot
        ));
    }

    #[test]
    fn single_header_creates_header_node() {
        let body = make_ast_body(|exprs, stmts, _, _| {
            let h = stmts.alloc(ast::Stmt::HeaderComment {
                name: "Setup".into(),
                level: 1,
            });
            Some(exprs.alloc(ast::Expr::Block {
                stmts: vec![h],
                tail_expr: None,
            }))
        });
        let graph = build_control_flow_graph_from_ast("Func", &body);
        assert_eq!(graph.nodes.len(), 2); // root + header
        let header = graph.nodes.get(&NodeId::new(1)).unwrap();
        assert!(matches!(header.node_type, NodeType::HeaderContextEnter));
        assert_eq!(header.label, "Setup");
    }

    #[test]
    fn if_else_creates_branch_group_and_arms() {
        let body = make_ast_body(|exprs, _, _, _| {
            let cond = exprs.alloc(ast::Expr::Literal(ast::Literal::Bool(true)));
            let then_b = exprs.alloc(ast::Expr::Null);
            let else_b = exprs.alloc(ast::Expr::Null);
            Some(exprs.alloc(ast::Expr::If {
                condition: cond,
                then_branch: then_b,
                else_branch: Some(else_b),
            }))
        });
        let graph = build_control_flow_graph_from_ast("Func", &body);
        // Root + BranchGroup + 2 BranchArms
        assert_eq!(graph.nodes.len(), 4);
    }

    #[test]
    fn while_loop_creates_loop_node() {
        let body = make_ast_body(|exprs, stmts, _, _| {
            let cond = exprs.alloc(ast::Expr::Literal(ast::Literal::Bool(true)));
            let body_expr = exprs.alloc(ast::Expr::Null);
            let while_stmt = stmts.alloc(ast::Stmt::While {
                condition: cond,
                body: body_expr,
                after: None,
                origin: ast::LoopOrigin::While,
            });
            Some(exprs.alloc(ast::Expr::Block {
                stmts: vec![while_stmt],
                tail_expr: None,
            }))
        });
        let graph = build_control_flow_graph_from_ast("Func", &body);
        assert_eq!(graph.nodes.len(), 2); // root + loop
        let loop_node = graph.nodes.get(&NodeId::new(1)).unwrap();
        assert!(matches!(loop_node.node_type, NodeType::Loop));
    }

    #[test]
    fn for_loop_uses_for_keyword() {
        let body = make_ast_body(|exprs, stmts, _, _| {
            let cond = exprs.alloc(ast::Expr::Literal(ast::Literal::Bool(true)));
            let body_expr = exprs.alloc(ast::Expr::Null);
            let for_stmt = stmts.alloc(ast::Stmt::While {
                condition: cond,
                body: body_expr,
                after: None,
                origin: ast::LoopOrigin::For,
            });
            Some(exprs.alloc(ast::Expr::Block {
                stmts: vec![for_stmt],
                tail_expr: None,
            }))
        });
        let graph = build_control_flow_graph_from_ast("Func", &body);
        let loop_node = graph.nodes.get(&NodeId::new(1)).unwrap();
        assert!(loop_node.label.starts_with("for"));
    }

    #[test]
    fn if_without_else_gets_synthetic_else() {
        let body = make_ast_body(|exprs, _, _, _| {
            let cond = exprs.alloc(ast::Expr::Literal(ast::Literal::Bool(true)));
            let then_b = exprs.alloc(ast::Expr::Null);
            Some(exprs.alloc(ast::Expr::If {
                condition: cond,
                then_branch: then_b,
                else_branch: None,
            }))
        });
        let graph = build_control_flow_graph_from_ast("Func", &body);
        // Root + BranchGroup + 2 BranchArms (then + synthetic else)
        assert_eq!(graph.nodes.len(), 4);
        let else_arm = graph
            .nodes
            .values()
            .find(|n| n.label == "else")
            .expect("should have synthetic else arm");
        assert!(matches!(else_arm.node_type, NodeType::BranchArm));
    }

    #[test]
    fn else_if_chain_flattened_into_single_branch_group() {
        let body = make_ast_body(|exprs, _, _, _| {
            let cond1 = exprs.alloc(ast::Expr::Literal(ast::Literal::Bool(true)));
            let then1 = exprs.alloc(ast::Expr::Null);
            let cond2 = exprs.alloc(ast::Expr::Literal(ast::Literal::Bool(false)));
            let then2 = exprs.alloc(ast::Expr::Null);
            let else_final = exprs.alloc(ast::Expr::Null);

            let inner_if = exprs.alloc(ast::Expr::If {
                condition: cond2,
                then_branch: then2,
                else_branch: Some(else_final),
            });

            Some(exprs.alloc(ast::Expr::If {
                condition: cond1,
                then_branch: then1,
                else_branch: Some(inner_if),
            }))
        });
        let graph = build_control_flow_graph_from_ast("Func", &body);
        // Root + 1 BranchGroup + 3 BranchArms (if, else if, else)
        assert_eq!(graph.nodes.len(), 5);
        let groups: Vec<_> = graph
            .nodes
            .values()
            .filter(|n| matches!(n.node_type, NodeType::BranchGroup))
            .collect();
        assert_eq!(groups.len(), 1);
    }

    #[test]
    fn match_creates_branch_group_with_arms() {
        let body = make_ast_body(|exprs, _, patterns, match_arms| {
            let scrutinee = exprs.alloc(ast::Expr::Path(vec!["x".into()]));
            let pat1 = patterns.alloc(ast::Pattern::Type(ast::TypeExpr::Literal {
                value: ast::Literal::Int(1),
                attrs: vec![],
            }));
            let pat2 = patterns.alloc(ast::Pattern::Type(ast::TypeExpr::Literal {
                value: ast::Literal::Int(2),
                attrs: vec![],
            }));
            let body1 = exprs.alloc(ast::Expr::Null);
            let body2 = exprs.alloc(ast::Expr::Null);
            let arm1 = match_arms.alloc(ast::MatchArm {
                pattern: pat1,
                guard: None,
                body: body1,
            });
            let arm2 = match_arms.alloc(ast::MatchArm {
                pattern: pat2,
                guard: None,
                body: body2,
            });
            Some(exprs.alloc(ast::Expr::Match {
                scrutinee,
                scrutinee_type: None,
                arms: vec![arm1, arm2],
            }))
        });
        let graph = build_control_flow_graph_from_ast("Func", &body);
        // Root + BranchGroup + 2 BranchArms
        assert_eq!(graph.nodes.len(), 4);
        let groups: Vec<_> = graph
            .nodes
            .values()
            .filter(|n| matches!(n.node_type, NodeType::BranchGroup))
            .collect();
        assert_eq!(groups.len(), 1);
        assert!(groups[0].label.starts_with("match"));
    }

    #[test]
    fn format_pattern_bind_renders_with_let_keyword() {
        let body = make_ast_body(|_, _, patterns, _| {
            patterns.alloc(ast::Pattern::Bind {
                name: "x".into(),
                subpat: None,
            });
            None
        });
        let builder = AstGraphBuilder::new("Func", &body);
        let pat = body.patterns.iter().next().unwrap().0;
        assert_eq!(builder.format_pattern(pat), "let x");
    }

    #[test]
    fn format_pattern_bind_with_ascription_renders_chain() {
        // `let x: int` is now Bind { name: x, subpat: Some(Type(int)) }.
        let body = make_ast_body(|_, _, patterns, _| {
            let int_ty = ast::TypeExpr::Path {
                segments: vec!["int".into()],
                generic_args: vec![],
                attrs: vec![],
            };
            let inner = patterns.alloc(ast::Pattern::Type(int_ty));
            patterns.alloc(ast::Pattern::Bind {
                name: "x".into(),
                subpat: Some(inner),
            });
            None
        });
        let builder = AstGraphBuilder::new("Func", &body);
        let pat = body.patterns.iter().last().unwrap().0;
        assert_eq!(builder.format_pattern(pat), "let x: int");
    }

    #[test]
    fn sequential_headers_at_same_level_are_siblings() {
        let body = make_ast_body(|exprs, stmts, _, _| {
            let h1 = stmts.alloc(ast::Stmt::HeaderComment {
                name: "First".into(),
                level: 1,
            });
            let h2 = stmts.alloc(ast::Stmt::HeaderComment {
                name: "Second".into(),
                level: 1,
            });
            Some(exprs.alloc(ast::Expr::Block {
                stmts: vec![h1, h2],
                tail_expr: None,
            }))
        });
        let graph = build_control_flow_graph_from_ast("Func", &body);
        assert_eq!(graph.nodes.len(), 3); // root + 2 headers
        // Both headers should have root as parent
        let h1 = graph.nodes.get(&NodeId::new(1)).unwrap();
        let h2 = graph.nodes.get(&NodeId::new(2)).unwrap();
        assert_eq!(h1.parent_node_id, Some(NodeId::new(0)));
        assert_eq!(h2.parent_node_id, Some(NodeId::new(0)));
        // And they should be connected by an edge
        let h1_edges = graph.edges_by_src.get(&NodeId::new(1));
        assert!(h1_edges.is_some());
        assert!(h1_edges.unwrap().iter().any(|e| e.dst == NodeId::new(2)));
    }

    #[test]
    fn nested_headers_form_hierarchy() {
        let body = make_ast_body(|exprs, stmts, _, _| {
            let h1 = stmts.alloc(ast::Stmt::HeaderComment {
                name: "Outer".into(),
                level: 1,
            });
            let h2 = stmts.alloc(ast::Stmt::HeaderComment {
                name: "Inner".into(),
                level: 2,
            });
            Some(exprs.alloc(ast::Expr::Block {
                stmts: vec![h1, h2],
                tail_expr: None,
            }))
        });
        let graph = build_control_flow_graph_from_ast("Func", &body);
        assert_eq!(graph.nodes.len(), 3); // root + 2 headers
        let inner = graph.nodes.get(&NodeId::new(2)).unwrap();
        // Inner should be child of Outer
        assert_eq!(inner.parent_node_id, Some(NodeId::new(1)));
    }

    #[test]
    fn missing_expr_produces_root_only() {
        let body = make_ast_body(|exprs, _, _, _| Some(exprs.alloc(ast::Expr::Missing)));
        let graph = build_control_flow_graph_from_ast("Func", &body);
        assert_eq!(graph.nodes.len(), 1);
    }

    #[test]
    fn let_with_if_initializer_creates_other_scope() {
        let body = make_ast_body(|exprs, stmts, patterns, _| {
            let cond = exprs.alloc(ast::Expr::Literal(ast::Literal::Bool(true)));
            let then_b = exprs.alloc(ast::Expr::Literal(ast::Literal::Int(1)));
            let else_b = exprs.alloc(ast::Expr::Literal(ast::Literal::Int(2)));
            let if_expr = exprs.alloc(ast::Expr::If {
                condition: cond,
                then_branch: then_b,
                else_branch: Some(else_b),
            });
            let pat = patterns.alloc(ast::Pattern::Bind {
                name: "x".into(),
                subpat: None,
            });
            let let_stmt = stmts.alloc(ast::Stmt::Let {
                pattern: pat,
                initializer: Some(if_expr),
                is_watched: false,
                origin: ast::LetOrigin::Source,
            });
            Some(exprs.alloc(ast::Expr::Block {
                stmts: vec![let_stmt],
                tail_expr: None,
            }))
        });
        let graph = build_control_flow_graph_from_ast("Func", &body);
        // Root + OtherScope("let x = ...") + BranchGroup + 2 BranchArms
        assert_eq!(graph.nodes.len(), 5);
        let scope = graph
            .nodes
            .values()
            .find(|n| matches!(n.node_type, NodeType::OtherScope))
            .expect("should have OtherScope");
        assert_eq!(scope.label, "let x = ...");
    }

    #[test]
    fn let_with_wildcard_pattern_keeps_let_keyword() {
        // Non-Bind let-stmt patterns (Wildcard, Type, Class destructure)
        // need the explicit `let ` keyword in CFG labels — without it the
        // node renders like an assignment (`_ = ...`) instead of a
        // declaration. Regression: previously `format_pattern` was assumed
        // to prefix every let pattern with `let `, but it only does so for
        // top-level Binds.
        let body = make_ast_body(|exprs, stmts, patterns, _| {
            let cond = exprs.alloc(ast::Expr::Literal(ast::Literal::Bool(true)));
            let then_b = exprs.alloc(ast::Expr::Literal(ast::Literal::Int(1)));
            let else_b = exprs.alloc(ast::Expr::Literal(ast::Literal::Int(2)));
            let if_expr = exprs.alloc(ast::Expr::If {
                condition: cond,
                then_branch: then_b,
                else_branch: Some(else_b),
            });
            let pat = patterns.alloc(ast::Pattern::Wildcard);
            let let_stmt = stmts.alloc(ast::Stmt::Let {
                pattern: pat,
                initializer: Some(if_expr),
                is_watched: false,
                origin: ast::LetOrigin::Source,
            });
            Some(exprs.alloc(ast::Expr::Block {
                stmts: vec![let_stmt],
                tail_expr: None,
            }))
        });
        let graph = build_control_flow_graph_from_ast("Func", &body);
        let scope = graph
            .nodes
            .values()
            .find(|n| matches!(n.node_type, NodeType::OtherScope))
            .expect("should have OtherScope");
        assert_eq!(scope.label, "let _ = ...");
    }

    #[test]
    fn call_scope_has_source_expr() {
        let body = make_ast_body(|exprs, _, _, _| {
            let callee = exprs.alloc(ast::Expr::Path(vec!["Summarize".into()]));
            let arg = exprs.alloc(ast::Expr::Path(vec!["text".into()]));
            let call = exprs.alloc(ast::Expr::Call {
                callee,
                type_args: vec![],
                args: vec![ast::CallArg::positional(arg)],
            });
            Some(call)
        });
        let graph = build_control_flow_graph_from_ast("Func", &body);
        assert_eq!(graph.nodes.len(), 2); // root + call
        let call_node = graph
            .nodes
            .values()
            .find(|n| matches!(n.node_type, NodeType::OtherScope))
            .expect("should have OtherScope for call");
        assert!(
            call_node.source_expr.is_some(),
            "call scope should have source_expr set"
        );
    }

    #[test]
    fn named_call_scope_preserves_label() {
        let body = make_ast_body(|exprs, _, _, _| {
            let callee = exprs.alloc(ast::Expr::Path(vec!["Summarize".into()]));
            let arg = exprs.alloc(ast::Expr::Path(vec!["text".into()]));
            let call = exprs.alloc(ast::Expr::Call {
                callee,
                args: vec![ast::CallArg::named("query", arg)],
                type_args: vec![],
            });
            Some(call)
        });
        let graph = build_control_flow_graph_from_ast("Func", &body);
        let call_node = graph
            .nodes
            .values()
            .find(|n| matches!(n.node_type, NodeType::OtherScope))
            .expect("should have OtherScope for call");
        assert!(call_node.source_expr.is_some());
        assert_eq!(call_node.label, "Summarize(query = text)");
    }

    #[test]
    fn named_optional_call_scope_preserves_label() {
        let body = make_ast_body(|exprs, _, _, _| {
            let callee = exprs.alloc(ast::Expr::Path(vec!["client".into()]));
            let arg = exprs.alloc(ast::Expr::Path(vec!["text".into()]));
            let call = exprs.alloc(ast::Expr::OptionalCall {
                callee,
                args: vec![ast::CallArg::named("query", arg)],
            });
            Some(call)
        });
        let graph = build_control_flow_graph_from_ast("Func", &body);
        let call_node = graph
            .nodes
            .values()
            .find(|n| matches!(n.node_type, NodeType::OtherScope))
            .expect("should have OtherScope for optional call");
        assert!(call_node.source_expr.is_some());
        assert_eq!(call_node.label, "client?.(query = text)");
    }

    #[test]
    fn if_branch_group_has_source_expr() {
        let body = make_ast_body(|exprs, _, _, _| {
            let cond = exprs.alloc(ast::Expr::Literal(ast::Literal::Bool(true)));
            let then_b = exprs.alloc(ast::Expr::Null);
            let else_b = exprs.alloc(ast::Expr::Null);
            Some(exprs.alloc(ast::Expr::If {
                condition: cond,
                then_branch: then_b,
                else_branch: Some(else_b),
            }))
        });
        let graph = build_control_flow_graph_from_ast("Func", &body);
        let branch_group = graph
            .nodes
            .values()
            .find(|n| matches!(n.node_type, NodeType::BranchGroup))
            .expect("should have BranchGroup");
        assert!(
            branch_group.source_expr.is_some(),
            "BranchGroup should have source_expr pointing to the If expression"
        );
    }

    #[test]
    fn loop_has_source_expr() {
        let body = make_ast_body(|exprs, stmts, _, _| {
            let cond = exprs.alloc(ast::Expr::Literal(ast::Literal::Bool(true)));
            let body_expr = exprs.alloc(ast::Expr::Null);
            let while_stmt = stmts.alloc(ast::Stmt::While {
                condition: cond,
                body: body_expr,
                after: None,
                origin: ast::LoopOrigin::While,
            });
            Some(exprs.alloc(ast::Expr::Block {
                stmts: vec![while_stmt],
                tail_expr: None,
            }))
        });
        let graph = build_control_flow_graph_from_ast("Func", &body);
        let loop_node = graph
            .nodes
            .values()
            .find(|n| matches!(n.node_type, NodeType::Loop))
            .expect("should have Loop");
        assert!(
            loop_node.source_expr.is_some(),
            "Loop should have source_expr pointing to the condition"
        );
    }

    #[test]
    fn header_has_tagged_source_expr() {
        let body = make_ast_body(|exprs, stmts, _, _| {
            let h = stmts.alloc(ast::Stmt::HeaderComment {
                name: "Setup".into(),
                level: 1,
            });
            Some(exprs.alloc(ast::Expr::Block {
                stmts: vec![h],
                tail_expr: None,
            }))
        });
        let graph = build_control_flow_graph_from_ast("Func", &body);
        let header = graph
            .nodes
            .values()
            .find(|n| matches!(n.node_type, NodeType::HeaderContextEnter))
            .expect("should have header");
        assert!(
            header.source_expr.is_some(),
            "Header should have a tagged source_expr for cursor matching"
        );
        let se = header.source_expr.unwrap();
        assert!(
            se & STMT_SOURCE_EXPR_TAG != 0,
            "Header source_expr should have the STMT tag bit set"
        );
    }

    #[test]
    fn synthetic_else_has_no_source_expr() {
        let body = make_ast_body(|exprs, _, _, _| {
            let cond = exprs.alloc(ast::Expr::Literal(ast::Literal::Bool(true)));
            let then_b = exprs.alloc(ast::Expr::Null);
            Some(exprs.alloc(ast::Expr::If {
                condition: cond,
                then_branch: then_b,
                else_branch: None,
            }))
        });
        let graph = build_control_flow_graph_from_ast("Func", &body);
        let else_arm = graph
            .nodes
            .values()
            .find(|n| n.label == "else")
            .expect("should have synthetic else arm");
        assert!(
            else_arm.source_expr.is_none(),
            "Synthetic else arm should not have source_expr"
        );
    }

    #[test]
    fn return_object_creates_leaf_node() {
        let body = make_ast_body(|exprs, stmts, _, _| {
            let field_val = exprs.alloc(ast::Expr::Literal(ast::Literal::Bool(true)));
            let obj = exprs.alloc(ast::Expr::Object {
                type_name: Some(TypePath::bare("MyResponse".into())),
                type_args: vec![],
                fields: vec![("ok".into(), field_val)],
                spreads: vec![],
            });
            let ret = stmts.alloc(ast::Stmt::Return(Some(obj)));
            Some(exprs.alloc(ast::Expr::Block {
                stmts: vec![ret],
                tail_expr: None,
            }))
        });
        let graph = build_control_flow_graph_from_ast("Func", &body);
        // Root + return leaf
        assert_eq!(graph.nodes.len(), 2);
        let ret_node = graph
            .nodes
            .values()
            .find(|n| n.label.starts_with("return"))
            .expect("should have return node");
        assert!(matches!(ret_node.node_type, NodeType::OtherScope));
        assert!(
            ret_node.label.contains("MyResponse"),
            "Return label should include the type name, got: {}",
            ret_node.label
        );
        assert!(
            ret_node.source_expr.is_some(),
            "Return node should have source_expr"
        );
    }

    #[test]
    fn return_call_does_not_double_node() {
        let body = make_ast_body(|exprs, stmts, _, _| {
            let callee = exprs.alloc(ast::Expr::Path(vec!["Process".into()]));
            let arg = exprs.alloc(ast::Expr::Path(vec!["input".into()]));
            let call = exprs.alloc(ast::Expr::Call {
                callee,
                type_args: vec![],
                args: vec![ast::CallArg::positional(arg)],
            });
            let ret = stmts.alloc(ast::Stmt::Return(Some(call)));
            Some(exprs.alloc(ast::Expr::Block {
                stmts: vec![ret],
                tail_expr: None,
            }))
        });
        let graph = build_control_flow_graph_from_ast("Func", &body);
        // Root + call scope (no extra return node — calls are handled by visit_expr)
        assert_eq!(graph.nodes.len(), 2);
        let call_node = graph
            .nodes
            .values()
            .find(|n| n.label.contains("Process"))
            .expect("should have call node");
        assert!(matches!(call_node.node_type, NodeType::OtherScope));
    }

    #[test]
    fn return_in_if_branches_creates_two_nodes() {
        let body = make_ast_body(|exprs, stmts, _, _| {
            let cond = exprs.alloc(ast::Expr::Literal(ast::Literal::Bool(true)));
            let obj_true = exprs.alloc(ast::Expr::Object {
                type_name: Some(TypePath::bare("Result".into())),
                type_args: vec![],
                fields: vec![],
                spreads: vec![],
            });
            let ret_true = stmts.alloc(ast::Stmt::Return(Some(obj_true)));
            let then_b = exprs.alloc(ast::Expr::Block {
                stmts: vec![ret_true],
                tail_expr: None,
            });

            let err_val = exprs.alloc(ast::Expr::Literal(ast::Literal::Bool(false)));
            let obj_false = exprs.alloc(ast::Expr::Object {
                type_name: Some(TypePath::bare("Result".into())),
                type_args: vec![],
                fields: vec![("err".into(), err_val)],
                spreads: vec![],
            });
            let ret_false = stmts.alloc(ast::Stmt::Return(Some(obj_false)));
            let else_b = exprs.alloc(ast::Expr::Block {
                stmts: vec![ret_false],
                tail_expr: None,
            });

            Some(exprs.alloc(ast::Expr::If {
                condition: cond,
                then_branch: then_b,
                else_branch: Some(else_b),
            }))
        });
        let graph = build_control_flow_graph_from_ast("Func", &body);
        let return_nodes: Vec<_> = graph
            .nodes
            .values()
            .filter(|n| n.label.starts_with("return"))
            .collect();
        assert_eq!(
            return_nodes.len(),
            2,
            "Should have two return nodes (one per branch)"
        );
    }

    #[test]
    fn render_object_expr_compact() {
        let mut exprs = Arena::new();
        let stmts = Arena::new();
        let patterns = Arena::new();
        let match_arms = Arena::new();
        let catch_arms = Arena::new();

        let field_val = exprs.alloc(ast::Expr::Literal(ast::Literal::Bool(true)));
        let obj = exprs.alloc(ast::Expr::Object {
            type_name: Some(TypePath::bare("Resp".into())),
            type_args: vec![],
            fields: vec![("ok".into(), field_val)],
            spreads: vec![],
        });

        let body = ast::ExprBody {
            exprs,
            stmts,
            patterns,
            match_arms,
            catch_arms,
            type_annotations: Arena::new(),
            root_expr: Some(obj),
        };

        let rendered = render_expr_compact_ast(&body, obj);
        assert_eq!(rendered, "Resp { ... }");
    }
}
