use crate::{
    SyntaxElement, SyntaxKind, TextRange,
    validated::{
        BinaryOp, FromCST, KnownKind, StrongAstError, SyntaxNodeIter, UnaryOp,
        ValidatedToken as Token,
        nodes::{Literal, MatchPattern, Statement, ThrowsClause, Type},
        tokens as t,
    },
};

#[derive(Debug)]
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

/// A node the strong AST does not model and prints verbatim: an unmodeled
/// expression (e.g. `defer { ... }`, `throw e`, `await f`,
/// `x.as<T>`) held as [`Expression::Unknown`], or a braceless jump held as
/// [`Expression::Return`], [`Expression::Break`], or [`Expression::Continue`].
///
/// Rather than a single whole-node span, this carries the node's true first and
/// last *token* ranges. The trivia classifier keys leading/trailing comments to
/// individual token ranges, so formatter token-anchor methods must return
/// those exact token ranges for a
/// comment to attach and emit. A whole-node span never matches a token key, so
/// a trailing comment on the node was silently dropped - the `defer` statement
/// comment-loss bug (B-629), and the same class of bug for a braceless `return`
/// arm. A whole-node span can also begin inside leading trivia (the parser
/// attaches a preceding comment to the node), which would re-print that comment
/// verbatim at the wrong indent; the `content_range` used for printing excludes
/// it.
#[derive(Debug)]
pub struct VerbatimSpan {
    /// Range of the first non-trivia token - the leading-trivia anchor.
    pub first_token: TextRange,
    /// Range of the last non-trivia token - the trailing-trivia anchor.
    pub last_token: TextRange,
}

impl VerbatimSpan {
    /// Build from the verbatim-printed syntax element, capturing its first and
    /// last non-trivia token ranges. Any leading/trailing trivia that the CST
    /// attaches inside the node is skipped so the anchors line up with the
    /// classifier's per-token comment keys.
    fn from_element(elem: &SyntaxElement) -> Self {
        if let Some(node) = elem.as_node() {
            let mut tokens = node
                .descendants_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .filter(|t| !t.kind().is_trivia());
            if let Some(first) = tokens.next() {
                let first_token = first.text_range();
                let last_token = tokens.last().map_or(first_token, |t| t.text_range());
                return VerbatimSpan {
                    first_token,
                    last_token,
                };
            }
        }
        // A bare token, or a node with only trivia: the whole span is the token.
        let whole = elem.text_range();
        VerbatimSpan {
            first_token: whole,
            last_token: whole,
        }
    }

    /// The verbatim source span to print: from the first token to the last,
    /// excluding any leading/trailing trivia the CST folded into the node.
    pub fn content_range(&self) -> TextRange {
        TextRange::new(self.first_token.start(), self.last_token.end())
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
            SyntaxKind::WORD | SyntaxKind::KW_CLIENT => {
                PathExpr::from_cst(elem).map(Expression::Path)?
            }
            SyntaxKind::PATH_EXPR => {
                // The parser wraps any postfix `<...>` in a PATH_EXPR. When the
                // base is a plain path (word / nested PATH_EXPR) it is a
                // `PathExpr`; otherwise (a parenthesized expr, lambda, etc.) it
                // is a generic instantiation on a non-path base.
                let node = StrongAstError::assert_is_node(elem.clone())?;
                let base_is_path = SyntaxNodeIter::new(&node).next().is_some_and(|c| {
                    is_path_segment_kind(c.kind()) || c.kind() == SyntaxKind::PATH_EXPR
                });
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

/// Corresponds to either a [`SyntaxKind::PATH_EXPR`] node or single [`SyntaxKind::WORD`] token.
#[derive(Debug)]
pub struct PathExpr {
    pub first: t::Word,
    pub rest: Vec<(t::Dot, t::Word)>,
    /// Trailing generic arguments, e.g. the `<int, string>` in `f<int, string>`
    /// or `baml.fetch_as<Todo>`. Only present at the tail of the path.
    pub generic_args: Option<GenericArgs>,
}

fn is_path_segment_kind(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::WORD
            | SyntaxKind::KW_CLIENT
            | SyntaxKind::KW_SPAWN
            | SyntaxKind::KW_AWAIT
            | SyntaxKind::KW_CLASS
            | SyntaxKind::KW_ENUM
            | SyntaxKind::KW_INTERFACE
            | SyntaxKind::KW_FUNCTION
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

        // First child: either a WORD, or a nested PATH_EXPR (the parser wraps
        // an existing path expr when it adds GENERIC_ARGS as a postfix).
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

        // Then: DOT WORD pairs, optionally followed by a single GENERIC_ARGS.
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

/// A generic instantiation whose base is not a plain path, e.g.
/// `(<T>(x: T) -> T { x })<int>` or `(foo)<int>`. Corresponds to a
/// [`SyntaxKind::PATH_EXPR`] node whose first child is an arbitrary expression
/// followed by `GENERIC_ARGS`.
#[derive(Debug)]
pub struct GenericApplyExpr {
    pub base: Box<Expression>,
    pub generic_args: GenericArgs,
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

/// Corresponds to a [`SyntaxKind::PAREN_EXPR`] node.
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

        let mut it = SyntaxNodeIter::new(&node);

        let open_paren = it.expect_parse()?;

        let expr = it.expect_next("an expression")?;
        let expr = Expression::from_cst(expr)?;

        let close_paren = it.expect_parse()?;

        it.expect_end()?;

        Ok(ParenExpr {
            open_paren,
            expr: Box::new(expr),
            close_paren,
        })
    }
}

impl KnownKind for ParenExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::PAREN_EXPR
    }
}

/// Corresponds to a [`SyntaxKind::BINARY_EXPR`] node.
#[derive(Debug)]
pub struct BinaryExpr {
    pub op: BinaryOp,
    pub sides: Box<(Expression, Expression)>,
}

impl FromCST for BinaryExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::BINARY_EXPR)?;

        let mut it = SyntaxNodeIter::new(&node);

        // Get left expression
        let left = it.expect_next("left expression")?;
        let left_expr = Expression::from_cst(left)?;

        // Get operator - handle `??` which appears as two consecutive QUESTION tokens
        let op_elem = it.expect_next("binary operator")?;
        let op = if op_elem.kind() == SyntaxKind::QUESTION {
            // Check for second QUESTION to form `??`
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

impl KnownKind for BinaryExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::BINARY_EXPR
    }
}
/// Corresponds to a [`SyntaxKind::IS_EXPR`] node.
///
/// `<expr> is <pattern>` - Rust `matches!`-style pattern test. Structure is
/// rigid (an expression LHS, a single keyword, a pattern RHS), so the
/// formatter prints it on a single line whenever it fits and otherwise
/// keeps the keyword glued to the pattern on the next line.
#[derive(Debug)]
pub struct IsExpr {
    pub lhs: Box<Expression>,
    pub keyword: t::Is,
    pub pattern: MatchPattern,
}

