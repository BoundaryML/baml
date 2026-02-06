use baml_compiler_syntax::{SyntaxElement, SyntaxKind};
use rowan::TextRange;

use crate::ast::{
    BlockExpr, Expression, FromCST, HeaderComment, StrongAstError, SyntaxNodeIter, Type,
};

use super::tokens as t;

#[derive(Debug)]
pub enum Statement {
    /// Assignment operations are parsed as binary expressions.
    ///
    /// Also note that the expression statement does not parse a following semicolon,
    /// so the caller should check for one and attach it to the expression if present.
    Expr(ExpressionStmt),
    Let(LetStmt),
    While(WhileStmt),
    Return(ReturnStmt),
    Break(BreakStmt),
    Continue(ContinueStmt),
    Assert(AssertStmt),
    For(ForStmt),
    HeaderComment(HeaderComment),
    /// There's a semicolon with no preceding statement.
    EmptySemicolon(t::Semicolon),
    Unknown(TextRange),
}

impl FromCST for Statement {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        match elem.kind() {
            SyntaxKind::LET_STMT => Ok(Statement::Let(LetStmt::from_cst(elem)?)),
            SyntaxKind::RETURN_STMT => Ok(Statement::Return(ReturnStmt::from_cst(elem)?)),
            SyntaxKind::WHILE_STMT => Ok(Statement::While(WhileStmt::from_cst(elem)?)),
            SyntaxKind::FOR_EXPR => Ok(Statement::For(ForStmt::from_cst(elem)?)),
            SyntaxKind::BREAK_STMT => Ok(Statement::Break(BreakStmt::from_cst(elem)?)),
            SyntaxKind::CONTINUE_STMT => Ok(Statement::Continue(ContinueStmt::from_cst(elem)?)),
            SyntaxKind::ASSERT_STMT => Ok(Statement::Assert(AssertStmt::from_cst(elem)?)),
            SyntaxKind::SEMICOLON => {
                let token = StrongAstError::assert_is_token(elem)?;
                Ok(Statement::EmptySemicolon(t::Semicolon::new_from_span(
                    token.text_range(),
                )))
            }
            SyntaxKind::HEADER_COMMENT => {
                let token = StrongAstError::assert_is_token(elem)?;
                Ok(Statement::HeaderComment(t::HeaderComment::new_from_span(
                    token.text_range(),
                )))
            }
            _ => ExpressionStmt::from_cst(elem).map(Statement::Expr),
        }
    }
}

/// Does not correspond to a [`SyntaxKind`].
///
/// Unlike most implementations of `FromCST`, this will never parse the semicolon, as it is not a child of the node.
/// Instead, the caller should check for a semicolon after the expression and add it to the `ExpressionStmt` if present.
#[derive(Debug)]
pub struct ExpressionStmt {
    pub expr: Expression,
    pub semicolon: Option<t::Semicolon>,
}

impl FromCST for ExpressionStmt {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        // Expression statements don't have their own node type
        // They are just expressions (possibly followed by a semicolon in the parent)
        let expr = Expression::from_cst(elem)?;

        // Note: The semicolon is typically consumed by the parent block parser
        // So we can't reliably detect it here
        Ok(ExpressionStmt {
            expr,
            semicolon: None,
        })
    }
}

/// Corresponds to a [`SyntaxKind::LET_STMT`] node.
#[derive(Debug)]
pub struct LetStmt {
    pub keyword: t::Let,
    pub name: t::Word,
    pub type_annotation: Option<(t::Colon, Type)>,
    pub initializer: Option<(t::Equals, Expression)>,
    pub semicolon: t::Semicolon,
}

impl FromCST for LetStmt {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::LET_STMT)?;

        let mut it = SyntaxNodeIter::new(node);

        // KW_LET
        let keyword = it.expect_token_of_kind(SyntaxKind::KW_LET)?;

        // Variable name
        let name = it.expect_token_of_kind(SyntaxKind::WORD)?;

        let mut next = it.expect_token("COLON, EQUALS, or SEMICOLON")?;
        let type_annotation = if next.kind() == SyntaxKind::COLON {
            let ty = it.expect_next("a type")?;
            next = it.expect_token("EQUALS or SEMICOLON")?;
            Some((
                t::Colon::new_from_span(next.text_range()),
                Type::from_cst(ty)?,
            ))
        } else {
            None
        };

        let initializer = if next.kind() == SyntaxKind::EQUALS {
            let value = it.expect_next("an expression")?;
            let value = Expression::from_cst(value)?;
            next = it.expect_token_of_kind(SyntaxKind::SEMICOLON)?;
            Some((t::Equals::new_from_span(next.text_range()), value))
        } else {
            None
        };

        StrongAstError::assert_kind_token(&next, SyntaxKind::SEMICOLON)?;
        it.expect_end()?;

        Ok(LetStmt {
            keyword: t::Let::new_from_span(keyword.text_range()),
            name: t::Word::new_from_span(name.text_range()),
            type_annotation,
            initializer,
            semicolon: t::Semicolon::new_from_span(next.text_range()),
        })
    }
}

