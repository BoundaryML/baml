use baml_db::baml_compiler_syntax::{SyntaxElement, SyntaxKind};
use rowan::TextRange;

use super::tokens as t;
use crate::{
    ast::{
        BlockExpr, Expression, FromCST, HeaderComment, KnownKind, MatchPattern, ParenExpr,
        StrongAstError, SyntaxNodeIter, TestExprDecl, TestSetDecl, Token,
    },
    printer::{PrintInfo, PrintMultiLine, Printable, Printer, Shape},
    trivia_classifier::TriviaSliceExt,
};

/// Does not correspond to a specific [`SyntaxKind`], but contains all possible statements.
//
// `For(ForStmt)` is the largest variant (~720 bytes); the next-largest sits
// well below it. The size difference is acknowledged here rather than
// boxed because `Statement` is constructed transiently during formatting,
// not stored at scale.
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
    TestExpr(TestExprDecl),
    /// A nested testset inside a testset body.
    TestSet(TestSetDecl),
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
            SyntaxKind::TEST_EXPR_DEF => TestExprDecl::from_cst(elem).map(Statement::TestExpr),
            SyntaxKind::TESTSET_DEF => TestSetDecl::from_cst(elem).map(Statement::TestSet),
            _ => ExpressionStmt::from_cst(elem).map(Statement::Expr),
        }
    }
}

impl Printable for Statement {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            Statement::Expr(expression_stmt) => expression_stmt.print(shape, printer),
            Statement::Let(let_stmt) => let_stmt.print(shape, printer),
            Statement::While(while_stmt) => while_stmt.print(shape, printer),
            Statement::WhileLet(while_let_stmt) => while_let_stmt.print(shape, printer),
            Statement::Return(return_stmt) => return_stmt.print(shape, printer),
            Statement::Break(break_stmt) => break_stmt.print(shape, printer),
            Statement::Continue(continue_stmt) => continue_stmt.print(shape, printer),
            Statement::For(for_stmt) => for_stmt.print(shape, printer),
            Statement::HeaderComment(header_comment) => {
                printer.print_raw_token(header_comment);
                PrintInfo::default_single_line()
            }
            Statement::EmptySemicolon(semicolon) => {
                printer.print_raw_token(semicolon);
                PrintInfo::default_single_line()
            }
            Statement::TestExpr(test_expr_decl) => test_expr_decl.print(shape, printer),
            Statement::TestSet(test_set_decl) => test_set_decl.print(shape, printer),
            Statement::Unknown(range) => {
                printer.print_input_range_trimmed_start(*range);
                PrintInfo::default_multi_lined()
            }
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            Statement::Expr(expr) => expr.leftmost_token(),
            Statement::Let(let_stmt) => let_stmt.leftmost_token(),
            Statement::While(while_stmt) => while_stmt.leftmost_token(),
            Statement::WhileLet(while_let_stmt) => while_let_stmt.leftmost_token(),
            Statement::Return(return_stmt) => return_stmt.leftmost_token(),
            Statement::Break(break_stmt) => break_stmt.leftmost_token(),
            Statement::Continue(continue_stmt) => continue_stmt.leftmost_token(),
            Statement::For(for_stmt) => for_stmt.leftmost_token(),
            Statement::HeaderComment(header_comment) => header_comment.span(),
            Statement::EmptySemicolon(semicolon) => semicolon.span(),
            Statement::TestExpr(t) => t.leftmost_token(),
            Statement::TestSet(t) => t.leftmost_token(),
            Statement::Unknown(range) => *range,
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            Statement::Expr(expr) => expr.rightmost_token(),
            Statement::Let(let_stmt) => let_stmt.rightmost_token(),
            Statement::While(while_stmt) => while_stmt.rightmost_token(),
            Statement::WhileLet(while_let_stmt) => while_let_stmt.rightmost_token(),
            Statement::Return(return_stmt) => return_stmt.rightmost_token(),
            Statement::Break(break_stmt) => break_stmt.rightmost_token(),
            Statement::Continue(continue_stmt) => continue_stmt.rightmost_token(),
            Statement::For(for_stmt) => for_stmt.rightmost_token(),
            Statement::HeaderComment(header_comment) => header_comment.span(),
            Statement::EmptySemicolon(semicolon) => semicolon.span(),
            Statement::TestExpr(t) => t.rightmost_token(),
            Statement::TestSet(t) => t.rightmost_token(),
            Statement::Unknown(range) => *range,
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

impl Printable for ExpressionStmt {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let info = printer.print(&self.expr, shape);
        if let Some(semicolon) = &self.semicolon {
            // Trivia between expr and semicolon
            let expr_trailing = printer.trivia.get_trailing_for_element(&self.expr);
            printer.print_trivia_squished(expr_trailing);
            let (semicolon_leading, _) = printer.trivia.get_for_range_split(semicolon.span());
            printer.print_trivia_squished(semicolon_leading);
            printer.print_raw_token(semicolon);
        } else if self.expr.statement_needs_semicolon() {
            printer.print_str(";");
        }
        info
    }
    fn leftmost_token(&self) -> TextRange {
        self.expr.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        if let Some(semicolon) = &self.semicolon {
            semicolon.span()
        } else {
            self.expr.rightmost_token()
        }
    }
}

