//! Reference: [baml_compiler_syntax::ast::Expr] and [baml_compiler_hir::body]

use baml_compiler_syntax::{SyntaxElement, SyntaxKind};
use rowan::TextRange;

use crate::ast::{
    BinaryOp, FromCST, MatchPattern, Statement, StrongAstError, SyntaxNodeIter, UnaryOp,
};

use super::tokens as t;

#[derive(Debug)]
pub enum Expression {
    Literal(Literal),
    /// Includes things like `null`, `true`, `false`, `baml.fs`, etc.
    Path(PathExpr),
    Paren(ParenExpr),
    Binary(BinaryExpr),
    Unary(UnaryExpr),
    If(IfExpr),
    Match(MatchExpr),
    Call(CallExpr),
    Index(IndexExpr),
    FieldAccess(FieldAccessExpr),
    Block(BlockExpr),
    ArrayInitializer(ArrayInitializer),
    MapInitializer(MapLiteral),
    ObjectInitializer(ObjectInitializer),
    RawString(t::RawString),
    Unknown(TextRange),
}

impl FromCST for Expression {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let expr = match elem.kind() {
            SyntaxKind::STRING_LITERAL => Expression::Literal(Literal::String(t::QuotedString {
                token_span: elem.text_range(),
            })),
            SyntaxKind::INTEGER_LITERAL => {
                Expression::Literal(Literal::Integer(t::IntegerLiteral {
                    token_span: elem.text_range(),
                }))
            }
            SyntaxKind::FLOAT_LITERAL => Expression::Literal(Literal::Float(t::FloatLiteral {
                token_span: elem.text_range(),
            })),
            SyntaxKind::PATH_EXPR => Expression::Path(PathExpr::from_cst(elem)?),
            SyntaxKind::PAREN_EXPR => Expression::Paren(ParenExpr::from_cst(elem)?),
            SyntaxKind::BINARY_EXPR => Expression::Binary(BinaryExpr::from_cst(elem)?),
            SyntaxKind::UNARY_EXPR => Expression::Unary(UnaryExpr::from_cst(elem)?),
            SyntaxKind::IF_EXPR => Expression::If(IfExpr::from_cst(elem)?),
            SyntaxKind::MATCH_EXPR => Expression::Match(MatchExpr::from_cst(elem)?),
            SyntaxKind::CALL_EXPR => Expression::Call(CallExpr::from_cst(elem)?),
            SyntaxKind::INDEX_EXPR => Expression::Index(IndexExpr::from_cst(elem)?),
            SyntaxKind::FIELD_ACCESS_EXPR => {
                Expression::FieldAccess(FieldAccessExpr::from_cst(elem)?)
            }
            SyntaxKind::BLOCK_EXPR => Expression::Block(BlockExpr::from_cst(elem)?),
            SyntaxKind::ARRAY_LITERAL => {
                Expression::ArrayInitializer(ArrayInitializer::from_cst(elem)?)
            }
            SyntaxKind::MAP_LITERAL => Expression::MapInitializer(MapLiteral::from_cst(elem)?),
            SyntaxKind::OBJECT_LITERAL => {
                Expression::ObjectInitializer(ObjectInitializer::from_cst(elem)?)
            }
            SyntaxKind::RAW_STRING_LITERAL => Expression::RawString(t::RawString {
                token_span: elem.text_range(),
            }),
            _ => Expression::Unknown(elem.text_range()),
        };
        Ok(expr)
    }
}

#[derive(Debug)]
pub enum Literal {
    String(t::QuotedString),
    Integer(t::IntegerLiteral),
    Float(t::FloatLiteral),
}

#[derive(Debug)]
pub struct PathExpr {
    pub first: t::Word,
    pub rest: Vec<(t::Dot, t::Word)>,
}

impl FromCST for PathExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::PATH_EXPR)?;

        let mut it = SyntaxNodeIter::new(node);

        // First WORD
        let first = it.expect_token_of_kind(SyntaxKind::WORD)?;

        let mut rest = Vec::new();

        // Collect DOT WORD pairs
        while let Some(elem) = it.next() {
            if elem.kind() == SyntaxKind::DOT {
                let dot_token = StrongAstError::assert_is_token(elem)?;
                let word = it.expect_token_of_kind(SyntaxKind::WORD)?;

                rest.push((
                    t::Dot::new_from_span(dot_token.text_range()),
                    t::Word::new_from_span(word.text_range()),
                ));
            } else {
                return Err(StrongAstError::UnexpectedAdditionalElement {
                    parent: first.text_range(),
                    at: elem.text_range(),
                });
            }
        }

        Ok(PathExpr {
            first: t::Word::new_from_span(first.text_range()),
            rest,
        })
    }
}

