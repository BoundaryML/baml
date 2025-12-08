use baml_types::ir_type::TypeIR;
use baml_viz_events::{encode_segments, PathSegment, RuntimeNodeType};
use internal_baml_diagnostics::Span;

use super::viz::{VizNode, VizNodes};
use crate::{hir, thir};

/// Build viz metadata nodes from THIR, mirroring control_flow.rs semantics
/// closely enough for runtime visualization.
pub fn build_viz_nodes(func: &thir::ExprFunction<(Span, Option<TypeIR>)>) -> VizNodes {
    let mut builder = Builder::new(&func.name);
    builder.visit_block(&func.body, BlockBehavior::Inline);
    builder.finish()
}

#[derive(Clone, Default)]
struct Counters {
    header: u16,
    branch_group: u16,
    branch_arm: u16,
    loop_node: u16,
    other_scope: u16,
}

impl Counters {
    fn next(&mut self, kind: CounterKind) -> u16 {
        match kind {
            CounterKind::Header => {
                let current = self.header;
                self.header += 1;
                current
            }
            CounterKind::BranchGroup => {
                let current = self.branch_group;
                self.branch_group += 1;
                current
            }
            CounterKind::BranchArm => {
                let current = self.branch_arm;
                self.branch_arm += 1;
                current
            }
            CounterKind::Loop => {
                let current = self.loop_node;
                self.loop_node += 1;
                current
            }
            CounterKind::OtherScope => {
                let current = self.other_scope;
                self.other_scope += 1;
                current
            }
        }
    }
}

#[derive(Clone, Copy)]
enum CounterKind {
    Header,
    BranchGroup,
    BranchArm,
    Loop,
    OtherScope,
}

#[derive(Clone)]
enum FrameEntry {
    FunctionRoot,
    Header { level: u8 },
    BranchGroup,
    BranchArm,
    Loop,
    OtherScope,
}

impl FrameEntry {
    fn children_are_linear(&self) -> bool {
        matches!(
            self,
            FrameEntry::FunctionRoot
                | FrameEntry::Header { .. }
                | FrameEntry::BranchGroup
                | FrameEntry::BranchArm
                | FrameEntry::OtherScope
        )
    }
}

struct Frame {
    entry: FrameEntry,
    lexical_segment: Option<PathSegment>,
    counters: Counters,
    last_linear_child: Option<String>,
    log_filter_key: String,
}

impl Frame {
    fn new(
        entry: FrameEntry,
        lexical_segment: Option<PathSegment>,
        log_filter_key: String,
    ) -> Self {
        Self {
            entry,
            lexical_segment,
            counters: Counters::default(),
            last_linear_child: None,
            log_filter_key,
        }
    }

    fn next_ordinal(&mut self, kind: CounterKind) -> u16 {
        self.counters.next(kind)
    }
}

struct Builder {
    function_name: String,
    frames: Vec<Frame>,
    nodes: VizNodes,
    next_node_id: u32,
}

impl Builder {
    fn new(function_name: &str) -> Self {
        let mut builder = Self {
            function_name: function_name.to_string(),
            frames: Vec::new(),
            nodes: VizNodes::new(),
            next_node_id: 0,
        };

        let segment = PathSegment::FunctionRoot { ordinal: 0 };
        let log_filter_key = encode_segments(function_name, std::slice::from_ref(&segment));
        let node_id = builder.allocate_node_id();
        builder.nodes.push(VizNode {
            node_id,
            log_filter_key: log_filter_key.clone(),
            parent_log_filter_key: None,
            node_type: RuntimeNodeType::FunctionRoot,
            label: function_name.to_string(),
            header_level: None,
        });

        builder.frames.push(Frame::new(
            FrameEntry::FunctionRoot,
            Some(segment),
            log_filter_key,
        ));
        builder
    }

    fn finish(self) -> VizNodes {
        self.nodes
    }

    fn current_parent_log_filter_key(&self) -> Option<String> {
        self.frames.last().map(|f| f.log_filter_key.clone())
    }

    fn build_log_filter_key(&self, segment: &PathSegment) -> String {
        let mut segments: Vec<PathSegment> = self
            .frames
            .iter()
            .filter_map(|f| f.lexical_segment.clone())
            .collect();
        segments.push(segment.clone());
        encode_segments(&self.function_name, &segments)
    }