/// Corresponds to a [`SyntaxKind::LET_STMT`] node.
///
/// Post-pattern-rewrite shape: `(KW_LET|KW_CONST)? PATTERN EQUALS? <expr>? (KW_ELSE BLOCK_EXPR)? SEMICOLON?`.
/// Simple bindings carry the introducer inside the [`super::MatchPattern`] (e.g.
/// `let x: int` parses as a `Chain([Bind, Type])`). Array destructuring uses
/// the statement-level introducer before an `ARRAY_PATTERN`. The optional
/// `else BLOCK_EXPR` tail is the `let … else` form: a refutable binding
/// whose else branch must diverge.
#[derive(Debug)]
pub struct LetStmt {
    pub let_keyword: Option<t::BindingKeyword>,
    pub pattern: super::MatchPattern,
    pub initializer: Option<(t::Equals, Expression)>,
    /// `else { … }` tail for `let … else`. None for plain `let`. Boxed
    /// to keep `LetStmt` (and the enclosing `Statement` enum) small — the
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

impl Printable for LetStmt {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;

        if let Some(let_keyword) = &self.let_keyword {
            printer.print_raw_token(let_keyword);
            printer.print_str(" ");
            // Preserve trivia between the introducer and the pattern — e.g.
            // `let /*keep*/ [x]` would otherwise lose the comment.
            let (_, let_trailing) = printer.trivia.get_for_range_split(let_keyword.span());
            if printer.print_trivia_squished(let_trailing) > 0 {
                printer.print_str(" ");
            }
        }
        // Simple binding patterns carry `let`, the binding name, and any `: T` narrow.
        multi_lined |= printer.print(&self.pattern, shape.clone()).multi_lined;

        if let Some((equals, expr)) = &self.initializer {
            let (_, equals_trailing) = printer.trivia.get_for_range_split(equals.span());
            printer.print_str(" ");
            printer.print_raw_token(equals);
            printer.print_str(" ");
            printer.print_trivia_squished(equals_trailing);
            let expr_leading = printer.trivia.get_leading_for_element(expr);
            printer.print_trivia_squished(expr_leading);
            multi_lined |= printer.print(expr, shape.clone()).multi_lined;
            // Trailing trivia between the initializer expression and what
            // follows: a semicolon, or an `else { … }` tail. Either way,
            // print it so inline comments aren't dropped.
            if (self.else_branch.is_none() && self.semicolon.is_some())
                || self.else_branch.is_some()
            {
                let expr_trailing = printer.trivia.get_trailing_for_element(expr);
                printer.print_trivia_squished(expr_trailing);
            }
        }

