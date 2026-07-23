use super::{
    AstToken, BinaryOp, FromCST, FunctionParamList, KnownKind, Literal, MatchPattern, Statement,
    StrongAstError, SyntaxElement, SyntaxKind, SyntaxNodeIter, TextRange, Type, UnaryOp, t,
};

validated_ast_data! {
    pub enum Expression {
        Literal(Literal),
        /// Includes things like `null`, `true`, `false`, `baml.fs`, etc.
        Path(PathExpr),
        /// A generic instantiation whose base is NOT a plain path - e.g.
        /// `(<T>(x: T) -> T { x })<int>` or `(foo)<int>`. The path-based form
        /// (`foo<int>`, `a.b.foo<int>`) is carried by [`PathExpr::generic_args`];
        /// the parser wraps both in a `PATH_EXPR` node, so this is selected when
        /// that node's first child is not a word/path.
        GenericApply(GenericApplyExpr),
        Paren(ParenExpr),
        Binary(BinaryExpr),
        Is(IsExpr),
        Unary(UnaryExpr),
        If(IfExpr),
        IfLet(IfLetExpr),
        Match(MatchExpr),
        Catch(CatchExpr),
        Call(CallExpr),
        Index(IndexExpr),
        FieldAccess(FieldAccessExpr),
        OptionalFieldAccess(OptionalFieldAccessExpr),
        OptionalIndex(OptionalIndexExpr),
        OptionalCall(OptionalCallExpr),
        EnvAccess(EnvAccessExpr),
        Block(BlockExpr),
        ArrayInitializer(ArrayInitializer),
        MapInitializer(MapLiteral),
        ObjectInitializer(ObjectInitializer),
        RawString(t::RawString),
        BacktickString(t::BacktickString),
        ByteString(t::ByteString),
        Lambda(Box<LambdaExpr>),
        /// A `spawn name? (with opts)? { ... }` task-spawn expression (BEP-034).
        Spawn(Box<SpawnExpr>),
        /// A braceless `return ...` in expression position (a `RETURN_EXPR`, e.g. a
        /// `catch`/`match` arm value like `_ => return 0`). Printed verbatim, like
        /// [`Expression::Unknown`] and backed by the same [`VerbatimSpan`], but kept
        /// as a distinct variant so the arm printers can recognize it: when they wrap
        /// a braceless arm body into a block they append the `;` that a block-position
        /// `return` requires, so the output round-trips through `RETURN_STMT` (i.e. is
        /// idempotent).
        Return(VerbatimSpan),
        /// A braceless `break` in expression position (a `BREAK_EXPR`, e.g. a
        /// `catch`/`match` arm value like `0 => break`). Handled exactly like
        /// [`Expression::Return`] and backed by the same [`VerbatimSpan`]: when an arm
        /// printer wraps it into a block it appends the `;` that a block-position
        /// `break` requires, so the output round-trips through `BREAK_STMT`.
        Break(VerbatimSpan),
        /// A braceless `continue` in expression position (a `CONTINUE_EXPR`). The
        /// `continue` counterpart of [`Expression::Break`].
        Continue(VerbatimSpan),
        Unknown(VerbatimSpan),
    }
}

validated_ast_data! {
    /// A node the strong AST does not model and prints verbatim: an unmodeled
    /// expression (e.g. `defer { ... }`, `throw e`, `await f`,
    /// `x.as<T>`) held as [`Expression::Unknown`], or a braceless jump held as
    /// [`Expression::Return`], [`Expression::Break`], or [`Expression::Continue`].
    ///
    /// Rather than a single whole-node span, this carries the node's true first and
    /// last *token* ranges. The trivia classifier keys leading/trailing comments to
    /// individual token ranges, so [`Printable::leftmost_token`] /
    /// [`Printable::rightmost_token`] must return those exact token ranges for a
    /// comment to attach and emit. A whole-node span never matches a token key, so
    /// a trailing comment on the node was silently dropped - the `defer` statement
    /// comment-loss bug (B-629), and the same class of bug for a braceless `return`
    /// arm. A whole-node span can also begin inside leading trivia (the parser
    /// attaches a preceding comment to the node), which would re-print that comment
    /// verbatim at the wrong indent; the `content_range` used for printing excludes
    /// it.
    pub struct VerbatimSpan {
        /// Range of the first non-trivia token - the leading-trivia anchor.
        first_token: TextRange,
        /// Range of the last non-trivia token - the trailing-trivia anchor.
        last_token: TextRange,
    }
}

impl VerbatimSpan {
    fn from_element(elem: &SyntaxElement) -> Self {
        if let Some(node) = elem.as_node() {
            let mut tokens = node
                .descendants_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .filter(|token| !token.kind().is_trivia());
            if let Some(first) = tokens.next() {
                let first_token = first.text_range();
                let last_token = tokens
                    .last()
                    .map_or(first_token, |token| token.text_range());
                return Self {
                    first_token,
                    last_token,
                };
            }
        }

        let whole = elem.text_range();
        Self {
            first_token: whole,
            last_token: whole,
        }
    }

    pub fn content_range(&self) -> TextRange {
        TextRange::new(self.first_token.start(), self.last_token.end())
    }

    pub fn first_token(&self) -> TextRange {
        self.first_token
    }

    pub fn last_token(&self) -> TextRange {
        self.last_token
    }
}

impl Expression {
    #[must_use]
    pub const fn statement_needs_semicolon(&self) -> bool {
        !matches!(
            self,
            Expression::If(_)
                | Expression::IfLet(_)
                | Expression::Match(_)
                | Expression::Lambda(_)
                | Expression::Spawn(_)
                | Expression::Unknown(_)
        )
    }
}