impl FromCST for IsExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::IS_EXPR)?;

        let mut it = SyntaxNodeIter::new(&node);
        let lhs_elem = it.expect_next("`is` left expression")?;
        let lhs = Expression::from_cst(lhs_elem)?;
        let kw_elem = it.expect_next("`is` keyword")?;
        let keyword = t::Is::from_cst(kw_elem)?;
        let pat_elem = it.expect_next("`is` pattern")?;
        let pattern = MatchPattern::from_cst(pat_elem)?;
        it.expect_end()?;

        Ok(IsExpr {
            lhs: Box::new(lhs),
            keyword,
            pattern,
        })
    }
}

impl KnownKind for IsExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::IS_EXPR
    }
}

/// Corresponds to a [`SyntaxKind::UNARY_EXPR`] node.
#[derive(Debug)]
pub struct UnaryExpr {
    pub op: UnaryOp,
    pub expr: Box<Expression>,
}

impl FromCST for UnaryExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::UNARY_EXPR)?;

        let mut it = SyntaxNodeIter::new(&node);

        // Get operator
        let op = it.expect_next("unary operator")?;
        let op = UnaryOp::from_cst(op)?;

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

impl KnownKind for UnaryExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::UNARY_EXPR
    }
}

/// Corresponds to a [`SyntaxKind::IF_EXPR`] node.
#[derive(Debug)]
pub struct IfExpr {
    pub keyword: t::If,
    /// The condition expression. Parens are optional in Baml, so this can be
    /// any expression - `if (a == b)` and `if a == b` are both valid.
    pub condition: Box<Expression>,
    pub block: BlockExpr,
    pub else_branch: Option<(t::Else, ElseExpr)>,
}

impl FromCST for IfExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::IF_EXPR)?;

        let mut it = SyntaxNodeIter::new(&node);

        // KW_IF
        let keyword = it.expect_parse()?;

        // Condition: any expression (parens are optional in Baml).
        let condition_elem = it.expect_next("an if condition expression")?;
        let condition = Box::new(Expression::from_cst(condition_elem)?);

        // BLOCK_EXPR
        let block: BlockExpr = it.expect_parse()?;

        // Optional else branch
        let else_branch = if let Some(elem) = it.next() {
            let else_token = t::Else::from_cst(elem)?;

            let else_body_node = it.expect_node("else body (if, if-let, or block)")?;
            let else_body = match else_body_node.kind() {
                SyntaxKind::IF_EXPR => ElseExpr::If(Box::new(IfExpr::from_cst(
                    SyntaxElement::Node(else_body_node),
                )?)),
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
}

impl KnownKind for IfExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::IF_EXPR
    }
}

/// Used in [`IfExpr`] / [`IfLetExpr`] to represent the else/else-if branch.
#[derive(Debug)]
pub enum ElseExpr {
    /// else if
    If(Box<IfExpr>),
    /// else if let
    IfLet(Box<IfLetExpr>),
    /// final else block
    Block(Box<BlockExpr>),
}