        if let Some(else_branch) = self.else_branch.as_deref() {
            let (else_kw, block) = else_branch;
            // Preserve trivia adjacent to the `else` keyword instead of
            // hardcoding bare spaces — a `// note` between the init and
            // `else`, or between `else` and the block, would otherwise be
            // dropped.
            let (else_leading, else_trailing) = printer.trivia.get_for_range_split(else_kw.span());
            printer.print_str(" ");
            printer.print_trivia_squished(else_leading);
            printer.print_raw_token(else_kw);
            printer.print_str(" ");
            printer.print_trivia_squished(else_trailing);
            multi_lined |= printer.print(block, shape).multi_lined;
        }

        if let Some(semicolon) = &self.semicolon {
            let (semicolon_leading, _) = printer.trivia.get_for_range_split(semicolon.span());
            printer.print_trivia_squished(semicolon_leading);
            printer.print_raw_token(semicolon);
        } else {
            printer.print_str(";");
        }
        PrintInfo { multi_lined }
    }
    fn leftmost_token(&self) -> TextRange {
        if let Some(let_keyword) = &self.let_keyword {
            let_keyword.span()
        } else {
            self.pattern.leftmost_token()
        }
    }
    fn rightmost_token(&self) -> TextRange {
        if let Some(semicolon) = &self.semicolon {
            return semicolon.span();
        }
        if let Some(else_branch) = self.else_branch.as_deref() {
            return else_branch.1.rightmost_token();
        }
        if let Some((_, expr)) = &self.initializer {
            return expr.rightmost_token();
        }
        self.pattern.rightmost_token()
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

impl Printable for WhileStmt {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");

        let condition_shape = Shape {
            width: shape.width,
            indent: shape.indent,
            first_line_offset: shape.first_line_offset + const { "while ".len() },
        };
        printer.print(&self.condition, condition_shape);

        printer.print_str(" ");

        let body_shape = Shape {
            width: shape.width,
            indent: shape.indent,
            first_line_offset: 0, // irrelevant since body new-lines immediately after `{`
        };
        printer.print(&self.body, body_shape);
        PrintInfo::default_multi_lined()
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.body.rightmost_token()
    }
}

/// Corresponds to a [`SyntaxKind::WHILE_LET_STMT`] node.
///
/// `while let PATTERN = SCRUTINEE { BODY }`. Combines `WhileStmt`'s statement
/// framing with `if let`'s `pattern = scrutinee` head, but — like `if let` and
/// unlike plain `while` — emits no parens around the scrutinee, and has no
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

impl Printable for WhileLetStmt {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        // A standalone `let` is present only for array-pattern heads; for other
        // heads the `let` lives inside the pattern. No parens around the pattern
        // or scrutinee (mirrors `if let`, unlike plain `while`).
        if let Some(let_keyword) = &self.let_keyword {
            printer.print_raw_token(let_keyword);
            printer.print_str(" ");
            let (_, trailing) = printer.trivia.get_for_range_split(let_keyword.span());
            if printer.print_trivia_squished(trailing) > 0 {
                printer.print_str(" ");
            }
        }
        printer.print(&self.pattern, shape.clone());
        printer.print_str(" ");
        printer.print_raw_token(&self.equals);
        printer.print_str(" ");
        printer.print(&*self.scrutinee, shape.clone());
        printer.print_str(" ");

        let body_shape = Shape {
            width: shape.width,
            indent: shape.indent,
            first_line_offset: 0, // irrelevant since body new-lines immediately after `{`
        };
        printer.print(&self.body, body_shape);
        PrintInfo::default_multi_lined()
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.body.rightmost_token()
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
        //   for (let i in expr) { ... }       — paren + LET_STMT
        //   for (let i = 0; cond; upd) { ... } — paren + LET_STMT + C-style
        //   for (i in expr) { ... }            — paren + bare WORD
        //   for i in expr { ... }              — no paren + bare WORD
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
            // C-style — only valid with a `let` binding and parens
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

impl Printable for ForStmt {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        printer.print(&self.args, shape.clone());
        printer.print_str(" ");
        printer.print(&self.body, shape);
        PrintInfo::default_multi_lined()
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.body.rightmost_token()
    }
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ForArgs {
    Iterator(ForIteratorArgs),
    CStyle(ForCStyleArgs),
}

impl Printable for ForArgs {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            ForArgs::Iterator(iter) => iter.print(shape, printer),
            ForArgs::CStyle(cstyle) => cstyle.print(shape, printer),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            ForArgs::Iterator(iter) => iter.leftmost_token(),
            ForArgs::CStyle(cstyle) => cstyle.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            ForArgs::Iterator(iter) => iter.rightmost_token(),
            ForArgs::CStyle(cstyle) => cstyle.rightmost_token(),
        }
    }
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

impl PrintMultiLine for ForCStyleArgs {
    /// Multi-line layout: each section (init, condition, update) on its own
    /// indented line. Parens wrap the entire construct.
    ///
    /// ```baml
    /// (
    ///     let i = 0;
    ///     i < some_long_expression;
    ///     i = i + 1
    /// )
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_shape = Shape::standalone(
            printer.config.line_width,
            shape.indent + printer.config.indent_width,
        );