impl FromCST for Expression {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let expr = match elem.kind() {
            SyntaxKind::STRING_LITERAL => t::QuotedString::from_cst(elem)
                .map(Literal::String)
                .map(Expression::Literal)?,
            SyntaxKind::INTEGER_LITERAL => Expression::Literal(Literal::Integer(
                t::IntegerLiteral::new_from_span(elem.text_range()),
            )),
            SyntaxKind::FLOAT_LITERAL => Expression::Literal(Literal::Float(
                t::FloatLiteral::new_from_span(elem.text_range()),
            )),
            SyntaxKind::KW_TRUE | SyntaxKind::KW_FALSE | SyntaxKind::KW_NULL => {
                Literal::from_cst(elem).map(Expression::Literal)?
            }
            SyntaxKind::WORD => PathExpr::from_cst(elem).map(Expression::Path)?,
            SyntaxKind::PATH_EXPR => {
                let node = StrongAstError::assert_is_node(elem.clone())?;
                let base_is_path = SyntaxNodeIter::new(&node)
                    .next()
                    .is_some_and(|c| matches!(c.kind(), SyntaxKind::WORD | SyntaxKind::PATH_EXPR));
                if base_is_path {
                    PathExpr::from_cst(elem).map(Expression::Path)?
                } else {
                    GenericApplyExpr::from_cst(elem).map(Expression::GenericApply)?
                }
            }
            SyntaxKind::PAREN_EXPR => ParenExpr::from_cst(elem).map(Expression::Paren)?,
            SyntaxKind::BINARY_EXPR => BinaryExpr::from_cst(elem).map(Expression::Binary)?,
            SyntaxKind::IS_EXPR => IsExpr::from_cst(elem).map(Expression::Is)?,
            SyntaxKind::UNARY_EXPR => UnaryExpr::from_cst(elem).map(Expression::Unary)?,
            SyntaxKind::IF_EXPR => IfExpr::from_cst(elem).map(Expression::If)?,
            SyntaxKind::IF_LET_EXPR => IfLetExpr::from_cst(elem).map(Expression::IfLet)?,
            SyntaxKind::MATCH_EXPR => MatchExpr::from_cst(elem).map(Expression::Match)?,
            SyntaxKind::CATCH_EXPR => CatchExpr::from_cst(elem).map(Expression::Catch)?,
            SyntaxKind::CALL_EXPR => CallExpr::from_cst(elem).map(Expression::Call)?,
            SyntaxKind::INDEX_EXPR => IndexExpr::from_cst(elem).map(Expression::Index)?,
            SyntaxKind::FIELD_ACCESS_EXPR => {
                FieldAccessExpr::from_cst(elem).map(Expression::FieldAccess)?
            }
            SyntaxKind::OPTIONAL_FIELD_ACCESS_EXPR => {
                OptionalFieldAccessExpr::from_cst(elem).map(Expression::OptionalFieldAccess)?
            }
            SyntaxKind::OPTIONAL_INDEX_EXPR => {
                OptionalIndexExpr::from_cst(elem).map(Expression::OptionalIndex)?
            }
            SyntaxKind::OPTIONAL_CALL_EXPR => {
                OptionalCallExpr::from_cst(elem).map(Expression::OptionalCall)?
            }
            SyntaxKind::ENV_ACCESS_EXPR => {
                EnvAccessExpr::from_cst(elem).map(Expression::EnvAccess)?
            }
            SyntaxKind::BLOCK_EXPR => BlockExpr::from_cst(elem).map(Expression::Block)?,
            SyntaxKind::ARRAY_LITERAL => {
                ArrayInitializer::from_cst(elem).map(Expression::ArrayInitializer)?
            }
            SyntaxKind::MAP_LITERAL => {
                MapLiteral::from_cst(elem).map(Expression::MapInitializer)?
            }
            SyntaxKind::OBJECT_LITERAL => {
                ObjectInitializer::from_cst(elem).map(Expression::ObjectInitializer)?
            }
            SyntaxKind::RAW_STRING_LITERAL => {
                t::RawString::from_cst(elem).map(Expression::RawString)?
            }
            SyntaxKind::BACKTICK_STRING_LITERAL => {
                t::BacktickString::from_cst(elem).map(Expression::BacktickString)?
            }
            SyntaxKind::BYTE_STRING_LITERAL => {
                t::ByteString::from_cst(elem).map(Expression::ByteString)?
            }
            SyntaxKind::LAMBDA_EXPR => Expression::Lambda(Box::new(LambdaExpr::from_cst(elem)?)),
            SyntaxKind::SPAWN_EXPR => Expression::Spawn(Box::new(SpawnExpr::from_cst(elem)?)),
            SyntaxKind::RETURN_EXPR => Expression::Return(VerbatimSpan::from_element(&elem)),
            SyntaxKind::BREAK_EXPR => Expression::Break(VerbatimSpan::from_element(&elem)),
            SyntaxKind::CONTINUE_EXPR => Expression::Continue(VerbatimSpan::from_element(&elem)),
            _ => Expression::Unknown(VerbatimSpan::from_element(&elem)),
        };
        Ok(expr)
    }
}

validated_ast_data! {
    /// Corresponds to either a [`SyntaxKind::PATH_EXPR`] node or single [`SyntaxKind::WORD`] token.
    pub struct PathExpr {
        pub first: t::Word,
        pub rest: Vec<(t::Dot, t::Word)>,
        /// Trailing generic arguments, e.g. the `<int, string>` in `f<int, string>`
        /// or `baml.fetch_as<Todo>`. Only present at the tail of the path.
        pub generic_args: Option<GenericArgs>,
    }
}

fn is_path_segment_kind(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::WORD | SyntaxKind::KW_CLIENT | SyntaxKind::KW_SPAWN | SyntaxKind::KW_AWAIT
    )
}

fn path_segment_from_cst(elem: SyntaxElement) -> Result<t::Word, StrongAstError> {
    let token = StrongAstError::assert_is_token(elem)?;
    if is_path_segment_kind(token.kind()) {
        Ok(t::Word::new_from_span(token.text_range()))
    } else {
        Err(StrongAstError::UnexpectedKindDesc {
            expected_desc: "path segment".into(),
            found: token.kind(),
            at: token.text_range(),
        })
    }
}

