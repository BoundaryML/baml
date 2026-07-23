use super::{
    BlockExpr, BreakStmt, ContinueStmt, Expression, FromCST, HeaderComment, KnownKind,
    MatchPattern, OptionalUnless, ParenExpr, SyntaxElement, SyntaxKind, SyntaxNodeIter,
    TestExprDecl, TestSetDecl, TextRange, ValidatedAstError, t,
};

validated_ast_data! {
    /// Does not correspond to a specific [`SyntaxKind`], but contains all possible statements.
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
        TestExpr(TestExprDecl),
        /// A nested testset inside a testset body.
        TestSet(TestSetDecl),
        Unknown(TextRange),
    }
}

impl FromCST for Statement {
    fn from_cst(elem: SyntaxElement) -> Result<Self, ValidatedAstError> {
        match elem.kind() {
            SyntaxKind::LET_STMT => LetStmt::from_cst(elem).map(Statement::Let),
            SyntaxKind::RETURN_STMT => ReturnStmt::from_cst(elem).map(Statement::Return),
            SyntaxKind::WHILE_STMT => WhileStmt::from_cst(elem).map(Statement::While),
            SyntaxKind::WHILE_LET_STMT => WhileLetStmt::from_cst(elem).map(Statement::WhileLet),
            SyntaxKind::FOR_EXPR => ForStmt::from_cst(elem).map(Statement::For),
            SyntaxKind::BREAK_STMT => BreakStmt::try_from(elem)
                .map(Statement::Break)
                .map_err(ValidatedAstError::from),
            SyntaxKind::CONTINUE_STMT => ContinueStmt::try_from(elem)
                .map(Statement::Continue)
                .map_err(ValidatedAstError::from),
            SyntaxKind::SEMICOLON => t::Semicolon::from_cst(elem).map(Statement::EmptySemicolon),
            SyntaxKind::HEADER_COMMENT => {
                t::HeaderComment::from_cst(elem).map(Statement::HeaderComment)
            }
            SyntaxKind::TEST_EXPR_DEF => TestExprDecl::from_cst(elem).map(Statement::TestExpr),
            SyntaxKind::TESTSET_DEF => TestSetDecl::from_cst(elem).map(Statement::TestSet),
            _ => ExpressionStmt::from_cst(elem).map(Statement::Expr),
        }
    }
}

validated_ast_data! {
    /// Does not correspond to a [`SyntaxKind`], but parses some [`Expression`] as a statement.
    ///
    /// Unlike most implementations of `FromCST`, this will never parse the semicolon, as it is not a child of the node.
    /// Instead, the caller should check for a semicolon after the expression and add it to the `ExpressionStmt` if present.
    pub struct ExpressionStmt {
        pub expr: Expression,
        pub semicolon: Option<t::Semicolon>,
    }
}

impl FromCST for ExpressionStmt {
    fn from_cst(elem: SyntaxElement) -> Result<Self, ValidatedAstError> {
        let expr = Expression::from_cst(elem)?;
        Ok(ExpressionStmt {
            expr,
            semicolon: None,
        })
    }
}

validated_ast_data! {
    /// Corresponds to a [`SyntaxKind::LET_STMT`] node.
    ///
    /// Post-pattern-rewrite shape: `(KW_LET|KW_CONST)? PATTERN EQUALS? <expr>? (KW_ELSE BLOCK_EXPR)? SEMICOLON?`.
    /// Simple bindings carry the introducer inside the [`super::MatchPattern`] (e.g.
    /// `let x: int` parses as a `Chain([Bind, Type])`). Array destructuring uses
    /// the statement-level introducer before an `ARRAY_PATTERN`. The optional
    /// `else BLOCK_EXPR` tail is the `let ... else` form: a refutable binding
    /// whose else branch must diverge.
    pub struct LetStmt {
        pub let_keyword: Option<t::BindingKeyword>,
        pub pattern: MatchPattern,
        pub initializer: Option<(t::Equals, Expression)>,
        /// `else { ... }` tail for `let ... else`. None for plain `let`. Boxed
        /// to keep `LetStmt` (and the enclosing `Statement` enum) small - the
        /// else branch is rare and a `BlockExpr` carries a full statement
        /// vector.
        pub else_branch: Option<Box<(t::Else, BlockExpr)>>,
        /// Not required in some contexts like for-let loops
        pub semicolon: Option<t::Semicolon>,
    }
}

