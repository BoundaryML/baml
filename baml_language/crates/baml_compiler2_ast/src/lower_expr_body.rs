//! CST `ExprFunctionBody` → `(ExprBody, AstSourceMap)`.
//!
//! Adapts the `LoweringContext` from `baml_compiler_hir/src/body.rs` which creates arenas,
//! walks block expressions, etc. Produces `ExprBody` (semantic data) and `AstSourceMap`
//! (parallel span storage) in one pass.

use baml_base::Name;
use baml_compiler_syntax::{SyntaxKind, SyntaxNode};
use la_arena::Arena;
use rowan::ast::AstNode;
use text_size::{TextRange, TextSize};

use crate::{
    LoweringDiagnostic,
    ast::{
        AssignOp, AstSourceMap, BinaryOp, CatchArm, CatchArmId, CatchClause, CatchClauseKind, Expr,
        ExprBody, ExprId, FunctionBodyDef, FunctionDef, LetOrigin, Literal, LoopOrigin, MatchArm,
        MatchArmId, Param, PatId, Pattern, SpannedTypeExpr, SpreadField, Stmt, StmtId, TypeAnnotId,
        TypeExpr, UnaryOp,
    },
};

/// Returns true if `kind` can serve as an identifier token in expression position.
///
/// The parser allows `KW_CLIENT` (and `WORD`) inside `PATH_EXPR` / `FIELD_ACCESS_EXPR`
/// nodes when `client` is used as a variable or field name. This must match
/// exactly what `parse_path_or_ident` accepts; adding a new keyword there
/// requires adding it here too.
fn is_ident_token(kind: SyntaxKind) -> bool {
    kind == SyntaxKind::WORD || kind == SyntaxKind::KW_CLIENT
}

/// Lower a CST `ExprFunctionBody` to an owned `ExprBody` + parallel `AstSourceMap`.
pub(crate) fn lower(
    expr_body: &baml_compiler_syntax::ast::ExprFunctionBody,
    param_names: &[Name],
    diags: &mut Vec<LoweringDiagnostic>,
) -> (ExprBody, AstSourceMap) {
    let mut ctx = LoweringContext::new();

    // Add function parameters to scope tracking (for gensym avoidance)
    for name in param_names {
        ctx.names_in_scope.insert(name.to_string());
    }

    // The EXPR_FUNCTION_BODY contains a BLOCK_EXPR as its child
    let root_expr = expr_body
        .syntax()
        .children()
        .find_map(baml_compiler_syntax::ast::BlockExpr::cast)
        .map(|block| ctx.lower_block_expr(&block));

    let (body, source_map, ctx_diags) = ctx.finish(root_expr);
    diags.extend(ctx_diags);
    (body, source_map)
}

/// Lower a `BLOCK_EXPR` node directly to an owned `ExprBody` + parallel `AstSourceMap`.
///
/// Used by `lower_cst` when synthesizing lambda bodies from `TEST_EXPR_DEF` / `TESTSET_DEF`
/// blocks, where there is no wrapping `EXPR_FUNCTION_BODY` node.
pub(crate) fn lower_block_node(
    block_node: &SyntaxNode,
    param_names: &[Name],
) -> (ExprBody, AstSourceMap, Vec<LoweringDiagnostic>) {
    let mut ctx = LoweringContext::new();
    for name in param_names {
        ctx.names_in_scope.insert(name.to_string());
    }
    let root_expr = baml_compiler_syntax::ast::BlockExpr::cast(block_node.clone())
        .map(|block| ctx.lower_block_expr(&block));
    ctx.finish(root_expr)
}

/// Lower a testset `BLOCK_EXPR` body node to an owned `ExprBody` + `AstSourceMap`.
///
/// The body may contain a mix of regular statements (let bindings, for loops, if conditions)
/// and `TEST_EXPR_DEF` / `TESTSET_DEF` nodes. The latter are converted to
/// `<collector_var>.register_test(...)` / `<collector_var>.register_test_set(...)` calls
/// so that the resulting body is a valid expression body for the testset collector lambda.
///
/// `collector_var` is the name of the `testing.TestSetCollector` parameter in scope.
/// `param_names` are additional parameters to seed `names_in_scope` (e.g. the parent scope).
///
/// The returned body always has a `null` tail expression so the collector lambda satisfies
/// the type checker's expectation that the body evaluates to `null`.
pub(crate) fn lower_testset_block_node(
    block_node: &SyntaxNode,
    collector_var: &Name,
    param_names: &[Name],
) -> (ExprBody, AstSourceMap, Vec<LoweringDiagnostic>) {
    let mut ctx = LoweringContext::new_testset_collector(collector_var.clone());
    ctx.names_in_scope.insert(collector_var.to_string());
    for name in param_names {
        ctx.names_in_scope.insert(name.to_string());
    }
    let range = block_node.text_range();
    let root_expr = baml_compiler_syntax::ast::BlockExpr::cast(block_node.clone()).map(|block| {
        let inner_block_id = ctx.lower_block_expr(&block);
        // Ensure the body ends with `null` so the collector lambda always returns null.
        // We extract the statements from the inner block and rebuild with a null tail.
        // If the inner block already has a tail expression, wrap everything in a new block.
        ctx.ensure_null_tail(inner_block_id, range)
    });
    ctx.finish(root_expr)
}

/// Lower a runner `SyntaxElement` (node or token) into an `ExprId` within the given context.
///
/// If the element is a node (e.g. `CALL_EXPR`, `OBJECT_LITERAL`), delegates to `lower_expr`.
/// If the element is a bare token (e.g. `INTEGER_LITERAL`, `WORD`), lowers inline.
/// Lower a bare token (not wrapped in a CST node) into an `Expr`.
/// Used for runner expressions that are simple literals or identifiers.
fn lower_bare_token_expr(kind: SyntaxKind, text: &str) -> Expr {
    match kind {
        SyntaxKind::INTEGER_LITERAL => {
            if let Ok(v) = text.parse::<i64>() {
                Expr::Literal(Literal::Int(v))
            } else {
                Expr::Missing
            }
        }
        SyntaxKind::FLOAT_LITERAL => Expr::Literal(Literal::Float(text.to_string())),
        k if is_ident_token(k) => match text {
            "null" => Expr::Null,
            "true" => Expr::Literal(Literal::Bool(true)),
            "false" => Expr::Literal(Literal::Bool(false)),
            _ => Expr::Path(vec![Name::new(text)]),
        },
        _ => Expr::Missing,
    }
}

pub(crate) fn lower_runner_element(
    ctx: &mut InitTestContext,
    element: &baml_compiler_syntax::SyntaxElement,
) -> ExprId {
    let span = element.text_range();
    match element {
        rowan::NodeOrToken::Node(node) => ctx.inner.lower_expr(node),
        rowan::NodeOrToken::Token(token) => {
            let expr = lower_bare_token_expr(token.kind(), token.text());
            ctx.inner.alloc_expr(expr, span)
        }
    }
}

/// Context for building the `$init_test` function body.
///
/// Wraps a `LoweringContext` so that runner expressions can be lowered
/// directly into the same arena (no IIFE indirection needed).
pub(crate) struct InitTestContext {
    inner: LoweringContext,
    /// Counter for generating unique synthetic spans for lambda expressions.
    /// Synthesized lambdas all share span `0..0`, which causes the HIR scope
    /// builder and MIR lowering to confuse them. Each lambda gets a unique
    /// synthetic span at offset `(counter * 2)..(counter * 2 + 1)` to make
    /// them distinguishable.
    synthetic_lambda_counter: u32,
}

impl InitTestContext {
    pub(crate) fn new() -> Self {
        let mut inner = LoweringContext::new();
        inner.names_in_scope.insert("registry".to_string());
        Self {
            inner,
            synthetic_lambda_counter: 0,
        }
    }

    /// Generate a unique synthetic span for a lambda expression.
    /// Each call returns a different 1-byte span to ensure that HIR Lambda
    /// scopes can be distinguished by their `range` field.
    pub(crate) fn next_lambda_span(&mut self) -> text_size::TextRange {
        let offset = self.synthetic_lambda_counter;
        self.synthetic_lambda_counter += 1;
        // Use offsets starting at 1 to avoid collision with the default 0..0 span
        // used for the function itself and non-lambda expressions.
        let start = text_size::TextSize::from((offset + 1) * 2);
        let end = start + text_size::TextSize::from(1);
        text_size::TextRange::new(start, end)
    }

    pub(crate) fn alloc_expr(&mut self, expr: Expr, span: text_size::TextRange) -> ExprId {
        self.inner.alloc_expr(expr, span)
    }

    pub(crate) fn alloc_stmt(
        &mut self,
        stmt: Stmt,
        span: text_size::TextRange,
    ) -> crate::ast::StmtId {
        self.inner.alloc_stmt(stmt, span)
    }

    pub(crate) fn finish(
        self,
        root_expr: Option<ExprId>,
    ) -> (ExprBody, AstSourceMap, Vec<LoweringDiagnostic>) {
        self.inner.finish(root_expr)
    }
}

/// Helper enum for building pattern elements during lowering.
enum PatternElement {
    /// Accumulated dotted path segments.
    Segments(Vec<Name>, TextSize),
    /// After seeing DOT: waiting for next word to add to the path.
    SegmentsAwaitingWord(Vec<Name>, TextSize),
    /// Seen `name:` - waiting for type expression
    TypedBindingStart(Name, TextSize),
}

struct LoweringContext {
    exprs: Arena<Expr>,
    stmts: Arena<Stmt>,
    patterns: Arena<Pattern>,
    match_arms: Arena<MatchArm>,
    catch_arms: Arena<CatchArm>,
    type_annotations: Arena<TypeExpr>,
    /// Parallel span storage
    source_map: AstSourceMap,
    /// All names used, for generating unique synthetic variable names.
    names_in_scope: std::collections::HashSet<String>,
    /// When set, `TEST_EXPR_DEF` and `TESTSET_DEF` nodes encountered during block
    /// lowering are converted to `<var>.register_test(...)` / `<var>.register_test_set(...)`
    /// calls using this variable name. This supports dynamic test generation inside
    /// `for`/`if` blocks inside a testset body.
    testset_collector_var: Option<Name>,
    /// Diagnostics accumulated during lowering.
    diags: Vec<LoweringDiagnostic>,
}

impl LoweringContext {
    fn new() -> Self {
        Self {
            exprs: Arena::new(),
            stmts: Arena::new(),
            patterns: Arena::new(),
            match_arms: Arena::new(),
            catch_arms: Arena::new(),
            type_annotations: Arena::new(),
            source_map: AstSourceMap::new(),
            names_in_scope: std::collections::HashSet::new(),
            testset_collector_var: None,
            diags: Vec::new(),
        }
    }

    fn new_testset_collector(collector_var: Name) -> Self {
        let mut ctx = Self::new();
        ctx.testset_collector_var = Some(collector_var);
        ctx
    }

    fn alloc_expr(&mut self, expr: Expr, range: TextRange) -> ExprId {
        let id = self.exprs.alloc(expr);
        self.source_map.expr_spans.alloc(range);
        id
    }

    fn alloc_stmt(&mut self, stmt: Stmt, range: TextRange) -> StmtId {
        let id = self.stmts.alloc(stmt);
        self.source_map.stmt_spans.alloc(range);
        id
    }

    fn alloc_pattern(&mut self, pattern: Pattern, range: TextRange) -> PatId {
        let id = self.patterns.alloc(pattern);
        self.source_map.pattern_spans.alloc(range);
        id
    }

    fn alloc_match_arm(&mut self, arm: MatchArm, range: TextRange) -> MatchArmId {
        let id = self.match_arms.alloc(arm);
        self.source_map.match_arm_spans.alloc(range);
        id
    }

    fn alloc_catch_arm(&mut self, arm: CatchArm, range: TextRange) -> CatchArmId {
        let id = self.catch_arms.alloc(arm);
        self.source_map.catch_arm_spans.alloc(range);
        id
    }