impl FromCST for PathExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        if is_path_segment_kind(elem.kind()) {
            let first = path_segment_from_cst(elem)?;
            return Ok(PathExpr {
                first,
                rest: Vec::new(),
                generic_args: None,
            });
        }
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::PATH_EXPR)?;
        let mut it = SyntaxNodeIter::new(&node);
        let next = it
            .next()
            .ok_or_else(|| StrongAstError::missing(SyntaxKind::WORD, it.parent))?;
        let (first, mut rest) = match next.kind() {
            kind if is_path_segment_kind(kind) => (path_segment_from_cst(next)?, Vec::new()),
            SyntaxKind::PATH_EXPR => {
                let nested = PathExpr::from_cst(next)?;
                if nested.generic_args.is_some() {
                    return Err(StrongAstError::UnexpectedAdditionalElement {
                        parent: it.parent,
                        at: nested
                            .generic_args
                            .as_ref()
                            .map_or_else(rowan::TextRange::default, |g| g.open_angle.span()),
                    });
                }
                (nested.first, nested.rest)
            }
            _ => {
                return Err(StrongAstError::UnexpectedAdditionalElement {
                    parent: it.parent,
                    at: next.text_range(),
                });
            }
        };
        let mut generic_args: Option<GenericArgs> = None;
        while let Some(elem) = it.next() {
            match elem.kind() {
                SyntaxKind::DOT => {
                    let dot = t::Dot::from_cst(elem)?;
                    let word = path_segment_from_cst(it.expect_next("path segment after `.`")?)?;
                    rest.push((dot, word));
                }
                SyntaxKind::GENERIC_ARGS => {
                    generic_args = Some(GenericArgs::from_cst(elem)?);
                    if let Some(extra) = it.next() {
                        return Err(StrongAstError::UnexpectedAdditionalElement {
                            parent: it.parent,
                            at: extra.text_range(),
                        });
                    }
                    break;
                }
                _ => {
                    return Err(StrongAstError::UnexpectedAdditionalElement {
                        parent: it.parent,
                        at: elem.text_range(),
                    });
                }
            }
        }
        Ok(PathExpr {
            first,
            rest,
            generic_args,
        })
    }
}

validated_ast_data! {
    /// A generic instantiation whose base is not a plain path, e.g.
    /// `(<T>(x: T) -> T { x })<int>` or `(foo)<int>`. Corresponds to a
    /// [`SyntaxKind::PATH_EXPR`] node whose first child is an arbitrary expression
    /// followed by `GENERIC_ARGS`.
    pub struct GenericApplyExpr {
        pub base: Box<Expression>,
        pub generic_args: GenericArgs,
    }
}

impl FromCST for GenericApplyExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::PATH_EXPR)?;
        let mut it = SyntaxNodeIter::new(&node);
        let base_elem = it
            .next()
            .ok_or_else(|| StrongAstError::missing(SyntaxKind::PAREN_EXPR, it.parent))?;
        let base = Box::new(Expression::from_cst(base_elem)?);
        let ga_elem = it
            .next()
            .ok_or_else(|| StrongAstError::missing(SyntaxKind::GENERIC_ARGS, it.parent))?;
        let generic_args = GenericArgs::from_cst(ga_elem)?;
        if let Some(extra) = it.next() {
            return Err(StrongAstError::UnexpectedAdditionalElement {
                parent: it.parent,
                at: extra.text_range(),
            });
        }
        Ok(GenericApplyExpr { base, generic_args })
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::PAREN_EXPR`] node.
    ParenExpr, PAREN_EXPR {
        open_paren: required t::LParen;
        expr: boxed Expression;
        close_paren: required t::RParen;
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::BINARY_EXPR`] node.
    custom BinaryExpr, BINARY_EXPR, parse_binary_expr {
        op: BinaryOp,
        sides: Box<(Expression, Expression)>,
    }
}

fn parse_binary_expr(elem: SyntaxElement) -> Result<BinaryExpr, StrongAstError> {
    let node = StrongAstError::assert_is_node(elem)?;
    StrongAstError::assert_kind_node(&node, SyntaxKind::BINARY_EXPR)?;
    let mut it = SyntaxNodeIter::new(&node);
    let left = it.expect_next("left expression")?;
    let left_expr = Expression::from_cst(left)?;
    let op_elem = it.expect_next("binary operator")?;
    let op = if op_elem.kind() == SyntaxKind::QUESTION {
        let first_range = op_elem.text_range();
        if let Some(second) = it.next_if_kind(SyntaxKind::QUESTION) {
            let combined_range = TextRange::new(first_range.start(), second.text_range().end());
            BinaryOp::QuestionQuestion(t::QuestionQuestion::new_from_span(combined_range))
        } else {
            return Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "binary operator".into(),
                found: SyntaxKind::QUESTION,
                at: first_range,
            });
        }
    } else {
        BinaryOp::from_cst(op_elem)?
    };
    let right = it.expect_next("right expression")?;
    let right_expr = Expression::from_cst(right)?;
    it.expect_end()?;
    Ok(BinaryExpr {
        op,
        sides: Box::new((left_expr, right_expr)),
    })
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::IS_EXPR`] node.
    ///
    /// `<expr> is <pattern>` is a Rust `matches!`-style pattern test.
    IsExpr, IS_EXPR {
        lhs: boxed Expression;
        keyword: required t::Is;
        pattern: required MatchPattern;
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::UNARY_EXPR`] node.
    UnaryExpr, UNARY_EXPR {
        op: required UnaryOp;
        expr: boxed Expression;
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::IF_EXPR`] node.
    custom IfExpr, IF_EXPR, parse_if_expr {
        keyword: t::If,
        /// The condition expression. Parens are optional in Baml, so this can be
        /// any expression - `if (a == b)` and `if a == b` are both valid.
        condition: Box<Expression>,
        block: BlockExpr,
        else_branch: Option<(t::Else, ElseExpr)>,
    }
}

fn parse_if_expr(elem: SyntaxElement) -> Result<IfExpr, StrongAstError> {
    let node = StrongAstError::assert_is_node(elem)?;
    StrongAstError::assert_kind_node(&node, SyntaxKind::IF_EXPR)?;
    let mut it = SyntaxNodeIter::new(&node);
    let keyword = it.expect_parse()?;
    let condition_elem = it.expect_next("an if condition expression")?;
    let condition = Box::new(Expression::from_cst(condition_elem)?);
    let block: BlockExpr = it.expect_parse()?;
    let else_branch = if let Some(elem) = it.next() {
        let else_token = t::Else::from_cst(elem)?;
        let else_body_node = it.expect_node("else body (if, if-let, or block)")?;
        let else_body = match else_body_node.kind() {
            SyntaxKind::IF_EXPR => ElseExpr::If(Box::new(IfExpr::from_cst(SyntaxElement::Node(
                else_body_node,
            ))?)),
            SyntaxKind::IF_LET_EXPR => ElseExpr::IfLet(Box::new(IfLetExpr::from_cst(
                SyntaxElement::Node(else_body_node),
            )?)),
            SyntaxKind::BLOCK_EXPR => ElseExpr::Block(Box::new(BlockExpr::from_cst(
                SyntaxElement::Node(else_body_node),
            )?)),
            _ => {
                return Err(StrongAstError::UnexpectedKindDesc {
                    expected_desc: "IF_EXPR, IF_LET_EXPR, or BLOCK_EXPR".into(),
                    found: else_body_node.kind(),
                    at: else_body_node.text_range(),
                });
            }
        };
        Some((else_token, else_body))
    } else {
        None
    };
    it.expect_end()?;
    Ok(IfExpr {
        keyword,
        condition,
        block,
        else_branch,
    })
}

validated_ast_data! {
    /// Used in [`IfExpr`] / [`IfLetExpr`] to represent the else/else-if branch.
    pub enum ElseExpr {
        /// else if
        If(Box<IfExpr>),
        /// else if let
        IfLet(Box<IfLetExpr>),
        /// final else block
        Block(Box<BlockExpr>),
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::IF_LET_EXPR`] node.
    ///
    /// `if let PATTERN = SCRUTINEE BLOCK (else (BLOCK | IF_EXPR | IF_LET_EXPR))?`
    custom IfLetExpr, IF_LET_EXPR, parse_if_let_expr {
        keyword: t::If,
        /// `let PATTERN` - the leading `let` is part of the pattern grammar
        /// (`parse_let_pattern`), so it's stored inside `pattern` rather than
        /// as a separate token.
        pattern: MatchPattern,
        equals: t::Equals,
        scrutinee: Box<Expression>,
        block: BlockExpr,
        else_branch: Option<(t::Else, ElseExpr)>,
    }
}