impl FromCST for LetStmt {
    fn from_cst(elem: SyntaxElement) -> Result<Self, ValidatedAstError> {
        let node = ValidatedAstError::assert_is_node(elem)?;
        ValidatedAstError::assert_kind_node(&node, SyntaxKind::LET_STMT)?;
        let mut it = SyntaxNodeIter::new(&node);
        let let_keyword = it
            .next_if(|elem| matches!(elem.kind(), SyntaxKind::KW_LET | SyntaxKind::KW_CONST))
            .map(t::BindingKeyword::from_cst)
            .transpose()?;
        let pattern: MatchPattern = it.expect_parse()?;
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
                BlockExpr::from_cst(block_elem)?,
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

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::WHILE_STMT`] node.
    WhileStmt, WHILE_STMT {
        keyword: required t::While;
        condition: required ParenExpr;
        body: required BlockExpr;
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::WHILE_LET_STMT`] node.
    ///
    /// `while let PATTERN = SCRUTINEE { BODY }`. Combines `WhileStmt`'s statement
    /// framing with `if let`'s `pattern = scrutinee` head, but - like `if let` and
    /// unlike plain `while` - emits no parens around the scrutinee, and has no
    /// `else` clause (loops produce unit).
    WhileLetStmt, WHILE_LET_STMT {
        keyword: required t::While;
        /// Standalone leading binding introducer, present only for top-level
        /// array-pattern heads (`while let [x] = xs`), where the parser keeps the
        /// introducer at the statement level instead of inside the pattern. For
        /// binding / class / type heads the introducer lives inside `pattern` and
        /// this is `None`. Mirrors
        /// `LetStmt::let_keyword`.
        let_keyword: optional_element t::BindingKeyword;
        pattern: required MatchPattern;
        equals: required t::Equals;
        scrutinee: boxed Expression;
        body: required BlockExpr;
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::FOR_EXPR`] node.
    custom ForStmt, FOR_EXPR, parse_for_stmt {
        keyword: t::For,
        args: ForArgs,
        body: BlockExpr,
    }
}

fn parse_for_stmt(elem: SyntaxElement) -> Result<ForStmt, ValidatedAstError> {
    let node = ValidatedAstError::assert_is_node(elem)?;
    ValidatedAstError::assert_kind_node(&node, SyntaxKind::FOR_EXPR)?;
    let mut it = SyntaxNodeIter::new(&node);
    let keyword = it.expect_parse()?;
    let open_paren: Option<t::LParen> = it
        .next_if_kind(SyntaxKind::L_PAREN)
        .map(t::LParen::from_cst)
        .transpose()?;
    let binding = if let Some(let_elem) = it.next_if_kind(SyntaxKind::LET_STMT) {
        ForBinding::Let(Box::new(LetStmt::from_cst(let_elem)?))
    } else {
        let word_elem = it.expect_next("for-loop binding (let or identifier)")?;
        let word = t::Word::from_cst(word_elem)?;
        ForBinding::Bare(word)
    };
    let args = if let Some(kw_in) = it.next_if_kind(SyntaxKind::KW_IN) {
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
        let ForBinding::Let(let_stmt) = binding else {
            return Err(ValidatedAstError::UnexpectedKindDesc {
                expected_desc: "C-style for loops require a `let` initializer".into(),
                found: SyntaxKind::FOR_EXPR,
                at: it.parent,
            });
        };
        let Some(open_paren) = open_paren else {
            return Err(ValidatedAstError::UnexpectedKindDesc {
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
    let body: BlockExpr = it.expect_parse()?;
    it.expect_end()?;
    Ok(ForStmt {
        keyword,
        args,
        body,
    })
}

validated_ast_data! {
    #[allow(clippy::large_enum_variant)]
    pub enum ForArgs {
        Iterator(ForIteratorArgs),
        CStyle(ForCStyleArgs),
    }
}

validated_ast_data! {
    pub struct ForCStyleArgs {
        pub open_paren: t::LParen,
        pub init: LetStmt,
        pub condition: Expression,
        pub semicolon: t::Semicolon,
        pub update: Box<Expression>,
        pub close_paren: t::RParen,
    }
}

validated_ast_data! {
    /// The binding side of a for-loop (`let i`, `let i: T`, or bare `i`).
    pub enum ForBinding {
        /// `for (let i in ...)` - full let-statement (may carry a type annotation).
        Let(Box<LetStmt>),
        /// `for (i in ...)` or `for i in ...` - bare identifier, no `let`.
        Bare(t::Word),
    }
}

validated_ast_data! {
    pub struct ForIteratorArgs {
        /// `None` for the parens-less form `for i in expr { ... }`.
        pub open_paren: Option<t::LParen>,
        pub binding: ForBinding,
        pub in_keyword: t::In,
        pub expression: Expression,
        /// Mirrors `open_paren` - present iff `open_paren` is.
        pub close_paren: Option<t::RParen>,
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::RETURN_STMT`] node.
    ReturnStmt, RETURN_STMT {
        keyword: required t::Return;
        /// Currently since all functions return a value, this should always be `Some` for valid code.
        /// However, we still handle the case of a missing return value here.
        value: spec OptionalUnless<Expression, t::Semicolon>;
        semicolon: optional_element t::Semicolon;
    }
}