#[derive(Debug)]
pub struct ParenExpr {
    pub open_paren: t::LParen,
    pub expr: Box<Expression>,
    pub close_paren: t::RParen,
}

impl FromCST for ParenExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::PAREN_EXPR)?;

        let mut it = SyntaxNodeIter::new(node);

        let open_paren = it.expect_token_of_kind(SyntaxKind::L_PAREN)?;

        let expr = it.expect_next("an expression")?;
        let expr = Expression::from_cst(expr)?;

        let close_paren = it.expect_token_of_kind(SyntaxKind::R_PAREN)?;

        it.expect_end()?;

        Ok(ParenExpr {
            open_paren: t::LParen::new_from_span(open_paren.text_range()),
            expr: Box::new(expr),
            close_paren: t::RParen::new_from_span(close_paren.text_range()),
        })
    }
}

#[derive(Debug)]
pub struct BinaryExpr {
    pub op: BinaryOp,
    pub sides: Box<(Expression, Expression)>,
}

impl FromCST for BinaryExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::BINARY_EXPR)?;

        let mut it = SyntaxNodeIter::new(node);

        // Get left expression
        let left = it.expect_next("left expression")?;
        let left_expr = Expression::from_cst(left)?;

        // Get operator
        let op_token = it.expect_token("binary operator")?;
        let op = BinaryOp::from_cst_token(op_token)?;

        // Get right expression
        let right = it.expect_next("right expression")?;
        let right_expr = Expression::from_cst(right)?;

        it.expect_end()?;

        Ok(BinaryExpr {
            op,
            sides: Box::new((left_expr, right_expr)),
        })
    }
}

#[derive(Debug)]
pub struct UnaryExpr {
    pub op: UnaryOp,
    pub expr: Box<Expression>,
}

impl FromCST for UnaryExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::UNARY_EXPR)?;

        let mut it = SyntaxNodeIter::new(node);

        // Get operator
        let op_token = it.expect_token("unary operator")?;
        let op = UnaryOp::from_cst_token(op_token)?;

        // Get expression
        let expr_node = it.expect_next("expression")?;
        let expr = Expression::from_cst(expr_node)?;

        it.expect_end()?;

        Ok(UnaryExpr {
            op,
            expr: Box::new(expr),
        })
    }
}

/// Corresponds to a [`SyntaxKind::IF_EXPR`] node.
#[derive(Debug)]
pub struct IfExpr {
    pub keyword: t::If,
    pub condition: ParenExpr,
    pub block: BlockExpr,
    pub else_branch: Option<(t::Else, ElseExpr)>,
}

impl FromCST for IfExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::IF_EXPR)?;

        let mut it = SyntaxNodeIter::new(node);

        // KW_IF
        let keyword = it.expect_token_of_kind(SyntaxKind::KW_IF)?;

        // PAREN_EXPR
        let condition = it.expect_node_of_kind(SyntaxKind::PAREN_EXPR)?;
        let condition = ParenExpr::from_cst(SyntaxElement::Node(condition))?;

        // BLOCK_EXPR
        let block_node = it.expect_node_of_kind(SyntaxKind::BLOCK_EXPR)?;
        let block = BlockExpr::from_cst(SyntaxElement::Node(block_node))?;

        // Optional else branch
        let else_branch = if let Some(elem) = it.next() {
            let else_token = StrongAstError::assert_is_token(elem)?;
            StrongAstError::assert_kind_token(&else_token, SyntaxKind::KW_ELSE)?;

            let else_body_node = it.expect_node("else body (if or block)")?;
            let else_body = match else_body_node.kind() {
                SyntaxKind::IF_EXPR => ElseExpr::If(Box::new(IfExpr::from_cst(
                    SyntaxElement::Node(else_body_node),
                )?)),
                SyntaxKind::BLOCK_EXPR => ElseExpr::Block(Box::new(BlockExpr::from_cst(
                    SyntaxElement::Node(else_body_node),
                )?)),
                _ => {
                    return Err(StrongAstError::UnexpectedKindDesc {
                        expected_desc: "IF_EXPR or BLOCK_EXPR".into(),
                        found: else_body_node.kind(),
                        at: else_body_node.text_range(),
                    });
                }
            };

            Some((t::Else::new_from_span(else_token.text_range()), else_body))
        } else {
            None
        };

        it.expect_end()?;

        Ok(IfExpr {
            keyword: t::If::new_from_span(keyword.text_range()),
            condition,
            block,
            else_branch,
        })
    }
}

