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

use crate::ast::{
    AssignOp, AstSourceMap, BinaryOp, CatchArm, CatchArmId, CatchClause, CatchClauseKind, Expr,
    ExprBody, ExprId, LetOrigin, Literal, LoopOrigin, MatchArm, MatchArmId, PatId, Pattern,
    SpreadField, Stmt, StmtId, TypeAnnotId, TypeExpr, UnaryOp,
};

/// Lower a CST `ExprFunctionBody` to an owned `ExprBody` + parallel `AstSourceMap`.
pub(crate) fn lower(
    expr_body: &baml_compiler_syntax::ast::ExprFunctionBody,
    param_names: &[Name],
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

    ctx.finish(root_expr)
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
        }
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

    fn alloc_type_annot(&mut self, spanned: crate::ast::SpannedTypeExpr) -> TypeAnnotId {
        let id = self.type_annotations.alloc(spanned.to_type_expr());
        self.source_map.type_annotation_spans.alloc(spanned.span);
        self.source_map.type_annotation_spanned_exprs.push(spanned);
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
            SyntaxKind::WORD => {
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

    fn finish(self, root_expr: Option<ExprId>) -> (ExprBody, AstSourceMap) {
        let body = ExprBody {
            exprs: self.exprs,
            stmts: self.stmts,
            patterns: self.patterns,
            match_arms: self.match_arms,
            catch_arms: self.catch_arms,
            type_annotations: self.type_annotations,
            root_expr,
        };
        (body, self.source_map)
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
                        SyntaxKind::ASSERT_STMT => self.lower_assert_stmt(node),
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
                        SyntaxKind::WORD => {
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
                        SyntaxKind::WORD => {
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
                        SyntaxKind::WORD => {
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
                        SyntaxKind::WORD => {
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
                            let spanned = crate::lower_type_expr::lower_type_expr_node(&type_expr);
                            scrutinee_type = Some(self.alloc_type_annot(spanned));
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
                            SyntaxKind::WORD => {
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
                                        SyntaxKind::WORD => {
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
                    SyntaxKind::WORD if seen_fat_arrow && body.is_none() => {
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
                        SyntaxKind::WORD => {
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
                                let span = token.text_range();
                                let ty = crate::ast::SpannedTypeExpr {
                                    kind: crate::ast::SpannedTypeExprKind::Path(vec![Name::new(
                                        &text,
                                    )]),
                                    span,
                                };
                                let pat = Pattern::TypedBinding { name, ty };
                                elements.push(self.alloc_pattern(pat, span));
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
                                    let spanned =
                                        crate::lower_type_expr::lower_type_expr_node(&type_expr);
                                    let pat = Pattern::TypedBinding { name, ty: spanned };
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
                    SyntaxKind::WORD if seen_fat_arrow && body.is_none() => {
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
                SyntaxKind::WORD => {
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
            // No callee node - check for a WORD token (simple function name)
            let word_token = node
                .children_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .find(|t| t.kind() == SyntaxKind::WORD);

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
                                SyntaxKind::WORD => {
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
        // PATH_EXPR contains WORD tokens joined by DOTs
        let mut segments = Vec::new();

        for elem in node.children_with_tokens() {
            if let rowan::NodeOrToken::Token(token) = elem {
                if token.kind() == SyntaxKind::WORD {
                    segments.push(Name::new(token.text()));
                }
            }
        }

        if segments.is_empty() {
            return self.alloc_expr(Expr::Missing, node.text_range());
        }

        // Check if single segment is a literal keyword
        if segments.len() == 1 {
            match segments[0].as_str() {
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
        let mut base = self.alloc_expr(Expr::Path(vec![segments[0].clone()]), node.text_range());
        for seg in &segments[1..] {
            base = self.alloc_expr(
                Expr::FieldAccess {
                    base,
                    field: seg.clone(),
                },
                node.text_range(),
            );
        }
        base
    }

    fn lower_field_access_expr(&mut self, node: &SyntaxNode) -> ExprId {
        let mut base = None;
        let mut field = None;

        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child) => {
                    if base.is_none() {
                        base = Some(self.lower_expr(&child));
                    }
                }
                rowan::NodeOrToken::Token(token) => {
                    if token.kind() == SyntaxKind::WORD && base.is_some() {
                        field = Some(Name::new(token.text()));
                    }
                }
            }
        }

        let base = base.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));
        let field = field.unwrap_or_else(|| Name::new("_"));

        self.alloc_expr(Expr::FieldAccess { base, field }, node.text_range())
    }

    fn lower_env_access_expr(&mut self, node: &SyntaxNode) -> ExprId {
        // ENV_ACCESS_EXPR is `env.VAR_NAME` — lower as field access on path "env"
        let range = node.text_range();
        let env_expr = self.alloc_expr(Expr::Path(vec![Name::new("env")]), range);

        // Find the field name after the DOT
        let mut field = None;
        let mut seen_dot = false;
        for elem in node.children_with_tokens() {
            if let rowan::NodeOrToken::Token(token) = elem {
                if token.kind() == SyntaxKind::DOT {
                    seen_dot = true;
                } else if seen_dot && token.kind() == SyntaxKind::WORD {
                    field = Some(Name::new(token.text()));
                    break;
                }
            }
        }

        let field = field.unwrap_or_else(|| Name::new("_"));
        self.alloc_expr(
            Expr::FieldAccess {
                base: env_expr,
                field,
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

        // Look for the optional type name (first WORD or path before the brace)
        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Token(token) => {
                    if token.kind() == SyntaxKind::L_BRACE {
                        break;
                    }
                    if token.kind() == SyntaxKind::WORD && type_name.is_none() {
                        type_name = Some(Name::new(token.text()));
                    }
                }
                rowan::NodeOrToken::Node(_) => break,
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
                                if t.kind() == SyntaxKind::WORD && !seen_colon =>
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
                            } else if !seen_colon
                                && key_expr.is_none()
                                && t.kind() == SyntaxKind::WORD
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

    fn try_lower_paren_token_content(&mut self, node: &SyntaxNode) -> Option<ExprId> {
        // Look for a single meaningful token inside the parentheses
        for elem in node.children_with_tokens() {
            if let rowan::NodeOrToken::Token(token) = elem {
                let span = token.text_range();
                match token.kind() {
                    SyntaxKind::WORD => {
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
            SyntaxKind::WORD => {
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
                            SyntaxKind::WORD => {
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
                                let spanned =
                                    crate::lower_type_expr::lower_type_expr_node(&type_expr);
                                type_annotation = Some(self.alloc_type_annot(spanned));
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
                                    .find(|t| t.kind() == SyntaxKind::WORD)
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
                        SyntaxKind::WORD if seen_let_kw && pattern_id.is_none() => {
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
                        SyntaxKind::WORD => {
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
        // FOR_EXPR: KW_FOR WORD KW_IN expr BLOCK_EXPR
        // Desugar into a while loop (simplified)
        let range = node.text_range();

        // Find the iter expression and body
        let mut _iter_name = Name::new("_iter");
        let mut iter_expr_opt = None;
        let mut body = None;
        let mut seen_in = false;

        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Token(token) => match token.kind() {
                    SyntaxKind::KW_IN => {
                        seen_in = true;
                    }
                    SyntaxKind::WORD if !seen_in => {
                        _iter_name = Name::new(token.text());
                    }
                    _ => {
                        if seen_in && iter_expr_opt.is_none() {
                            iter_expr_opt = self.try_lower_bare_token(&token);
                        }
                    }
                },
                rowan::NodeOrToken::Node(child) => {
                    if seen_in && iter_expr_opt.is_none() {
                        iter_expr_opt = Some(self.lower_expr(&child));
                    } else if iter_expr_opt.is_some() && body.is_none() {
                        body = Some(self.lower_expr(&child));
                    }
                }
            }
        }

        let _iter_expr = iter_expr_opt.unwrap_or_else(|| self.alloc_expr(Expr::Missing, range));
        let body = body.unwrap_or_else(|| self.alloc_expr(Expr::Missing, range));

        // For simplicity, represent as a While loop with a synthetic condition
        let cond = self.alloc_expr(Expr::Literal(Literal::Bool(true)), range);
        self.alloc_stmt(
            Stmt::While {
                condition: cond,
                body,
                after: None,
                origin: LoopOrigin::For,
            },
            range,
        )
    }

    fn lower_assert_stmt(&mut self, node: &SyntaxNode) -> StmtId {
        let condition = if let Some(n) = node.children().next() {
            self.lower_expr(&n)
        } else {
            // Try bare token: `assert x;`
            node.children_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .find_map(|t| self.try_lower_bare_token(&t))
                .unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()))
        };

        self.alloc_stmt(Stmt::Assert { condition }, node.text_range())
    }

    fn lower_header_comment(&mut self, node: &SyntaxNode) -> StmtId {
        // HEADER_COMMENT: # level heading Name
        let mut name = Name::new("_");
        let mut level = 1usize;

        for elem in node.children_with_tokens() {
            if let rowan::NodeOrToken::Token(token) = elem {
                match token.kind() {
                    SyntaxKind::WORD => {
                        name = Name::new(token.text());
                    }
                    SyntaxKind::HASH => {
                        level += 1;
                    }
                    _ => {}
                }
            }
        }

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
