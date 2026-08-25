use rowan::ast::AstNode as _;

use crate::{
    SyntaxElement, SyntaxKind, TextRange, ast as raw_ast,
    validated::{
        FromCST, HeaderComment, KnownKind, StrongAstError, SyntaxNodeIter,
        nodes::{BlockExpr, Expression, MatchPattern, ParenExpr},
        tokens as t,
    },
};

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum Statement {
    /// Assignment operations are parsed as binary expressions.
    ///
    /// Also note that the expression statement does not parse a following semicolon,
    /// so the caller should check for one and attach it to the expression if present.
    Expr(ExpressionStmt),
    Let(LetStmt),
    While(WhileStmt),
    WhileLet(WhileLetStmt),
    Return(ReturnStmt),
    Break(BreakStmt),
    Continue(ContinueStmt),
    For(ForStmt),
    HeaderComment(HeaderComment),
    /// There's a semicolon with no preceding statement.
    EmptySemicolon(t::Semicolon),
    /// An expression-body test nested inside a testset body.
    TestExpr(raw_ast::TestExprDef),
    /// A nested testset inside a testset body.
    TestSet(raw_ast::TestsetDef),
    Unknown(TextRange),
}

impl FromCST for Statement {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        match elem.kind() {
            SyntaxKind::LET_STMT => LetStmt::from_cst(elem).map(Statement::Let),
            SyntaxKind::RETURN_STMT => ReturnStmt::from_cst(elem).map(Statement::Return),
            SyntaxKind::WHILE_STMT => WhileStmt::from_cst(elem).map(Statement::While),
            SyntaxKind::WHILE_LET_STMT => WhileLetStmt::from_cst(elem).map(Statement::WhileLet),
            SyntaxKind::FOR_EXPR => ForStmt::from_cst(elem).map(Statement::For),
            SyntaxKind::BREAK_STMT => BreakStmt::from_cst(elem).map(Statement::Break),
            SyntaxKind::CONTINUE_STMT => ContinueStmt::from_cst(elem).map(Statement::Continue),
            SyntaxKind::SEMICOLON => t::Semicolon::from_cst(elem).map(Statement::EmptySemicolon),
            SyntaxKind::HEADER_COMMENT => {
                t::HeaderComment::from_cst(elem).map(Statement::HeaderComment)
            }
            SyntaxKind::TEST_EXPR_DEF => {
                let node = StrongAstError::assert_is_node(elem)?;
                Ok(Statement::TestExpr(
                    raw_ast::TestExprDef::cast(node).expect("checked expression test"),
                ))
            }
            SyntaxKind::TESTSET_DEF => {
                let node = StrongAstError::assert_is_node(elem)?;
                Ok(Statement::TestSet(
                    raw_ast::TestsetDef::cast(node).expect("checked test set"),
                ))
            }
            _ => ExpressionStmt::from_cst(elem).map(Statement::Expr),
        }
    }
}

/// Does not correspond to a [`SyntaxKind`], but parses some [`Expression`] as a statement.
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
///
/// Post-pattern-rewrite shape: `(KW_LET|KW_CONST)? PATTERN EQUALS? <expr>? (KW_ELSE BLOCK_EXPR)? SEMICOLON?`.
/// Simple bindings carry the introducer inside the [`super::MatchPattern`] (e.g.
/// `let x: int` parses as a `Chain([Bind, Type])`). Array destructuring uses
/// the statement-level introducer before an `ARRAY_PATTERN`. The optional
/// `else BLOCK_EXPR` tail is the `let ... else` form: a refutable binding
/// whose else branch must diverge.
#[derive(Debug)]
pub struct LetStmt {
    pub let_keyword: Option<t::BindingKeyword>,
    pub pattern: super::MatchPattern,
    pub initializer: Option<(t::Equals, Expression)>,
    /// `else { ... }` tail for `let ... else`. None for plain `let`. Boxed
    /// to keep `LetStmt` (and the enclosing `Statement` enum) small - the
    /// else branch is rare and a `BlockExpr` carries a full statement
    /// vector.
    pub else_branch: Option<Box<(t::Else, super::BlockExpr)>>,
    /// Not required in some contexts like for-let loops
    pub semicolon: Option<t::Semicolon>,
}

impl FromCST for LetStmt {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::LET_STMT)?;
        let mut it = SyntaxNodeIter::new(&node);

        let let_keyword = it
            .next_if(|elem| matches!(elem.kind(), SyntaxKind::KW_LET | SyntaxKind::KW_CONST))
            .map(t::BindingKeyword::from_cst)
            .transpose()?;

        let pattern: super::MatchPattern = it.expect_parse()?;

        let initializer = if let Some(equals) = it.next_if_kind(SyntaxKind::EQUALS) {
            let value = it.expect_next("an expression")?;
            Some((t::Equals::from_cst(equals)?, Expression::from_cst(value)?))
        } else {
            None
        };

        let else_branch = if let Some(else_kw) = it.next_if_kind(SyntaxKind::KW_ELSE) {
            let block_elem = it.expect_next("block after `else`")?;
            Some(Box::new((
                t::Else::from_cst(else_kw)?,
                super::BlockExpr::from_cst(block_elem)?,
            )))
        } else {
            None
        };

        let semicolon = it.next().map(t::Semicolon::from_cst).transpose()?;
        it.expect_end()?;

        Ok(LetStmt {
            let_keyword,
            pattern,
            initializer,
            else_branch,
            semicolon,
        })
    }
}