#[derive(Debug)]
pub enum ElseExpr {
    /// else if
    If(Box<IfExpr>),
    /// final else block
    Block(Box<BlockExpr>),
}

/// Corresponds to a [`SyntaxKind::MATCH_EXPR`] node.
#[derive(Debug)]
pub struct MatchExpr {
    pub keyword: t::Match,
    pub open_paren: t::LParen,
    pub scrutinee: Box<Expression>,
    pub close_paren: t::RParen,
    pub open_brace: t::LBrace,
    pub arms: Vec<(MatchArm, Option<t::Comma>)>,
    pub close_brace: t::RBrace,
}

impl FromCST for MatchExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::MATCH_EXPR)?;

        let mut it = SyntaxNodeIter::new(node);

        // KW_MATCH
        let keyword = it.expect_token_of_kind(SyntaxKind::KW_MATCH)?;

        // L_PAREN
        let open_paren = it.expect_token_of_kind(SyntaxKind::L_PAREN)?;

        // Scrutinee expression (can be any node that represents an expression)
        let scrutinee_node = it.expect_next("scrutinee expression")?;
        let scrutinee = Box::new(Expression::from_cst(scrutinee_node)?);

        // R_PAREN
        let close_paren = it.expect_token_of_kind(SyntaxKind::R_PAREN)?;

        // L_BRACE
        let open_brace = it.expect_token_of_kind(SyntaxKind::L_BRACE)?;

        // Collect match arms
        let mut arms = Vec::new();
        while let Some(elem) = it.next() {
            if elem.kind() == SyntaxKind::R_BRACE {
                let token = StrongAstError::assert_is_token(elem)?;
                let close_brace = t::RBrace::new_from_span(token.text_range());
                it.expect_end()?;

                return Ok(MatchExpr {
                    keyword: t::Match::new_from_span(keyword.text_range()),
                    open_paren: t::LParen::new_from_span(open_paren.text_range()),
                    scrutinee,
                    close_paren: t::RParen::new_from_span(close_paren.text_range()),
                    open_brace: t::LBrace::new_from_span(open_brace.text_range()),
                    arms,
                    close_brace,
                });
            }

            let arm_node = StrongAstError::assert_is_node(elem)?;
            let arm = MatchArm::from_cst(SyntaxElement::Node(arm_node))?;

            // Check for optional comma
            let comma = if let Some(next_elem) = it.next() {
                if next_elem.kind() == SyntaxKind::COMMA {
                    let comma_token = StrongAstError::assert_is_token(next_elem)?;
                    Some(t::Comma::new_from_span(comma_token.text_range()))
                } else if next_elem.kind() == SyntaxKind::R_BRACE {
                    let token = StrongAstError::assert_is_token(next_elem)?;
                    let close_brace = t::RBrace::new_from_span(token.text_range());
                    arms.push((arm, None));
                    it.expect_end()?;

                    return Ok(MatchExpr {
                        keyword: t::Match::new_from_span(keyword.text_range()),
                        open_paren: t::LParen::new_from_span(open_paren.text_range()),
                        scrutinee,
                        close_paren: t::RParen::new_from_span(close_paren.text_range()),
                        open_brace: t::LBrace::new_from_span(open_brace.text_range()),
                        arms,
                        close_brace,
                    });
                } else {
                    return Err(StrongAstError::UnexpectedKindDesc {
                        expected_desc: "COMMA or R_BRACE".into(),
                        found: next_elem.kind(),
                        at: next_elem.text_range(),
                    });
                }
            } else {
                None
            };

            arms.push((arm, comma));
        }

        Err(StrongAstError::missing(
            SyntaxKind::R_BRACE,
            open_brace.text_range(),
        ))
    }
}

