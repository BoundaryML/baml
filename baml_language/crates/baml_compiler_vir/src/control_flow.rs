//! Control flow visualization graph for BAML functions.
//!
//! Graph types and the AST-based builder live in `baml_compiler2_visualization`.
//! This module re-exports those and adds a VIR-specific builder that walks
//! the expression-only Validated IR.

// Re-export everything from the visualization crate so existing consumers
// (db.rs, baml_tests, etc.) continue to work via `baml_compiler_vir::control_flow::*`.
pub use baml_compiler2_visualization::control_flow::*;

use crate::{BinaryOp, Expr, ExprBody, ExprId, Literal, MatchArm, Pattern, UnaryOp};

// ---------------------------------------------------------------------------
// Public API — VIR-specific
// ---------------------------------------------------------------------------

/// Build a control flow visualization graph from a VIR expression body.
pub fn build_control_flow_graph(function_name: &str, body: &ExprBody) -> ControlFlowGraph {
    let mut builder = GraphBuilder::new(function_name, body);
    builder.visit_expr(body.root);
    builder.finish()
}

// ---------------------------------------------------------------------------
// Graph builder — walks VIR ExprBody
// ---------------------------------------------------------------------------

struct GraphBuilder<'a> {
    body: &'a ExprBody,
    function_name: String,
    graph: GraphAccumulator,
    frames: Vec<Frame>,
}

impl<'a> GraphBuilder<'a> {
    fn new(function_name: &str, body: &'a ExprBody) -> Self {
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

    // -- Main dispatch --

    fn visit_expr(&mut self, id: ExprId) {
        let expr = self.body.expr(id).clone();
        match &expr {
            Expr::Seq { first, second } => {
                self.visit_expr(*first);
                self.visit_expr(*second);
            }

            Expr::Let {
                value,
                body,
                pattern,
                ..
            } => {
                // Check if value contains control flow worth wrapping
                let value_expr = self.body.expr(*value).clone();
                let needs_scope = matches!(
                    value_expr,
                    Expr::If { .. } | Expr::While { .. } | Expr::Match { .. }
                );
                if needs_scope {
                    let pat_name = self.format_pattern(*pattern);
                    let label = format!("let {pat_name} = ...");
                    self.emit_other_scope(*value, Some(label), Some(id.into_raw().into_u32()));
                } else {
                    self.visit_expr(*value);
                }
                self.visit_expr(*body);
            }

            Expr::NotifyBlock { name, level } => {
                self.enter_header(name.as_ref(), *level, Some(id.into_raw().into_u32()));
            }

            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.visit_if(*condition, *then_branch, *else_branch, id);
            }

            Expr::While { condition, body } => {
                self.visit_loop(*condition, *body, id);
            }

            Expr::Match {
                scrutinee, arms, ..
            } => {
                self.visit_match(*scrutinee, arms, id);
            }

            // All other expressions don't create graph nodes
            _ => {}
        }
    }

    // -- If/else chain flattening --

    fn visit_if(
        &mut self,
        condition: ExprId,
        then_branch: ExprId,
        else_branch: Option<ExprId>,
        expr_id: ExprId,
    ) {
        let parent_depth = self.frames.len();
        let ordinal = {
            let frame = self
                .frames
                .last_mut()
                .expect("frame stack should not be empty");
            frame.next_ordinal(&CounterKind::BranchGroup)
        };
        let label = format!("if ({})", render_expr_compact(self.body, condition));
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
            Some(expr_id.into_raw().into_u32()),
            NodeType::BranchGroup,
        );
        self.graph.add_node(node);
        let parent_index = self.current_parent_index();
        self.register_child_with_parent(parent_index, node_id);
        self.frames
            .push(Frame::new(FrameEntry::BranchGroup, node_id, Some(segment)));

        // First arm: "if (condition)"
        let arm_label = format!("if ({})", render_expr_compact(self.body, condition));
        self.visit_branch_arm(
            arm_label,
            then_branch,
            Some(then_branch.into_raw().into_u32()),
        );

        // Flatten else-if chains
        let mut current_else = else_branch;
        while let Some(else_id) = current_else {
            let else_expr = self.body.expr(else_id).clone();
            match else_expr {
                Expr::If {
                    condition: else_cond,
                    then_branch: else_then,
                    else_branch: else_else,
                } => {
                    let arm_label =
                        format!("else if ({})", render_expr_compact(self.body, else_cond));
                    self.visit_branch_arm(
                        arm_label,
                        else_then,
                        Some(else_then.into_raw().into_u32()),
                    );
                    current_else = else_else;
                }
                _ => {
                    self.visit_branch_arm(
                        "else".to_string(),
                        else_id,
                        Some(else_id.into_raw().into_u32()),
                    );
                    current_else = None;
                }
            }
        }