/// Corresponds to a [`SyntaxKind::IF_LET_EXPR`] node.
///
/// `if let PATTERN = SCRUTINEE BLOCK (else (BLOCK | IF_EXPR | IF_LET_EXPR))?`
#[derive(Debug)]
pub struct IfLetExpr {
    pub keyword: t::If,
    /// `let PATTERN` - the leading `let` is part of the pattern grammar
    /// (`parse_let_pattern`), so it's stored inside `pattern` rather than
    /// as a separate token.
    pub pattern: MatchPattern,
    pub equals: t::Equals,
    pub scrutinee: Box<Expression>,
    pub block: BlockExpr,
    pub else_branch: Option<(t::Else, ElseExpr)>,
}

impl FromCST for IfLetExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::IF_LET_EXPR)?;

        let mut it = SyntaxNodeIter::new(&node);

        // KW_IF
        let keyword = it.expect_parse()?;

        // PATTERN (consumes its own leading `let` token)
        let pattern = it.expect_parse()?;

        // `=` separator between pattern and scrutinee
        let equals = it.expect_parse()?;

        // Scrutinee: any expression
        let scrutinee_elem = it.expect_next("if-let scrutinee expression")?;
        let scrutinee = Box::new(Expression::from_cst(scrutinee_elem)?);

        // Then block
        let block: BlockExpr = it.expect_parse()?;

        // Optional else / else-if / else-if-let
        let else_branch = if let Some(elem) = it.next() {
            let else_token = t::Else::from_cst(elem)?;
            let else_body_node = it.expect_node("else body (if, if-let, or block)")?;
            let else_body = match else_body_node.kind() {
                SyntaxKind::IF_EXPR => ElseExpr::If(Box::new(IfExpr::from_cst(
                    SyntaxElement::Node(else_body_node),
                )?)),
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
}

impl KnownKind for IfLetExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::IF_LET_EXPR
    }
}

/// An element of a match/catch arm list: an arm, or a `//#` header comment
/// appearing between arms. Headers are legal arm-list elements (the parser
/// consumes them there, mirroring statement blocks), so the strong AST must
/// carry them through formatting.
#[derive(Debug)]
pub enum ArmListItem<A> {
    Arm(A),
    Header(t::HeaderComment),
}

/// Corresponds to a [`SyntaxKind::MATCH_EXPR`] node.
#[derive(Debug)]
pub struct MatchExpr {
    pub keyword: t::Match,
    pub open_paren: t::LParen,
    pub scrutinee: Box<Expression>,
    pub close_paren: t::RParen,
    pub open_brace: t::LBrace,
    pub arms: Vec<ArmListItem<MatchArm>>,
    pub close_brace: t::RBrace,
}

impl FromCST for MatchExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::MATCH_EXPR)?;

        let mut it = SyntaxNodeIter::new(&node);

        // KW_MATCH
        let keyword = it.expect_parse()?;

        // L_PAREN
        let open_paren = it.expect_parse()?;

        // Scrutinee expression (can be any node that represents an expression)
        let scrutinee_node = it.expect_next("scrutinee expression")?;
        let scrutinee = Box::new(Expression::from_cst(scrutinee_node)?);

        // R_PAREN
        let close_paren = it.expect_parse()?;

        // L_BRACE
        let open_brace = it.expect_parse()?;

        // Collect match arms
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
                    arms.push(ArmListItem::Arm(arm));
                }
                SyntaxKind::HEADER_COMMENT => {
                    arms.push(ArmListItem::Header(t::HeaderComment::from_cst(elem)?));
                }
                _ => {
                    return Err(StrongAstError::UnexpectedKindDesc {
                        expected_desc: "MATCH_ARM, HEADER_COMMENT, or R_BRACE".into(),
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
}

impl KnownKind for MatchExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::MATCH_EXPR
    }
}

/// Corresponds to a [`SyntaxKind::MATCH_ARM`] node.
#[derive(Debug)]
pub struct MatchArm {
    pub pattern: MatchPattern,
    pub guard: Option<MatchGuard>,
    pub fat_arrow: t::FatArrow,
    pub body: Expression,
    pub comma: Option<t::Comma>,
}

impl FromCST for MatchArm {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::MATCH_ARM)?;

        let mut it = SyntaxNodeIter::new(&node);

        // MATCH_PATTERN
        let pattern: MatchPattern = it.expect_parse()?;

        // Check for optional guard (if condition)
        let guard = it
            .next_if_kind(SyntaxKind::MATCH_GUARD)
            .map(MatchGuard::from_cst)
            .transpose()?;

        // FAT_ARROW
        let fat_arrow = it.expect_parse()?;

        // Body expression
        let body_node = it.expect_next("match arm body")?;
        let body = Expression::from_cst(body_node)?;

        let comma = it.next().map(t::Comma::from_cst).transpose()?;

        it.expect_end()?;

        Ok(MatchArm {
            pattern,
            guard,
            fat_arrow,
            body,
            comma,
        })
    }
}