/// Corresponds to a [`SyntaxKind::MATCH_ARM`] node.
#[derive(Debug)]
pub struct MatchArm {
    pub pattern: MatchPattern,
    pub guard: Option<(t::If, Box<Expression>)>,
    pub fat_arrow: t::FatArrow,
    pub body: Box<Expression>,
}

impl FromCST for MatchArm {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::MATCH_ARM)?;

        let mut it = SyntaxNodeIter::new(node);

        // MATCH_PATTERN
        let pattern_node = it.expect_node_of_kind(SyntaxKind::MATCH_PATTERN)?;
        let pattern_range = pattern_node.text_range();
        let pattern = MatchPattern::from_cst(SyntaxElement::Node(pattern_node))?;

        // Check for optional guard (if condition)
        let guard = if let Some(elem) = it.next() {
            if elem.kind() == SyntaxKind::KW_IF {
                let if_token = StrongAstError::assert_is_token(elem)?;
                let guard_expr_node = it.expect_next("guard expression")?;
                let guard_expr = Expression::from_cst(guard_expr_node)?;
                Some((
                    t::If::new_from_span(if_token.text_range()),
                    Box::new(guard_expr),
                ))
            } else if elem.kind() == SyntaxKind::FAT_ARROW {
                // No guard, this is the fat arrow
                let fat_arrow_token = StrongAstError::assert_is_token(elem)?;
                let body_node = it.expect_next("match arm body")?;
                let body = Expression::from_cst(body_node)?;
                it.expect_end()?;

                return Ok(MatchArm {
                    pattern,
                    guard: None,
                    fat_arrow: t::FatArrow::new_from_span(fat_arrow_token.text_range()),
                    body: Box::new(body),
                });
            } else {
                return Err(StrongAstError::UnexpectedKindDesc {
                    expected_desc: "KW_IF or FAT_ARROW".into(),
                    found: elem.kind(),
                    at: elem.text_range(),
                });
            }
        } else {
            return Err(StrongAstError::missing_desc("FAT_ARROW", pattern_range));
        };

        // FAT_ARROW
        let fat_arrow = it.expect_token_of_kind(SyntaxKind::FAT_ARROW)?;

        // Body expression
        let body_node = it.expect_next("match arm body")?;
        let body = Expression::from_cst(body_node)?;

        it.expect_end()?;

        Ok(MatchArm {
            pattern,
            guard,
            fat_arrow: t::FatArrow::new_from_span(fat_arrow.text_range()),
            body: Box::new(body),
        })
    }
}

/// Corresponds to a [`SyntaxKind::CALL_EXPR`] node.
#[derive(Debug)]
pub struct CallExpr {
    pub callee: Box<Expression>,
    pub open_paren: t::LParen,
    pub args: Vec<Expression>,
    pub close_paren: t::RParen,
}

impl FromCST for CallExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::CALL_EXPR)?;

        let mut it = SyntaxNodeIter::new(node);

        // Callee expression
        let callee_node = it.expect_next("callee expression")?;
        let callee = Box::new(Expression::from_cst(callee_node)?);

        // CALL_ARGS
        let args_node = it.expect_node_of_kind(SyntaxKind::CALL_ARGS)?;
        let mut args_it = SyntaxNodeIter::new(args_node);

        // L_PAREN
        let open_paren = args_it.expect_token_of_kind(SyntaxKind::L_PAREN)?;

        // Collect arguments
        let mut args = Vec::new();
        while let Some(elem) = args_it.next() {
            if elem.kind() == SyntaxKind::R_PAREN {
                let token = StrongAstError::assert_is_token(elem)?;
                args_it.expect_end()?;
                it.expect_end()?;

                return Ok(CallExpr {
                    callee,
                    open_paren: t::LParen::new_from_span(open_paren.text_range()),
                    args,
                    close_paren: t::RParen::new_from_span(token.text_range()),
                });
            }

            let arg_node = StrongAstError::assert_is_node(elem)?;
            args.push(Expression::from_cst(SyntaxElement::Node(arg_node))?);

            // Check for comma or closing paren
            if let Some(next) = args_it.next() {
                if next.kind() == SyntaxKind::COMMA {
                    // Continue to next argument
                    continue;
                } else if next.kind() == SyntaxKind::R_PAREN {
                    let token = StrongAstError::assert_is_token(next)?;
                    args_it.expect_end()?;
                    it.expect_end()?;

                    return Ok(CallExpr {
                        callee,
                        open_paren: t::LParen::new_from_span(open_paren.text_range()),
                        args,
                        close_paren: t::RParen::new_from_span(token.text_range()),
                    });
                } else {
                    return Err(StrongAstError::UnexpectedKindDesc {
                        expected_desc: "COMMA or R_PAREN".into(),
                        found: next.kind(),
                        at: next.text_range(),
                    });
                }
            }
        }

        Err(StrongAstError::missing(
            SyntaxKind::R_PAREN,
            open_paren.text_range(),
        ))
    }
}