    fn alloc_type_annot(&mut self, ty: TypeExpr, range: TextRange) -> TypeAnnotId {
        let id = self.type_annotations.alloc(ty);
        self.source_map.type_annotation_spans.alloc(range);
        id
    }

    /// Try to lower a bare token (not wrapped in a node) into an expression.
    ///
    /// The parser sometimes emits single identifiers and literals as bare tokens
    /// rather than wrapping them in `PATH_EXPR` or other nodes. This helper handles
    /// those cases so lowering functions that iterate `children_with_tokens()` can
    /// process both nodes and tokens uniformly.
    fn try_lower_bare_token(
        &mut self,
        token: &rowan::SyntaxToken<baml_compiler_syntax::BamlLanguage>,
    ) -> Option<ExprId> {
        let span = token.text_range();
        match token.kind() {
            k if is_ident_token(k) => {
                let text = token.text();
                let expr = match text {
                    "true" => Expr::Literal(Literal::Bool(true)),
                    "false" => Expr::Literal(Literal::Bool(false)),
                    "null" => Expr::Null,
                    _ => Expr::Path(vec![Name::new(text)]),
                };
                Some(self.alloc_expr(expr, span))
            }
            SyntaxKind::INTEGER_LITERAL => {
                let value = token.text().parse::<i64>().unwrap_or(0);
                Some(self.alloc_expr(Expr::Literal(Literal::Int(value)), span))
            }
            SyntaxKind::FLOAT_LITERAL => Some(self.alloc_expr(
                Expr::Literal(Literal::Float(token.text().to_string())),
                span,
            )),
            _ => None,
        }
    }

    /// Ensure a block expression ends with a `null` tail.
    ///
    /// If `block_id` refers to a `Block` with no tail expression, this adds a `null` tail
    /// by constructing a new block expression that reuses the same statements.
    /// If the block already has a non-null tail, this wraps it in a new block that evaluates
    /// the original block as a statement and then returns null.
    fn ensure_null_tail(&mut self, block_id: ExprId, range: TextRange) -> ExprId {
        match self.exprs[block_id].clone() {
            Expr::Block { stmts, tail_expr } => {
                match tail_expr {
                    None => {
                        // No tail — add explicit null tail by allocating a new block
                        let null_id = self.alloc_expr(Expr::Null, range);
                        self.alloc_expr(
                            Expr::Block {
                                stmts,
                                tail_expr: Some(null_id),
                            },
                            range,
                        )
                    }
                    Some(t) if matches!(self.exprs[t], Expr::Null) => {
                        // Already has null tail — return as-is
                        block_id
                    }
                    Some(_) => {
                        // Has a non-null tail expression — keep it as a statement and add null
                        let inner_as_stmt = self.alloc_stmt(Stmt::Expr(block_id), range);
                        let null_id = self.alloc_expr(Expr::Null, range);
                        self.alloc_expr(
                            Expr::Block {
                                stmts: vec![inner_as_stmt],
                                tail_expr: Some(null_id),
                            },
                            range,
                        )
                    }
                }
            }
            _ => {
                // Not a block — wrap in a block with null tail
                let inner_as_stmt = self.alloc_stmt(Stmt::Expr(block_id), range);
                let null_id = self.alloc_expr(Expr::Null, range);
                self.alloc_expr(
                    Expr::Block {
                        stmts: vec![inner_as_stmt],
                        tail_expr: Some(null_id),
                    },
                    range,
                )
            }
        }
    }

    fn finish(
        self,
        root_expr: Option<ExprId>,
    ) -> (ExprBody, AstSourceMap, Vec<LoweringDiagnostic>) {
        let body = ExprBody {
            exprs: self.exprs,
            stmts: self.stmts,
            patterns: self.patterns,
            match_arms: self.match_arms,
            catch_arms: self.catch_arms,
            type_annotations: self.type_annotations,
            root_expr,
        };
        (body, self.source_map, self.diags)
    }

    fn lower_block_expr(&mut self, block: &baml_compiler_syntax::ast::BlockExpr) -> ExprId {
        use baml_compiler_syntax::ast::BlockElement;

        let mut stmts = Vec::new();
        let mut tail_expr = None;

        let elements: Vec<_> = block.elements().collect();

        for (idx, element) in elements.iter().enumerate() {
            let is_last = idx == elements.len() - 1;
            match element {
                BlockElement::Stmt(node) => {
                    let stmt_id = match node.kind() {
                        SyntaxKind::LET_STMT => self.lower_let_stmt(node, false),
                        SyntaxKind::WATCH_LET => self.lower_let_stmt(node, true),
                        SyntaxKind::RETURN_STMT => self.lower_return_stmt(node),
                        SyntaxKind::THROW_STMT => self.lower_throw_stmt(node),
                        SyntaxKind::WHILE_STMT => self.lower_while_stmt(node),
                        SyntaxKind::FOR_EXPR => self.lower_for_stmt(node),
                        SyntaxKind::BREAK_STMT => self.alloc_stmt(Stmt::Break, node.text_range()),
                        SyntaxKind::CONTINUE_STMT => {
                            self.alloc_stmt(Stmt::Continue, node.text_range())
                        }
                        SyntaxKind::TEST_EXPR_DEF => {
                            if self.testset_collector_var.is_some() {
                                let expr_id = self.lower_test_expr_as_register_call(node);
                                self.alloc_stmt(Stmt::Expr(expr_id), node.text_range())
                            } else {
                                // Invalid context — parser already emitted a diagnostic
                                self.alloc_stmt(Stmt::Missing, node.text_range())
                            }
                        }
                        SyntaxKind::TESTSET_DEF => {
                            if self.testset_collector_var.is_some() {
                                let expr_id = self.lower_testset_as_register_call(node);
                                self.alloc_stmt(Stmt::Expr(expr_id), node.text_range())
                            } else {
                                // Invalid context — parser already emitted a diagnostic
                                self.alloc_stmt(Stmt::Missing, node.text_range())
                            }
                        }
                        _ => self.alloc_stmt(Stmt::Missing, node.text_range()),
                    };
                    stmts.push(stmt_id);
                }
                BlockElement::ExprNode(node) => {
                    // First, try to lower as an assignment statement
                    if let Some(stmt_id) = self.try_lower_assignment(node) {
                        stmts.push(stmt_id);
                        continue;
                    }

                    let expr_id = self.lower_expr(node);
                    let has_semicolon = element.has_trailing_semicolon();

                    if is_last && !has_semicolon {
                        tail_expr = Some(expr_id);
                    } else {
                        stmts.push(self.alloc_stmt(Stmt::Expr(expr_id), node.text_range()));
                    }
                }
                BlockElement::ExprToken(token) => {
                    let span = token.text_range();
                    let expr_id = match token.kind() {
                        k if is_ident_token(k) => {
                            let text = token.text();
                            let e = match text {
                                "true" => Expr::Literal(Literal::Bool(true)),
                                "false" => Expr::Literal(Literal::Bool(false)),
                                "null" => Expr::Null,
                                _ => Expr::Path(vec![Name::new(text)]),
                            };
                            self.alloc_expr(e, span)
                        }
                        SyntaxKind::INTEGER_LITERAL => {
                            let value = token.text().parse::<i64>().unwrap_or(0);
                            self.alloc_expr(Expr::Literal(Literal::Int(value)), span)
                        }
                        SyntaxKind::FLOAT_LITERAL => {
                            let text = token.text().to_string();
                            self.alloc_expr(Expr::Literal(Literal::Float(text)), span)
                        }
                        SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL => {
                            let text = token.text().to_string();
                            let content = strip_string_delimiters(&text);
                            self.alloc_expr(Expr::Literal(Literal::String(content)), span)
                        }
                        _ => self.alloc_expr(Expr::Missing, span),
                    };

                    let has_semicolon = element.has_trailing_semicolon();
                    if is_last && !has_semicolon {
                        tail_expr = Some(expr_id);
                    } else {
                        stmts.push(self.alloc_stmt(Stmt::Expr(expr_id), span));
                    }
                }
                BlockElement::HeaderComment(node) => {
                    let stmt_id = self.lower_header_comment(node);
                    stmts.push(stmt_id);
                }
            }
        }

        self.alloc_expr(
            Expr::Block { stmts, tail_expr },
            block.syntax().text_range(),
        )
    }