impl KnownKind for MatchArm {
    fn kind() -> SyntaxKind {
        SyntaxKind::MATCH_ARM
    }
}

/// Corresponds to a [`SyntaxKind::MATCH_GUARD`] node.
#[derive(Debug)]
pub struct MatchGuard {
    pub keyword: t::If,
    pub condition: Expression,
}

impl FromCST for MatchGuard {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::MATCH_GUARD)?;

        let mut it = SyntaxNodeIter::new(&node);

        let if_token = it.expect_parse()?;

        let condition = it.expect_next("a condition")?;
        let condition = Expression::from_cst(condition)?;

        it.expect_end()?;

        Ok(MatchGuard {
            keyword: if_token,
            condition,
        })
    }
}

impl KnownKind for MatchGuard {
    fn kind() -> SyntaxKind {
        SyntaxKind::MATCH_GUARD
    }
}

/// Corresponds to a [`SyntaxKind::CATCH_EXPR`] node.
#[derive(Debug)]
pub struct CatchExpr {
    pub base: Box<Expression>,
    pub clauses: Vec<CatchClause>,
}

impl FromCST for CatchExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::CATCH_EXPR)?;

        let mut it = SyntaxNodeIter::new(&node);
        let base = Box::new(Expression::from_cst(
            it.expect_next("catch base expression")?,
        )?);

        let mut clauses = Vec::new();
        for elem in it {
            if elem.kind() != SyntaxKind::CATCH_CLAUSE {
                return Err(StrongAstError::UnexpectedKindDesc {
                    expected_desc: "CATCH_CLAUSE".into(),
                    found: elem.kind(),
                    at: elem.text_range(),
                });
            }
            clauses.push(CatchClause::from_cst(elem)?);
        }

        Ok(Self { base, clauses })
    }
}

impl KnownKind for CatchExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::CATCH_EXPR
    }
}

/// The `catch`, `catch_all`, or `catch_all_panics` keyword that starts a catch clause.
#[derive(Debug)]
pub enum CatchKeyword {
    Catch(t::Catch),
    CatchAll(t::CatchAll),
    CatchAllPanics(t::CatchAllPanics),
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

impl Token for CatchKeyword {
    fn span(&self) -> TextRange {
        match self {
            CatchKeyword::Catch(keyword) => keyword.span(),
            CatchKeyword::CatchAll(keyword) => keyword.span(),
            CatchKeyword::CatchAllPanics(keyword) => keyword.span(),
        }
    }
}

/// `catch (binding)` and optional stack-trace bindings use small wrapper nodes.
#[derive(Debug)]
pub struct CatchBinding {
    pub name: t::Word,
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

/// Corresponds to a [`SyntaxKind::CATCH_CLAUSE`] node.
#[derive(Debug)]
pub struct CatchClause {
    pub keyword: CatchKeyword,
    pub open_paren: t::LParen,
    pub binding: CatchBinding,
    pub stack_trace_binding: Option<(t::Comma, CatchBinding)>,
    pub close_paren: t::RParen,
    pub open_brace: t::LBrace,
    pub arms: Vec<ArmListItem<CatchArm>>,
    pub close_brace: t::RBrace,
}

impl FromCST for CatchClause {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::CATCH_CLAUSE)?;

        let mut it = SyntaxNodeIter::new(&node);
        let keyword = CatchKeyword::from_cst(it.expect_next("catch keyword")?)?;
        let open_paren = it.expect_parse()?;
        let binding = CatchBinding::from_cst_kind(
            it.expect_next("catch binding")?,
            SyntaxKind::CATCH_BINDING,
        )?;
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
                SyntaxKind::CATCH_ARM => arms.push(ArmListItem::Arm(CatchArm::from_cst(elem)?)),
                SyntaxKind::HEADER_COMMENT => {
                    arms.push(ArmListItem::Header(t::HeaderComment::from_cst(elem)?));
                }
                found => {
                    return Err(StrongAstError::UnexpectedKindDesc {
                        expected_desc: "CATCH_ARM, HEADER_COMMENT, or R_BRACE".into(),
                        found,
                        at: elem.text_range(),
                    });
                }
            }
        };
        it.expect_end()?;

        Ok(Self {
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
}

impl KnownKind for CatchClause {
    fn kind() -> SyntaxKind {
        SyntaxKind::CATCH_CLAUSE
    }
}

/// Corresponds to a [`SyntaxKind::CATCH_ARM`] node.
#[derive(Debug)]
pub struct CatchArm {
    pub pattern: MatchPattern,
    pub fat_arrow: t::FatArrow,
    pub body: Expression,
    pub comma: Option<t::Comma>,
}

impl FromCST for CatchArm {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::CATCH_ARM)?;

        let mut it = SyntaxNodeIter::new(&node);
        let pattern = it.expect_parse()?;
        let fat_arrow = it.expect_parse()?;
        let body = Expression::from_cst(it.expect_next("catch arm body")?)?;
        let comma = it.next().map(t::Comma::from_cst).transpose()?;
        it.expect_end()?;

        Ok(Self {
            pattern,
            fat_arrow,
            body,
            comma,
        })
    }
}