/// Corresponds to a [`SyntaxKind::CALL_ARGS`] node.
#[derive(Debug)]
pub struct CallArgs {
    pub open_paren: t::LParen,
    pub args: Vec<(Expression, Option<t::Comma>)>,
    pub close_paren: t::RParen,
}
impl FromCST for CallArgs {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::CALL_ARGS)?;

        let mut it = SyntaxNodeIter::new(node);

        let open_paren = it.expect_token_of_kind(SyntaxKind::L_PAREN)?;

        let mut args = Vec::new();
        let mut peek = None;
        let end_paren = loop {
            let Some(elem) = peek.take().or_else(|| it.next()) else {
                return Err(StrongAstError::missing(
                    SyntaxKind::R_PAREN,
                    open_paren.text_range(),
                ));
            };
            match elem.kind() {
                SyntaxKind::R_PAREN => {
                    let token = StrongAstError::assert_is_token(elem)?;
                    break token;
                }
                _ => {
                    let expr = Expression::from_cst(elem)?;
                    let comma = match it.next() {
                        Some(elem) if elem.kind() == SyntaxKind::COMMA => {
                            let comma = StrongAstError::assert_is_token(elem)?;
                            Some(t::Comma::new_from_span(comma.text_range()))
                        }
                        otherwise => {
                            peek = otherwise;
                            None
                        }
                    };
                    args.push((expr, comma));
                }
            }
        };

        it.expect_end()?;

        Ok(CallArgs {
            open_paren: t::LParen::new_from_span(open_paren.text_range()),
            args,
            close_paren: t::RParen::new_from_span(end_paren.text_range()),
        })
    }
}

#[derive(Debug)]
pub struct IndexExpr {
    pub base: Box<Expression>,
    pub open_bracket: t::LBracket,
    pub index: Box<Expression>,
    pub close_bracket: t::RBracket,
}

impl FromCST for IndexExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::INDEX_EXPR)?;

        let mut it = SyntaxNodeIter::new(node);

        // Base expression
        let base_node = it.expect_next("base expression")?;
        let base = Box::new(Expression::from_cst(base_node)?);

        // L_BRACKET
        let open_bracket = it.expect_token_of_kind(SyntaxKind::L_BRACKET)?;

        // Index expression
        let index_node = it.expect_next("index expression")?;
        let index = Box::new(Expression::from_cst(index_node)?);

        // R_BRACKET
        let close_bracket = it.expect_token_of_kind(SyntaxKind::R_BRACKET)?;

        it.expect_end()?;

        Ok(IndexExpr {
            base,
            open_bracket: t::LBracket::new_from_span(open_bracket.text_range()),
            index,
            close_bracket: t::RBracket::new_from_span(close_bracket.text_range()),
        })
    }
}

#[derive(Debug)]
pub struct FieldAccessExpr {
    pub base: Box<Expression>,
    pub dot: t::Dot,
    pub field: t::Word,
}

impl FromCST for FieldAccessExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::FIELD_ACCESS_EXPR)?;

        let mut it = SyntaxNodeIter::new(node);

        // Base expression
        let base_node = it.expect_next("base expression")?;
        let base = Box::new(Expression::from_cst(base_node)?);

        // DOT
        let dot = it.expect_token_of_kind(SyntaxKind::DOT)?;

        // WORD (field name)
        let field = it.expect_token_of_kind(SyntaxKind::WORD)?;

        it.expect_end()?;

        Ok(FieldAccessExpr {
            base,
            dot: t::Dot::new_from_span(dot.text_range()),
            field: t::Word {
                token_span: field.text_range(),
            },
        })
    }
}