/// Corresponds to a [`SyntaxKind::WHILE_STMT`] node.
#[derive(Debug)]
pub struct WhileStmt {
    pub keyword: t::While,
    pub condition: ParenExpr,
    pub body: BlockExpr,
}

impl FromCST for WhileStmt {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::WHILE_STMT)?;

        let mut it = SyntaxNodeIter::new(&node);

        // KW_WHILE
        let keyword = it.expect_parse()?;

        // PAREN_EXPR
        let condition: ParenExpr = it.expect_parse()?;

        // BLOCK_EXPR
        let body: BlockExpr = it.expect_parse()?;

        it.expect_end()?;

        Ok(WhileStmt {
            keyword,
            condition,
            body,
        })
    }
}

impl KnownKind for WhileStmt {
    fn kind() -> SyntaxKind {
        SyntaxKind::WHILE_STMT
    }
}

/// Corresponds to a [`SyntaxKind::WHILE_LET_STMT`] node.
///
/// `while let PATTERN = SCRUTINEE { BODY }`. Combines `WhileStmt`'s statement
/// framing with `if let`'s `pattern = scrutinee` head, but - like `if let` and
/// unlike plain `while` - emits no parens around the scrutinee, and has no
/// `else` clause (loops produce unit).
#[derive(Debug)]
pub struct WhileLetStmt {
    pub keyword: t::While,
    /// Standalone leading binding introducer, present only for top-level
    /// array-pattern heads (`while let [x] = xs`), where the parser keeps the
    /// introducer at the statement level instead of inside the pattern. For
    /// binding / class / type heads the introducer lives inside `pattern` and
    /// this is `None`. Mirrors
    /// `LetStmt::let_keyword`.
    pub let_keyword: Option<t::BindingKeyword>,
    pub pattern: MatchPattern,
    pub equals: t::Equals,
    pub scrutinee: Box<Expression>,
    pub body: BlockExpr,
}

impl FromCST for WhileLetStmt {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::WHILE_LET_STMT)?;

        let mut it = SyntaxNodeIter::new(&node);

        // KW_WHILE
        let keyword = it.expect_parse()?;

        // Optional standalone KW_LET/KW_CONST for top-level array-pattern heads
        // (`while let [x] = xs`); for other heads `let` is inside the pattern.
        let let_keyword = it
            .next_if(|elem| matches!(elem.kind(), SyntaxKind::KW_LET | SyntaxKind::KW_CONST))
            .map(t::BindingKeyword::from_cst)
            .transpose()?;

        // PATTERN (carries its own leading `let` unless consumed above)
        let pattern = it.expect_parse()?;

        // `=` separator between pattern and scrutinee
        let equals = it.expect_parse()?;

        // Scrutinee: any expression
        let scrutinee_elem = it.expect_next("while-let scrutinee expression")?;
        let scrutinee = Box::new(Expression::from_cst(scrutinee_elem)?);

        // BLOCK_EXPR body (no else clause)
        let body: BlockExpr = it.expect_parse()?;

        it.expect_end()?;

        Ok(WhileLetStmt {
            keyword,
            let_keyword,
            pattern,
            equals,
            scrutinee,
            body,
        })
    }
}