        printer.print_raw_token(&self.open_paren);
        printer.print_trivia_all_trailing_for(self.open_paren.span());
        printer.print_newline();

        let (init_leading, init_trailing) = printer.trivia.get_for_element(&self.init);
        printer.print_trivia_with_newline(init_leading.trim_blanks(), inner_shape.indent);
        printer.print_spaces(inner_shape.indent);
        self.init.print(inner_shape.clone(), printer);
        printer.print_trivia_trailing(init_trailing);
        printer.print_newline();

        let (cond_leading, cond_trailing) = printer.trivia.get_for_element(&self.condition);
        printer.print_trivia_with_newline(cond_leading.trim_blanks(), inner_shape.indent);
        printer.print_spaces(inner_shape.indent);
        self.condition.print(inner_shape.clone(), printer);
        printer.print_trivia_squished(cond_trailing); // always squished before `;`

        let (semi_leading, semi_trailing) =
            printer.trivia.get_for_range_split(self.semicolon.span());
        printer.print_trivia_squished(semi_leading); // always squished before `;`
        printer.print_raw_token(&self.semicolon);
        printer.print_trivia_trailing(semi_trailing);
        printer.print_newline();

        let (update_leading, update_trailing) = printer.trivia.get_for_element(&*self.update);
        printer.print_trivia_with_newline(update_leading.trim_blanks(), inner_shape.indent);
        printer.print_spaces(inner_shape.indent);
        self.update.print(inner_shape.clone(), printer);
        printer.print_trivia_trailing(update_trailing);
        printer.print_newline();

        let (close_paren_leading, _) = printer.trivia.get_for_range_split(self.close_paren.span());
        printer.print_trivia_with_newline(close_paren_leading.trim_blanks(), inner_shape.indent);

        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_paren);
        PrintInfo::default_multi_lined()
    }
}