    fn lower_expr(&mut self, node: &SyntaxNode) -> ExprId {
        match node.kind() {
            SyntaxKind::BINARY_EXPR => self.lower_binary_expr(node),
            SyntaxKind::UNARY_EXPR => self.lower_unary_expr(node),
            SyntaxKind::CALL_EXPR => self.lower_call_expr(node),
            SyntaxKind::IF_EXPR => self.lower_if_expr(node),
            SyntaxKind::MATCH_EXPR => self.lower_match_expr(node),
            SyntaxKind::CATCH_EXPR => self.lower_catch_expr(node),
            SyntaxKind::THROW_EXPR => self.lower_throw_expr(node),
            SyntaxKind::BLOCK_EXPR => {
                if let Some(block) = baml_compiler_syntax::ast::BlockExpr::cast(node.clone()) {
                    self.lower_block_expr(&block)
                } else {
                    self.alloc_expr(Expr::Missing, node.text_range())
                }
            }
            SyntaxKind::PATH_EXPR => self.lower_path_expr(node),
            SyntaxKind::FIELD_ACCESS_EXPR => self.lower_field_access_expr(node),
            SyntaxKind::ENV_ACCESS_EXPR => self.lower_env_access_expr(node),
            SyntaxKind::INDEX_EXPR => self.lower_index_expr(node),
            SyntaxKind::PAREN_EXPR => {
                if let Some(inner) = node.children().next() {
                    self.lower_expr(&inner)
                } else {
                    self.try_lower_paren_token_content(node)
                        .unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()))
                }
            }
            SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL => {
                self.lower_string_literal(node)
            }
            SyntaxKind::ARRAY_LITERAL => self.lower_array_literal(node),
            SyntaxKind::OBJECT_LITERAL => self.lower_object_literal(node),
            SyntaxKind::MAP_LITERAL => self.lower_map_literal(node),
            SyntaxKind::LAMBDA_EXPR => self.lower_lambda_expr(node),
            _ => {
                if let Some(literal) = self.try_lower_literal_token(node) {
                    literal
                } else {
                    self.alloc_expr(Expr::Missing, node.text_range())
                }
            }
        }
    }

    fn lower_binary_expr(&mut self, node: &SyntaxNode) -> ExprId {
        let mut lhs = None;
        let mut rhs = None;
        let mut op = None;

        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child_node) => {
                    let expr_id = self.lower_expr(&child_node);
                    if lhs.is_none() {
                        lhs = Some(expr_id);
                    } else {
                        rhs = Some(expr_id);
                    }
                }
                rowan::NodeOrToken::Token(token) => {
                    let span = token.text_range();
                    match token.kind() {
                        SyntaxKind::PLUS => op = Some(BinaryOp::Add),
                        SyntaxKind::MINUS => op = Some(BinaryOp::Sub),
                        SyntaxKind::STAR => op = Some(BinaryOp::Mul),
                        SyntaxKind::SLASH => op = Some(BinaryOp::Div),
                        SyntaxKind::PERCENT => op = Some(BinaryOp::Mod),
                        SyntaxKind::EQUALS_EQUALS => op = Some(BinaryOp::Eq),
                        SyntaxKind::NOT_EQUALS => op = Some(BinaryOp::Ne),
                        SyntaxKind::LESS => op = Some(BinaryOp::Lt),
                        SyntaxKind::LESS_EQUALS => op = Some(BinaryOp::Le),
                        SyntaxKind::GREATER => op = Some(BinaryOp::Gt),
                        SyntaxKind::GREATER_EQUALS => op = Some(BinaryOp::Ge),
                        SyntaxKind::AND_AND => op = Some(BinaryOp::And),
                        SyntaxKind::OR_OR => op = Some(BinaryOp::Or),
                        SyntaxKind::AND => op = Some(BinaryOp::BitAnd),
                        SyntaxKind::PIPE => op = Some(BinaryOp::BitOr),
                        SyntaxKind::CARET => op = Some(BinaryOp::BitXor),
                        SyntaxKind::LESS_LESS => op = Some(BinaryOp::Shl),
                        SyntaxKind::GREATER_GREATER => op = Some(BinaryOp::Shr),
                        SyntaxKind::KW_INSTANCEOF => op = Some(BinaryOp::Instanceof),
                        SyntaxKind::INTEGER_LITERAL => {
                            let value = token.text().parse::<i64>().unwrap_or(0);
                            let expr_id = self.alloc_expr(Expr::Literal(Literal::Int(value)), span);
                            if lhs.is_none() {
                                lhs = Some(expr_id);
                            } else {
                                rhs = Some(expr_id);
                            }
                        }
                        SyntaxKind::FLOAT_LITERAL => {
                            let expr_id = self.alloc_expr(
                                Expr::Literal(Literal::Float(token.text().to_string())),
                                span,
                            );
                            if lhs.is_none() {
                                lhs = Some(expr_id);
                            } else {
                                rhs = Some(expr_id);
                            }
                        }
                        k if is_ident_token(k) => {
                            let text = token.text();
                            let expr_id = match text {
                                "true" => self.alloc_expr(Expr::Literal(Literal::Bool(true)), span),
                                "false" => {
                                    self.alloc_expr(Expr::Literal(Literal::Bool(false)), span)
                                }
                                "null" => self.alloc_expr(Expr::Null, span),
                                _ => self.alloc_expr(Expr::Path(vec![Name::new(text)]), span),
                            };
                            if lhs.is_none() {
                                lhs = Some(expr_id);
                            } else {
                                rhs = Some(expr_id);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        let lhs = lhs.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));
        let rhs = rhs.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));
        let op = op.unwrap_or(BinaryOp::Add);

        self.alloc_expr(Expr::Binary { op, lhs, rhs }, node.text_range())
    }

    fn try_lower_assignment(&mut self, node: &SyntaxNode) -> Option<StmtId> {
        if node.kind() != SyntaxKind::BINARY_EXPR {
            return None;
        }

        // Check for an assignment operator first (avoid allocating expressions early)
        let mut assign_op: Option<Option<AssignOp>> = None;

        for child in node.children_with_tokens() {
            if let rowan::NodeOrToken::Token(token) = child {
                match token.kind() {
                    SyntaxKind::EQUALS => assign_op = Some(None),
                    SyntaxKind::PLUS_EQUALS => assign_op = Some(Some(AssignOp::Add)),
                    SyntaxKind::MINUS_EQUALS => assign_op = Some(Some(AssignOp::Sub)),
                    SyntaxKind::STAR_EQUALS => assign_op = Some(Some(AssignOp::Mul)),
                    SyntaxKind::SLASH_EQUALS => assign_op = Some(Some(AssignOp::Div)),
                    SyntaxKind::PERCENT_EQUALS => assign_op = Some(Some(AssignOp::Mod)),
                    SyntaxKind::AND_EQUALS => assign_op = Some(Some(AssignOp::BitAnd)),
                    SyntaxKind::PIPE_EQUALS => assign_op = Some(Some(AssignOp::BitOr)),
                    SyntaxKind::CARET_EQUALS => assign_op = Some(Some(AssignOp::BitXor)),
                    SyntaxKind::LESS_LESS_EQUALS => assign_op = Some(Some(AssignOp::Shl)),
                    SyntaxKind::GREATER_GREATER_EQUALS => assign_op = Some(Some(AssignOp::Shr)),
                    _ => {}
                }
            }
        }

        let assign_op = assign_op?;

        let mut lhs: Option<ExprId> = None;
        let mut rhs: Option<ExprId> = None;

        for child in node.children_with_tokens() {
            match child {
                rowan::NodeOrToken::Node(n) => {
                    let expr_id = self.lower_expr(&n);
                    if lhs.is_none() {
                        lhs = Some(expr_id);
                    } else {
                        rhs = Some(expr_id);
                    }
                }
                rowan::NodeOrToken::Token(token) => {
                    let span = token.text_range();
                    match token.kind() {
                        SyntaxKind::INTEGER_LITERAL => {
                            let value = token.text().parse::<i64>().unwrap_or(0);
                            let expr_id = self.alloc_expr(Expr::Literal(Literal::Int(value)), span);
                            if lhs.is_none() {
                                lhs = Some(expr_id);
                            } else {
                                rhs = Some(expr_id);
                            }
                        }
                        k if is_ident_token(k) => {
                            let text = token.text();
                            let expr_id = match text {
                                "true" => self.alloc_expr(Expr::Literal(Literal::Bool(true)), span),
                                "false" => {
                                    self.alloc_expr(Expr::Literal(Literal::Bool(false)), span)
                                }
                                "null" => self.alloc_expr(Expr::Null, span),
                                _ => self.alloc_expr(Expr::Path(vec![Name::new(text)]), span),
                            };
                            if lhs.is_none() {
                                lhs = Some(expr_id);
                            } else {
                                rhs = Some(expr_id);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        let target = lhs.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));
        let value = rhs.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));

        let stmt = match assign_op {
            None => Stmt::Assign { target, value },
            Some(op) => Stmt::AssignOp { target, op, value },
        };

        Some(self.alloc_stmt(stmt, node.text_range()))
    }

    fn lower_unary_expr(&mut self, node: &SyntaxNode) -> ExprId {
        let mut op = None;
        let mut operand = None;
        let mut double_op = false;

        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child_node) => {
                    operand = Some(self.lower_expr(&child_node));
                }
                rowan::NodeOrToken::Token(token) => {
                    let span = token.text_range();
                    match token.kind() {
                        SyntaxKind::NOT => op = Some(UnaryOp::Not),
                        SyntaxKind::MINUS => op = Some(UnaryOp::Neg),
                        SyntaxKind::MINUS_MINUS => {
                            op = Some(UnaryOp::Neg);
                            double_op = true;
                        }
                        SyntaxKind::INTEGER_LITERAL => {
                            let value = token.text().parse::<i64>().unwrap_or(0);
                            operand =
                                Some(self.alloc_expr(Expr::Literal(Literal::Int(value)), span));
                        }
                        SyntaxKind::FLOAT_LITERAL => {
                            operand = Some(self.alloc_expr(
                                Expr::Literal(Literal::Float(token.text().to_string())),
                                span,
                            ));
                        }
                        k if is_ident_token(k) => {
                            let text = token.text();
                            let expr_id = match text {
                                "true" => self.alloc_expr(Expr::Literal(Literal::Bool(true)), span),
                                "false" => {
                                    self.alloc_expr(Expr::Literal(Literal::Bool(false)), span)
                                }
                                "null" => self.alloc_expr(Expr::Null, span),
                                _ => self.alloc_expr(Expr::Path(vec![Name::new(text)]), span),
                            };
                            operand = Some(expr_id);
                        }
                        _ => {}
                    }
                }
            }
        }

        let expr = operand.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));

        let Some(op) = op else {
            return expr;
        };

        let result = self.alloc_expr(Expr::Unary { op, expr }, node.text_range());

        if double_op {
            self.alloc_expr(Expr::Unary { op, expr: result }, node.text_range())
        } else {
            result
        }
    }

    fn lower_if_expr(&mut self, node: &SyntaxNode) -> ExprId {
        // Collect sub-expressions from both child nodes and bare tokens
        let mut sub_exprs = Vec::new();
        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child) => {
                    sub_exprs.push(self.lower_expr(&child));
                }
                rowan::NodeOrToken::Token(token) => {
                    if let Some(expr_id) = self.try_lower_bare_token(&token) {
                        sub_exprs.push(expr_id);
                    }
                }
            }
        }

        let condition = sub_exprs
            .first()
            .copied()
            .unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));

        let then_branch = sub_exprs
            .get(1)
            .copied()
            .unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));

        let else_branch = sub_exprs.get(2).copied();

        self.alloc_expr(
            Expr::If {
                condition,
                then_branch,
                else_branch,
            },
            node.text_range(),
        )
    }

    fn lower_match_expr(&mut self, node: &SyntaxNode) -> ExprId {
        let mut scrutinee = None;
        let mut scrutinee_type = None;
        let mut arm_ids = Vec::new();

        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child) => match child.kind() {
                    SyntaxKind::MATCH_ARM => {
                        let arm_id = self.lower_match_arm(&child);
                        arm_ids.push(arm_id);
                    }
                    SyntaxKind::TYPE_EXPR => {
                        if let Some(type_expr) =
                            baml_compiler_syntax::ast::TypeExpr::cast(child.clone())
                        {
                            let span = child.text_range();
                            let ty = crate::lower_type_expr::lower_type_expr_node(&type_expr);
                            scrutinee_type = Some(self.alloc_type_annot(ty, span));
                        }
                    }
                    _ => {
                        if scrutinee.is_none() {
                            scrutinee = Some(self.lower_expr(&child));
                        }
                    }
                },
                rowan::NodeOrToken::Token(token) => {
                    if scrutinee.is_none() {
                        let span = token.text_range();
                        match token.kind() {
                            SyntaxKind::INTEGER_LITERAL => {
                                let value = token.text().parse::<i64>().unwrap_or(0);
                                scrutinee =
                                    Some(self.alloc_expr(Expr::Literal(Literal::Int(value)), span));
                            }
                            k if is_ident_token(k) => {
                                let text = token.text();
                                let e = match text {
                                    "true" => Expr::Literal(Literal::Bool(true)),
                                    "false" => Expr::Literal(Literal::Bool(false)),
                                    "null" => Expr::Null,
                                    _ => Expr::Path(vec![Name::new(text)]),
                                };
                                scrutinee = Some(self.alloc_expr(e, span));
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        let scrutinee =
            scrutinee.unwrap_or_else(|| self.alloc_expr(Expr::Missing, TextRange::default()));

        self.alloc_expr(
            Expr::Match {
                scrutinee,
                scrutinee_type,
                arms: arm_ids,
            },
            node.text_range(),
        )
    }

    fn lower_match_arm(&mut self, node: &SyntaxNode) -> MatchArmId {
        let arm_span = node.text_range();
        let mut pattern = None;
        let mut guard = None;
        let mut body = None;
        let mut seen_fat_arrow = false;

        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child) => match child.kind() {
                    SyntaxKind::MATCH_PATTERN => {
                        pattern = Some(self.lower_match_pattern(&child));
                    }
                    SyntaxKind::MATCH_GUARD => {
                        if let Some(expr_node) = child.children().next() {
                            guard = Some(self.lower_expr(&expr_node));
                        } else {
                            for tok in child.children_with_tokens() {
                                if let rowan::NodeOrToken::Token(t) = tok {
                                    match t.kind() {
                                        SyntaxKind::KW_IF => continue,
                                        k if is_ident_token(k) => {
                                            let text = t.text();
                                            let range = t.text_range();
                                            let e = match text {
                                                "true" => Expr::Literal(Literal::Bool(true)),
                                                "false" => Expr::Literal(Literal::Bool(false)),
                                                "null" => Expr::Null,
                                                _ => Expr::Path(vec![Name::new(text)]),
                                            };
                                            guard = Some(self.alloc_expr(e, range));
                                            break;
                                        }
                                        SyntaxKind::INTEGER_LITERAL => {
                                            let value = t.text().parse::<i64>().unwrap_or(0);
                                            guard = Some(self.alloc_expr(
                                                Expr::Literal(Literal::Int(value)),
                                                t.text_range(),
                                            ));
                                            break;
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                    SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL
                        if seen_fat_arrow && body.is_none() =>
                    {
                        body = Some(self.lower_string_literal(&child));
                    }
                    _ => {
                        if seen_fat_arrow && body.is_none() {
                            body = Some(self.lower_expr(&child));
                        }
                    }
                },
                rowan::NodeOrToken::Token(token) => match token.kind() {
                    SyntaxKind::FAT_ARROW => {
                        seen_fat_arrow = true;
                    }
                    SyntaxKind::INTEGER_LITERAL if seen_fat_arrow && body.is_none() => {
                        let value = token.text().parse::<i64>().unwrap_or(0);
                        body = Some(
                            self.alloc_expr(Expr::Literal(Literal::Int(value)), token.text_range()),
                        );
                    }
                    SyntaxKind::FLOAT_LITERAL if seen_fat_arrow && body.is_none() => {
                        let text = token.text().to_string();
                        body =
                            Some(self.alloc_expr(
                                Expr::Literal(Literal::Float(text)),
                                token.text_range(),
                            ));
                    }
                    SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL
                        if seen_fat_arrow && body.is_none() =>
                    {
                        let content = strip_string_delimiters(token.text());
                        body = Some(self.alloc_expr(
                            Expr::Literal(Literal::String(content)),
                            token.text_range(),
                        ));
                    }
                    k if is_ident_token(k) && seen_fat_arrow && body.is_none() => {
                        let text = token.text();
                        let range = token.text_range();
                        let e = match text {
                            "true" => Expr::Literal(Literal::Bool(true)),
                            "false" => Expr::Literal(Literal::Bool(false)),
                            "null" => Expr::Null,
                            _ => Expr::Path(vec![Name::new(text)]),
                        };
                        body = Some(self.alloc_expr(e, range));
                    }
                    _ => {}
                },
            }
        }

        let arm = MatchArm {
            pattern: pattern
                .unwrap_or_else(|| self.patterns.alloc(Pattern::Binding(Name::new("_")))),
            guard,
            body: body.unwrap_or_else(|| self.exprs.alloc(Expr::Missing)),
        };

        self.alloc_match_arm(arm, arm_span)
    }

    fn lower_match_pattern(&mut self, node: &SyntaxNode) -> PatId {
        let mut elements: Vec<PatId> = Vec::new();
        let mut current_element: Option<PatternElement> = None;
        let mut pending_negation = false;

        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Token(token) => {
                    match token.kind() {
                        SyntaxKind::PIPE => {
                            if let Some(el) = current_element.take() {
                                elements.push(self.finalize_pattern_element(el));
                            }
                        }
                        SyntaxKind::MINUS => {
                            pending_negation = true;
                        }
                        k if is_ident_token(k) => {
                            let text = token.text().to_string();

                            if let Some(PatternElement::SegmentsAwaitingWord(mut segs, start)) =
                                current_element.take()
                            {
                                segs.push(Name::new(&text));
                                current_element = Some(PatternElement::Segments(segs, start));
                                continue;
                            }

                            if let Some(PatternElement::TypedBindingStart(name, _start)) =
                                current_element.take()
                            {
                                // After `name:`, we expect the type to be a node child (TYPE_EXPR),
                                // but sometimes parser emits it as a WORD token directly.
                                // Treat it as a named type.
                                let pat = Pattern::TypedBinding {
                                    name,
                                    ty: crate::ast::TypeExpr::Path {
                                        segments: vec![Name::new(&text)],
                                        attrs: vec![],
                                    },
                                };
                                elements.push(self.alloc_pattern(pat, token.text_range()));
                                continue;
                            }

                            match text.as_str() {
                                "true" => {
                                    if let Some(el) = current_element.take() {
                                        elements.push(self.finalize_pattern_element(el));
                                    }
                                    elements.push(self.alloc_pattern(
                                        Pattern::Literal(Literal::Bool(true)),
                                        token.text_range(),
                                    ));
                                }
                                "false" => {
                                    if let Some(el) = current_element.take() {
                                        elements.push(self.finalize_pattern_element(el));
                                    }
                                    elements.push(self.alloc_pattern(
                                        Pattern::Literal(Literal::Bool(false)),
                                        token.text_range(),
                                    ));
                                }
                                "null" => {
                                    if let Some(el) = current_element.take() {
                                        elements.push(self.finalize_pattern_element(el));
                                    }
                                    elements.push(
                                        self.alloc_pattern(Pattern::Null, token.text_range()),
                                    );
                                }
                                _ => {
                                    if let Some(el) = current_element.take() {
                                        elements.push(self.finalize_pattern_element(el));
                                    }
                                    current_element = Some(PatternElement::Segments(
                                        vec![Name::new(&text)],
                                        token.text_range().start(),
                                    ));
                                }
                            }
                        }
                        SyntaxKind::DOT => {
                            if let Some(PatternElement::Segments(segs, start)) =
                                current_element.take()
                            {
                                current_element =
                                    Some(PatternElement::SegmentsAwaitingWord(segs, start));
                            }
                        }
                        SyntaxKind::COLON => {
                            if let Some(PatternElement::Segments(segs, start)) =
                                current_element.take()
                            {
                                if segs.len() == 1 {
                                    current_element = Some(PatternElement::TypedBindingStart(
                                        segs.into_iter().next().unwrap(),
                                        start,
                                    ));
                                } else {
                                    // Multi-segment path followed by colon — not valid; treat as binding
                                    let name = segs.last().cloned().unwrap_or(Name::new("_"));
                                    current_element =
                                        Some(PatternElement::TypedBindingStart(name, start));
                                }
                            }
                        }
                        SyntaxKind::INTEGER_LITERAL => {
                            if let Some(el) = current_element.take() {
                                elements.push(self.finalize_pattern_element(el));
                            }
                            let value = token.text().parse::<i64>().unwrap_or(0);
                            let value = if pending_negation { -value } else { value };
                            pending_negation = false;
                            elements.push(self.alloc_pattern(
                                Pattern::Literal(Literal::Int(value)),
                                token.text_range(),
                            ));
                        }
                        SyntaxKind::FLOAT_LITERAL => {
                            if let Some(el) = current_element.take() {
                                elements.push(self.finalize_pattern_element(el));
                            }
                            let text = token.text().to_string();
                            let text = if pending_negation {
                                format!("-{text}")
                            } else {
                                text
                            };
                            pending_negation = false;
                            elements.push(self.alloc_pattern(
                                Pattern::Literal(Literal::Float(text)),
                                token.text_range(),
                            ));
                        }
                        _ => {}
                    }
                }
                rowan::NodeOrToken::Node(child) => {
                    match child.kind() {
                        SyntaxKind::TYPE_EXPR => {
                            // Could be typed binding's type or part of pattern
                            if let Some(PatternElement::TypedBindingStart(name, _)) =
                                current_element.take()
                            {
                                if let Some(type_expr) =
                                    baml_compiler_syntax::ast::TypeExpr::cast(child.clone())
                                {
                                    let ty =
                                        crate::lower_type_expr::lower_type_expr_node(&type_expr);
                                    let pat = Pattern::TypedBinding { name, ty };
                                    elements.push(self.alloc_pattern(pat, child.text_range()));
                                }
                            }
                        }
                        SyntaxKind::STRING_LITERAL => {
                            if let Some(el) = current_element.take() {
                                elements.push(self.finalize_pattern_element(el));
                            }
                            let text = child.text().to_string();
                            let content = strip_string_delimiters(&text);
                            elements.push(self.alloc_pattern(
                                Pattern::Literal(Literal::String(content)),
                                child.text_range(),
                            ));
                        }
                        SyntaxKind::MATCH_PATTERN | SyntaxKind::CATCH_PATTERN => {
                            if let Some(el) = current_element.take() {
                                elements.push(self.finalize_pattern_element(el));
                            }
                            let nested_pat = self.lower_match_pattern(&child);
                            match &self.patterns[nested_pat] {
                                Pattern::Union(sub) => elements.extend(sub.iter().copied()),
                                _ => elements.push(nested_pat),
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        if let Some(el) = current_element.take() {
            elements.push(self.finalize_pattern_element(el));
        }
        let _ = pending_negation; // consumed above

        match elements.len() {
            0 => self.alloc_pattern(Pattern::Binding(Name::new("_")), TextRange::default()),
            1 => elements.remove(0),
            _ => {
                let range = TextRange::default();
                let union_pat = Pattern::Union(elements);
                self.alloc_pattern(union_pat, range)
            }
        }
    }

    fn finalize_pattern_element(&mut self, el: PatternElement) -> PatId {
        match el {
            PatternElement::Segments(segs, start) => {
                let range = TextRange::new(start, start);
                match segs.len() {
                    0 => self.alloc_pattern(Pattern::Binding(Name::new("_")), range),
                    1 => self
                        .alloc_pattern(Pattern::Binding(segs.into_iter().next().unwrap()), range),
                    _ => {
                        // Multi-segment: last is variant, rest form enum name
                        let iter = segs.into_iter();
                        let mut collected = Vec::new();
                        for s in iter {
                            collected.push(s);
                        }
                        let variant = collected.pop().unwrap();
                        let enum_name = Name::new(
                            collected
                                .iter()
                                .map(Name::as_str)
                                .collect::<Vec<_>>()
                                .join("."),
                        );
                        self.alloc_pattern(Pattern::EnumVariant { enum_name, variant }, range)
                    }
                }
            }
            PatternElement::SegmentsAwaitingWord(segs, start) => {
                // Incomplete dotted path (ended with a dot) — treat as binding
                let range = TextRange::new(start, start);
                let name = segs.last().cloned().unwrap_or(Name::new("_"));
                self.alloc_pattern(Pattern::Binding(name), range)
            }
            PatternElement::TypedBindingStart(name, start) => {
                // `name:` with no type — treat as simple binding
                let range = TextRange::new(start, start);
                self.alloc_pattern(Pattern::Binding(name), range)
            }
        }
    }

    fn lower_catch_expr(&mut self, node: &SyntaxNode) -> ExprId {
        use baml_compiler_syntax::SyntaxKind;

        let mut base = None;
        let mut clauses = Vec::new();

        for child in node.children() {
            match child.kind() {
                SyntaxKind::CATCH_CLAUSE => clauses.push(self.lower_catch_clause(&child)),
                _ if base.is_none() => {
                    base = Some(self.lower_expr(&child));
                }
                _ => {}
            }
        }

        let base = base.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));
        self.alloc_expr(Expr::Catch { base, clauses }, node.text_range())
    }

    fn lower_catch_clause(&mut self, node: &SyntaxNode) -> CatchClause {
        use baml_compiler_syntax::SyntaxKind;

        let mut kind = CatchClauseKind::Catch;
        let mut binding = None;
        let mut arms = Vec::new();

        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Token(token) => match token.kind() {
                    SyntaxKind::KW_CATCH => kind = CatchClauseKind::Catch,
                    SyntaxKind::KW_CATCH_ALL => kind = CatchClauseKind::CatchAll,
                    SyntaxKind::WORD if token.text() == "catch_all_panics" => {
                        kind = CatchClauseKind::CatchAllPanics;
                    }
                    _ => {}
                },
                rowan::NodeOrToken::Node(child) => match child.kind() {
                    SyntaxKind::CATCH_PATTERN => {
                        binding = Some(self.lower_catch_pattern(&child));
                    }
                    SyntaxKind::CATCH_ARM => {
                        let arm = self.lower_catch_arm(&child);
                        arms.push(arm);
                    }
                    _ => {}
                },
            }
        }

        CatchClause {
            kind,
            binding: binding.unwrap_or_else(|| {
                self.alloc_pattern(Pattern::Binding(Name::new("_")), node.text_range())
            }),
            arms,
        }
    }

    fn lower_catch_arm(&mut self, node: &SyntaxNode) -> CatchArmId {
        use baml_compiler_syntax::SyntaxKind;

        let mut pattern = None;
        let mut body = None;
        let mut seen_fat_arrow = false;

        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child) => match child.kind() {
                    SyntaxKind::CATCH_PATTERN => {
                        pattern = Some(self.lower_catch_pattern(&child));
                    }
                    SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL
                        if seen_fat_arrow && body.is_none() =>
                    {
                        body = Some(self.lower_string_literal(&child));
                    }
                    _ if seen_fat_arrow && body.is_none() => {
                        body = Some(self.lower_expr(&child));
                    }
                    _ => {}
                },
                rowan::NodeOrToken::Token(token) => match token.kind() {
                    SyntaxKind::FAT_ARROW => seen_fat_arrow = true,
                    SyntaxKind::INTEGER_LITERAL if seen_fat_arrow && body.is_none() => {
                        let value = token.text().parse::<i64>().unwrap_or(0);
                        body = Some(
                            self.alloc_expr(Expr::Literal(Literal::Int(value)), token.text_range()),
                        );
                    }
                    SyntaxKind::FLOAT_LITERAL if seen_fat_arrow && body.is_none() => {
                        body = Some(self.alloc_expr(
                            Expr::Literal(Literal::Float(token.text().to_string())),
                            token.text_range(),
                        ));
                    }
                    SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL
                        if seen_fat_arrow && body.is_none() =>
                    {
                        body = Some(self.alloc_expr(
                            Expr::Literal(Literal::String(strip_string_delimiters(token.text()))),
                            token.text_range(),
                        ));
                    }
                    k if is_ident_token(k) && seen_fat_arrow && body.is_none() => {
                        let expr = match token.text() {
                            "true" => Expr::Literal(Literal::Bool(true)),
                            "false" => Expr::Literal(Literal::Bool(false)),
                            "null" => Expr::Null,
                            _ => Expr::Path(vec![Name::new(token.text())]),
                        };
                        body = Some(self.alloc_expr(expr, token.text_range()));
                    }
                    _ => {}
                },
            }
        }

        let pattern = match pattern {
            Some(pattern) => pattern,
            None => self.alloc_pattern(Pattern::Binding(Name::new("_")), node.text_range()),
        };
        let body = match body {
            Some(body) => body,
            None => self.alloc_expr(Expr::Missing, node.text_range()),
        };

        self.alloc_catch_arm(CatchArm { pattern, body }, node.text_range())
    }

    fn lower_catch_pattern(&mut self, node: &SyntaxNode) -> PatId {
        self.lower_match_pattern(node)
    }

    fn lower_throw_expr(&mut self, node: &SyntaxNode) -> ExprId {
        let value = if let Some(child) = node.children().next() {
            self.lower_expr(&child)
        } else {
            self.lower_throw_value_token(node)
                .unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()))
        };
        self.alloc_expr(Expr::Throw { value }, node.text_range())
    }

    fn lower_throw_stmt(&mut self, node: &SyntaxNode) -> StmtId {
        use baml_compiler_syntax::SyntaxKind;

        let expr_child = node.children().find(|c| {
            matches!(
                c.kind(),
                SyntaxKind::THROW_EXPR
                    | SyntaxKind::CATCH_EXPR
                    | SyntaxKind::EXPR
                    | SyntaxKind::BINARY_EXPR
                    | SyntaxKind::UNARY_EXPR
                    | SyntaxKind::CALL_EXPR
                    | SyntaxKind::PATH_EXPR
                    | SyntaxKind::FIELD_ACCESS_EXPR
                    | SyntaxKind::ENV_ACCESS_EXPR
                    | SyntaxKind::INDEX_EXPR
                    | SyntaxKind::IF_EXPR
                    | SyntaxKind::MATCH_EXPR
                    | SyntaxKind::BLOCK_EXPR
                    | SyntaxKind::PAREN_EXPR
                    | SyntaxKind::STRING_LITERAL
                    | SyntaxKind::RAW_STRING_LITERAL
                    | SyntaxKind::OBJECT_LITERAL
                    | SyntaxKind::ARRAY_LITERAL
                    | SyntaxKind::MAP_LITERAL
            )
        });

        if let Some(child) = expr_child.clone() {
            if child.kind() != SyntaxKind::THROW_EXPR {
                let expr_id = self.lower_expr(&child);
                return self.alloc_stmt(Stmt::Expr(expr_id), node.text_range());
            }
        }

        let value = expr_child
            .filter(|c| c.kind() == SyntaxKind::THROW_EXPR)
            .map(|throw_expr_node| {
                if let Some(child) = throw_expr_node.children().next() {
                    self.lower_expr(&child)
                } else {
                    self.lower_throw_value_token(&throw_expr_node)
                        .unwrap_or_else(|| {
                            self.alloc_expr(Expr::Missing, throw_expr_node.text_range())
                        })
                }
            })
            .unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));

        self.alloc_stmt(Stmt::Throw { value }, node.text_range())
    }

    fn lower_throw_value_token(&mut self, node: &SyntaxNode) -> Option<ExprId> {
        use baml_compiler_syntax::SyntaxKind;

        for token in node
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
        {
            match token.kind() {
                SyntaxKind::KW_THROW => continue,
                SyntaxKind::INTEGER_LITERAL => {
                    let value = token.text().parse::<i64>().unwrap_or(0);
                    return Some(
                        self.alloc_expr(Expr::Literal(Literal::Int(value)), token.text_range()),
                    );
                }
                SyntaxKind::FLOAT_LITERAL => {
                    return Some(self.alloc_expr(
                        Expr::Literal(Literal::Float(token.text().to_string())),
                        token.text_range(),
                    ));
                }
                SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL => {
                    return Some(self.alloc_expr(
                        Expr::Literal(Literal::String(strip_string_delimiters(token.text()))),
                        token.text_range(),
                    ));
                }
                k if is_ident_token(k) => {
                    let expr = match token.text() {
                        "true" => Expr::Literal(Literal::Bool(true)),
                        "false" => Expr::Literal(Literal::Bool(false)),
                        "null" => Expr::Null,
                        _ => Expr::Path(vec![Name::new(token.text())]),
                    };
                    return Some(self.alloc_expr(expr, token.text_range()));
                }
                _ => {}
            }
        }
        None
    }

    fn lower_call_expr(&mut self, node: &SyntaxNode) -> ExprId {
        // CALL_EXPR structure: callee expr node (or WORD token), then CALL_ARGS node
        let callee_node = node.children().find(|n| n.kind() != SyntaxKind::CALL_ARGS);

        let callee = if let Some(n) = callee_node {
            self.lower_expr(&n)
        } else {
            // No callee node - check for an identifier token (simple function name)
            let word_token = node
                .children_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .find(|t| is_ident_token(t.kind()));

            if let Some(token) = word_token {
                self.alloc_expr(
                    Expr::Path(vec![Name::new(token.text())]),
                    token.text_range(),
                )
            } else {
                self.alloc_expr(Expr::Missing, node.text_range())
            }
        };

        // Find CALL_ARGS node and extract arguments
        let args = node
            .children()
            .find(|n| n.kind() == SyntaxKind::CALL_ARGS)
            .map(|args_node| {
                let mut args = Vec::new();
                for element in args_node.children_with_tokens() {
                    match element {
                        rowan::NodeOrToken::Node(child_node) => {
                            // Skip COMMA and other punctuation nodes if any
                            if is_expr_node_kind(child_node.kind()) {
                                args.push(self.lower_expr(&child_node));
                            }
                        }
                        rowan::NodeOrToken::Token(token) => {
                            let span = token.text_range();
                            match token.kind() {
                                SyntaxKind::INTEGER_LITERAL => {
                                    let value = token.text().parse::<i64>().unwrap_or(0);
                                    args.push(
                                        self.alloc_expr(Expr::Literal(Literal::Int(value)), span),
                                    );
                                }
                                SyntaxKind::FLOAT_LITERAL => {
                                    let text = token.text().to_string();
                                    args.push(
                                        self.alloc_expr(Expr::Literal(Literal::Float(text)), span),
                                    );
                                }
                                SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL => {
                                    let content = strip_string_delimiters(token.text());
                                    args.push(
                                        self.alloc_expr(
                                            Expr::Literal(Literal::String(content)),
                                            span,
                                        ),
                                    );
                                }
                                k if is_ident_token(k) => {
                                    let text = token.text();
                                    let e = match text {
                                        "true" => Expr::Literal(Literal::Bool(true)),
                                        "false" => Expr::Literal(Literal::Bool(false)),
                                        "null" => Expr::Null,
                                        _ => Expr::Path(vec![Name::new(text)]),
                                    };
                                    args.push(self.alloc_expr(e, span));
                                }
                                _ => {}
                            }
                        }
                    }
                }
                args
            })
            .unwrap_or_default();

        self.alloc_expr(Expr::Call { callee, args }, node.text_range())
    }

    fn lower_path_expr(&mut self, node: &SyntaxNode) -> ExprId {
        // PATH_EXPR contains WORD (or keyword-as-ident) tokens joined by DOTs.
        let mut segments: Vec<(Name, TextRange)> = Vec::new();

        for elem in node.children_with_tokens() {
            if let rowan::NodeOrToken::Token(token) = elem {
                if is_ident_token(token.kind()) {
                    segments.push((Name::new(token.text()), token.text_range()));
                }
            }
        }

        if segments.is_empty() {
            return self.alloc_expr(Expr::Missing, node.text_range());
        }

        // Check if single segment is a literal keyword
        if segments.len() == 1 {
            match segments[0].0.as_str() {
                "true" => {
                    return self.alloc_expr(Expr::Literal(Literal::Bool(true)), node.text_range());
                }
                "false" => {
                    return self.alloc_expr(Expr::Literal(Literal::Bool(false)), node.text_range());
                }
                "null" => return self.alloc_expr(Expr::Null, node.text_range()),
                _ => {}
            }
        }

        // Desugar multi-segment paths into FieldAccess chains:
        //   Color.Red  → FieldAccess { base: Path(["Color"]), field: "Red" }
        //   a.b.c      → FieldAccess { base: FieldAccess { base: Path(["a"]), field: "b" }, field: "c" }
        // After this, Path is always single-segment (a bare identifier).
        let mut base = self.alloc_expr(Expr::Path(vec![segments[0].0.clone()]), node.text_range());
        for (seg, seg_range) in &segments[1..] {
            let id = self.alloc_expr(
                Expr::FieldAccess {
                    base,
                    field: seg.clone(),
                },
                node.text_range(),
            );
            self.source_map
                .field_access_member_spans
                .insert(id, *seg_range);
            base = id;
        }
        base
    }

    fn lower_field_access_expr(&mut self, node: &SyntaxNode) -> ExprId {
        let mut base = None;
        let mut field = None;
        let mut field_range = None;

        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child) => {
                    if base.is_none() {
                        base = Some(self.lower_expr(&child));
                    }
                }
                rowan::NodeOrToken::Token(token) => {
                    if is_ident_token(token.kind()) && base.is_some() {
                        field = Some(Name::new(token.text()));
                        field_range = Some(token.text_range());
                    }
                }
            }
        }

        let base = base.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));
        let field = field.unwrap_or_else(|| Name::new("_"));

        let id = self.alloc_expr(Expr::FieldAccess { base, field }, node.text_range());
        if let Some(range) = field_range {
            self.source_map.field_access_member_spans.insert(id, range);
        }
        id
    }

    fn lower_env_access_expr(&mut self, node: &SyntaxNode) -> ExprId {
        // Desugar `env.VAR_NAME` → `baml.env.get_or_panic("VAR_NAME")`
        let range = node.text_range();

        let mut field_text = None;
        let mut seen_dot = false;
        for elem in node.children_with_tokens() {
            if let rowan::NodeOrToken::Token(token) = elem {
                if token.kind() == SyntaxKind::DOT {
                    seen_dot = true;
                } else if seen_dot && is_ident_token(token.kind()) {
                    field_text = Some(token.text().to_string());
                    break;
                }
            }
        }

        let var_name = field_text.unwrap_or_else(|| "_".to_string());
        let callee = self.alloc_expr(
            Expr::Path(vec![
                Name::new("baml"),
                Name::new("env"),
                Name::new("get_or_panic"),
            ]),
            range,
        );
        let arg = self.alloc_expr(Expr::Literal(Literal::String(var_name)), range);
        self.alloc_expr(
            Expr::Call {
                callee,
                args: vec![arg],
            },
            range,
        )
    }

    fn lower_index_expr(&mut self, node: &SyntaxNode) -> ExprId {
        let mut base = None;
        let mut index = None;
        let mut seen_lbracket = false;

        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child) => {
                    if !seen_lbracket {
                        if base.is_none() {
                            base = Some(self.lower_expr(&child));
                        }
                    } else if index.is_none() {
                        index = Some(self.lower_expr(&child));
                    }
                }
                rowan::NodeOrToken::Token(token) => {
                    if token.kind() == SyntaxKind::L_BRACKET {
                        seen_lbracket = true;
                    } else if !seen_lbracket && base.is_none() {
                        base = self.try_lower_bare_token(&token);
                    } else if seen_lbracket && index.is_none() {
                        index = self.try_lower_bare_token(&token);
                    }
                }
            }
        }

        let base = base.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));
        let index = index.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));

        self.alloc_expr(Expr::Index { base, index }, node.text_range())
    }

    fn lower_string_literal(&mut self, node: &SyntaxNode) -> ExprId {
        let text = node.text().to_string();
        let content = strip_string_delimiters(&text);
        self.alloc_expr(Expr::Literal(Literal::String(content)), node.text_range())
    }

    fn lower_array_literal(&mut self, node: &SyntaxNode) -> ExprId {
        let mut elements = Vec::new();
        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child) => {
                    elements.push(self.lower_expr(&child));
                }
                rowan::NodeOrToken::Token(token) => {
                    if let Some(expr_id) = self.try_lower_bare_token(&token) {
                        elements.push(expr_id);
                    }
                }
            }
        }
        self.alloc_expr(Expr::Array { elements }, node.text_range())
    }

    fn lower_object_literal(&mut self, node: &SyntaxNode) -> ExprId {
        let mut fields = Vec::new();
        let mut spreads = Vec::new();
        let mut position = 0;
        let mut type_name = None;

        // Look for the optional type name (first WORD or path before the brace).
        // The type name may be:
        //   - A simple WORD token: `MyClass { ... }`
        //   - A qualified path node: `baml.errors.DevOther { ... }` (parsed as PATH_EXPR)
        // For qualified paths, extract the final segment as the class name.
        'outer: for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Token(token) => {
                    if token.kind() == SyntaxKind::L_BRACE {
                        break;
                    }
                    if is_ident_token(token.kind()) && type_name.is_none() {
                        type_name = Some(Name::new(token.text()));
                    }
                }
                rowan::NodeOrToken::Node(child_node) => {
                    // A child node before L_BRACE is the type name path (e.g. PATH_EXPR).
                    // Walk its tokens to find the last WORD — that's the class name.
                    let mut last_word: Option<Name> = None;
                    for token in child_node
                        .children_with_tokens()
                        .filter_map(rowan::NodeOrToken::into_token)
                    {
                        if is_ident_token(token.kind()) {
                            last_word = Some(Name::new(token.text()));
                        }
                    }
                    if let Some(name) = last_word {
                        type_name = Some(name);
                    }
                    // After handling the path node, stop scanning for more pre-brace items.
                    break 'outer;
                }
            }
        }

        // Object fields are child nodes after L_BRACE
        // They come as key-value pairs: WORD COLON expr or SPREAD expr
        for child in node.children() {
            match child.kind() {
                SyntaxKind::OBJECT_FIELD => {
                    // OBJECT_FIELD: WORD COLON expr
                    let mut key = None;
                    let mut val = None;
                    let mut seen_colon = false;
                    for elem in child.children_with_tokens() {
                        match elem {
                            rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::COLON => {
                                seen_colon = true;
                            }
                            rowan::NodeOrToken::Token(t)
                                if is_ident_token(t.kind()) && !seen_colon =>
                            {
                                if key.is_none() {
                                    key = Some(Name::new(t.text()));
                                }
                            }
                            rowan::NodeOrToken::Node(n) if seen_colon && val.is_none() => {
                                val = Some(self.lower_expr(&n));
                            }
                            rowan::NodeOrToken::Token(t) if seen_colon && val.is_none() => {
                                val = self.try_lower_bare_token(&t);
                            }
                            rowan::NodeOrToken::Token(_) => {}
                            rowan::NodeOrToken::Node(_) => {}
                        }
                    }
                    if let (Some(k), Some(val_id)) = (key, val) {
                        fields.push((k, val_id));
                    }
                    position += 1;
                }
                SyntaxKind::SPREAD_ELEMENT => {
                    // SPREAD_ELEMENT: ... expr
                    let spread_expr = if let Some(expr_node) = child.children().next() {
                        Some(self.lower_expr(&expr_node))
                    } else {
                        // Try bare token: ...x where x is a single identifier
                        child
                            .children_with_tokens()
                            .filter_map(rowan::NodeOrToken::into_token)
                            .find_map(|t| self.try_lower_bare_token(&t))
                    };
                    if let Some(expr) = spread_expr {
                        spreads.push(SpreadField { expr, position });
                    }
                    position += 1;
                }
                _ => {}
            }
        }

        self.alloc_expr(
            Expr::Object {
                type_name,
                fields,
                spreads,
            },
            node.text_range(),
        )
    }

    fn lower_map_literal(&mut self, node: &SyntaxNode) -> ExprId {
        // MAP_LITERAL uses OBJECT_FIELD children (same as OBJECT_LITERAL).
        // Each OBJECT_FIELD: key (WORD or expr), COLON, value expr.
        // For maps the key can also be a string literal or expression.
        let entries = node
            .children()
            .filter(|n| n.kind() == SyntaxKind::OBJECT_FIELD)
            .filter_map(|field_node| {
                // Key: first child node that can be an expression, or first WORD token
                let mut key_expr = None;
                let mut val_expr = None;
                let mut seen_colon = false;

                for elem in field_node.children_with_tokens() {
                    match elem {
                        rowan::NodeOrToken::Token(t) => {
                            if t.kind() == SyntaxKind::COLON {
                                seen_colon = true;
                            } else if !seen_colon && key_expr.is_none() && is_ident_token(t.kind())
                            {
                                let span = t.text_range();
                                key_expr = Some(self.alloc_expr(
                                    Expr::Literal(Literal::String(t.text().to_string())),
                                    span,
                                ));
                            } else if !seen_colon
                                && key_expr.is_none()
                                && (t.kind() == SyntaxKind::STRING_LITERAL
                                    || t.kind() == SyntaxKind::RAW_STRING_LITERAL)
                            {
                                let content = strip_string_delimiters(t.text());
                                let span = t.text_range();
                                key_expr = Some(
                                    self.alloc_expr(Expr::Literal(Literal::String(content)), span),
                                );
                            } else if seen_colon && val_expr.is_none() {
                                val_expr = self.try_lower_bare_token(&t);
                            }
                        }
                        rowan::NodeOrToken::Node(n) => {
                            if !seen_colon && key_expr.is_none() {
                                key_expr = Some(self.lower_expr(&n));
                            } else if seen_colon && val_expr.is_none() {
                                val_expr = Some(self.lower_expr(&n));
                            }
                        }
                    }
                }

                match (key_expr, val_expr) {
                    (Some(k), Some(v)) => Some((k, v)),
                    _ => None,
                }
            })
            .collect();

        self.alloc_expr(Expr::Map { entries }, node.text_range())
    }

    fn lower_lambda_expr(&mut self, node: &SyntaxNode) -> ExprId {
        use baml_compiler_syntax::ast;

        // Extract optional generic params: <T>, <K, V>, etc.
        let generic_params = crate::lower_cst::extract_generic_params(node);

        // Lower parameter list — gives us Vec<Param>
        let params = node
            .children()
            .find(|n| n.kind() == SyntaxKind::PARAMETER_LIST)
            .and_then(ast::ParameterList::cast)
            .map(|pl| crate::lower_cst::lower_params(&pl, "<lambda>", &mut self.diags))
            .unwrap_or_default();

        let param_names: Vec<Name> = params.iter().map(|p| p.name.clone()).collect();

        // Lower optional return type: the TYPE_EXPR that is a direct child of the
        // lambda node, appearing after PARAMETER_LIST but before THROWS_CLAUSE/BLOCK_EXPR.
        // We scan children in order, skipping items until after PARAMETER_LIST.
        let return_type = {
            let mut after_params = false;
            let mut found: Option<SpannedTypeExpr> = None;
            for child in node.children() {
                match child.kind() {
                    SyntaxKind::PARAMETER_LIST | SyntaxKind::GENERIC_PARAM_LIST => {
                        after_params = true;
                    }
                    SyntaxKind::THROWS_CLAUSE | SyntaxKind::BLOCK_EXPR => {
                        break;
                    }
                    SyntaxKind::TYPE_EXPR if after_params && found.is_none() => {
                        if let Some(te) = ast::TypeExpr::cast(child.clone()) {
                            found = Some(SpannedTypeExpr {
                                expr: crate::lower_type_expr::lower_type_expr_node(&te),
                                span: child.text_range(),
                            });
                        }
                    }
                    _ => {}
                }
            }
            found
        };

        // Lower optional throws clause
        let throws = node
            .children()
            .find(|n| n.kind() == SyntaxKind::THROWS_CLAUSE)
            .and_then(ast::ThrowsClause::cast)
            .and_then(|tc| tc.type_expr())
            .map(|te| SpannedTypeExpr {
                span: te.syntax().text_range(),
                expr: crate::lower_type_expr::lower_type_expr_node(&te),
            });

        // Lower body via a FRESH LoweringContext — lambda gets its own ExprBody.
        let body = node
            .children()
            .find(|n| n.kind() == SyntaxKind::BLOCK_EXPR)
            .and_then(ast::BlockExpr::cast)
            .map(|block| {
                let mut lambda_ctx = LoweringContext::new();
                for name in &param_names {
                    lambda_ctx.names_in_scope.insert(name.to_string());
                }
                let root_expr = lambda_ctx.lower_block_expr(&block);
                let (body, source_map, lambda_diags) = lambda_ctx.finish(Some(root_expr));
                self.diags.extend(lambda_diags);
                FunctionBodyDef::Expr(body, source_map)
            });

        let func_def = FunctionDef {
            name: Name::new("<anonymous function>"),
            generic_params,
            params,
            return_type,
            throws,
            body,
            declarative_meta: None,
            attributes: Vec::new(),
            span: node.text_range(),
            name_span: node.text_range(), // synthetic: use the lambda span
        };

        self.alloc_expr(Expr::Lambda(Box::new(func_def)), node.text_range())
    }

    fn try_lower_paren_token_content(&mut self, node: &SyntaxNode) -> Option<ExprId> {
        // Look for a single meaningful token inside the parentheses
        for elem in node.children_with_tokens() {
            if let rowan::NodeOrToken::Token(token) = elem {
                let span = token.text_range();
                match token.kind() {
                    k if is_ident_token(k) => {
                        let text = token.text();
                        let e = match text {
                            "true" => Expr::Literal(Literal::Bool(true)),
                            "false" => Expr::Literal(Literal::Bool(false)),
                            "null" => Expr::Null,
                            _ => Expr::Path(vec![Name::new(text)]),
                        };
                        return Some(self.alloc_expr(e, span));
                    }
                    SyntaxKind::INTEGER_LITERAL => {
                        let value = token.text().parse::<i64>().unwrap_or(0);
                        return Some(self.alloc_expr(Expr::Literal(Literal::Int(value)), span));
                    }
                    SyntaxKind::FLOAT_LITERAL => {
                        let text = token.text().to_string();
                        return Some(self.alloc_expr(Expr::Literal(Literal::Float(text)), span));
                    }
                    SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL => {
                        let content = strip_string_delimiters(token.text());
                        return Some(
                            self.alloc_expr(Expr::Literal(Literal::String(content)), span),
                        );
                    }
                    _ => {}
                }
            }
        }
        None
    }

    fn try_lower_literal_token(&mut self, node: &SyntaxNode) -> Option<ExprId> {
        // Check if this node is a single token node that we can treat as a literal
        let mut tokens = node
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|t| !t.kind().is_trivia());

        let token = tokens.next()?;
        if tokens.next().is_some() {
            return None; // Multiple tokens — not a simple literal
        }

        let span = token.text_range();
        match token.kind() {
            SyntaxKind::INTEGER_LITERAL => {
                let value = token.text().parse::<i64>().unwrap_or(0);
                Some(self.alloc_expr(Expr::Literal(Literal::Int(value)), span))
            }
            SyntaxKind::FLOAT_LITERAL => {
                let text = token.text().to_string();
                Some(self.alloc_expr(Expr::Literal(Literal::Float(text)), span))
            }
            k if is_ident_token(k) => {
                let text = token.text();
                let e = match text {
                    "true" => Expr::Literal(Literal::Bool(true)),
                    "false" => Expr::Literal(Literal::Bool(false)),
                    "null" => Expr::Null,
                    _ => Expr::Path(vec![Name::new(text)]),
                };
                Some(self.alloc_expr(e, span))
            }
            _ => None,
        }
    }

    fn lower_let_stmt(&mut self, node: &SyntaxNode, is_watched: bool) -> StmtId {
        let mut pattern_id = None;
        let mut type_annotation = None;
        let mut initializer = None;

        // LET_STMT: KW_LET PATTERN (COLON TYPE)? EQUALS expr SEMICOLON
        // Walk children_with_tokens to find the pattern and initializer
        let mut seen_equals = false;
        let mut seen_colon = false;

        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Token(token) => match token.kind() {
                    SyntaxKind::EQUALS => {
                        seen_equals = true;
                        seen_colon = false;
                    }
                    SyntaxKind::COLON => {
                        seen_colon = true;
                    }
                    SyntaxKind::KW_LET | SyntaxKind::KW_WATCH => {}
                    _ if seen_equals && initializer.is_none() => {
                        // Token-level initializer (e.g. `let x = 1;` where 1 is INTEGER_LITERAL token)
                        let span = token.text_range();
                        match token.kind() {
                            SyntaxKind::INTEGER_LITERAL => {
                                let value = token.text().parse::<i64>().unwrap_or(0);
                                initializer =
                                    Some(self.alloc_expr(Expr::Literal(Literal::Int(value)), span));
                            }
                            SyntaxKind::FLOAT_LITERAL => {
                                let text = token.text().to_string();
                                initializer = Some(
                                    self.alloc_expr(Expr::Literal(Literal::Float(text)), span),
                                );
                            }
                            SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL => {
                                let content = strip_string_delimiters(token.text());
                                initializer = Some(
                                    self.alloc_expr(Expr::Literal(Literal::String(content)), span),
                                );
                            }
                            k if is_ident_token(k) => {
                                let text = token.text();
                                let e = match text {
                                    "true" => Expr::Literal(Literal::Bool(true)),
                                    "false" => Expr::Literal(Literal::Bool(false)),
                                    "null" => Expr::Null,
                                    _ => Expr::Path(vec![Name::new(text)]),
                                };
                                initializer = Some(self.alloc_expr(e, span));
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                },
                rowan::NodeOrToken::Node(child) => {
                    if !seen_equals {
                        if seen_colon {
                            // Type annotation
                            if let Some(type_expr) =
                                baml_compiler_syntax::ast::TypeExpr::cast(child.clone())
                            {
                                let span = child.text_range();
                                let ty = crate::lower_type_expr::lower_type_expr_node(&type_expr);
                                type_annotation = Some(self.alloc_type_annot(ty, span));
                                seen_colon = false;
                            }
                        } else if pattern_id.is_none() {
                            // Pattern comes first, before the colon or equals
                            if child.kind() == SyntaxKind::MATCH_PATTERN {
                                pattern_id = Some(self.lower_match_pattern(&child));
                            } else {
                                // Simple binding in a let — just a WORD token as the pattern
                                // Try to get a name from the node
                                let name = child
                                    .children_with_tokens()
                                    .filter_map(rowan::NodeOrToken::into_token)
                                    .find(|t| is_ident_token(t.kind()))
                                    .map(|t| Name::new(t.text()))
                                    .unwrap_or(Name::new("_"));
                                let range = child.text_range();
                                pattern_id =
                                    Some(self.alloc_pattern(Pattern::Binding(name), range));
                            }
                        }
                    } else if initializer.is_none() {
                        initializer = Some(self.lower_expr(&child));
                    }
                }
            }
        }

        // Also look for a simple WORD pattern in token children (common for `let x = ...`)
        if pattern_id.is_none() {
            let mut seen_let_kw = false;
            for elem in node.children_with_tokens() {
                if let rowan::NodeOrToken::Token(token) = elem {
                    match token.kind() {
                        SyntaxKind::KW_LET | SyntaxKind::KW_WATCH => {
                            seen_let_kw = true;
                        }
                        k if is_ident_token(k) && seen_let_kw && pattern_id.is_none() => {
                            let range = token.text_range();
                            pattern_id =
                                Some(self.alloc_pattern(
                                    Pattern::Binding(Name::new(token.text())),
                                    range,
                                ));
                        }
                        SyntaxKind::EQUALS | SyntaxKind::COLON => break,
                        _ => {}
                    }
                }
            }
        }

        let pattern = pattern_id.unwrap_or_else(|| {
            self.alloc_pattern(Pattern::Binding(Name::new("_")), TextRange::default())
        });

        let origin = if is_watched {
            // TODO: Handle watched let statements
            LetOrigin::Source
        } else {
            LetOrigin::Source
        };

        self.alloc_stmt(
            Stmt::Let {
                pattern,
                type_annotation,
                initializer,
                is_watched,
                origin,
            },
            node.text_range(),
        )
    }

    fn lower_return_stmt(&mut self, node: &SyntaxNode) -> StmtId {
        // RETURN_STMT: KW_RETURN expr?
        // Try child nodes first, then fall back to token-level expressions
        let expr = if let Some(child_node) = node.children().next() {
            Some(self.lower_expr(&child_node))
        } else {
            // No child node — check for a token-level expression (e.g. `return 1;`)
            let mut result = None;
            for elem in node.children_with_tokens() {
                if let rowan::NodeOrToken::Token(token) = elem {
                    let span = token.text_range();
                    match token.kind() {
                        SyntaxKind::KW_RETURN | SyntaxKind::SEMICOLON => continue,
                        SyntaxKind::INTEGER_LITERAL => {
                            let value = token.text().parse::<i64>().unwrap_or(0);
                            result =
                                Some(self.alloc_expr(Expr::Literal(Literal::Int(value)), span));
                            break;
                        }
                        SyntaxKind::FLOAT_LITERAL => {
                            let text = token.text().to_string();
                            result =
                                Some(self.alloc_expr(Expr::Literal(Literal::Float(text)), span));
                            break;
                        }
                        SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL => {
                            let content = strip_string_delimiters(token.text());
                            result = Some(
                                self.alloc_expr(Expr::Literal(Literal::String(content)), span),
                            );
                            break;
                        }
                        k if is_ident_token(k) => {
                            let text = token.text();
                            let e = match text {
                                "true" => Expr::Literal(Literal::Bool(true)),
                                "false" => Expr::Literal(Literal::Bool(false)),
                                "null" => Expr::Null,
                                _ => Expr::Path(vec![Name::new(text)]),
                            };
                            result = Some(self.alloc_expr(e, span));
                            break;
                        }
                        _ => {}
                    }
                }
            }
            result
        };
        self.alloc_stmt(Stmt::Return(expr), node.text_range())
    }

    fn lower_while_stmt(&mut self, node: &SyntaxNode) -> StmtId {
        let mut sub_exprs = Vec::new();
        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child) => {
                    sub_exprs.push(self.lower_expr(&child));
                }
                rowan::NodeOrToken::Token(token) => {
                    if let Some(expr_id) = self.try_lower_bare_token(&token) {
                        sub_exprs.push(expr_id);
                    }
                }
            }
        }

        let condition = sub_exprs
            .first()
            .copied()
            .unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));
        let body = sub_exprs
            .get(1)
            .copied()
            .unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));

        self.alloc_stmt(
            Stmt::While {
                condition,
                body,
                after: None,
                origin: LoopOrigin::While,
            },
            node.text_range(),
        )
    }

    fn lower_for_stmt(&mut self, node: &SyntaxNode) -> StmtId {
        // FOR_EXPR can take two forms:
        //   Iterator-style:  for (let var in <expr>) { <body> }
        //   C-style:         for (let i = 0; cond; update) { <body> }
        //
        // Detection: C-style has no KW_IN token among FOR_EXPR's direct children.
        // The C-style LET_STMT child contains EQUALS and SEMICOLON tokens.
        //
        // C-style is desugared to:
        //   Stmt::Let { ... }   // init
        //   Stmt::While { condition, body, after: Some(update_stmt), origin: LoopOrigin::For }
        // These two statements are wrapped in Expr::Block → Stmt::Expr so the
        // function can return a single StmtId.
        let range = node.text_range();

        // Determine if this is a C-style for loop by checking for KW_IN.
        let is_c_style = !node
            .children_with_tokens()
            .any(|e| matches!(&e, rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::KW_IN));

        if is_c_style {
            return self.lower_c_style_for(node, range);
        }

        // --- Iterator-style for loop ---
        //
        // Parenthesized:     for (let var in <expr>) { <body> }
        // Non-parenthesized: for var in <expr> { <body> }
        //
        // In the parenthesized form, the variable binding is wrapped in a
        // LET_STMT child node (from `parse_for_in_pattern`). In the
        // non-parenthesized form, the variable is a bare WORD token.
        //
        // We emit a first-class Stmt::For (NOT desugared to While here).
        // Desugaring to index-based basic blocks happens at MIR lowering time.

        let mut iter_name = Name::new("_iter_var");
        let mut iter_expr_opt = None;
        let mut body_opt = None;
        let mut seen_in = false;
        let mut seen_let_stmt = false;

        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Token(token) => match token.kind() {
                    SyntaxKind::KW_IN => {
                        seen_in = true;
                    }
                    // Non-parenthesized form: `for i in xs` — bare WORD before KW_IN
                    k if is_ident_token(k) && !seen_in => {
                        iter_name = Name::new(token.text());
                    }
                    _ => {
                        if seen_in && iter_expr_opt.is_none() {
                            iter_expr_opt = self.try_lower_bare_token(&token);
                        }
                    }
                },
                rowan::NodeOrToken::Node(child) => {
                    if !seen_in && !seen_let_stmt && child.kind() == SyntaxKind::LET_STMT {
                        // Parenthesized form: `for (let var in xs)` — variable is
                        // inside a LET_STMT child node produced by parse_for_in_pattern.
                        // Extract the variable name from the first WORD token in the node.
                        for t in child.children_with_tokens() {
                            if let rowan::NodeOrToken::Token(tok) = t {
                                if is_ident_token(tok.kind()) {
                                    iter_name = Name::new(tok.text());
                                    break;
                                }
                            }
                        }
                        seen_let_stmt = true;
                    } else if seen_in && iter_expr_opt.is_none() {
                        iter_expr_opt = Some(self.lower_expr(&child));
                    } else if iter_expr_opt.is_some() && body_opt.is_none() {
                        body_opt = Some(self.lower_expr(&child));
                    }
                }
            }
        }

        let collection = iter_expr_opt.unwrap_or_else(|| self.alloc_expr(Expr::Missing, range));
        let body = body_opt.unwrap_or_else(|| self.alloc_expr(Expr::Missing, range));
        let binding = self.alloc_pattern(Pattern::Binding(iter_name), range);

        self.alloc_stmt(
            Stmt::For {
                binding,
                collection,
                body,
            },
            range,
        )
    }

    /// Desugar a C-style for loop `for (let i = 0; cond; update) { body }`
    /// into:
    ///   ```text
    ///   {
    ///     let i = 0;                // init_stmt  (Stmt::Let)
    ///     while cond {              // Stmt::While
    ///       body;
    ///     } after { update; }
    ///   }
    ///   ```
    /// The two statements are wrapped in an `Expr::Block` so this function can
    /// return a single `StmtId` (as `Stmt::Expr(block)`).
    fn lower_c_style_for(&mut self, node: &SyntaxNode, range: text_size::TextRange) -> StmtId {
        // C-style CST structure (direct children of FOR_EXPR):
        //   KW_FOR  L_PAREN  LET_STMT  BINARY_EXPR(cond)  SEMICOLON  BINARY_EXPR(update)  R_PAREN  BLOCK_EXPR
        //
        // We collect child nodes in order; the three significant nodes are:
        //   [0] LET_STMT      — initializer
        //   [1] BINARY_EXPR   — condition
        //   [2] BINARY_EXPR   — update
        //   [3] BLOCK_EXPR    — body
        let child_nodes: Vec<SyntaxNode> = node.children().collect();

        // Pull out init (LET_STMT), cond, update, body nodes by position.
        // child_nodes order: LET_STMT, BINARY_EXPR(cond), BINARY_EXPR(update), BLOCK_EXPR
        let init_node = child_nodes
            .iter()
            .find(|n| n.kind() == SyntaxKind::LET_STMT)
            .cloned();
        // BINARY_EXPRs appear in document order: first is condition, second is update.
        let binary_exprs: Vec<SyntaxNode> = child_nodes
            .iter()
            .filter(|n| n.kind() == SyntaxKind::BINARY_EXPR)
            .cloned()
            .collect();
        let block_node = child_nodes
            .iter()
            .find(|n| n.kind() == SyntaxKind::BLOCK_EXPR)
            .cloned();

        // Lower the initializer as a Let statement.
        let init_stmt = if let Some(let_node) = init_node {
            self.lower_let_stmt(&let_node, false)
        } else {
            self.alloc_stmt(Stmt::Missing, range)
        };

        // Lower the condition expression (first BINARY_EXPR).
        let condition = if let Some(cond_node) = binary_exprs.first() {
            self.lower_expr(cond_node)
        } else {
            self.alloc_expr(Expr::Missing, range)
        };

        // Lower the update expression (second BINARY_EXPR) as a statement.
        // `i += 1` is an assignment-op, so try_lower_assignment handles it.
        let after_stmt = if let Some(update_node) = binary_exprs.get(1) {
            let update_range = update_node.text_range();
            let stmt_opt = self.try_lower_assignment(update_node);
            Some(stmt_opt.unwrap_or_else(|| {
                // Plain expression update (e.g. function call)
                let expr_id = self.lower_expr(update_node);
                self.alloc_stmt(Stmt::Expr(expr_id), update_range)
            }))
        } else {
            None
        };

        // Lower the loop body.
        let body = if let Some(blk) = block_node {
            self.lower_expr(&blk)
        } else {
            self.alloc_expr(Expr::Missing, range)
        };

        // Build Stmt::While.
        let while_stmt = self.alloc_stmt(
            Stmt::While {
                condition,
                body,
                after: after_stmt,
                origin: LoopOrigin::For,
            },
            range,
        );

        // Wrap both statements in a block expression so we return one StmtId.
        let block_expr = self.alloc_expr(
            Expr::Block {
                stmts: vec![init_stmt, while_stmt],
                tail_expr: None,
            },
            range,
        );
        self.alloc_stmt(Stmt::Expr(block_expr), range)
    }

    /// Lower a `TEST_EXPR_DEF` node as a `<collector>.register_test(name, lambda, null)` call.
    ///
    /// Used when `test` appears inside a testset body (possibly nested inside a `for`/`if`).
    /// Requires `self.testset_collector_var` to be set.
    fn lower_test_expr_as_register_call(&mut self, node: &SyntaxNode) -> ExprId {
        let span = node.text_range();
        let collector_name = self
            .testset_collector_var
            .clone()
            .unwrap_or_else(|| Name::new("testset"));

        // Extract test name from STRING_LITERAL child (may be a BINARY_EXPR for concatenation)
        let name_expr = self.lower_test_name_expr(node, span);

        // Find the BLOCK_EXPR child (the test body)
        let body_node_opt = node.children().find(|c| c.kind() == SyntaxKind::BLOCK_EXPR);

        let (lambda_body, lambda_source_map, lambda_diags) = if let Some(body_node) = body_node_opt
        {
            // Lower the body using a fresh context (no collector var — test bodies don't nest)
            crate::lower_expr_body::lower_block_node(
                &body_node,
                std::slice::from_ref(&collector_name),
            )
        } else {
            // Empty body: produce null
            let mut sub_ctx = LoweringContext::new();
            let null_expr = sub_ctx.alloc_expr(Expr::Null, span);
            sub_ctx.finish(Some(null_expr))
        };
        self.diags.extend(lambda_diags);

        let lambda_def = FunctionDef {
            name: Name::new("<test body>"),
            generic_params: vec![],
            params: vec![],
            return_type: None,
            throws: None,
            body: Some(FunctionBodyDef::Expr(lambda_body, lambda_source_map)),
            declarative_meta: None,
            attributes: vec![],
            span,
            name_span: span,
        };

        // <collector>.register_test(name_expr, lambda, runner_or_null)
        let collector_ref = self.alloc_expr(Expr::Path(vec![collector_name]), span);
        let method_target = self.alloc_expr(
            Expr::FieldAccess {
                base: collector_ref,
                field: Name::new("register_test"),
            },
            span,
        );
        let lambda_arg = self.alloc_expr(Expr::Lambda(Box::new(lambda_def)), span);
        let runner_arg = match crate::lower_cst::extract_runner_element(node) {
            Some(rowan::NodeOrToken::Node(runner_node)) => self.lower_expr(&runner_node),
            Some(rowan::NodeOrToken::Token(token)) => {
                let expr = lower_bare_token_expr(token.kind(), token.text());
                self.alloc_expr(expr, span)
            }
            None => self.alloc_expr(Expr::Null, span),
        };

        self.alloc_expr(
            Expr::Call {
                callee: method_target,
                args: vec![name_expr, lambda_arg, runner_arg],
            },
            span,
        )
    }

    /// Lower a `TESTSET_DEF` node as a `<collector>.register_test_set(name, sub_collector_lambda, null)` call.
    ///
    /// The sub-collector lambda body is produced by recursively lowering the testset's
    /// `BLOCK_EXPR` body using a nested `LoweringContext` with `testset_collector_var = "testset"`.
    fn lower_testset_as_register_call(&mut self, node: &SyntaxNode) -> ExprId {
        let span = node.text_range();
        let collector_name = self
            .testset_collector_var
            .clone()
            .unwrap_or_else(|| Name::new("testset"));

        // Extract testset name
        let name_expr = self.lower_test_name_expr(node, span);

        // Find the BLOCK_EXPR child (the testset body)
        let body_node_opt = node.children().find(|c| c.kind() == SyntaxKind::BLOCK_EXPR);

        let (sub_body, sub_source_map, sub_diags) = if let Some(body_node) = body_node_opt {
            crate::lower_expr_body::lower_testset_block_node(
                &body_node,
                &Name::new("testset"),
                std::slice::from_ref(&collector_name),
            )
        } else {
            let mut sub_ctx = LoweringContext::new();
            let null_expr = sub_ctx.alloc_expr(Expr::Null, span);
            sub_ctx.finish(Some(null_expr))
        };
        self.diags.extend(sub_diags);

        let sub_param = Param {
            name: Name::new("testset"),
            type_expr: Some(SpannedTypeExpr {
                expr: TypeExpr::Path {
                    segments: vec![Name::new("testing"), Name::new("TestCollector")],
                    attrs: vec![],
                },
                span,
            }),
            span,
            name_span: span,
        };

        let sub_collector_def = FunctionDef {
            name: Name::new("<testset collector>"),
            generic_params: vec![],
            params: vec![sub_param],
            return_type: None,
            throws: None,
            body: Some(FunctionBodyDef::Expr(sub_body, sub_source_map)),
            declarative_meta: None,
            attributes: vec![],
            span,
            name_span: span,
        };

        // <collector>.register_test_set(name_expr, sub_collector_lambda, runner_or_null)
        let collector_ref = self.alloc_expr(Expr::Path(vec![collector_name]), span);
        let method_target = self.alloc_expr(
            Expr::FieldAccess {
                base: collector_ref,
                field: Name::new("register_test_set"),
            },
            span,
        );
        let sub_collector_arg = self.alloc_expr(Expr::Lambda(Box::new(sub_collector_def)), span);
        let runner_arg = match crate::lower_cst::extract_runner_element(node) {
            Some(rowan::NodeOrToken::Node(runner_node)) => self.lower_expr(&runner_node),
            Some(rowan::NodeOrToken::Token(token)) => {
                let expr = lower_bare_token_expr(token.kind(), token.text());
                self.alloc_expr(expr, span)
            }
            None => self.alloc_expr(Expr::Null, span),
        };

        self.alloc_expr(
            Expr::Call {
                callee: method_target,
                args: vec![name_expr, sub_collector_arg, runner_arg],
            },
            span,
        )
    }

    /// Extract the name expression from a `TEST_EXPR_DEF` or `TESTSET_DEF` node.
    ///
    /// The name can be a plain `STRING_LITERAL` or a `BINARY_EXPR` (e.g. `"check " + case`).
    fn lower_test_name_expr(&mut self, node: &SyntaxNode, span: TextRange) -> ExprId {
        // The name is the first expression child after the keyword (KW_TEST / KW_TESTSET),
        // before KW_WITH or BLOCK_EXPR. It can be any expression node.
        let name_element = node.children_with_tokens().find(|c| {
            let kind = c.kind();
            !matches!(
                kind,
                SyntaxKind::KW_TEST
                    | SyntaxKind::KW_TESTSET
                    | SyntaxKind::KW_WITH
                    | SyntaxKind::BLOCK_EXPR
                    | SyntaxKind::WHITESPACE
                    | SyntaxKind::NEWLINE
                    | SyntaxKind::LINE_COMMENT
            )
        });

        match name_element {
            Some(rowan::NodeOrToken::Node(ref name_node)) => self.lower_expr(name_node),
            Some(rowan::NodeOrToken::Token(ref token)) => {
                let expr = lower_bare_token_expr(token.kind(), token.text());
                self.alloc_expr(expr, token.text_range())
            }
            None => self.alloc_expr(Expr::Literal(Literal::String(String::new())), span),
        }
    }

    fn lower_header_comment(&mut self, node: &SyntaxNode) -> StmtId {
        // HEADER_COMMENT raw text looks like: //# Title or //## Section Name
        // Strip the leading "//" prefix, then count '#' characters for the level
        // (number_of_hashes + 1 so that //# => level=2, //## => level=3, etc.),
        // and use the remaining text (trimmed) as the name.
        let raw = node.text().to_string();
        let after_slashes = raw.strip_prefix("//").unwrap_or(&raw);
        let hash_count = after_slashes.chars().take_while(|c| *c == '#').count();
        let level = hash_count;
        let title = after_slashes[hash_count..].trim();
        let name = if title.is_empty() {
            Name::new("_")
        } else {
            Name::new(title)
        };

        self.alloc_stmt(Stmt::HeaderComment { name, level }, node.text_range())
    }
}