/// Corresponds to a [`SyntaxKind::BLOCK_EXPR`].
#[derive(Debug)]
pub struct BlockExpr {
    pub open_brace: t::LBrace,
    pub stmts: Vec<Statement>,
    /// Possible tail expression.
    /// If not in a block that can have a tail expression, this should be treated as a normal [`Statement::Expr`].
    pub expr: Option<Box<Expression>>,
    pub close_brace: t::RBrace,
}

impl FromCST for BlockExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::BLOCK_EXPR)?;

        let mut it = SyntaxNodeIter::new(node);

        let open_brace = it.expect_token_of_kind(SyntaxKind::L_BRACE)?;

        // Collect all statements and optional final expression
        let mut stmts = Vec::new();
        let close_brace = loop {
            let Some(elem) = it.next() else {
                return Err(StrongAstError::missing(
                    SyntaxKind::R_BRACE,
                    open_brace.text_range(),
                ));
            };
            if elem.kind() == SyntaxKind::R_BRACE {
                let close_brace = StrongAstError::assert_is_token(elem)?;
                break close_brace;
            }

            let stmt = Statement::from_cst(elem)?;
            if let Some(Statement::Expr(expr)) = stmts.last_mut()
                && expr.semicolon.is_none()
                && let Statement::EmptySemicolon(semi) = stmt
            {
                // Attach semicolon to preceding expression since expressions don't immediately parse semicolons
                expr.semicolon = Some(semi);
                continue;
            }
            stmts.push(stmt);
        };

        // If final statement is a expression without semicolon, extract it as a tail expression
        let expr = match stmts.pop() {
            Some(Statement::Expr(expr)) if expr.semicolon.is_none() => Some(expr.expr),
            Some(stmt) => {
                stmts.push(stmt);
                None
            }
            None => None,
        };

        it.expect_end()?;

        Ok(BlockExpr {
            open_brace: t::LBrace::new_from_span(open_brace.text_range()),
            stmts,
            expr: expr.map(Box::new),
            close_brace: t::RBrace::new_from_span(close_brace.text_range()),
        })
    }
}

#[derive(Debug)]
pub struct ArrayInitializer {
    pub open_bracket: t::LBracket,
    /// Commas are optional for all elements.
    /// For example, `[1 2 3]` is equivalent to `[1, 2, 3]` in BAML.
    ///
    /// While this is valid, excluding commas is *strongly* discouraged as it is a crime against software and also more error-prone:
    /// if `[1, -2, 3]` is written as `[1 -2 3]`, it will be parsed as `[1-2, 3]` instead (the `-` will be treated as a binary operator instead of a unary operator).
    pub elements: Vec<(Expression, Option<t::Comma>)>,
    pub close_bracket: t::RBracket,
}

impl FromCST for ArrayInitializer {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::ARRAY_LITERAL)?;

        let mut it = SyntaxNodeIter::new(node);

        // L_BRACKET
        let open_bracket = it.expect_token_of_kind(SyntaxKind::L_BRACKET)?;

        let mut elements: Vec<(Expression, Option<t::Comma>)> = Vec::new();

        let close_bracket = loop {
            let Some(elem) = it.next() else {
                return Err(StrongAstError::missing(
                    SyntaxKind::R_BRACKET,
                    open_bracket.text_range(),
                ));
            };
            match elem.kind() {
                SyntaxKind::R_BRACKET => {
                    let token = StrongAstError::assert_is_token(elem)?;
                    break token;
                }
                SyntaxKind::COMMA => {
                    let comma = StrongAstError::assert_is_token(elem)?;
                    if let Some(last) = elements.last_mut()
                        && last.1.is_none()
                    {
                        last.1 = Some(t::Comma::new_from_span(comma.text_range()));
                    } else {
                        return Err(StrongAstError::UnexpectedKindDesc {
                            expected_desc: "expression or R_BRACKET".into(),
                            found: comma.kind(),
                            at: comma.text_range(),
                        });
                    }
                }
                _ => {
                    let expr = Expression::from_cst(elem)?;
                    elements.push((expr, None));
                }
            }
        };

        return Ok(ArrayInitializer {
            open_bracket: t::LBracket::new_from_span(open_bracket.text_range()),
            elements,
            close_bracket: t::RBracket::new_from_span(close_bracket.text_range()),
        });
    }
}