impl ForCStyleArgs {
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        printer.print_raw_token(&self.open_paren);
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_paren.span());
        printer.try_print_trivia_single_line_squished(open_trailing)?;

        let (init_leading, init_trailing) = printer.trivia.get_for_element(&self.init);
        printer.try_print_trivia_single_line_squished(init_leading)?;
        if printer
            .print(&self.init, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        printer.try_print_trivia_single_line_squished(init_trailing)?;
        printer.print_str(" ");

        let (cond_leading, cond_trailing) = printer.trivia.get_for_element(&self.condition);
        printer.try_print_trivia_single_line_squished(cond_leading)?;
        if printer
            .print(&self.condition, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        printer.print_trivia_squished(cond_trailing); // always squished before `;`

        let (semi_leading, semi_trailing) =
            printer.trivia.get_for_range_split(self.semicolon.span());
        printer.print_trivia_squished(semi_leading); // always squished before `;`
        printer.print_raw_token(&self.semicolon);
        printer.try_print_trivia_single_line_squished(semi_trailing)?;
        printer.print_str(" ");

        let (update_leading, update_trailing) = printer.trivia.get_for_element(&*self.update);
        printer.try_print_trivia_single_line_squished(update_leading)?;
        if printer
            .print(&*self.update, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        printer.try_print_trivia_single_line_squished(update_trailing)?;

        let (close_leading, _) = printer.trivia.get_for_range_split(self.close_paren.span());
        printer.try_print_trivia_single_line_squished(close_leading)?;
        printer.print_raw_token(&self.close_paren);

        if printer.output.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
    }
}

impl Printable for ForCStyleArgs {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
    fn leftmost_token(&self) -> TextRange {
        self.open_paren.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.close_paren.span()
    }
}

/// The binding side of a for-loop (`let i`, `let i: T`, or bare `i`).
#[derive(Debug)]
pub enum ForBinding {
    /// `for (let i in ...)` — full let-statement (may carry a type annotation).
    Let(Box<LetStmt>),
    /// `for (i in ...)` or `for i in ...` — bare identifier, no `let`.
    Bare(t::Word),
}

#[derive(Debug)]
pub struct ForIteratorArgs {
    /// `None` for the parens-less form `for i in expr { ... }`.
    pub open_paren: Option<t::LParen>,
    pub binding: ForBinding,
    pub in_keyword: t::In,
    pub expression: Expression,
    /// Mirrors `open_paren` — present iff `open_paren` is.
    pub close_paren: Option<t::RParen>,
}

impl PrintMultiLine for ForIteratorArgs {
    /// Multi-line layout: the iterator expression wraps to an indented new line
    /// after the `in` keyword.
    ///
    /// ```baml
    /// for (
    ///     let variable in some_long_iterator_expression
    /// )
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_shape =
            Shape::standalone(shape.width, shape.indent + printer.config.indent_width);

        if let Some(open_paren) = &self.open_paren {
            printer.print_raw_token(open_paren);
            printer.print_trivia_all_trailing_for(open_paren.span());
            printer.print_newline();
            let binding_leading = match &self.binding {
                ForBinding::Let(let_stmt) => printer.trivia.get_leading_for_element(&**let_stmt),
                ForBinding::Bare(word) => printer.trivia.get_for_range_split(word.span()).0,
            };
            printer.print_trivia_with_newline(binding_leading, inner_shape.indent);
            printer.print_spaces(inner_shape.indent);
        }

        match &self.binding {
            ForBinding::Let(let_stmt) => {
                if let Some(let_keyword) = &let_stmt.let_keyword {
                    printer.print_raw_token(let_keyword);
                    printer.print_spaces(1);
                    // Preserve trivia between `let` and the pattern.
                    let (_, let_trailing) = printer.trivia.get_for_range_split(let_keyword.span());
                    if printer.print_trivia_squished(let_trailing) > 0 {
                        printer.print_spaces(1);
                    }
                }
                printer.print(&let_stmt.pattern, inner_shape.clone());
            }
            ForBinding::Bare(word) => {
                printer.print_raw_token(word);
            }
        }
        printer.print_str(" ");
        printer.print_raw_token(&self.in_keyword);
        printer.print_spaces(1);

        let (_, in_trailing) = printer.trivia.get_for_range_split(self.in_keyword.span());
        let (expr_leading, expr_trailing) = printer.trivia.get_for_element(&self.expression);
        printer.print_trivia_squished(in_trailing);
        printer.print_trivia_squished(expr_leading);
        let curr_line_len = printer.current_line_len();
        let offset = curr_line_len.saturating_sub(inner_shape.indent);
        let expr_shape = Shape {
            width: printer.config.line_width.saturating_sub(curr_line_len),
            indent: inner_shape.indent,
            first_line_offset: offset,
        };
        self.expression.print(expr_shape, printer);
        printer.print_trivia_trailing(expr_trailing);

        if let Some(close_paren) = &self.close_paren {
            printer.print_newline();
            let (close_paren_leading, _) = printer.trivia.get_for_range_split(close_paren.span());
            printer.print_trivia_with_newline(close_paren_leading, inner_shape.indent);
            printer.print_spaces(shape.indent);
            printer.print_raw_token(close_paren);
        }
        PrintInfo::default_multi_lined()
    }
}

impl ForIteratorArgs {
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        if let Some(open_paren) = &self.open_paren {
            printer.print_raw_token(open_paren);
            let (_, open_trailing) = printer.trivia.get_for_range_split(open_paren.span());
            printer.try_print_trivia_single_line_squished(open_trailing)?;
        }