/// Check if a `SyntaxKind` represents an expression node (vs. punctuation/keyword).
fn is_expr_node_kind(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::EXPR
            | SyntaxKind::BINARY_EXPR
            | SyntaxKind::UNARY_EXPR
            | SyntaxKind::CALL_EXPR
            | SyntaxKind::PATH_EXPR
            | SyntaxKind::FIELD_ACCESS_EXPR
            | SyntaxKind::ENV_ACCESS_EXPR
            | SyntaxKind::INDEX_EXPR
            | SyntaxKind::IF_EXPR
            | SyntaxKind::MATCH_EXPR
            | SyntaxKind::CATCH_EXPR
            | SyntaxKind::THROW_EXPR
            | SyntaxKind::BLOCK_EXPR
            | SyntaxKind::PAREN_EXPR
            | SyntaxKind::ARRAY_LITERAL
            | SyntaxKind::STRING_LITERAL
            | SyntaxKind::RAW_STRING_LITERAL
            | SyntaxKind::OBJECT_LITERAL
            | SyntaxKind::MAP_LITERAL
            | SyntaxKind::LAMBDA_EXPR
    )
}

/// Strip string delimiters from a raw token text, returning the content as an owned `String`.
fn strip_string_delimiters(text: &str) -> String {
    let text = text.trim();
    if text.starts_with("#\"") && text.ends_with("\"#") {
        text[2..text.len() - 2].to_string()
    } else if text.starts_with('"') && text.ends_with('"') && text.len() >= 2 {
        text[1..text.len() - 1].to_string()
    } else {
        text.to_string()
    }
}