/// Corresponds to a [`SyntaxKind::WHILE_STMT`] node.
#[derive(Debug)]
pub struct WhileStmt {
    pub keyword: t::While,
    pub open_paren: t::LParen,
    pub condition: Expression,
    pub close_paren: t::RParen,
    pub body: BlockExpr,
}

impl FromCST for WhileStmt {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::WHILE_STMT)?;

        let mut it = SyntaxNodeIter::new(node);

        // KW_WHILE
        let keyword = it.expect_token_of_kind(SyntaxKind::KW_WHILE)?;

        // L_PAREN
        let open_paren = it.expect_token_of_kind(SyntaxKind::L_PAREN)?;

        // Condition expression
        let cond = it.expect_next("condition expression")?;
        let condition = Expression::from_cst(cond)?;

        // R_PAREN
        let close_paren = it.expect_token_of_kind(SyntaxKind::R_PAREN)?;

        // BLOCK_EXPR
        let body_node = it.expect_node_of_kind(SyntaxKind::BLOCK_EXPR)?;
        let body = BlockExpr::from_cst(SyntaxElement::Node(body_node))?;

        it.expect_end()?;

        Ok(WhileStmt {
            keyword: t::While::new_from_span(keyword.text_range()),
            open_paren: t::LParen::new_from_span(open_paren.text_range()),
            condition,
            close_paren: t::RParen::new_from_span(close_paren.text_range()),
            body,
        })
    }
}

/// Corresponds to a [`SyntaxKind::FOR_EXPR`] node.
#[derive(Debug)]
pub struct ForStmt {
    pub keyword: t::For,
    pub args: ForArgs,
    pub body: BlockExpr,
}

impl FromCST for ForStmt {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::FOR_EXPR)?;

        let mut it = SyntaxNodeIter::new(node);

        // KW_FOR
        let keyword = it.expect_token_of_kind(SyntaxKind::KW_FOR)?;

        fn take_args(it: &mut SyntaxNodeIter) -> Result<ForArgs, StrongAstError> {
            let open_paren = it.expect_token_of_kind(SyntaxKind::L_PAREN)?;

            let args_first = it.expect_next("a statement")?;
            let args_first = Statement::from_cst(args_first)?;

            let (args_first, args_second) = if let Statement::Let(let_stmt) = args_first {
                // Could be either for-in pattern or C-style
                let args_second = it.expect_next("an expression or KW_IN")?;
                if args_second.kind() == SyntaxKind::KW_IN {
                    // We are iterator style
                    let kw_in = StrongAstError::assert_is_token(args_second)?;
                    let kw_in = t::In::new_from_span(kw_in.text_range());

                    let expr = it.expect_next("iterator expression")?;
                    let expression = Expression::from_cst(expr)?;

                    let close_paren = it.expect_token_of_kind(SyntaxKind::R_PAREN)?;

                    return Ok(ForArgs::Iterator(ForIteratorArgs {
                        open_paren: t::LParen::new_from_span(open_paren.text_range()),
                        let_stmt,
                        in_keyword: kw_in,
                        expression,
                        close_paren: t::RParen::new_from_span(close_paren.text_range()),
                    }));
                }
                (Statement::Let(let_stmt), Expression::from_cst(args_second)?)
            } else if let Statement::Expr(expr_stmt) = args_first {
                // we need to parse the semicolon
                let semicolon = it.expect_token_of_kind(SyntaxKind::SEMICOLON)?;
                let expr_stmt = ExpressionStmt {
                    expr: expr_stmt.expr,
                    semicolon: Some(t::Semicolon::new_from_span(semicolon.text_range())),
                };

                let args_second = it.expect_next("an expression")?;
                (
                    Statement::Expr(expr_stmt),
                    Expression::from_cst(args_second)?,
                )
            } else {
                let args_second = it.expect_next("an expression")?;
                (args_first, Expression::from_cst(args_second)?)
            };

            // For loop is C-style
            let semicolon = it.expect_token_of_kind(SyntaxKind::SEMICOLON)?;

            let args_last = it.expect_next("a statement")?;
            let args_last = Statement::from_cst(args_last)?;

            let close_paren = it.expect_token_of_kind(SyntaxKind::R_PAREN)?;

            Ok(ForArgs::CStyle(ForCStyleArgs {
                open_paren: t::LParen::new_from_span(open_paren.text_range()),
                init: Box::new(args_first),
                condition: args_second,
                semicolon: t::Semicolon::new_from_span(semicolon.text_range()),
                update: Box::new(args_last),
                close_paren: t::RParen::new_from_span(close_paren.text_range()),
            }))
        }

        let args = take_args(&mut it)?;

        // BLOCK_EXPR
        let body_node = it.expect_node_of_kind(SyntaxKind::BLOCK_EXPR)?;
        let body = BlockExpr::from_cst(SyntaxElement::Node(body_node))?;

        it.expect_end()?;

        Ok(ForStmt {
            keyword: t::For::new_from_span(keyword.text_range()),
            args,
            body,
        })
    }
}