fn parse_if_let_expr(elem: SyntaxElement) -> Result<IfLetExpr, StrongAstError> {
    let node = StrongAstError::assert_is_node(elem)?;
    StrongAstError::assert_kind_node(&node, SyntaxKind::IF_LET_EXPR)?;
    let mut it = SyntaxNodeIter::new(&node);
    let keyword = it.expect_parse()?;
    let pattern = it.expect_parse()?;
    let equals = it.expect_parse()?;
    let scrutinee_elem = it.expect_next("if-let scrutinee expression")?;
    let scrutinee = Box::new(Expression::from_cst(scrutinee_elem)?);
    let block: BlockExpr = it.expect_parse()?;
    let else_branch = if let Some(elem) = it.next() {
        let else_token = t::Else::from_cst(elem)?;
        let else_body_node = it.expect_node("else body (if, if-let, or block)")?;
        let else_body = match else_body_node.kind() {
            SyntaxKind::IF_EXPR => ElseExpr::If(Box::new(IfExpr::from_cst(SyntaxElement::Node(
                else_body_node,
            ))?)),
            SyntaxKind::IF_LET_EXPR => ElseExpr::IfLet(Box::new(IfLetExpr::from_cst(
                SyntaxElement::Node(else_body_node),
            )?)),
            SyntaxKind::BLOCK_EXPR => ElseExpr::Block(Box::new(BlockExpr::from_cst(
                SyntaxElement::Node(else_body_node),
            )?)),
            _ => {
                return Err(StrongAstError::UnexpectedKindDesc {
                    expected_desc: "IF_EXPR, IF_LET_EXPR, or BLOCK_EXPR".into(),
                    found: else_body_node.kind(),
                    at: else_body_node.text_range(),
                });
            }
        };
        Some((else_token, else_body))
    } else {
        None
    };
    it.expect_end()?;
    Ok(IfLetExpr {
        keyword,
        pattern,
        equals,
        scrutinee,
        block,
        else_branch,
    })
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::MATCH_EXPR`] node.
    custom MatchExpr, MATCH_EXPR, parse_match_expr {
        keyword: t::Match,
        open_paren: t::LParen,
        scrutinee: Box<Expression>,
        close_paren: t::RParen,
        open_brace: t::LBrace,
        arms: Vec<MatchArm>,
        close_brace: t::RBrace,
    }
}

fn parse_match_expr(elem: SyntaxElement) -> Result<MatchExpr, StrongAstError> {
    let node = StrongAstError::assert_is_node(elem)?;
    StrongAstError::assert_kind_node(&node, SyntaxKind::MATCH_EXPR)?;
    let mut it = SyntaxNodeIter::new(&node);
    let keyword = it.expect_parse()?;
    let open_paren = it.expect_parse()?;
    let scrutinee_node = it.expect_next("scrutinee expression")?;
    let scrutinee = Box::new(Expression::from_cst(scrutinee_node)?);
    let close_paren = it.expect_parse()?;
    let open_brace = it.expect_parse()?;
    let mut arms = Vec::new();
    let close_brace = loop {
        let Some(elem) = it.next() else {
            return Err(StrongAstError::missing(SyntaxKind::R_BRACE, it.parent));
        };
        match elem.kind() {
            SyntaxKind::R_BRACE => {
                break t::RBrace::from_cst(elem)?;
            }
            SyntaxKind::MATCH_ARM => {
                let arm = MatchArm::from_cst(elem)?;
                arms.push(arm);
            }
            _ => {
                return Err(StrongAstError::UnexpectedKindDesc {
                    expected_desc: "MATCH_ARM or R_BRACE".into(),
                    found: elem.kind(),
                    at: elem.text_range(),
                });
            }
        }
    };
    it.expect_end()?;
    Ok(MatchExpr {
        keyword,
        open_paren,
        scrutinee,
        close_paren,
        open_brace,
        arms,
        close_brace,
    })
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::MATCH_ARM`] node.
    MatchArm, MATCH_ARM {
        pattern: required MatchPattern;
        guard: optional MatchGuard;
        fat_arrow: required t::FatArrow;
        body: required Expression;
        comma: optional_element t::Comma;
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::MATCH_GUARD`] node.
    MatchGuard, MATCH_GUARD {
        keyword: required t::If;
        condition: required Expression;
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::CATCH_EXPR`] node.
    CatchExpr, CATCH_EXPR {
        base: boxed Expression;
        clauses: rest CatchClause;
    }
}

validated_ast_data! {
    /// The `catch`, `catch_all`, or `catch_all_panics` keyword that starts a catch clause.
    pub enum CatchKeyword {
        Catch(t::Catch),
        CatchAll(t::CatchAll),
        CatchAllPanics(t::CatchAllPanics),
    }
}

impl AstToken for CatchKeyword {
    fn span(&self) -> TextRange {
        match self {
            Self::Catch(keyword) => keyword.span(),
            Self::CatchAll(keyword) => keyword.span(),
            Self::CatchAllPanics(keyword) => keyword.span(),
        }
    }
}