/// Corresponds to a [`SyntaxKind::OBJECT_LITERAL`] node.
#[derive(Debug)]
pub struct ObjectInitializer {
    pub name: t::Word,
    pub open_brace: t::LBrace,
    pub fields: Vec<ObjectField>,
    pub close_brace: t::RBrace,
}

impl FromCST for ObjectInitializer {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::OBJECT_LITERAL)?;

        let mut it = SyntaxNodeIter::new(node);

        // WORD (object type name)
        let name = it.expect_token_of_kind(SyntaxKind::WORD)?;

        let open_brace = it.expect_token_of_kind(SyntaxKind::L_BRACE)?;

        let mut fields = Vec::new();
        let close_brace = loop {
            let Some(elem) = it.next() else {
                return Err(StrongAstError::missing(
                    SyntaxKind::R_BRACE,
                    open_brace.text_range(),
                ));
            };
            match elem.kind() {
                SyntaxKind::R_BRACE => {
                    let token = StrongAstError::assert_is_token(elem)?;
                    break token;
                }
                SyntaxKind::OBJECT_FIELD => {
                    let field = ObjectField::from_cst(elem)?;
                    fields.push(field);
                }
                _ => {
                    return Err(StrongAstError::UnexpectedKindDesc {
                        expected_desc: "OBJECT_FIELD or R_BRACE".into(),
                        found: elem.kind(),
                        at: elem.text_range(),
                    });
                }
            }
        };

        it.expect_end()?;

        Ok(ObjectInitializer {
            name: t::Word::new_from_span(name.text_range()),
            open_brace: t::LBrace::new_from_span(open_brace.text_range()),
            fields,
            close_brace: t::RBrace::new_from_span(close_brace.text_range()),
        })
    }
}

/// Corresponds to a [`SyntaxKind::MAP_LITERAL`] node.
#[derive(Debug)]
pub struct MapLiteral {
    pub open_brace: t::LBrace,
    pub fields: Vec<ObjectField>,
    pub close_brace: t::RBrace,
}

impl FromCST for MapLiteral {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::MAP_LITERAL)?;

        let mut it = SyntaxNodeIter::new(node);

        let open_brace = it.expect_token_of_kind(SyntaxKind::L_BRACE)?;

        let mut fields = Vec::new();
        let close_brace = loop {
            let Some(elem) = it.next() else {
                return Err(StrongAstError::missing(
                    SyntaxKind::R_BRACE,
                    open_brace.text_range(),
                ));
            };
            match elem.kind() {
                SyntaxKind::R_BRACE => {
                    let token = StrongAstError::assert_is_token(elem)?;
                    break token;
                }
                SyntaxKind::OBJECT_FIELD => {
                    let field = ObjectField::from_cst(elem)?;
                    fields.push(field);
                }
                _ => {
                    return Err(StrongAstError::UnexpectedKindDesc {
                        expected_desc: "OBJECT_FIELD or R_BRACE".into(),
                        found: elem.kind(),
                        at: elem.text_range(),
                    });
                }
            }
        };

        it.expect_end()?;

        Ok(MapLiteral {
            open_brace: t::LBrace::new_from_span(open_brace.text_range()),
            fields,
            close_brace: t::RBrace::new_from_span(close_brace.text_range()),
        })
    }
}

/// Corresponds to a [`SyntaxKind::OBJECT_FIELD`] node.
#[derive(Debug)]
pub struct ObjectField {
    pub name: t::Word,
    pub colon: t::Colon,
    pub value: Expression,
    pub comma: Option<t::Comma>,
}

impl FromCST for ObjectField {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::OBJECT_FIELD)?;

        let mut it = SyntaxNodeIter::new(node);

        let name = it.expect_token_of_kind(SyntaxKind::WORD)?;

        let colon = it.expect_token_of_kind(SyntaxKind::COLON)?;

        let value = it.expect_next("value")?;
        let value = Expression::from_cst(value)?;

        let comma = it
            .next()
            .map(|elem| {
                let comma = StrongAstError::assert_is_token(elem)?;
                StrongAstError::assert_kind_token(&comma, SyntaxKind::COMMA)?;
                Ok(t::Comma::new_from_span(comma.text_range()))
            })
            .transpose()?;

        it.expect_end()?;

        Ok(ObjectField {
            name: t::Word::new_from_span(name.text_range()),
            colon: t::Colon::new_from_span(colon.text_range()),
            value,
            comma,
        })
    }
}