impl KnownKind for CatchArm {
    fn kind() -> SyntaxKind {
        SyntaxKind::CATCH_ARM
    }
}

/// Corresponds to a [`SyntaxKind::CALL_EXPR`] node.
#[derive(Debug)]
pub struct CallExpr {
    pub callee: Box<Expression>,
    pub args: CallArgs,
}

impl FromCST for CallExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::CALL_EXPR)?;

        let mut it = SyntaxNodeIter::new(&node);

        // Callee expression
        let callee_node = it.expect_next("callee expression")?;
        let callee = Box::new(Expression::from_cst(callee_node)?);

        // CALL_ARGS
        let args: CallArgs = it.expect_parse()?;

        Ok(CallExpr { callee, args })
    }
}

impl KnownKind for CallExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::CALL_EXPR
    }
}

/// Corresponds to a [`SyntaxKind::CALL_ARGS`] node.
#[derive(Debug)]
pub struct CallArgs {
    pub open_paren: t::LParen,
    pub args: Vec<(CallArg, Option<t::Comma>)>,
    pub close_paren: t::RParen,
}
impl FromCST for CallArgs {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
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
}

/// Corresponds to a [`SyntaxKind::CALL_ARG`] node.
#[derive(Debug)]
pub struct CallArg {
    pub label: Option<(t::Word, t::Equals)>,
    pub expr: Expression,
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

impl KnownKind for CallArgs {
    fn kind() -> SyntaxKind {
        SyntaxKind::CALL_ARGS
    }
}

/// Corresponds to a [`SyntaxKind::INDEX_EXPR`] node.
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

        let mut it = SyntaxNodeIter::new(&node);

        // Base expression
        let base_node = it.expect_next("base expression")?;
        let base = Box::new(Expression::from_cst(base_node)?);

        // L_BRACKET
        let open_bracket = it.expect_parse()?;

        // Index expression
        let index_node = it.expect_next("index expression")?;
        let index = Box::new(Expression::from_cst(index_node)?);

        // R_BRACKET
        let close_bracket = it.expect_parse()?;

        it.expect_end()?;

        Ok(IndexExpr {
            base,
            open_bracket,
            index,
            close_bracket,
        })
    }
}

impl KnownKind for IndexExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::INDEX_EXPR
    }
}

/// Corresponds to a [`SyntaxKind::FIELD_ACCESS_EXPR`] node.
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

        let mut it = SyntaxNodeIter::new(&node);

        // Base expression
        let base_node = it.expect_next("base expression")?;
        let base = Box::new(Expression::from_cst(base_node)?);

        // DOT
        let dot = it.expect_parse()?;

        // WORD (field name)
        let field = it.expect_parse()?;

        it.expect_end()?;

        Ok(FieldAccessExpr { base, dot, field })
    }
}

impl KnownKind for FieldAccessExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::FIELD_ACCESS_EXPR
    }
}

/// Corresponds to a [`SyntaxKind::OPTIONAL_FIELD_ACCESS_EXPR`] node: `base?.field`.
#[derive(Debug)]
pub struct OptionalFieldAccessExpr {
    pub base: Box<Expression>,
    pub question_dot: t::QuestionDot,
    pub field: t::Word,
}

impl FromCST for OptionalFieldAccessExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::OPTIONAL_FIELD_ACCESS_EXPR)?;

        let mut it = SyntaxNodeIter::new(&node);

        let base_node = it.expect_next("base expression")?;
        let base = Box::new(Expression::from_cst(base_node)?);

        let question_dot = it.expect_parse()?;

        let field = it.expect_parse()?;

        it.expect_end()?;

        Ok(OptionalFieldAccessExpr {
            base,
            question_dot,
            field,
        })
    }
}

impl KnownKind for OptionalFieldAccessExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::OPTIONAL_FIELD_ACCESS_EXPR
    }
}

/// Corresponds to a [`SyntaxKind::OPTIONAL_INDEX_EXPR`] node: `base?.[index]`.
#[derive(Debug)]
pub struct OptionalIndexExpr {
    pub base: Box<Expression>,
    pub question_dot: t::QuestionDot,
    pub open_bracket: t::LBracket,
    pub index: Box<Expression>,
    pub close_bracket: t::RBracket,
}

impl FromCST for OptionalIndexExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::OPTIONAL_INDEX_EXPR)?;

        let mut it = SyntaxNodeIter::new(&node);

        let base_node = it.expect_next("base expression")?;
        let base = Box::new(Expression::from_cst(base_node)?);

        let question_dot = it.expect_parse()?;

        let open_bracket = it.expect_parse()?;

        let index_node = it.expect_next("index expression")?;
        let index = Box::new(Expression::from_cst(index_node)?);

        let close_bracket = it.expect_parse()?;

        it.expect_end()?;

        Ok(OptionalIndexExpr {
            base,
            question_dot,
            open_bracket,
            index,
            close_bracket,
        })
    }
}