impl FromCST for CatchKeyword {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        match elem.kind() {
            SyntaxKind::KW_CATCH => t::Catch::from_cst(elem).map(Self::Catch),
            SyntaxKind::KW_CATCH_ALL => t::CatchAll::from_cst(elem).map(Self::CatchAll),
            SyntaxKind::KW_CATCH_ALL_PANICS => {
                t::CatchAllPanics::from_cst(elem).map(Self::CatchAllPanics)
            }
            found => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "KW_CATCH, KW_CATCH_ALL, or KW_CATCH_ALL_PANICS".into(),
                found,
                at: elem.text_range(),
            }),
        }
    }
}

validated_ast_data! {
    /// `catch (binding)` and optional stack-trace bindings use small wrapper nodes.
    pub struct CatchBinding {
        pub name: t::Word,
    }
}

impl CatchBinding {
    fn from_cst_kind(elem: SyntaxElement, kind: SyntaxKind) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, kind)?;
        let mut it = SyntaxNodeIter::new(&node);
        let name = it.expect_parse()?;
        it.expect_end()?;
        Ok(Self { name })
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::CATCH_CLAUSE`] node.
    custom CatchClause, CATCH_CLAUSE, parse_catch_clause {
        keyword: CatchKeyword,
        open_paren: t::LParen,
        binding: CatchBinding,
        stack_trace_binding: Option<(t::Comma, CatchBinding)>,
        close_paren: t::RParen,
        open_brace: t::LBrace,
        arms: Vec<CatchArm>,
        close_brace: t::RBrace,
    }
}

fn parse_catch_clause(elem: SyntaxElement) -> Result<CatchClause, StrongAstError> {
    let node = StrongAstError::assert_is_node(elem)?;
    StrongAstError::assert_kind_node(&node, SyntaxKind::CATCH_CLAUSE)?;
    let mut it = SyntaxNodeIter::new(&node);
    let keyword = CatchKeyword::from_cst(it.expect_next("catch keyword")?)?;
    let open_paren = it.expect_parse()?;
    let binding =
        CatchBinding::from_cst_kind(it.expect_next("catch binding")?, SyntaxKind::CATCH_BINDING)?;
    let stack_trace_binding = it
        .next_if_kind(SyntaxKind::COMMA)
        .map(|comma| {
            Ok::<_, StrongAstError>((
                t::Comma::from_cst(comma)?,
                CatchBinding::from_cst_kind(
                    it.expect_next("catch stack trace binding")?,
                    SyntaxKind::CATCH_STACK_TRACE_BINDING,
                )?,
            ))
        })
        .transpose()?;
    let close_paren = it.expect_parse()?;
    let open_brace = it.expect_parse()?;
    let mut arms = Vec::new();
    let close_brace = loop {
        let Some(elem) = it.next() else {
            return Err(StrongAstError::missing(SyntaxKind::R_BRACE, it.parent));
        };
        match elem.kind() {
            SyntaxKind::R_BRACE => break t::RBrace::from_cst(elem)?,
            SyntaxKind::CATCH_ARM => arms.push(CatchArm::from_cst(elem)?),
            found => {
                return Err(StrongAstError::UnexpectedKindDesc {
                    expected_desc: "CATCH_ARM or R_BRACE".into(),
                    found,
                    at: elem.text_range(),
                });
            }
        }
    };
    it.expect_end()?;
    Ok(CatchClause {
        keyword,
        open_paren,
        binding,
        stack_trace_binding,
        close_paren,
        open_brace,
        arms,
        close_brace,
    })
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::CATCH_ARM`] node.
    CatchArm, CATCH_ARM {
        pattern: required MatchPattern;
        fat_arrow: required t::FatArrow;
        body: required Expression;
        comma: optional_element t::Comma;
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::CALL_EXPR`] node.
    CallExpr, CALL_EXPR {
        callee: boxed Expression;
        args: required CallArgs;
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::CALL_ARGS`] node.
    custom CallArgs, CALL_ARGS, parse_call_args {
        open_paren: t::LParen,
        args: Vec<(CallArg, Option<t::Comma>)>,
        close_paren: t::RParen,
    }
}

fn parse_call_args(elem: SyntaxElement) -> Result<CallArgs, StrongAstError> {
    let node = StrongAstError::assert_is_node(elem)?;
    StrongAstError::assert_kind_node(&node, SyntaxKind::CALL_ARGS)?;
    let mut it = SyntaxNodeIter::new(&node);
    let open_paren = it.expect_parse()?;
    let mut args = Vec::new();
    let close_paren = loop {
        let Some(elem) = it.next() else {
            return Err(StrongAstError::missing(SyntaxKind::R_PAREN, it.parent));
        };
        if elem.kind() == SyntaxKind::R_PAREN {
            break t::RParen::from_cst(elem)?;
        }
        let arg = if elem.kind() == SyntaxKind::CALL_ARG {
            CallArg::from_cst(elem)?
        } else {
            CallArg {
                label: None,
                expr: Expression::from_cst(elem)?,
            }
        };
        let comma = it
            .next_if_kind(SyntaxKind::COMMA)
            .map(t::Comma::from_cst)
            .transpose()?;
        args.push((arg, comma));
    };
    it.expect_end()?;
    Ok(CallArgs {
        open_paren,
        args,
        close_paren,
    })
}

validated_ast_data! {
    /// Corresponds to a [`SyntaxKind::CALL_ARG`] node.
    pub struct CallArg {
        pub label: Option<(t::Word, t::Equals)>,
        pub expr: Expression,
    }
}

impl FromCST for CallArg {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::CALL_ARG)?;
        let children: Vec<_> = node
            .children_with_tokens()
            .filter(|elem| !elem.kind().is_trivia())
            .collect();
        let (label, expr_elem) = if children.len() >= 3
            && matches!(children[0].kind(), SyntaxKind::WORD | SyntaxKind::KW_CLIENT)
            && children[1].kind() == SyntaxKind::EQUALS
        {
            let name = t::Word::new_from_span(children[0].text_range());
            let equals = t::Equals::from_cst(children[1].clone())?;
            (Some((name, equals)), children[2].clone())
        } else {
            let Some(expr_elem) = children.first().cloned() else {
                return Err(StrongAstError::missing_desc(
                    "call argument",
                    node.text_range(),
                ));
            };
            (None, expr_elem)
        };
        let expr = Expression::from_cst(expr_elem)?;
        Ok(CallArg { label, expr })
    }
}

