use super::{
    AstToken, BinaryOp, FromCST, FunctionParamList, KnownKind, Literal, MatchPattern,
    OptionalPrefixed, OptionalRemaining, SeparatedUntil, SeparatedValuesUntil, Statement,
    SyntaxElement, SyntaxKind, SyntaxNodeIter, TextRange, Type, UnaryOp, Until, ValidatedAstError,
    t,
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
    fn from_cst(elem: SyntaxElement) -> Result<Self, ValidatedAstError> {
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
                let node = ValidatedAstError::assert_is_node(elem.clone())?;
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

fn path_segment_from_cst(elem: SyntaxElement) -> Result<t::Word, ValidatedAstError> {
    let token = ValidatedAstError::assert_is_token(elem)?;
    if is_path_segment_kind(token.kind()) {
        Ok(t::Word::new_from_span(token.text_range()))
    } else {
        Err(ValidatedAstError::UnexpectedKindDesc {
            expected_desc: "path segment".into(),
            found: token.kind(),
            at: token.text_range(),
        })
    }
}

impl FromCST for PathExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, ValidatedAstError> {
        if is_path_segment_kind(elem.kind()) {
            let first = path_segment_from_cst(elem)?;
            return Ok(PathExpr {
                first,
                rest: Vec::new(),
                generic_args: None,
            });
        }
        let node = ValidatedAstError::assert_is_node(elem)?;
        ValidatedAstError::assert_kind_node(&node, SyntaxKind::PATH_EXPR)?;
        let mut it = SyntaxNodeIter::new(&node);
        let next = it
            .next()
            .ok_or_else(|| ValidatedAstError::missing(SyntaxKind::WORD, it.parent))?;
        let (first, mut rest) = match next.kind() {
            kind if is_path_segment_kind(kind) => (path_segment_from_cst(next)?, Vec::new()),
            SyntaxKind::PATH_EXPR => {
                let nested = PathExpr::from_cst(next)?;
                if nested.generic_args.is_some() {
                    return Err(ValidatedAstError::UnexpectedAdditionalElement {
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
                return Err(ValidatedAstError::UnexpectedAdditionalElement {
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
                        return Err(ValidatedAstError::UnexpectedAdditionalElement {
                            parent: it.parent,
                            at: extra.text_range(),
                        });
                    }
                    break;
                }
                _ => {
                    return Err(ValidatedAstError::UnexpectedAdditionalElement {
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
    fn from_cst(elem: SyntaxElement) -> Result<Self, ValidatedAstError> {
        let node = ValidatedAstError::assert_is_node(elem)?;
        ValidatedAstError::assert_kind_node(&node, SyntaxKind::PATH_EXPR)?;
        let mut it = SyntaxNodeIter::new(&node);
        let base_elem = it
            .next()
            .ok_or_else(|| ValidatedAstError::missing(SyntaxKind::PAREN_EXPR, it.parent))?;
        let base = Box::new(Expression::from_cst(base_elem)?);
        let ga_elem = it
            .next()
            .ok_or_else(|| ValidatedAstError::missing(SyntaxKind::GENERIC_ARGS, it.parent))?;
        let generic_args = GenericArgs::from_cst(ga_elem)?;
        if let Some(extra) = it.next() {
            return Err(ValidatedAstError::UnexpectedAdditionalElement {
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

fn parse_binary_expr(elem: SyntaxElement) -> Result<BinaryExpr, ValidatedAstError> {
    let node = ValidatedAstError::assert_is_node(elem)?;
    ValidatedAstError::assert_kind_node(&node, SyntaxKind::BINARY_EXPR)?;
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
            return Err(ValidatedAstError::UnexpectedKindDesc {
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
    IfExpr, IF_EXPR {
        keyword: required t::If;
        /// The condition expression. Parens are optional in Baml, so this can be
        /// any expression - `if (a == b)` and `if a == b` are both valid.
        condition: boxed Expression;
        block: required BlockExpr;
        else_branch: spec OptionalPrefixed<t::Else, ElseExpr>;
    }
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

impl FromCST for ElseExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, ValidatedAstError> {
        match elem.kind() {
            SyntaxKind::IF_EXPR => IfExpr::from_cst(elem).map(Box::new).map(Self::If),
            SyntaxKind::IF_LET_EXPR => IfLetExpr::from_cst(elem).map(Box::new).map(Self::IfLet),
            SyntaxKind::BLOCK_EXPR => BlockExpr::from_cst(elem).map(Box::new).map(Self::Block),
            found => Err(ValidatedAstError::UnexpectedKindDesc {
                expected_desc: "IF_EXPR, IF_LET_EXPR, or BLOCK_EXPR".into(),
                found,
                at: elem.text_range(),
            }),
        }
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::IF_LET_EXPR`] node.
    ///
    /// `if let PATTERN = SCRUTINEE BLOCK (else (BLOCK | IF_EXPR | IF_LET_EXPR))?`
    IfLetExpr, IF_LET_EXPR {
        keyword: required t::If;
        /// `let PATTERN` - the leading `let` is part of the pattern grammar
        /// (`parse_let_pattern`), so it's stored inside `pattern` rather than
        /// as a separate token.
        pattern: required MatchPattern;
        equals: required t::Equals;
        scrutinee: boxed Expression;
        block: required BlockExpr;
        else_branch: spec OptionalPrefixed<t::Else, ElseExpr>;
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::MATCH_EXPR`] node.
    MatchExpr, MATCH_EXPR {
        keyword: required t::Match;
        open_paren: required t::LParen;
        scrutinee: boxed Expression;
        close_paren: required t::RParen;
        open_brace: required t::LBrace;
        arms: spec Until<MatchArm, t::RBrace>;
        close_brace: required t::RBrace;
    }
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

validated_ast_enum! {
    /// The `catch`, `catch_all`, or `catch_all_panics` keyword that starts a catch clause.
    pub enum CatchKeyword {
        KW_CATCH => Catch(t::Catch),
        KW_CATCH_ALL => CatchAll(t::CatchAll),
        KW_CATCH_ALL_PANICS => CatchAllPanics(t::CatchAllPanics),
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

validated_ast_data! {
    /// `catch (binding)` and optional stack-trace bindings use small wrapper nodes.
    pub struct CatchBinding {
        pub name: t::Word,
    }
}

impl CatchBinding {
    fn from_cst_kind(elem: SyntaxElement, kind: SyntaxKind) -> Result<Self, ValidatedAstError> {
        let node = ValidatedAstError::assert_is_node(elem)?;
        ValidatedAstError::assert_kind_node(&node, kind)?;
        let mut it = SyntaxNodeIter::new(&node);
        let name = it.expect_parse()?;
        it.expect_end()?;
        Ok(Self { name })
    }
}

impl FromCST for CatchBinding {
    fn from_cst(elem: SyntaxElement) -> Result<Self, ValidatedAstError> {
        match elem.kind() {
            SyntaxKind::CATCH_BINDING => Self::from_cst_kind(elem, SyntaxKind::CATCH_BINDING),
            SyntaxKind::CATCH_STACK_TRACE_BINDING => {
                Self::from_cst_kind(elem, SyntaxKind::CATCH_STACK_TRACE_BINDING)
            }
            found => Err(ValidatedAstError::UnexpectedKindDesc {
                expected_desc: "CATCH_BINDING or CATCH_STACK_TRACE_BINDING".into(),
                found,
                at: elem.text_range(),
            }),
        }
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::CATCH_CLAUSE`] node.
    CatchClause, CATCH_CLAUSE {
        keyword: required CatchKeyword;
        open_paren: required t::LParen;
        binding: required CatchBinding;
        stack_trace_binding: spec OptionalPrefixed<t::Comma, CatchBinding>;
        close_paren: required t::RParen;
        open_brace: required t::LBrace;
        arms: spec Until<CatchArm, t::RBrace>;
        close_brace: required t::RBrace;
    }
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
    CallArgs, CALL_ARGS {
        open_paren: required t::LParen;
        args: spec SeparatedUntil<CallArg, t::Comma, t::RParen>;
        close_paren: required t::RParen;
    }
}

validated_ast_data! {
    /// Corresponds to a [`SyntaxKind::CALL_ARG`] node.
    pub struct CallArg {
        pub label: Option<(t::Word, t::Equals)>,
        pub expr: Expression,
    }
}

impl FromCST for CallArg {
    fn from_cst(elem: SyntaxElement) -> Result<Self, ValidatedAstError> {
        if elem.kind() != SyntaxKind::CALL_ARG {
            return Ok(CallArg {
                label: None,
                expr: Expression::from_cst(elem)?,
            });
        }
        let node = ValidatedAstError::assert_is_node(elem)?;
        ValidatedAstError::assert_kind_node(&node, SyntaxKind::CALL_ARG)?;
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
                return Err(ValidatedAstError::missing_desc(
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

fn parse_block_expr(elem: SyntaxElement) -> Result<BlockExpr, ValidatedAstError> {
    let node = ValidatedAstError::assert_is_node(elem)?;
    ValidatedAstError::assert_kind_node(&node, SyntaxKind::BLOCK_EXPR)?;
    let mut it = SyntaxNodeIter::new(&node);
    let open_brace = it.expect_parse()?;
    let mut stmts = Vec::new();
    let close_brace = loop {
        let Some(elem) = it.next() else {
            return Err(ValidatedAstError::missing(SyntaxKind::R_BRACE, it.parent));
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
    ArrayInitializer, ARRAY_LITERAL {
        open_bracket: required t::LBracket;
        /// Commas are optional for all elements.
        /// For example, `[1 2 3]` is equivalent to `[1, 2, 3]` in BAML.
        ///
        /// While this is valid, excluding commas is *strongly* discouraged as it is a crime against software and also more error-prone:
        /// if `[1, -2, 3]` is written as `[1 -2 3]`, it will be parsed as `[1-2, 3]` instead (the `-` will be treated as a binary operator instead of a unary operator).
        elements: spec SeparatedUntil<Expression, t::Comma, t::RBracket>;
        close_bracket: required t::RBracket;
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::OBJECT_LITERAL`] node.
    ObjectInitializer, OBJECT_LITERAL {
        name: required PathExpr;
        open_brace: required t::LBrace;
        fields: spec SeparatedUntil<ObjectField, t::Comma, t::RBrace>;
        close_brace: required t::RBrace;
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::MAP_LITERAL`] node.
    MapLiteral, MAP_LITERAL {
        open_brace: required t::LBrace;
        fields: spec SeparatedUntil<ObjectField, t::Comma, t::RBrace>;
        close_brace: required t::RBrace;
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::OBJECT_FIELD`] node.
    ObjectField, OBJECT_FIELD {
        name: required ObjectFieldKey;
        /// Absent for property shorthand (`{ options }`). The parser only permits
        /// shorthand for a bare identifier, never for a quoted or qualified key.
        colon: optional_element t::Colon;
        value: spec OptionalRemaining<Expression>;
    }
}

validated_ast_enum! {
    /// Represents the a valid key for an [`ObjectField`].
    pub enum ObjectFieldKey {
        WORD => Word(t::Word),
        STRING_LITERAL => String(t::QuotedString),
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::GENERIC_PARAM_LIST`] node.
    ///
    /// Contains `<T, U>` generic parameter declarations for a lambda expression.
    /// Printed as `<T>` or `<K, V>` etc.
    GenericParamList, GENERIC_PARAM_LIST {
        open_angle: required t::Less;
        /// Comma-separated type parameter declarations.
        params: spec SeparatedValuesUntil<GenericParam, t::Comma, t::Greater>;
        close_angle: required t::Greater;
    }
}

validated_ast_node! {
    GenericParam, GENERIC_PARAM {
        name: required t::Word;
        bounds: optional GenericParamBounds;
    }
}

validated_ast_data! {
    pub struct GenericParamBounds {
        pub extends: t::Extends,
        pub bounds: Vec<(Type, Option<t::And>)>,
    }
}

impl FromCST for GenericParamBounds {
    fn from_cst(elem: SyntaxElement) -> Result<Self, ValidatedAstError> {
        let node = ValidatedAstError::assert_is_node(elem)?;
        ValidatedAstError::assert_kind_node(&node, SyntaxKind::GENERIC_PARAM_BOUNDS)?;
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

impl KnownKind for GenericParamBounds {
    fn kind() -> SyntaxKind {
        SyntaxKind::GENERIC_PARAM_BOUNDS
    }
}

validated_ast_node! {
    /// Corresponds to a [`SyntaxKind::GENERIC_ARGS`] node.
    ///
    /// Contains `<Type1, Type2, ...>` generic arguments at a call site
    /// or generic-typed path (e.g. `f<int, string>(...)`, `Box<int> { ... }`).
    GenericArgs, GENERIC_ARGS {
        open_angle: required t::Less;
        /// Comma-separated type arguments.
        args: spec SeparatedUntil<Type, t::Comma, t::Greater>;
        close_angle: required t::Greater;
    }
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

validated_ast_enum! {
    /// Arrow token in a lambda expression. Accepts either `->` (canonical) or
    /// `=>` (accepted permissively for ergonomic parity with JS/TS arrow functions);
    /// the formatter always emits `->`.
    pub enum LambdaArrow {
        ARROW => Arrow(t::Arrow),
        FAT_ARROW => FatArrow(t::FatArrow),
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

fn parse_spawn_expr(elem: SyntaxElement) -> Result<SpawnExpr, ValidatedAstError> {
    let node = ValidatedAstError::assert_is_node(elem)?;
    ValidatedAstError::assert_kind_node(&node, SyntaxKind::SPAWN_EXPR)?;
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
                return Err(ValidatedAstError::UnexpectedAdditionalElement {
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