impl KnownKind for WhileLetStmt {
    fn kind() -> SyntaxKind {
        SyntaxKind::WHILE_LET_STMT
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

        let mut it = SyntaxNodeIter::new(&node);

        // KW_FOR
        let keyword = it.expect_parse()?;

        // Three legal shapes:
        //   for (let i in expr) { ... }       - paren + LET_STMT
        //   for (let i = 0; cond; upd) { ... } - paren + LET_STMT + C-style
        //   for (i in expr) { ... }            - paren + bare WORD
        //   for i in expr { ... }              - no paren + bare WORD
        let open_paren: Option<t::LParen> = it
            .next_if_kind(SyntaxKind::L_PAREN)
            .map(t::LParen::from_cst)
            .transpose()?;

        // Binding: either a LET_STMT node or a bare Word token.
        let binding = if let Some(let_elem) = it.next_if_kind(SyntaxKind::LET_STMT) {
            ForBinding::Let(Box::new(LetStmt::from_cst(let_elem)?))
        } else {
            let word_elem = it.expect_next("for-loop binding (let or identifier)")?;
            let word = t::Word::from_cst(word_elem)?;
            ForBinding::Bare(word)
        };

        let args = if let Some(kw_in) = it.next_if_kind(SyntaxKind::KW_IN) {
            // for-in
            let expr = it.expect_next("iterator expression")?;
            let expression = Expression::from_cst(expr)?;

            let close_paren = open_paren.as_ref().map(|_| it.expect_parse()).transpose()?;

            ForArgs::Iterator(ForIteratorArgs {
                open_paren,
                binding,
                in_keyword: t::In::from_cst(kw_in)?,
                expression,
                close_paren,
            })
        } else {
            // C-style - only valid with a `let` binding and parens
            let ForBinding::Let(let_stmt) = binding else {
                return Err(StrongAstError::UnexpectedKindDesc {
                    expected_desc: "C-style for loops require a `let` initializer".into(),
                    found: SyntaxKind::FOR_EXPR,
                    at: it.parent,
                });
            };
            let Some(open_paren) = open_paren else {
                return Err(StrongAstError::UnexpectedKindDesc {
                    expected_desc: "C-style for loops require parentheses".into(),
                    found: SyntaxKind::FOR_EXPR,
                    at: it.parent,
                });
            };

            let condition = it.expect_next("an expression")?;
            let condition = Expression::from_cst(condition)?;

            let semicolon = it.expect_parse()?;

            let update = it.expect_next("an expression")?;
            let update = Expression::from_cst(update)?;

            let close_paren = it.expect_parse()?;

            ForArgs::CStyle(ForCStyleArgs {
                open_paren,
                init: *let_stmt,
                condition,
                semicolon,
                update: Box::new(update),
                close_paren,
            })
        };

        // BLOCK_EXPR
        let body: BlockExpr = it.expect_parse()?;

        it.expect_end()?;

        Ok(ForStmt {
            keyword,
            args,
            body,
        })
    }
}

impl KnownKind for ForStmt {
    fn kind() -> SyntaxKind {
        SyntaxKind::FOR_EXPR
    }
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ForArgs {
    Iterator(ForIteratorArgs),
    CStyle(ForCStyleArgs),
}

#[derive(Debug)]
pub struct ForCStyleArgs {
    pub open_paren: t::LParen,
    pub init: LetStmt,
    pub condition: Expression,
    pub semicolon: t::Semicolon,
    pub update: Box<Expression>,
    pub close_paren: t::RParen,
}

/// The binding side of a for-loop (`let i`, `let i: T`, or bare `i`).
#[derive(Debug)]
pub enum ForBinding {
    /// `for (let i in ...)` - full let-statement (may carry a type annotation).
    Let(Box<LetStmt>),
    /// `for (i in ...)` or `for i in ...` - bare identifier, no `let`.
    Bare(t::Word),
}

#[derive(Debug)]
pub struct ForIteratorArgs {
    /// `None` for the parens-less form `for i in expr { ... }`.
    pub open_paren: Option<t::LParen>,
    pub binding: ForBinding,
    pub in_keyword: t::In,
    pub expression: Expression,
    /// Mirrors `open_paren` - present iff `open_paren` is.
    pub close_paren: Option<t::RParen>,
}

/// Corresponds to a [`SyntaxKind::RETURN_STMT`] node.
#[derive(Debug)]
pub struct ReturnStmt {
    pub keyword: t::Return,
    /// Currently since all functions return a value, this should always be `Some` for valid code.
    /// However, we still handle the case of a missing return value here.
    pub value: Option<Expression>,
    pub semicolon: Option<t::Semicolon>,
}

impl FromCST for ReturnStmt {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::RETURN_STMT)?;

        let mut it = SyntaxNodeIter::new(&node);

        // KW_RETURN
        let keyword = it.expect_parse()?;

        // Optional return value
        let value = it
            .next_if(|elem| elem.kind() != SyntaxKind::SEMICOLON)
            .map(Expression::from_cst)
            .transpose()?;

        // Optional semicolon
        let semicolon = it.next().map(t::Semicolon::from_cst).transpose()?;

        it.expect_end()?;

        Ok(ReturnStmt {
            keyword,
            value,
            semicolon,
        })
    }
}

impl KnownKind for ReturnStmt {
    fn kind() -> SyntaxKind {
        SyntaxKind::RETURN_STMT
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

        let mut it = SyntaxNodeIter::new(&node);

        let keyword = it.expect_parse()?;

        let semicolon = it.next().map(t::Semicolon::from_cst).transpose()?;

        it.expect_end()?;

        Ok(BreakStmt { keyword, semicolon })
    }
}

impl KnownKind for BreakStmt {
    fn kind() -> SyntaxKind {
        SyntaxKind::BREAK_STMT
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

        let mut it = SyntaxNodeIter::new(&node);

        let keyword = it.expect_parse()?;

        let semicolon = it.next().map(t::Semicolon::from_cst).transpose()?;

        it.expect_end()?;

        Ok(ContinueStmt { keyword, semicolon })
    }
}

impl KnownKind for ContinueStmt {
    fn kind() -> SyntaxKind {
        SyntaxKind::CONTINUE_STMT
    }
}