    fn allocate_node_id(&mut self) -> u32 {
        let id = self.next_node_id;
        self.next_node_id += 1;
        id
    }

    fn push_child(&mut self, node: VizNode, entry: FrameEntry, segment: PathSegment) {
        let log_filter_key = node.log_filter_key.clone();
        let parent_index = self.frames.len() - 1;
        if self.frames[parent_index].entry.children_are_linear() {
            if let Some(prev) = self.frames[parent_index].last_linear_child.clone() {
                // Edges are not stored here, but we maintain last child to mirror ordering.
                let _ = prev;
            }
            self.frames[parent_index].last_linear_child = Some(log_filter_key.clone());
        }
        self.nodes.push(node);
        self.frames
            .push(Frame::new(entry, Some(segment), log_filter_key));
    }

    fn pop_to(&mut self, depth: usize) {
        while self.frames.len() > depth {
            self.frames.pop();
        }
    }

    fn visit_block(
        &mut self,
        block: &thir::Block<(Span, Option<TypeIR>)>,
        _behavior: BlockBehavior,
    ) {
        let depth = self.frames.len();
        for stmt in &block.statements {
            self.visit_statement(stmt);
        }
        if let Some(expr) = &block.trailing_expr {
            self.visit_expr(expr, BlockBehavior::Inline);
        }
        self.pop_to(depth);
    }

    fn visit_statement(&mut self, stmt: &thir::Statement<(Span, Option<TypeIR>)>) {
        match stmt {
            thir::Statement::HeaderContextEnter(header) => self.enter_header(header),
            thir::Statement::Let { name, value, .. } => {
                let behavior = if matches!(value, thir::Expr::Block(_, _)) {
                    BlockBehavior::Wrap(Some(format!("let {name} = {{ ... }}")))
                } else {
                    BlockBehavior::Inline
                };
                self.visit_expr(value, behavior);
            }
            thir::Statement::Assign { value, .. }
            | thir::Statement::AssignOp { value, .. }
            | thir::Statement::DeclareAndAssign { value, .. } => {
                let behavior = if matches!(value, thir::Expr::Block(_, _)) {
                    BlockBehavior::Wrap(None)
                } else {
                    BlockBehavior::Inline
                };
                self.visit_expr(value, behavior);
            }
            thir::Statement::Expression { expr, .. }
            | thir::Statement::SemicolonExpression { expr, .. } => {
                let behavior = if matches!(expr, thir::Expr::Block(_, _)) {
                    BlockBehavior::Wrap(None)
                } else {
                    BlockBehavior::Inline
                };
                self.visit_expr(expr, behavior);
            }
            thir::Statement::While {
                condition,
                block,
                span,
            } => {
                self.visit_loop(LoopFlavor::While { condition }, block, span.clone());
            }
            thir::Statement::ForLoop {
                identifier,
                iterator,
                block,
                span,
            } => {
                self.visit_loop(
                    LoopFlavor::For {
                        identifier,
                        iterator,
                    },
                    block,
                    span.clone(),
                );
            }
            thir::Statement::CForLoop {
                condition, block, ..
            } => {
                let span = block.span.clone();
                let condition_ref = condition.as_ref();
                self.visit_loop(
                    LoopFlavor::CFor {
                        condition: condition_ref,
                    },
                    block,
                    span,
                );
            }
            thir::Statement::Return { expr, .. } => self.visit_expr(expr, BlockBehavior::Inline),
            thir::Statement::Assert { condition, .. } => {
                self.visit_expr(condition, BlockBehavior::Inline);
            }
            thir::Statement::Break(_)
            | thir::Statement::Continue(_)
            | thir::Statement::Declare { .. }
            | thir::Statement::WatchOptions { .. }
            | thir::Statement::WatchNotify { .. } => {}
        }
    }

    fn visit_expr(&mut self, expr: &thir::Expr<(Span, Option<TypeIR>)>, behavior: BlockBehavior) {
        match expr {
            thir::Expr::If(cond, then_expr, else_expr, meta) => {
                self.visit_if(cond, then_expr, else_expr.as_deref(), meta.0.clone())
            }
            thir::Expr::Block(block, meta) => match behavior {
                BlockBehavior::Wrap(label) => {
                    self.emit_other_scope(block, meta.0.clone(), label.clone());
                }
                BlockBehavior::Inline => {
                    self.visit_block(block, BlockBehavior::Inline);
                }
            },
            _ => {}
        }
    }