#[derive(Debug)]
pub enum ForArgs {
    Iterator(ForIteratorArgs),
    CStyle(ForCStyleArgs),
}

#[derive(Debug)]
pub struct ForCStyleArgs {
    pub open_paren: t::LParen,
    pub init: Box<Statement>,
    pub condition: Expression,
    pub semicolon: t::Semicolon,
    pub update: Box<Statement>,
    pub close_paren: t::RParen,
}

#[derive(Debug)]
pub struct ForIteratorArgs {
    pub open_paren: t::LParen,
    pub let_stmt: LetStmt,
    pub in_keyword: t::In,
    pub expression: Expression,
    pub close_paren: t::RParen,
}

#[derive(Debug)]
pub struct ReturnStmt {
    pub keyword: t::Return,
    pub value: Option<Expression>,
    pub semicolon: Option<t::Semicolon>,
}

impl FromCST for ReturnStmt {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::RETURN_STMT)?;

        let mut it = SyntaxNodeIter::new(node);

        // KW_RETURN
        let keyword = it.expect_token_of_kind(SyntaxKind::KW_RETURN)?;

        // Optional return value
        let value = if let Some(elem) = it.next() {
            if elem.kind() == SyntaxKind::SEMICOLON {
                // Just a semicolon, no value
                let token = StrongAstError::assert_is_token(elem)?;
                it.expect_end()?;
                return Ok(ReturnStmt {
                    keyword: t::Return::new_from_span(keyword.text_range()),
                    value: None,
                    semicolon: Some(t::Semicolon::new_from_span(token.text_range())),
                });
            } else {
                // Expression value
                Some(Expression::from_cst(elem)?)
            }
        } else {
            None
        };

        // Optional semicolon
        let semicolon = it
            .next()
            .map(|elem| {
                let token = StrongAstError::assert_is_token(elem)?;
                StrongAstError::assert_kind_token(&token, SyntaxKind::SEMICOLON)?;
                Ok(t::Semicolon::new_from_span(token.text_range()))
            })
            .transpose()?;

        it.expect_end()?;

        Ok(ReturnStmt {
            keyword: t::Return::new_from_span(keyword.text_range()),
            value,
            semicolon,
        })
    }
}

/// Corresponds to a [`SyntaxKind::BREAK_STMT`] node.
#[derive(Debug)]
pub struct BreakStmt {
    pub keyword: t::Break,
    pub semicolon: Option<t::Semicolon>,
}

impl FromCST for BreakStmt {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::BREAK_STMT)?;

        let mut it = SyntaxNodeIter::new(node);

        let keyword = it.expect_token_of_kind(SyntaxKind::KW_BREAK)?;

        let semicolon = it
            .next()
            .map(|elem| {
                let token = StrongAstError::assert_is_token(elem)?;
                StrongAstError::assert_kind_token(&token, SyntaxKind::SEMICOLON)?;
                Ok(t::Semicolon::new_from_span(token.text_range()))
            })
            .transpose()?;

        it.expect_end()?;

        Ok(BreakStmt {
            keyword: t::Break::new_from_span(keyword.text_range()),
            semicolon,
        })
    }
}

/// Corresponds to a [`SyntaxKind::CONTINUE_STMT`] node.
#[derive(Debug)]
pub struct ContinueStmt {
    pub keyword: t::Continue,
    pub semicolon: Option<t::Semicolon>,
}

impl FromCST for ContinueStmt {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::CONTINUE_STMT)?;

        let mut it = SyntaxNodeIter::new(node);

        let keyword = it.expect_token_of_kind(SyntaxKind::KW_CONTINUE)?;

        let semicolon = it
            .next()
            .map(|elem| {
                let token = StrongAstError::assert_is_token(elem)?;
                StrongAstError::assert_kind_token(&token, SyntaxKind::SEMICOLON)?;
                Ok(t::Semicolon::new_from_span(token.text_range()))
            })
            .transpose()?;

        it.expect_end()?;

        Ok(ContinueStmt {
            keyword: t::Continue::new_from_span(keyword.text_range()),
            semicolon,
        })
    }
}

/// Corresponds to a [`SyntaxKind::ASSERT_STMT`] node.
#[derive(Debug)]
pub struct AssertStmt {
    pub keyword: t::Assert,
    pub condition: Expression,
    pub semicolon: Option<t::Semicolon>,
}

impl FromCST for AssertStmt {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::ASSERT_STMT)?;

        let mut it = SyntaxNodeIter::new(node);

        let keyword = it.expect_token_of_kind(SyntaxKind::KW_ASSERT)?;

        let condition = it.expect_next("some expression")?;
        let condition = Expression::from_cst(condition)?;

        let semicolon = it
            .next()
            .map(|elem| {
                let token = StrongAstError::assert_is_token(elem)?;
                StrongAstError::assert_kind_token(&token, SyntaxKind::SEMICOLON)?;
                Ok(t::Semicolon::new_from_span(token.text_range()))
            })
            .transpose()?;

        it.expect_end()?;

        Ok(AssertStmt {
            keyword: t::Assert::new_from_span(keyword.text_range()),
            condition,
            semicolon,
        })
    }
}