impl KnownKind for OptionalIndexExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::OPTIONAL_INDEX_EXPR
    }
}

/// Corresponds to a [`SyntaxKind::OPTIONAL_CALL_EXPR`] node: `callee?.(args)`.
#[derive(Debug)]
pub struct OptionalCallExpr {
    pub callee: Box<Expression>,
    pub question_dot: t::QuestionDot,
    pub args: CallArgs,
}

impl FromCST for OptionalCallExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::OPTIONAL_CALL_EXPR)?;

        let mut it = SyntaxNodeIter::new(&node);

        let callee_node = it.expect_next("callee expression")?;
        let callee = Box::new(Expression::from_cst(callee_node)?);

        let question_dot = it.expect_parse()?;

        let args: CallArgs = it.expect_parse()?;

        it.expect_end()?;

        Ok(OptionalCallExpr {
            callee,
            question_dot,
            args,
        })
    }
}

impl KnownKind for OptionalCallExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::OPTIONAL_CALL_EXPR
    }
}

/// Corresponds to a [`SyntaxKind::ENV_ACCESS_EXPR`] node.
#[derive(Debug)]
pub struct EnvAccessExpr {
    pub keyword: t::Word,
    pub dot: t::Dot,
    pub field: t::Word,
}

impl FromCST for EnvAccessExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::ENV_ACCESS_EXPR)?;

        let mut it = SyntaxNodeIter::new(&node);

        let keyword = it.expect_parse()?;

        let dot = it.expect_parse()?;

        let field = it.expect_parse()?;

        it.expect_end()?;

        Ok(EnvAccessExpr {
            keyword,
            dot,
            field,
        })
    }
}

impl KnownKind for EnvAccessExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::ENV_ACCESS_EXPR
    }
}

/// Corresponds to a [`SyntaxKind::BLOCK_EXPR`] node.
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

        let mut it = SyntaxNodeIter::new(&node);

        let open_brace = it.expect_parse()?;

        // Collect all statements and optional final expression
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
            open_brace,
            stmts,
            expr: expr.map(Box::new),
            close_brace,
        })
    }
}

impl KnownKind for BlockExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::BLOCK_EXPR
    }
}

/// Corresponds to a [`SyntaxKind::ARRAY_LITERAL`] node.
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
}

impl KnownKind for ArrayInitializer {
    fn kind() -> SyntaxKind {
        SyntaxKind::ARRAY_LITERAL
    }
}

/// Corresponds to a [`SyntaxKind::OBJECT_LITERAL`] node.
#[derive(Debug)]
pub struct ObjectInitializer {
    pub name: PathExpr,
    pub open_brace: t::LBrace,
    /// Fields and `...spread` members, in source order. Order is significant:
    /// later members win at runtime, so it must be preserved verbatim.
    pub fields: Vec<(ObjectMember, Option<t::Comma>)>,
    pub close_brace: t::RBrace,
}

impl FromCST for ObjectInitializer {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::OBJECT_LITERAL)?;

        let mut it = SyntaxNodeIter::new(&node);

        // WORD (object type name)
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
                SyntaxKind::OBJECT_FIELD | SyntaxKind::SPREAD_ELEMENT => {
                    let field = ObjectMember::from_cst(elem)?;
                    let comma = it
                        .next_if_kind(SyntaxKind::COMMA)
                        .map(t::Comma::from_cst)
                        .transpose()?;
                    fields.push((field, comma));
                }
                _ => {
                    return Err(StrongAstError::UnexpectedKindDesc {
                        expected_desc: "OBJECT_FIELD, SPREAD_ELEMENT, or R_BRACE".into(),
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
}

impl KnownKind for ObjectInitializer {
    fn kind() -> SyntaxKind {
        SyntaxKind::OBJECT_LITERAL
    }
}

/// Corresponds to a [`SyntaxKind::MAP_LITERAL`] node.
#[derive(Debug)]
pub struct MapLiteral {
    pub open_brace: t::LBrace,
    pub fields: Vec<(ObjectField, Option<t::Comma>)>,
    pub close_brace: t::RBrace,
}

impl FromCST for MapLiteral {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
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
}

impl KnownKind for MapLiteral {
    fn kind() -> SyntaxKind {
        SyntaxKind::MAP_LITERAL
    }
}

/// Corresponds to a [`SyntaxKind::OBJECT_FIELD`] node.
#[derive(Debug)]
pub struct ObjectField {
    pub name: ObjectFieldKey,
    /// Absent for property shorthand (`{ options }`). The parser only permits
    /// shorthand for a bare identifier, never for a quoted or qualified key.
    pub colon: Option<t::Colon>,
    pub value: Option<Expression>,
}

impl FromCST for ObjectField {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
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
}

impl KnownKind for ObjectField {
    fn kind() -> SyntaxKind {
        SyntaxKind::OBJECT_FIELD
    }
}

/// A member of an [`ObjectInitializer`]: either a `name: value` field or a
/// `...expr` spread element.
///
/// Only [`SyntaxKind::OBJECT_LITERAL`] admits spreads; map literals and array
/// literals keep using [`ObjectField`] directly.
#[derive(Debug)]
pub enum ObjectMember {
    Field(ObjectField),
    Spread(SpreadElement),
}

impl FromCST for ObjectMember {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        match elem.kind() {
            SyntaxKind::OBJECT_FIELD => Ok(ObjectMember::Field(ObjectField::from_cst(elem)?)),
            SyntaxKind::SPREAD_ELEMENT => Ok(ObjectMember::Spread(SpreadElement::from_cst(elem)?)),
            _ => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "OBJECT_FIELD or SPREAD_ELEMENT".into(),
                found: elem.kind(),
                at: elem.text_range(),
            }),
        }
    }
}