        match &self.binding {
            ForBinding::Let(let_stmt) => {
                if let Some(let_keyword) = &let_stmt.let_keyword {
                    printer.print_raw_token(let_keyword);
                    printer.print_spaces(1);
                    // Preserve trivia between `let` and the pattern.
                    let (_, let_trailing) = printer.trivia.get_for_range_split(let_keyword.span());
                    if printer.print_trivia_squished(let_trailing) > 0 {
                        printer.print_spaces(1);
                    }
                }
                if printer
                    .print(&let_stmt.pattern, Shape::unlimited_single_line())
                    .multi_lined
                {
                    return None;
                }
            }
            ForBinding::Bare(word) => {
                printer.print_raw_token(word);
            }
        }
        printer.print_str(" ");
        printer.print_raw_token(&self.in_keyword);
        printer.print_str(" ");

        let (_, in_trailing) = printer.trivia.get_for_range_split(self.in_keyword.span());
        let (expr_leading, expr_trailing) = printer.trivia.get_for_element(&self.expression);
        printer.print_trivia_squished(in_trailing);
        printer.print_trivia_squished(expr_leading);
        if printer
            .print(&self.expression, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        printer.try_print_trivia_single_line_squished(expr_trailing)?;

        if let Some(close_paren) = &self.close_paren {
            let (close_leading, _) = printer.trivia.get_for_range_split(close_paren.span());
            printer.try_print_trivia_single_line_squished(close_leading)?;
            printer.print_raw_token(close_paren);
        }

        if printer.output.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
    }
}

impl Printable for ForIteratorArgs {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
    fn leftmost_token(&self) -> TextRange {
        if let Some(open_paren) = &self.open_paren {
            return open_paren.span();
        }
        match &self.binding {
            ForBinding::Let(let_stmt) => let_stmt
                .let_keyword
                .as_ref()
                .map(Token::span)
                .unwrap_or_else(|| let_stmt.pattern.leftmost_token()),
            ForBinding::Bare(word) => word.span(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        if let Some(close_paren) = &self.close_paren {
            return close_paren.span();
        }
        self.expression.rightmost_token()
    }
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

impl Printable for ReturnStmt {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);
        if self.value.is_some() || self.semicolon.is_some() {
            // kw is not the last element
            let (_, kw_trailing) = printer.trivia.get_for_range_split(self.keyword.span());
            printer.print_trivia_squished(kw_trailing);
        }

        if let Some(value) = &self.value {
            let (value_leading, value_trailing) = printer.trivia.get_for_element(value);
            printer.print_str(" ");
            printer.print_trivia_squished(value_leading);
            printer.print(value, shape);
            if self.semicolon.is_some() {
                // value is not the last element
                printer.print_trivia_squished(value_trailing);
            }
        }

        if let Some(semicolon) = &self.semicolon {
            let (semicolon_leading, _) = printer.trivia.get_for_range_split(semicolon.span());
            printer.print_trivia_squished(semicolon_leading);
            printer.print_raw_token(semicolon);
        } else {
            printer.print_str(";");
        }

        PrintInfo::default_single_line()
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        if let Some(semicolon) = &self.semicolon {
            return semicolon.span();
        }
        if let Some(value) = &self.value {
            return value.rightmost_token();
        }
        self.keyword.span()
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

impl Printable for BreakStmt {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);

        if let Some(semicolon) = self.semicolon.as_ref() {
            printer.print_raw_token(semicolon);
        } else {
            printer.print_str(";");
        }

        PrintInfo::default_single_line()
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.semicolon
            .as_ref()
            .map_or(self.keyword.span(), Token::span)
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

impl Printable for ContinueStmt {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);
        if let Some(semicolon) = self.semicolon.as_ref() {
            printer.print_raw_token(semicolon);
        } else {
            printer.print_str(";");
        }
        PrintInfo::default_single_line()
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.semicolon
            .as_ref()
            .map_or(self.keyword.span(), Token::span)
    }
}