impl CallArg {
    /// A block-terminal argument (a lambda or a `spawn { ... }`) that may hug
    /// the call parens instead of forcing the whole call to break: the
    /// argument's block opens on the call line and its `}` is immediately
    /// followed by `)`.
    pub const fn is_huggable(&self) -> bool {
        matches!(self.expr, Expression::Lambda(_) | Expression::Spawn(_))
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::INDEX_EXPR`] node.
    IndexExpr, INDEX_EXPR {
        base: boxed Expression;
        open_bracket: required t::LBracket;
        index: boxed Expression;
        close_bracket: required t::RBracket;
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::FIELD_ACCESS_EXPR`] node.
    FieldAccessExpr, FIELD_ACCESS_EXPR {
        base: boxed Expression;
        dot: required t::Dot;
        field: required t::Word;
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::OPTIONAL_FIELD_ACCESS_EXPR`] node: `base?.field`.
    OptionalFieldAccessExpr, OPTIONAL_FIELD_ACCESS_EXPR {
        base: boxed Expression;
        question_dot: required t::QuestionDot;
        field: required t::Word;
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::OPTIONAL_INDEX_EXPR`] node: `base?.[index]`.
    OptionalIndexExpr, OPTIONAL_INDEX_EXPR {
        base: boxed Expression;
        question_dot: required t::QuestionDot;
        open_bracket: required t::LBracket;
        index: boxed Expression;
        close_bracket: required t::RBracket;
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::OPTIONAL_CALL_EXPR`] node: `callee?.(args)`.
    OptionalCallExpr, OPTIONAL_CALL_EXPR {
        callee: boxed Expression;
        question_dot: required t::QuestionDot;
        args: required CallArgs;
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::ENV_ACCESS_EXPR`] node.
    EnvAccessExpr, ENV_ACCESS_EXPR {
        keyword: required t::Word;
        dot: required t::Dot;
        field: required t::Word;
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::BLOCK_EXPR`] node.
    custom BlockExpr, BLOCK_EXPR, parse_block_expr {
        open_brace: t::LBrace,
        stmts: Vec<Statement>,
        /// Possible tail expression.
        /// If not in a block that can have a tail expression, this should be treated as a normal [`Statement::Expr`].
        expr: Option<Box<Expression>>,
        close_brace: t::RBrace,
    }
}

fn parse_block_expr(elem: SyntaxElement) -> Result<BlockExpr, StrongAstError> {
    let node = StrongAstError::assert_is_node(elem)?;
    StrongAstError::assert_kind_node(&node, SyntaxKind::BLOCK_EXPR)?;
    let mut it = SyntaxNodeIter::new(&node);
    let open_brace = it.expect_parse()?;
    let mut stmts = Vec::new();
    let close_brace = loop {
        let Some(elem) = it.next() else {
            return Err(StrongAstError::missing(SyntaxKind::R_BRACE, it.parent));
        };
        if elem.kind() == SyntaxKind::R_BRACE {
            break t::RBrace::from_cst(elem)?;
        }
        let stmt = Statement::from_cst(elem)?;
        if let Some(Statement::Expr(expr)) = stmts.last_mut()
            && expr.semicolon.is_none()
            && let Statement::EmptySemicolon(semi) = stmt
        {
            expr.semicolon = Some(semi);
            continue;
        }
        stmts.push(stmt);
    };
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
        open_brace,
        stmts,
        expr: expr.map(Box::new),
        close_brace,
    })
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::ARRAY_LITERAL`] node.
    custom ArrayInitializer, ARRAY_LITERAL, parse_array_initializer {
        open_bracket: t::LBracket,
        /// Commas are optional for all elements.
        /// For example, `[1 2 3]` is equivalent to `[1, 2, 3]` in BAML.
        ///
        /// While this is valid, excluding commas is *strongly* discouraged as it is a crime against software and also more error-prone:
        /// if `[1, -2, 3]` is written as `[1 -2 3]`, it will be parsed as `[1-2, 3]` instead (the `-` will be treated as a binary operator instead of a unary operator).
        elements: Vec<(Expression, Option<t::Comma>)>,
        close_bracket: t::RBracket,
    }
}

fn parse_array_initializer(elem: SyntaxElement) -> Result<ArrayInitializer, StrongAstError> {
    let node = StrongAstError::assert_is_node(elem)?;
    StrongAstError::assert_kind_node(&node, SyntaxKind::ARRAY_LITERAL)?;
    let mut it = SyntaxNodeIter::new(&node);
    let open_bracket = it.expect_parse()?;
    let mut elements: Vec<(Expression, Option<t::Comma>)> = Vec::new();
    let close_bracket = loop {
        let Some(elem) = it.next() else {
            return Err(StrongAstError::missing(SyntaxKind::R_BRACKET, it.parent));
        };
        if elem.kind() == SyntaxKind::R_BRACKET {
            break t::RBracket::from_cst(elem)?;
        }
        let expr = Expression::from_cst(elem)?;
        let comma = it
            .next_if_kind(SyntaxKind::COMMA)
            .map(t::Comma::from_cst)
            .transpose()?;
        elements.push((expr, comma));
    };
    Ok(ArrayInitializer {
        open_bracket,
        elements,
        close_bracket,
    })
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::OBJECT_LITERAL`] node.
    custom ObjectInitializer, OBJECT_LITERAL, parse_object_initializer {
        name: PathExpr,
        open_brace: t::LBrace,
        fields: Vec<(ObjectField, Option<t::Comma>)>,
        close_brace: t::RBrace,
    }
}

fn parse_object_initializer(elem: SyntaxElement) -> Result<ObjectInitializer, StrongAstError> {
    let node = StrongAstError::assert_is_node(elem)?;
    StrongAstError::assert_kind_node(&node, SyntaxKind::OBJECT_LITERAL)?;
    let mut it = SyntaxNodeIter::new(&node);
    let name = it.expect_next("a WORD or PATH_EXPR")?;
    let name = PathExpr::from_cst(name)?;
    let open_brace = it.expect_parse()?;
    let mut fields = Vec::new();
    let close_brace = loop {
        let Some(elem) = it.next() else {
            return Err(StrongAstError::missing(SyntaxKind::R_BRACE, it.parent));
        };
        match elem.kind() {
            SyntaxKind::R_BRACE => {
                break t::RBrace::from_cst(elem)?;
            }
            SyntaxKind::OBJECT_FIELD => {
                let field = ObjectField::from_cst(elem)?;
                let comma = it
                    .next_if_kind(SyntaxKind::COMMA)
                    .map(t::Comma::from_cst)
                    .transpose()?;
                fields.push((field, comma));
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
        name,
        open_brace,
        fields,
        close_brace,
    })
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::MAP_LITERAL`] node.
    custom MapLiteral, MAP_LITERAL, parse_map_literal {
        open_brace: t::LBrace,
        fields: Vec<(ObjectField, Option<t::Comma>)>,
        close_brace: t::RBrace,
    }
}