/// Corresponds to a [`SyntaxKind::SPREAD_ELEMENT`] node.
///
/// Struct-update spread inside an object literal: `Type { ...base, field: v }`.
#[derive(Debug)]
pub struct SpreadElement {
    pub dot_dot_dot: t::DotDotDot,
    pub value: Expression,
}

impl FromCST for SpreadElement {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::SPREAD_ELEMENT)?;

        let mut it = SyntaxNodeIter::new(&node);

        let dot_dot_dot = it.expect_parse()?;
        let value = it.expect_next("spread value")?;
        let value = Expression::from_cst(value)?;

        it.expect_end()?;

        Ok(SpreadElement { dot_dot_dot, value })
    }
}

impl KnownKind for SpreadElement {
    fn kind() -> SyntaxKind {
        SyntaxKind::SPREAD_ELEMENT
    }
}

/// Represents the a valid key for an [`ObjectField`].
#[derive(Debug)]
pub enum ObjectFieldKey {
    Word(t::Word),
    String(t::QuotedString),
}

impl FromCST for ObjectFieldKey {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        match elem.kind() {
            // `client` (KW_CLIENT) is a keyword but a valid field name, e.g.
            // `Agent { client: ... }` - mirror `parse_object_field`.
            kind if t::is_word_like(kind) => Ok(ObjectFieldKey::Word(t::Word::from_cst(elem)?)),
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

/// Corresponds to a [`SyntaxKind::GENERIC_PARAM_LIST`] node.
///
/// Contains `<T, U>` generic parameter declarations for a lambda expression.
/// Printed as `<T>` or `<K, V>` etc.
#[derive(Debug)]
pub struct GenericParamList {
    pub open_angle: t::Less,
    /// Comma-separated type parameter declarations.
    pub params: Vec<GenericParam>,
    pub close_angle: t::Greater,
}

#[derive(Debug)]
pub struct GenericParam {
    pub name: t::Word,
    pub bounds: Option<GenericParamBounds>,
    pub comma: Option<t::Comma>,
}

#[derive(Debug)]
pub struct GenericParamBounds {
    pub extends: t::Extends,
    pub bounds: Vec<(Type, Option<t::And>)>,
}

impl FromCST for GenericParamList {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
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

impl KnownKind for GenericParamList {
    fn kind() -> SyntaxKind {
        SyntaxKind::GENERIC_PARAM_LIST
    }
}

/// Corresponds to a [`SyntaxKind::GENERIC_ARGS`] node.
///
/// Contains `<Type1, Type2, ...>` generic arguments at a call site
/// or generic-typed path (e.g. `f<int, string>(...)`, `Box<int> { ... }`).
#[derive(Debug)]
pub struct GenericArgs {
    pub open_angle: t::Less,
    /// Comma-separated static or contextual runtime type arguments.
    pub args: Vec<(GenericArg, Option<t::Comma>)>,
    pub close_angle: t::Greater,
}

#[derive(Debug)]
pub enum GenericArg {
    Type(Type),
    Unreflect(UnreflectArg),
}

#[derive(Debug)]
pub struct UnreflectArg {
    pub keyword: t::Word,
    pub open_paren: t::LParen,
    pub expr: Box<Expression>,
    pub close_paren: t::RParen,
}

impl FromCST for UnreflectArg {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::UNREFLECT_ARG)?;
        let mut it = SyntaxNodeIter::new(&node);
        let keyword = it.expect_parse()?;
        let open_paren = it.expect_parse()?;
        let expr = Box::new(Expression::from_cst(it.next().ok_or_else(|| {
            StrongAstError::missing(SyntaxKind::PATH_EXPR, it.parent)
        })?)?);
        let close_paren = it.expect_parse()?;
        it.expect_end()?;
        Ok(Self {
            keyword,
            open_paren,
            expr,
            close_paren,
        })
    }
}

impl FromCST for GenericArgs {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
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
                    let arg = GenericArg::Type(Type::from_cst(elem)?);
                    let comma = it
                        .next_if_kind(SyntaxKind::COMMA)
                        .map(t::Comma::from_cst)
                        .transpose()?;
                    args.push((arg, comma));
                }
                SyntaxKind::UNREFLECT_ARG => {
                    let arg = GenericArg::Unreflect(UnreflectArg::from_cst(elem)?);
                    let comma = it
                        .next_if_kind(SyntaxKind::COMMA)
                        .map(t::Comma::from_cst)
                        .transpose()?;
                    args.push((arg, comma));
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
}

impl KnownKind for GenericArgs {
    fn kind() -> SyntaxKind {
        SyntaxKind::GENERIC_ARGS
    }
}

/// Arrow token in a function signature. Accepts either `->` (canonical) or
/// `=>` (accepted permissively for ergonomic parity with JS/TS arrow functions);
/// the formatter always emits `->`. Shared by declarations and lambdas so the
/// compiler's permissive syntax and formatter repair stay in lockstep.
#[derive(Debug)]
pub enum FunctionArrow {
    Arrow(t::Arrow),
    FatArrow(t::FatArrow),
}

impl FromCST for FunctionArrow {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let token = StrongAstError::assert_is_token(elem)?;
        match token.kind() {
            SyntaxKind::ARROW => Ok(FunctionArrow::Arrow(t::Arrow::new_from_span(
                token.text_range(),
            ))),
            SyntaxKind::FAT_ARROW => Ok(FunctionArrow::FatArrow(t::FatArrow::new_from_span(
                token.text_range(),
            ))),
            _ => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "ARROW or FAT_ARROW".into(),
                found: token.kind(),
                at: token.text_range(),
            }),
        }
    }
}