        // Synthetic "else" arm if no else branch
        if else_branch.is_none() {
            self.emit_synthetic_branch_arm("else".to_string(), Some(expr_id.into_raw().into_u32()));
        }

        self.pop_frames_to(parent_depth);
    }

    fn visit_branch_arm(&mut self, label: String, body_expr: ExprId, source_expr: Option<u32>) {
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
            source_expr,
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

    /// Create a branch arm node with no body traversal (for synthetic else arms).
    fn emit_synthetic_branch_arm(&mut self, label: String, source_expr: Option<u32>) {
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
            source_expr,
            NodeType::BranchArm,
        );
        self.graph.add_node(node);
        let parent_index = self.current_parent_index();
        self.register_child_with_parent(parent_index, node_id);
        // No body traversal — this arm is empty.
    }

    // -- Match expressions --

    fn visit_match(&mut self, scrutinee: ExprId, arms: &[MatchArm], expr_id: ExprId) {
        let parent_depth = self.frames.len();
        let ordinal = {
            let frame = self
                .frames
                .last_mut()
                .expect("frame stack should not be empty");
            frame.next_ordinal(&CounterKind::BranchGroup)
        };
        let label = format!("match ({})", render_expr_compact(self.body, scrutinee));
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
            Some(expr_id.into_raw().into_u32()),
            NodeType::BranchGroup,
        );
        self.graph.add_node(node);
        let parent_index = self.current_parent_index();
        self.register_child_with_parent(parent_index, node_id);
        self.frames
            .push(Frame::new(FrameEntry::BranchGroup, node_id, Some(segment)));

        for arm in arms {
            let arm_label = self.format_pattern(arm.pattern);
            self.visit_branch_arm(arm_label, arm.body, Some(arm.body.into_raw().into_u32()));
        }

        self.pop_frames_to(parent_depth);
    }

    // -- While loops --

    fn visit_loop(&mut self, condition: ExprId, body: ExprId, expr_id: ExprId) {
        let parent_depth = self.frames.len();
        let ordinal = {
            let frame = self
                .frames
                .last_mut()
                .expect("frame stack should not be empty");
            frame.next_ordinal(&CounterKind::Loop)
        };
        let label = format!("while ({})", render_expr_compact(self.body, condition));
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
            Some(expr_id.into_raw().into_u32()),
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
    fn enter_header(&mut self, title: &str, level: usize, source_expr: Option<u32>) {
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

    // -- OtherScope --

    fn emit_other_scope(
        &mut self,
        inner_expr: ExprId,
        label: Option<String>,
        source_expr: Option<u32>,
    ) {
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
            source_expr,
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

    // -- Helpers --

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

    fn format_pattern(&self, pat_id: crate::PatId) -> String {
        let pat = self.body.pattern(pat_id);
        match pat {
            Pattern::Binding(name) => name.to_string(),
            Pattern::TypedBinding { name, ty } => format!("{name}: {ty}"),
            Pattern::Literal(lit) => format_literal(lit),
            Pattern::EnumVariant { enum_name, variant } => format!("{enum_name}.{variant}"),
            Pattern::Union(pats) => {
                let parts: Vec<_> = pats.iter().map(|p| self.format_pattern(*p)).collect();
                parts.join(" | ")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// VIR-specific compact expression renderer (for labels)
// ---------------------------------------------------------------------------

fn render_expr_compact(body: &ExprBody, id: ExprId) -> String {
    let expr = body.expr(id);
    match expr {
        Expr::Literal(lit) => format_literal(lit),
        Expr::Var(name) => name.to_string(),
        Expr::Path(segments) => {
            let parts: Vec<_> = segments.iter().map(ToString::to_string).collect();
            parts.join(".")
        }
        Expr::Binary { op, lhs, rhs } => {
            let op_str = match op {
                BinaryOp::Add => "+",
                BinaryOp::Sub => "-",
                BinaryOp::Mul => "*",
                BinaryOp::Div => "/",
                BinaryOp::Mod => "%",
                BinaryOp::Eq => "==",
                BinaryOp::Ne => "!=",
                BinaryOp::Lt => "<",
                BinaryOp::Le => "<=",
                BinaryOp::Gt => ">",
                BinaryOp::Ge => ">=",
                BinaryOp::And => "&&",
                BinaryOp::Or => "||",
                BinaryOp::BitAnd => "&",
                BinaryOp::BitOr => "|",
                BinaryOp::BitXor => "^",
                BinaryOp::Shl => "<<",
                BinaryOp::Shr => ">>",
            };
            format!(
                "{} {} {}",
                render_expr_compact(body, *lhs),
                op_str,
                render_expr_compact(body, *rhs)
            )
        }
        Expr::Unary { op, operand } => {
            let op_str = match op {
                UnaryOp::Not => "!",
                UnaryOp::Neg => "-",
            };
            format!("{op_str}{}", render_expr_compact(body, *operand))
        }
        Expr::FieldAccess { base, field } => {
            format!("{}.{field}", render_expr_compact(body, *base))
        }
        Expr::Index { base, index } => {
            format!(
                "{}[{}]",
                render_expr_compact(body, *base),
                render_expr_compact(body, *index)
            )
        }
        Expr::Call { callee, args } => {
            let callee_str = render_expr_compact(body, *callee);
            let args_str: Vec<_> = args.iter().map(|a| render_expr_compact(body, *a)).collect();
            format!("{}({})", callee_str, args_str.join(", "))
        }
        _ => "...".to_string(),
    }
}

fn format_literal(lit: &Literal) -> String {
    match lit {
        Literal::Int(n) => n.to_string(),
        Literal::Float(s) => s.clone(),
        Literal::String(s) => format!("{s:?}"),
        Literal::Bool(b) => b.to_string(),
        Literal::Null => "null".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Tests (VIR-specific)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use la_arena::Arena;

    use super::*;
    use crate::{Expr, ExprBody, Pattern};

    fn make_body(build: impl FnOnce(&mut Arena<Expr>, &mut Arena<Pattern>) -> ExprId) -> ExprBody {
        let mut exprs = Arena::new();
        let mut patterns = Arena::new();
        let root = build(&mut exprs, &mut patterns);
        ExprBody {
            exprs,
            patterns,
            expr_types: HashMap::default(),
            enum_variant_exprs: HashMap::default(),
            resolutions: HashMap::default(),
            source_spans: HashMap::default(),
            root,
        }
    }

    #[test]
    fn empty_function_has_root_only() {
        let body = make_body(|exprs, _| exprs.alloc(Expr::Unit));
        let graph = build_control_flow_graph("MyFunc", &body);
        assert_eq!(graph.nodes.len(), 1);
        assert!(matches!(
            graph.nodes.values().next().unwrap().node_type,
            NodeType::FunctionRoot
        ));
    }

    #[test]
    fn llm_function_has_two_nodes() {
        let graph = build_llm_control_flow_graph("MyLlm", "gpt-4");
        assert_eq!(graph.nodes.len(), 2);
        let root = graph.nodes.get(&NodeId::new(0)).unwrap();
        assert!(matches!(root.node_type, NodeType::FunctionRoot));
        let scope = graph.nodes.get(&NodeId::new(1)).unwrap();
        assert!(matches!(scope.node_type, NodeType::OtherScope));
        assert_eq!(scope.label, "LLM client: gpt-4");
    }

    #[test]
    fn single_header_creates_header_node() {
        let body = make_body(|exprs, _| {
            let header = exprs.alloc(Expr::NotifyBlock {
                name: "Setup".into(),
                level: 1,
            });
            let unit = exprs.alloc(Expr::Unit);
            exprs.alloc(Expr::Seq {
                first: header,
                second: unit,
            })
        });
        let graph = build_control_flow_graph("Func", &body);
        assert_eq!(graph.nodes.len(), 2); // root + header
        let header = graph.nodes.get(&NodeId::new(1)).unwrap();
        assert!(matches!(header.node_type, NodeType::HeaderContextEnter));
        assert_eq!(header.label, "Setup");
    }

    #[test]
    fn if_else_creates_branch_group_and_arms() {
        let body = make_body(|exprs, _| {
            let cond = exprs.alloc(Expr::Literal(Literal::Bool(true)));
            let then_b = exprs.alloc(Expr::Unit);
            let else_b = exprs.alloc(Expr::Unit);
            exprs.alloc(Expr::If {
                condition: cond,
                then_branch: then_b,
                else_branch: Some(else_b),
            })
        });
        let graph = build_control_flow_graph("Func", &body);
        // Root + BranchGroup + 2 BranchArms
        assert_eq!(graph.nodes.len(), 4);
    }

    #[test]
    fn while_loop_creates_loop_node() {
        let body = make_body(|exprs, _| {
            let cond = exprs.alloc(Expr::Literal(Literal::Bool(true)));
            let body_expr = exprs.alloc(Expr::Unit);
            exprs.alloc(Expr::While {
                condition: cond,
                body: body_expr,
            })
        });
        let graph = build_control_flow_graph("Func", &body);
        assert_eq!(graph.nodes.len(), 2); // root + loop
        let loop_node = graph.nodes.get(&NodeId::new(1)).unwrap();
        assert!(matches!(loop_node.node_type, NodeType::Loop));
    }

    #[test]
    fn if_without_else_gets_synthetic_else() {
        let body = make_body(|exprs, _| {
            let cond = exprs.alloc(Expr::Literal(Literal::Bool(true)));
            let then_b = exprs.alloc(Expr::Unit);
            exprs.alloc(Expr::If {
                condition: cond,
                then_branch: then_b,
                else_branch: None,
            })
        });
        let graph = build_control_flow_graph("Func", &body);
        // Root + BranchGroup + 2 BranchArms (then + synthetic else)
        assert_eq!(graph.nodes.len(), 4);
        // Find the "else" arm
        let else_arm = graph
            .nodes
            .values()
            .find(|n| n.label == "else")
            .expect("should have synthetic else arm");
        assert!(matches!(else_arm.node_type, NodeType::BranchArm));
    }

    #[test]
    fn else_if_chain_flattened_into_single_branch_group() {
        let body = make_body(|exprs, _| {
            let cond1 = exprs.alloc(Expr::Literal(Literal::Bool(true)));
            let then1 = exprs.alloc(Expr::Unit);
            let cond2 = exprs.alloc(Expr::Literal(Literal::Bool(false)));
            let then2 = exprs.alloc(Expr::Unit);
            let else_final = exprs.alloc(Expr::Unit);

            // else if (false) { } else { }
            let inner_if = exprs.alloc(Expr::If {
                condition: cond2,
                then_branch: then2,
                else_branch: Some(else_final),
            });

            // if (true) { } else if (false) { } else { }
            exprs.alloc(Expr::If {
                condition: cond1,
                then_branch: then1,
                else_branch: Some(inner_if),
            })
        });
        let graph = build_control_flow_graph("Func", &body);
        // Root + 1 BranchGroup + 3 BranchArms (if, else if, else)
        assert_eq!(graph.nodes.len(), 5);
        // Only one BranchGroup
        let groups: Vec<_> = graph
            .nodes
            .values()
            .filter(|n| matches!(n.node_type, NodeType::BranchGroup))
            .collect();
        assert_eq!(groups.len(), 1);
    }

    #[test]
    fn match_creates_branch_group_with_arms() {
        let body = make_body(|exprs, patterns| {
            let scrutinee = exprs.alloc(Expr::Var("x".into()));
            let pat1 = patterns.alloc(Pattern::Literal(Literal::Int(1)));
            let pat2 = patterns.alloc(Pattern::Literal(Literal::Int(2)));
            let body1 = exprs.alloc(Expr::Unit);
            let body2 = exprs.alloc(Expr::Unit);
            exprs.alloc(Expr::Match {
                scrutinee,
                arms: vec![
                    MatchArm {
                        pattern: pat1,
                        guard: None,
                        body: body1,
                    },
                    MatchArm {
                        pattern: pat2,
                        guard: None,
                        body: body2,
                    },
                ],
                is_exhaustive: true,
            })
        });
        let graph = build_control_flow_graph("Func", &body);
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
        let body = make_body(|exprs, _| {
            let h1 = exprs.alloc(Expr::NotifyBlock {
                name: "First".into(),
                level: 1,
            });
            let h2 = exprs.alloc(Expr::NotifyBlock {
                name: "Second".into(),
                level: 1,
            });
            let unit = exprs.alloc(Expr::Unit);
            let seq2 = exprs.alloc(Expr::Seq {
                first: h2,
                second: unit,
            });
            exprs.alloc(Expr::Seq {
                first: h1,
                second: seq2,
            })
        });
        let graph = build_control_flow_graph("Func", &body);
        assert_eq!(graph.nodes.len(), 3); // root + 2 headers
        // Both headers should have root as parent
        let h1 = graph.nodes.get(&NodeId::new(1)).unwrap();
        let h2 = graph.nodes.get(&NodeId::new(2)).unwrap();
        assert_eq!(h1.parent_node_id, Some(NodeId::new(0)));
        assert_eq!(h2.parent_node_id, Some(NodeId::new(0)));
        // And they should be connected by an edge
        let root_edges = graph.edges_by_src.get(&NodeId::new(1));
        assert!(root_edges.is_some());
        assert!(root_edges.unwrap().iter().any(|e| e.dst == NodeId::new(2)));
    }

    #[test]
    fn nested_headers_form_hierarchy() {
        let body = make_body(|exprs, _| {
            let h1 = exprs.alloc(Expr::NotifyBlock {
                name: "Outer".into(),
                level: 1,
            });
            let h2 = exprs.alloc(Expr::NotifyBlock {
                name: "Inner".into(),
                level: 2,
            });
            let unit = exprs.alloc(Expr::Unit);
            let seq2 = exprs.alloc(Expr::Seq {
                first: h2,
                second: unit,
            });
            exprs.alloc(Expr::Seq {
                first: h1,
                second: seq2,
            })
        });
        let graph = build_control_flow_graph("Func", &body);
        assert_eq!(graph.nodes.len(), 3); // root + 2 headers
        let inner = graph.nodes.get(&NodeId::new(2)).unwrap();
        // Inner should be child of Outer
        assert_eq!(inner.parent_node_id, Some(NodeId::new(1)));
    }
}