    fn visit_if(
        &mut self,
        condition: &thir::Expr<(Span, Option<TypeIR>)>,
        then_expr: &thir::Expr<(Span, Option<TypeIR>)>,
        else_expr: Option<&thir::Expr<(Span, Option<TypeIR>)>>,
        _span: Span,
    ) {
        let parent_depth = self.frames.len();
        let ordinal = self
            .frames
            .last_mut()
            .expect("frame stack")
            .next_ordinal(CounterKind::BranchGroup);
        let label = format!("if ({})", render_expr(condition));
        let slug = slug_or_default(&label, &format!("if-{ordinal}"));
        let segment = PathSegment::BranchGroup { slug, ordinal };
        let log_filter_key = self.build_log_filter_key(&segment);
        let parent = self.current_parent_log_filter_key();
        let node = VizNode {
            node_id: self.allocate_node_id(),
            log_filter_key: log_filter_key.clone(),
            parent_log_filter_key: parent,
            node_type: RuntimeNodeType::BranchGroup,
            label: label.clone(),
            header_level: None,
        };
        self.push_child(node, FrameEntry::BranchGroup, segment);

        let arm_label = format!("if ({})", render_expr(condition));
        self.visit_branch_arm(arm_label, then_expr, expr_span(then_expr));

        let mut current_else = else_expr;
        while let Some(expr) = current_else {
            if let thir::Expr::If(else_cond, then_expr, else_expr, _) = expr {
                let label = format!("else if ({})", render_expr(else_cond));
                self.visit_branch_arm(label, then_expr, expr_span(then_expr));
                current_else = else_expr.as_deref();
            } else {
                self.visit_branch_arm("else".to_string(), expr, expr_span(expr));
                current_else = None;
            }
        }

        self.pop_to(parent_depth);
    }

    fn visit_branch_arm(
        &mut self,
        label: String,
        expr: &thir::Expr<(Span, Option<TypeIR>)>,
        _span: Span,
    ) {
        let parent_depth = self.frames.len();
        let ordinal = self
            .frames
            .last_mut()
            .expect("branch group frame")
            .next_ordinal(CounterKind::BranchArm);
        let slug = slug_or_default(&label, &format!("branch-arm-{ordinal}"));
        let segment = PathSegment::BranchArm { slug, ordinal };
        let log_filter_key = self.build_log_filter_key(&segment);
        let parent = self.current_parent_log_filter_key();
        let node = VizNode {
            node_id: self.allocate_node_id(),
            log_filter_key: log_filter_key.clone(),
            parent_log_filter_key: parent,
            node_type: RuntimeNodeType::BranchArm,
            label,
            header_level: None,
        };
        self.push_child(node, FrameEntry::BranchArm, segment);
        self.visit_expr(expr, BlockBehavior::Inline);
        self.pop_to(parent_depth);
    }

    fn visit_loop(
        &mut self,
        flavor: LoopFlavor<'_>,
        block: &thir::Block<(Span, Option<TypeIR>)>,
        _span: Span,
    ) {
        let parent_depth = self.frames.len();
        let ordinal = self
            .frames
            .last_mut()
            .expect("frame stack")
            .next_ordinal(CounterKind::Loop);
        let label = flavor.label();
        let slug = slug_or_default(&label, &format!("loop-{ordinal}"));
        let segment = PathSegment::Loop { slug, ordinal };
        let log_filter_key = self.build_log_filter_key(&segment);
        let parent = self.current_parent_log_filter_key();
        let node = VizNode {
            node_id: self.allocate_node_id(),
            log_filter_key: log_filter_key.clone(),
            parent_log_filter_key: parent,
            node_type: RuntimeNodeType::Loop,
            label,
            header_level: None,
        };
        self.push_child(node, FrameEntry::Loop, segment);
        self.visit_block(block, BlockBehavior::Inline);
        self.pop_to(parent_depth);
    }