impl KnownKind for FunctionArrow {
    fn kind() -> SyntaxKind {
        // Primary/canonical kind; `from_cst` also accepts FAT_ARROW.
        SyntaxKind::ARROW
    }
}

/// Corresponds to a [`SyntaxKind::LAMBDA_EXPR`] node.
///
/// Syntax: `[<T, U>] (params) (-> | =>) [RetType] [throws E] { body }`
#[derive(Debug)]
pub struct LambdaExpr {
    pub generic_params: Option<GenericParamList>,
    pub param_list: super::FunctionParamList,
    pub arrow: FunctionArrow,
    pub return_type: Option<Type>,
    pub throws: Option<ThrowsClause>,
    pub block: BlockExpr,
}

#[allow(clippy::redundant_closure_for_method_calls)]
impl FromCST for LambdaExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::LAMBDA_EXPR)?;

        let mut it = SyntaxNodeIter::new(&node);

        // Optional generic params: <T, U>
        let generic_params = if it.peek().map(|e| e.kind()) == Some(SyntaxKind::GENERIC_PARAM_LIST)
        {
            let elem = it.next().expect("peeked");
            Some(GenericParamList::from_cst(elem)?)
        } else {
            None
        };

        // Parameter list: (x: int, y: string) or ()
        let param_list: super::FunctionParamList = it.expect_parse()?;

        // Arrow: `->` or `=>` (formatter normalizes to `->`)
        let arrow: FunctionArrow = it.expect_parse()?;

        // Optional return type: TYPE_EXPR before THROWS_CLAUSE or BLOCK_EXPR
        let return_type = if it.peek().map(|e| e.kind()) == Some(SyntaxKind::TYPE_EXPR) {
            let elem = it.next().expect("peeked");
            Some(Type::from_cst(elem)?)
        } else {
            None
        };

        // Optional throws clause
        let throws = if it.peek().map(|e| e.kind()) == Some(SyntaxKind::THROWS_CLAUSE) {
            let elem = it.next().expect("peeked");
            Some(ThrowsClause::from_cst(elem)?)
        } else {
            None
        };

        // Block body
        let block: BlockExpr = it.expect_parse()?;

        it.expect_end()?;

        Ok(LambdaExpr {
            generic_params,
            param_list,
            arrow,
            return_type,
            throws,
            block,
        })
    }
}

impl KnownKind for LambdaExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::LAMBDA_EXPR
    }
}

/// Corresponds to a [`SyntaxKind::SPAWN_EXPR`] node.
///
/// `spawn name_expr? (with expr (, expr)*)? { body }` (BEP-034). The name
/// expression and the `with` options clause are both optional; the body is
/// always a brace-delimited block.
#[derive(Debug)]
pub struct SpawnExpr {
    pub keyword: t::Spawn,
    /// Optional task-name expression between `spawn` and `with`/the body.
    pub name: Option<Expression>,
    pub with_clause: Option<SpawnWithClause>,
    pub block: BlockExpr,
}

pub type SpawnWithClause = (t::With, Vec<(Expression, Option<t::Comma>)>);

impl FromCST for SpawnExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
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
}

impl KnownKind for SpawnExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::SPAWN_EXPR
    }
}