fn parse_map_literal(elem: SyntaxElement) -> Result<MapLiteral, StrongAstError> {
    let node = StrongAstError::assert_is_node(elem)?;
    StrongAstError::assert_kind_node(&node, SyntaxKind::MAP_LITERAL)?;
    let mut it = SyntaxNodeIter::new(&node);
    let open_brace = it.expect_parse()?;
    let mut fields = Vec::new();
    let close_brace = loop {
        let Some(elem) = it.next() else {
            return Err(StrongAstError::missing(SyntaxKind::R_BRACE, it.parent));
        };
        match elem.kind() {
            SyntaxKind::R_BRACE => {
                break t::RBrace::from_cst(elem)?;
            }
            SyntaxKind::OBJECT_FIELD => {
                let field = ObjectField::from_cst(elem)?;
                let comma = it
                    .next_if_kind(SyntaxKind::COMMA)
                    .map(t::Comma::from_cst)
                    .transpose()?;
                fields.push((field, comma));
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
        open_brace,
        fields,
        close_brace,
    })
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::OBJECT_FIELD`] node.
    custom ObjectField, OBJECT_FIELD, parse_object_field {
        name: ObjectFieldKey,
        /// Absent for property shorthand (`{ options }`). The parser only permits
        /// shorthand for a bare identifier, never for a quoted or qualified key.
        colon: Option<t::Colon>,
        value: Option<Expression>,
    }
}

fn parse_object_field(elem: SyntaxElement) -> Result<ObjectField, StrongAstError> {
    let node = StrongAstError::assert_is_node(elem)?;
    StrongAstError::assert_kind_node(&node, SyntaxKind::OBJECT_FIELD)?;
    let mut it = SyntaxNodeIter::new(&node);
    let name = it.expect_next("WORD or STRING_LITERAL")?;
    let name = ObjectFieldKey::from_cst(name)?;
    let colon = it
        .next_if_kind(SyntaxKind::COLON)
        .map(t::Colon::from_cst)
        .transpose()?;
    let value = if colon.is_some() {
        let value = it.expect_next("value")?;
        Some(Expression::from_cst(value)?)
    } else {
        None
    };
    it.expect_end()?;
    Ok(ObjectField { name, colon, value })
}

validated_ast_data! {
    /// Represents the a valid key for an [`ObjectField`].
    pub enum ObjectFieldKey {
        Word(t::Word),
        String(t::QuotedString),
    }
}

impl FromCST for ObjectFieldKey {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        match elem.kind() {
            SyntaxKind::WORD => Ok(ObjectFieldKey::Word(t::Word::from_cst(elem)?)),
            SyntaxKind::STRING_LITERAL => {
                Ok(ObjectFieldKey::String(t::QuotedString::from_cst(elem)?))
            }
            _ => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "WORD or STRING_LITERAL".into(),
                found: elem.kind(),
                at: elem.text_range(),
            }),
        }
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::GENERIC_PARAM_LIST`] node.
    ///
    /// Contains `<T, U>` generic parameter declarations for a lambda expression.
    /// Printed as `<T>` or `<K, V>` etc.
    custom GenericParamList, GENERIC_PARAM_LIST, parse_generic_param_list {
        open_angle: t::Less,
        /// Comma-separated type parameter declarations.
        params: Vec<GenericParam>,
        close_angle: t::Greater,
    }
}

fn parse_generic_param_list(elem: SyntaxElement) -> Result<GenericParamList, StrongAstError> {
    let node = StrongAstError::assert_is_node(elem)?;
    StrongAstError::assert_kind_node(&node, SyntaxKind::GENERIC_PARAM_LIST)?;
    let mut it = SyntaxNodeIter::new(&node);
    let open_angle: t::Less = it.expect_parse()?;
    let mut params = Vec::new();
    let close_angle = loop {
        let Some(elem) = it.next() else {
            return Err(StrongAstError::missing(SyntaxKind::GREATER, it.parent));
        };
        match elem.kind() {
            SyntaxKind::GREATER => {
                break t::Greater::from_cst(elem)?;
            }
            SyntaxKind::GENERIC_PARAM => {
                let param_node = StrongAstError::assert_is_node(elem)?;
                let mut param_it = SyntaxNodeIter::new(&param_node);
                let name: t::Word = param_it.expect_parse()?;
                let bounds = if param_it.peek().map(SyntaxElement::kind)
                    == Some(SyntaxKind::GENERIC_PARAM_BOUNDS)
                {
                    let elem = param_it.next().expect("peeked");
                    Some(GenericParamBounds::from_cst(elem)?)
                } else {
                    None
                };
                param_it.expect_end()?;
                let comma = it
                    .next_if_kind(SyntaxKind::COMMA)
                    .map(t::Comma::from_cst)
                    .transpose()?;
                params.push(GenericParam {
                    name,
                    bounds,
                    comma,
                });
            }
            _ => {
                return Err(StrongAstError::UnexpectedAdditionalElement {
                    parent: it.parent,
                    at: elem.text_range(),
                });
            }
        }
    };
    it.expect_end()?;
    Ok(GenericParamList {
        open_angle,
        params,
        close_angle,
    })
}

validated_ast_data! {
    pub struct GenericParam {
        pub name: t::Word,
        pub bounds: Option<GenericParamBounds>,
        pub comma: Option<t::Comma>,
    }
}

validated_ast_data! {
    pub struct GenericParamBounds {
        pub extends: t::Extends,
        pub bounds: Vec<(Type, Option<t::And>)>,
    }
}

