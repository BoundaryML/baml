//! Build a control flow visualization graph directly from a compiler2 AST.
//!
//! This is the AST-based counterpart of `super::build_control_flow_graph` which
//! walks VIR expressions. The advantage of building from the AST is resilience:
//! the compiler2 AST uses `Expr::Missing` / `Stmt::Missing` sentinels for error
//! recovery, so the CFG survives parse and type errors.

use baml_compiler2_ast as ast;

use super::{
    ControlFlowGraph, CounterKind, Frame, FrameEntry, GraphAccumulator, Node, NodeId, NodeType,
    PathSegment, encode_segments, slugify,
};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Build a control flow visualization graph from a compiler2 AST expression body.
pub fn build_control_flow_graph_from_ast(
    function_name: &str,
    body: &ast::ExprBody,
) -> ControlFlowGraph {
    let root_expr = match body.root_expr {
        Some(id) => id,
        None => {
            // No root expression — return a graph with just the root node.
            let mut graph = GraphAccumulator::default();
            let root_id = graph.allocate_id();
            let root_segment = PathSegment::FunctionRoot { ordinal: 0 };
            let root_key = encode_segments(function_name, std::slice::from_ref(&root_segment));
            graph.add_node(Node::root(root_id, root_key, function_name));
            return graph.finish();
        }
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
                self.visit_if(*condition, *then_branch, *else_branch);
            }

            ast::Expr::Match {
                scrutinee, arms, ..
            } => {
                self.visit_match(*scrutinee, arms);
            }

            // All other expressions don't create graph nodes.
            _ => {}
        }
    }

    fn visit_stmt(&mut self, id: ast::StmtId) {
        let stmt = self.body.stmts[id].clone();
        match &stmt {
            ast::Stmt::HeaderComment { name, level } => {
                self.enter_header(name.as_ref(), *level);
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
                initializer,
                pattern,
                ..
            } => {
                if let Some(init) = initializer {
                    let init_expr = self.body.exprs[*init].clone();
                    let needs_scope =
                        matches!(init_expr, ast::Expr::If { .. } | ast::Expr::Match { .. });
                    if needs_scope {
                        let pat_name = self.format_pattern(*pattern);
                        let label = format!("let {pat_name} = ...");
                        self.emit_other_scope(*init, Some(label));
                    } else {
                        self.visit_expr(*init);
                    }
                }
            }

            ast::Stmt::Expr(expr_id) => {
                self.visit_expr(*expr_id);
            }

            // Return, Break, Continue, Assign, AssignOp, Assert, Missing — no graph nodes.
            _ => {}
        }
    }

    // -- If/else chain flattening --

    fn visit_if(
        &mut self,
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
            None, // source_expr: None for AST spike
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
            None, // source_expr: None for AST spike
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

    fn visit_match(&mut self, scrutinee: ast::ExprId, arms: &[ast::MatchArmId]) {
        let parent_depth = self.frames.len();
        let ordinal = {
            let frame = self
                .frames
                .last_mut()
                .expect("frame stack should not be empty");
            frame.next_ordinal(&CounterKind::BranchGroup)
        };
        let label = format!(
            "match ({})",
            render_expr_compact_ast(self.body, scrutinee)
        );
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
            None,
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

    fn visit_loop(
        &mut self,
        condition: ast::ExprId,
        body: ast::ExprId,
        origin: ast::LoopOrigin,
    ) {
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
            None,
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
    fn enter_header(&mut self, title: &str, level: usize) {
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
        let node = Node::new(
            node_id,
            parent_id,
            log_filter_key,
            title.to_string(),
            None,
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
            None,
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
            ast::Pattern::Binding(name) => name.to_string(),
            ast::Pattern::TypedBinding { name, .. } => name.to_string(),
            ast::Pattern::Literal(lit) => format_literal_ast(lit),
            ast::Pattern::EnumVariant { enum_name, variant } => {
                format!("{enum_name}.{variant}")
            }
            ast::Pattern::Union(pats) => {
                let parts: Vec<_> = pats.iter().map(|p| self.format_pattern(*p)).collect();
                parts.join(" | ")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Compact expression renderer for AST Expr (for labels)
// ---------------------------------------------------------------------------

fn render_expr_compact_ast(body: &ast::ExprBody, id: ast::ExprId) -> String {
    let expr = &body.exprs[id];
    match expr {
        ast::Expr::Literal(lit) => format_literal_ast(lit),
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
                ast::BinaryOp::Instanceof => "instanceof",
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
        ast::Expr::FieldAccess { base, field } => {
            format!("{}.{field}", render_expr_compact_ast(body, *base))
        }
        ast::Expr::Index { base, index } => {
            format!(
                "{}[{}]",
                render_expr_compact_ast(body, *base),
                render_expr_compact_ast(body, *index)
            )
        }
        ast::Expr::Call { callee, args } => {
            let callee_str = render_expr_compact_ast(body, *callee);
            let args_str: Vec<_> = args
                .iter()
                .map(|a| render_expr_compact_ast(body, *a))
                .collect();
            format!("{}({})", callee_str, args_str.join(", "))
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
        ast::Literal::Null => "null".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
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
        let root_expr = build(&mut exprs, &mut stmts, &mut patterns, &mut match_arms);
        ast::ExprBody {
            exprs,
            stmts,
            patterns,
            match_arms,
            type_annotations: Arena::new(),
            root_expr,
        }
    }

    #[test]
    fn empty_function_has_root_only() {
        let body = make_ast_body(|exprs, _, _, _| {
            Some(exprs.alloc(ast::Expr::Literal(ast::Literal::Null)))
        });
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
            let then_b = exprs.alloc(ast::Expr::Literal(ast::Literal::Null));
            let else_b = exprs.alloc(ast::Expr::Literal(ast::Literal::Null));
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
            let body_expr = exprs.alloc(ast::Expr::Literal(ast::Literal::Null));
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
            let body_expr = exprs.alloc(ast::Expr::Literal(ast::Literal::Null));
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
            let then_b = exprs.alloc(ast::Expr::Literal(ast::Literal::Null));
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
            let then1 = exprs.alloc(ast::Expr::Literal(ast::Literal::Null));
            let cond2 = exprs.alloc(ast::Expr::Literal(ast::Literal::Bool(false)));
            let then2 = exprs.alloc(ast::Expr::Literal(ast::Literal::Null));
            let else_final = exprs.alloc(ast::Expr::Literal(ast::Literal::Null));

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
            let pat1 = patterns.alloc(ast::Pattern::Literal(ast::Literal::Int(1)));
            let pat2 = patterns.alloc(ast::Pattern::Literal(ast::Literal::Int(2)));
            let body1 = exprs.alloc(ast::Expr::Literal(ast::Literal::Null));
            let body2 = exprs.alloc(ast::Expr::Literal(ast::Literal::Null));
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
            let pat = patterns.alloc(ast::Pattern::Binding("x".into()));
            let let_stmt = stmts.alloc(ast::Stmt::Let {
                pattern: pat,
                type_annotation: None,
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
}