    fn emit_other_scope(
        &mut self,
        block: &thir::Block<(Span, Option<TypeIR>)>,
        _span: Span,
        label: Option<String>,
    ) {
        let parent_depth = self.frames.len();
        let ordinal = self
            .frames
            .last_mut()
            .expect("frame stack")
            .next_ordinal(CounterKind::OtherScope);
        let slug = slug_or_default(
            label.as_deref().unwrap_or(""),
            &format!("other-scope-{ordinal}"),
        );
        let segment = PathSegment::OtherScope { slug, ordinal };
        let log_filter_key = self.build_log_filter_key(&segment);
        let parent = self.current_parent_log_filter_key();
        let node = VizNode {
            node_id: self.allocate_node_id(),
            log_filter_key: log_filter_key.clone(),
            parent_log_filter_key: parent,
            node_type: RuntimeNodeType::OtherScope,
            label: label.unwrap_or_default(),
            header_level: None,
        };
        self.push_child(node, FrameEntry::OtherScope, segment);
        self.visit_block(block, BlockBehavior::Inline);
        self.pop_to(parent_depth);
    }

    fn enter_header(&mut self, header: &hir::HeaderContext) {
        let level = header.level.max(1);
        self.pop_headers_to(level - 1);
        let ordinal = self
            .frames
            .last_mut()
            .expect("frame stack")
            .next_ordinal(CounterKind::Header);
        let slug = slug_or_default(&header.title, &format!("header-{ordinal}"));
        let segment = PathSegment::Header { slug, ordinal };
        let log_filter_key = self.build_log_filter_key(&segment);
        let parent = self.current_parent_log_filter_key();
        let node = VizNode {
            node_id: self.allocate_node_id(),
            log_filter_key: log_filter_key.clone(),
            parent_log_filter_key: parent,
            node_type: RuntimeNodeType::HeaderContextEnter,
            label: header.title.clone(),
            header_level: Some(header.level),
        };
        self.push_child(node, FrameEntry::Header { level }, segment);
    }

    fn pop_headers_to(&mut self, desired_level: u8) {
        while let Some(frame) = self.frames.last() {
            match frame.entry {
                FrameEntry::Header { level } if level > desired_level => {
                    self.frames.pop();
                }
                FrameEntry::Header { .. }
                | FrameEntry::FunctionRoot
                | FrameEntry::BranchGroup
                | FrameEntry::BranchArm
                | FrameEntry::Loop
                | FrameEntry::OtherScope => break,
            }
        }
    }
}

#[derive(Clone)]
enum LoopFlavor<'a> {
    While {
        condition: &'a thir::Expr<(Span, Option<TypeIR>)>,
    },
    For {
        identifier: &'a str,
        iterator: &'a thir::Expr<(Span, Option<TypeIR>)>,
    },
    CFor {
        condition: Option<&'a thir::Expr<(Span, Option<TypeIR>)>>,
    },
}

impl<'a> LoopFlavor<'a> {
    fn label(&self) -> String {
        match self {
            LoopFlavor::While { condition } => format!("while ({})", render_expr(condition)),
            LoopFlavor::For {
                identifier,
                iterator,
            } => {
                format!("for ({identifier} in {})", render_expr(iterator))
            }
            LoopFlavor::CFor { condition } => {
                if let Some(cond) = condition {
                    format!("for ({})", render_expr(cond))
                } else {
                    "for (...)".to_string()
                }
            }
        }
    }
}

#[derive(Clone)]
enum BlockBehavior {
    Inline,
    Wrap(Option<String>),
}

impl BlockBehavior {
    fn wrap(label: Option<String>) -> Self {
        BlockBehavior::Wrap(label)
    }
}

fn expr_span(_expr: &thir::Expr<(Span, Option<TypeIR>)>) -> Span {
    Span::fake()
}

fn render_expr(expr: &thir::Expr<(Span, Option<TypeIR>)>) -> String {
    expr.dump_str()
}

fn slugify(input: &str) -> String {
    let mut slug = String::with_capacity(input.len());
    let mut last_dash = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    slug.trim_matches('-').to_string()
}

fn slug_or_default(label: &str, default: &str) -> String {
    let candidate = slugify(label);
    if candidate.is_empty() {
        default.to_string()
    } else {
        candidate
    }
}