impl FromCST for GenericParamBounds {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::GENERIC_PARAM_BOUNDS)?;
        let mut it = SyntaxNodeIter::new(&node);
        let extends: t::Extends = it.expect_parse()?;
        let mut bounds = Vec::new();
        while it.peek().is_some() {
            let ty: Type = it.expect_parse()?;
            let and = it
                .next_if_kind(SyntaxKind::AND)
                .map(t::And::from_cst)
                .transpose()?;
            bounds.push((ty, and));
        }
        it.expect_end()?;
        Ok(GenericParamBounds { extends, bounds })
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::GENERIC_ARGS`] node.
    ///
    /// Contains `<Type1, Type2, ...>` generic arguments at a call site
    /// or generic-typed path (e.g. `f<int, string>(...)`, `Box<int> { ... }`).
    custom GenericArgs, GENERIC_ARGS, parse_generic_args {
        open_angle: t::Less,
        /// Comma-separated type arguments.
        args: Vec<(Type, Option<t::Comma>)>,
        close_angle: t::Greater,
    }
}

fn parse_generic_args(elem: SyntaxElement) -> Result<GenericArgs, StrongAstError> {
    let node = StrongAstError::assert_is_node(elem)?;
    StrongAstError::assert_kind_node(&node, SyntaxKind::GENERIC_ARGS)?;
    let mut it = SyntaxNodeIter::new(&node);
    let open_angle: t::Less = it.expect_parse()?;
    let mut args = Vec::new();
    let close_angle = loop {
        let Some(elem) = it.next() else {
            return Err(StrongAstError::missing(SyntaxKind::GREATER, it.parent));
        };
        match elem.kind() {
            SyntaxKind::GREATER => {
                break t::Greater::from_cst(elem)?;
            }
            SyntaxKind::TYPE_EXPR => {
                let ty = Type::from_cst(elem)?;
                let comma = it
                    .next_if_kind(SyntaxKind::COMMA)
                    .map(t::Comma::from_cst)
                    .transpose()?;
                args.push((ty, comma));
            }
            _ => {
                return Err(StrongAstError::UnexpectedAdditionalElement {
                    parent: it.parent,
                    at: elem.text_range(),
                });
            }
        }
    };
    it.expect_end()?;
    Ok(GenericArgs {
        open_angle,
        args,
        close_angle,
    })
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::THROWS_CLAUSE`] node.
    ///
    /// Contains `throws <type>`.
    ThrowsClause, THROWS_CLAUSE {
        keyword: required t::Throws;
        ty: required Type;
    }
}

validated_ast_node! {
    custom LambdaArrow, ARROW, parse_lambda_arrow,
    /// Arrow token in a lambda expression. Accepts either `->` (canonical) or
    /// `=>` (accepted permissively for ergonomic parity with JS/TS arrow functions);
    /// the formatter always emits `->`.
    pub enum LambdaArrow {
        Arrow(t::Arrow),
        FatArrow(t::FatArrow),
    }
}

fn parse_lambda_arrow(elem: SyntaxElement) -> Result<LambdaArrow, StrongAstError> {
    let token = StrongAstError::assert_is_token(elem)?;
    match token.kind() {
        SyntaxKind::ARROW => Ok(LambdaArrow::Arrow(t::Arrow::new_from_span(
            token.text_range(),
        ))),
        SyntaxKind::FAT_ARROW => Ok(LambdaArrow::FatArrow(t::FatArrow::new_from_span(
            token.text_range(),
        ))),
        _ => Err(StrongAstError::UnexpectedKindDesc {
            expected_desc: "ARROW or FAT_ARROW".into(),
            found: token.kind(),
            at: token.text_range(),
        }),
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::LAMBDA_EXPR`] node.
    ///
    /// Syntax: `[<T, U>] (params) (-> | =>) [RetType] [throws E] { body }`
    LambdaExpr, LAMBDA_EXPR {
        generic_params: optional GenericParamList;
        param_list: required FunctionParamList;
        arrow: required LambdaArrow;
        return_type: optional Type;
        throws: optional ThrowsClause;
        block: required BlockExpr;
    }
}

/// The `with` options clause of a [`SpawnExpr`]: the keyword and its
/// comma-separated expressions (in v1 a single `baml.spawn.options(...)`
/// call).
pub type SpawnWithClause = (t::With, Vec<(Expression, Option<t::Comma>)>);

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::SPAWN_EXPR`] node.
    ///
    /// `spawn name_expr? (with expr (, expr)*)? { body }` (BEP-034). The name
    /// expression and the `with` options clause are both optional; the body is
    /// always a brace-delimited block.
    custom SpawnExpr, SPAWN_EXPR, parse_spawn_expr {
        keyword: t::Spawn,
        /// Optional task-name expression between `spawn` and `with`/the body.
        name: Option<Expression>,
        with_clause: Option<SpawnWithClause>,
        block: BlockExpr,
    }
}

fn parse_spawn_expr(elem: SyntaxElement) -> Result<SpawnExpr, StrongAstError> {
    let node = StrongAstError::assert_is_node(elem)?;
    StrongAstError::assert_kind_node(&node, SyntaxKind::SPAWN_EXPR)?;
    let mut it = SyntaxNodeIter::new(&node);
    let keyword: t::Spawn = it.expect_parse()?;
    let mut name = None;
    let mut with_clause = None;
    let block = loop {
        let elem = it.expect_next("spawn body block")?;
        match elem.kind() {
            SyntaxKind::BLOCK_EXPR => break BlockExpr::from_cst(elem)?,
            SyntaxKind::KW_WITH => {
                let with_kw = t::With::from_cst(elem)?;
                let mut options = Vec::new();
                while let Some(next) = it.peek() {
                    if next.kind() == SyntaxKind::BLOCK_EXPR {
                        break;
                    }
                    let expr = Expression::from_cst(it.next().expect("peeked"))?;
                    let comma = it
                        .next_if_kind(SyntaxKind::COMMA)
                        .map(t::Comma::from_cst)
                        .transpose()?;
                    options.push((expr, comma));
                }
                with_clause = Some((with_kw, options));
            }
            _ if name.is_none() && with_clause.is_none() => {
                name = Some(Expression::from_cst(elem)?);
            }
            _ => {
                return Err(StrongAstError::UnexpectedAdditionalElement {
                    parent: it.parent,
                    at: elem.text_range(),
                });
            }
        }
    };
    it.expect_end()?;
    Ok(SpawnExpr {
        keyword,
        name,
        with_clause,
        block,
    })
}
