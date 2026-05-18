//! Reference: [`baml_db::baml_compiler_syntax::ast::Expr`] and [`baml_db::baml_compiler_hir::body`]

use baml_db::baml_compiler_syntax::{SyntaxElement, SyntaxKind};
use rowan::TextRange;

use crate::{
    ast::{
        BinaryOp, FromCST, KnownKind, MatchPattern, Statement, StrongAstError, SyntaxNodeIter,
        Token, UnaryOp, tokens as t,
    },
    printer::{PrintInfo, PrintMultiLine, Printable, Printer, Shape},
    trivia_classifier::TriviaSliceExt,
};

#[derive(Debug)]
pub enum Expression {
    Literal(Literal),
    /// Includes things like `null`, `true`, `false`, `baml.fs`, etc.
    Path(PathExpr),
    Paren(ParenExpr),
    Binary(BinaryExpr),
    Is(IsExpr),
    Unary(UnaryExpr),
    If(IfExpr),
    Match(MatchExpr),
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
    ByteString(t::ByteString),
    Lambda(Box<LambdaExpr>),
    Unknown(TextRange),
}

impl Expression {
    #[must_use]
    pub const fn statement_needs_semicolon(&self) -> bool {
        !matches!(
            self,
            Expression::If(_)
                | Expression::Match(_)
                | Expression::Lambda(_)
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
            SyntaxKind::PATH_EXPR | SyntaxKind::WORD => {
                PathExpr::from_cst(elem).map(Expression::Path)?
            }
            SyntaxKind::PAREN_EXPR => ParenExpr::from_cst(elem).map(Expression::Paren)?,
            SyntaxKind::BINARY_EXPR => BinaryExpr::from_cst(elem).map(Expression::Binary)?,
            SyntaxKind::IS_EXPR => IsExpr::from_cst(elem).map(Expression::Is)?,
            SyntaxKind::UNARY_EXPR => UnaryExpr::from_cst(elem).map(Expression::Unary)?,
            SyntaxKind::IF_EXPR => IfExpr::from_cst(elem).map(Expression::If)?,
            SyntaxKind::MATCH_EXPR => MatchExpr::from_cst(elem).map(Expression::Match)?,
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
            SyntaxKind::BYTE_STRING_LITERAL => {
                t::ByteString::from_cst(elem).map(Expression::ByteString)?
            }
            SyntaxKind::LAMBDA_EXPR => Expression::Lambda(Box::new(LambdaExpr::from_cst(elem)?)),
            _ => Expression::Unknown(elem.text_range()),
        };
        Ok(expr)
    }
}

impl Expression {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        match self {
            Expression::Literal(lit) => lit.single_line_width(input),
            Expression::Path(path) => path.single_line_width(input),
            Expression::Paren(paren) => paren.single_line_width(input),
            Expression::Binary(binary) => binary.single_line_width(input),
            Expression::Is(is) => is.single_line_width(input),
            Expression::Unary(unary) => unary.single_line_width(input),
            Expression::If(_) => None,
            Expression::Match(_) => None,
            Expression::Call(call) => call.single_line_width(input),
            Expression::Index(index) => index.single_line_width(input),
            Expression::FieldAccess(fa) => fa.single_line_width(input),
            Expression::OptionalFieldAccess(fa) => fa.single_line_width(input),
            Expression::OptionalIndex(index) => index.single_line_width(input),
            Expression::OptionalCall(call) => call.single_line_width(input),
            Expression::EnvAccess(env) => env.single_line_width(input),
            Expression::Block(_) => None,
            Expression::ArrayInitializer(array) => array.single_line_width(input),
            Expression::MapInitializer(map) => map.single_line_width(input),
            Expression::ObjectInitializer(obj) => obj.single_line_width(input),
            Expression::RawString(raw) => {
                if input.input[raw.span()].contains('\n') {
                    None
                } else {
                    Some(usize::from(raw.span().len()))
                }
            }
            Expression::ByteString(bs) => Some(usize::from(bs.span().len())),
            Expression::Lambda(_) => None,
            Expression::Unknown(_) => None,
        }
    }
}

impl Printable for Expression {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            Expression::Literal(lit) => lit.print(shape, printer),
            chain @ (Expression::Path(_)
            | Expression::Call(_)
            | Expression::Index(_)
            | Expression::FieldAccess(_)
            | Expression::OptionalFieldAccess(_)
            | Expression::OptionalIndex(_)
            | Expression::OptionalCall(_)) => {
                // These are all chains of postfix expressions
                let chain = PrintChain::new(chain);
                chain.print(shape, printer)
            }
            Expression::Paren(paren) => paren.print(shape, printer),
            Expression::Binary(binary) => binary.print(shape, printer),
            Expression::Is(is) => is.print(shape, printer),
            Expression::Unary(unary) => unary.print(shape, printer),
            Expression::If(if_expr) => if_expr.print(shape, printer),
            Expression::Match(match_expr) => match_expr.print(shape, printer),
            Expression::EnvAccess(env) => env.print(shape, printer),
            Expression::Block(block) => block.print(shape, printer),
            Expression::ArrayInitializer(array) => array.print(shape, printer),
            Expression::MapInitializer(map) => map.print(shape, printer),
            Expression::ObjectInitializer(obj) => obj.print(shape, printer),
            Expression::RawString(raw) => raw.print(shape, printer),
            Expression::ByteString(bs) => bs.print(shape, printer),
            Expression::Lambda(lambda) => lambda.print(shape, printer),
            Expression::Unknown(range) => {
                printer.print_input_range_trimmed_start(*range);
                PrintInfo::default_multi_lined()
            }
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            Expression::Literal(lit) => lit.leftmost_token(),
            Expression::Path(path) => path.leftmost_token(),
            Expression::Paren(paren) => paren.leftmost_token(),
            Expression::Binary(binary) => binary.leftmost_token(),
            Expression::Is(is) => is.leftmost_token(),
            Expression::Unary(unary) => unary.leftmost_token(),
            Expression::If(if_expr) => if_expr.leftmost_token(),
            Expression::Match(match_expr) => match_expr.leftmost_token(),
            Expression::Call(call) => call.leftmost_token(),
            Expression::Index(index) => index.leftmost_token(),
            Expression::FieldAccess(fa) => fa.base.leftmost_token(),
            Expression::OptionalFieldAccess(fa) => fa.base.leftmost_token(),
            Expression::OptionalIndex(index) => index.base.leftmost_token(),
            Expression::OptionalCall(call) => call.callee.leftmost_token(),
            Expression::EnvAccess(env) => env.leftmost_token(),
            Expression::Block(block) => block.leftmost_token(),
            Expression::ArrayInitializer(array) => array.leftmost_token(),
            Expression::MapInitializer(map) => map.leftmost_token(),
            Expression::ObjectInitializer(obj) => obj.leftmost_token(),
            Expression::RawString(raw) => raw.leftmost_token(),
            Expression::ByteString(bs) => bs.leftmost_token(),
            Expression::Lambda(lambda) => lambda.leftmost_token(),
            Expression::Unknown(range) => *range,
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            Expression::Literal(lit) => lit.rightmost_token(),
            Expression::Path(path) => path.rightmost_token(),
            Expression::Paren(paren) => paren.rightmost_token(),
            Expression::Binary(binary) => binary.rightmost_token(),
            Expression::Is(is) => is.rightmost_token(),
            Expression::Unary(unary) => unary.rightmost_token(),
            Expression::If(if_expr) => if_expr.rightmost_token(),
            Expression::Match(match_expr) => match_expr.rightmost_token(),
            Expression::Call(call) => call.rightmost_token(),
            Expression::Index(index) => index.rightmost_token(),
            Expression::FieldAccess(fa) => fa.field.span(),
            Expression::OptionalFieldAccess(fa) => fa.field.span(),
            Expression::OptionalIndex(index) => index.close_bracket.span(),
            Expression::OptionalCall(call) => call.args.rightmost_token(),
            Expression::EnvAccess(env) => env.rightmost_token(),
            Expression::Block(block) => block.rightmost_token(),
            Expression::ArrayInitializer(array) => array.rightmost_token(),
            Expression::MapInitializer(map) => map.rightmost_token(),
            Expression::ObjectInitializer(obj) => obj.rightmost_token(),
            Expression::RawString(raw) => raw.rightmost_token(),
            Expression::ByteString(bs) => bs.rightmost_token(),
            Expression::Lambda(lambda) => lambda.rightmost_token(),
            Expression::Unknown(range) => *range,
        }
    }
}

#[derive(Debug)]
pub enum Literal {
    String(t::QuotedString),
    Integer(t::IntegerLiteral),
    Float(t::FloatLiteral),
}

impl FromCST for Literal {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        match elem.kind() {
            SyntaxKind::STRING_LITERAL => Ok(Literal::String(t::QuotedString::from_cst(elem)?)),
            SyntaxKind::INTEGER_LITERAL => Ok(Literal::Integer(t::IntegerLiteral::from_cst(elem)?)),
            SyntaxKind::FLOAT_LITERAL => Ok(Literal::Float(t::FloatLiteral::from_cst(elem)?)),
            _ => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "STRING_LITERAL, INTEGER_LITERAL, or FLOAT_LITERAL".into(),
                found: elem.kind(),
                at: elem.text_range(),
            }),
        }
    }
}

impl Literal {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        match self {
            Literal::String(s) => {
                if input.input[s.span()].contains('\n') {
                    None
                } else {
                    Some(usize::from(s.span().len()))
                }
            }
            Literal::Integer(i) => Some(usize::from(i.span().len())),
            Literal::Float(f) => Some(usize::from(f.span().len())),
        }
    }
}

impl Printable for Literal {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            Literal::String(s) => printer.print_raw_token(s),
            Literal::Integer(i) => printer.print_raw_token(i),
            Literal::Float(f) => printer.print_raw_token(f),
        }
        PrintInfo::default_single_line()
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            Literal::String(s) => s.leftmost_token(),
            Literal::Integer(i) => i.span(),
            Literal::Float(f) => f.span(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            Literal::String(s) => s.rightmost_token(),
            Literal::Integer(i) => i.span(),
            Literal::Float(f) => f.span(),
        }
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

impl FromCST for PathExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        if elem.kind() == SyntaxKind::WORD {
            let first = t::Word::from_cst(elem)?;
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
            SyntaxKind::WORD => (t::Word::from_cst(next)?, Vec::new()),
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
                    let word = it.expect_parse()?;
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

impl PathExpr {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    #[allow(clippy::unnecessary_wraps)]
    pub(crate) fn single_line_width(&self, _input: &Printer<'_>) -> Option<usize> {
        let mut len = usize::from(self.first.span().len());
        for (dot, word) in &self.rest {
            len += usize::from(dot.span().len()) + usize::from(word.span().len());
        }
        if let Some(ref ga) = self.generic_args {
            len += ga.formatted_single_line_width();
        }
        Some(len)
    }
}

impl Printable for PathExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        if self.rest.is_empty() {
            printer.print_raw_token(&self.first);
            if let Some(ref ga) = self.generic_args {
                printer.print(ga, shape);
            }
            return PrintInfo::default_single_line();
        }
        let first = Expression::Path(PathExpr {
            first: self.first.clone(),
            rest: Vec::new(),
            generic_args: None,
        });
        let chain_members = self
            .rest
            .iter()
            .map(|(dot, word)| PrintChainItem::FieldAccess(dot, word))
            .collect();
        let chain = PrintChain {
            first: &first,
            chain_members,
        };
        let info = chain.print(shape.clone(), printer);
        if let Some(ref ga) = self.generic_args {
            printer.print(ga, shape);
        }
        info
    }
    fn leftmost_token(&self) -> TextRange {
        self.first.span()
    }
    fn rightmost_token(&self) -> TextRange {
        if let Some(ref ga) = self.generic_args {
            return ga.close_angle.span();
        }
        self.rest
            .last()
            .map_or(&self.first, |(_, word)| word)
            .span()
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

impl ParenExpr {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let inner = self.expr.single_line_width(input)?;
        let (_, open_trailing) = input.trivia.get_for_range_split(self.open_paren.span());
        let (expr_leading, expr_trailing) = input.trivia.get_for_element(&*self.expr);
        let (close_leading, _) = input.trivia.get_for_range_split(self.close_paren.span());
        let trivia_len = open_trailing
            .iter()
            .chain(expr_leading)
            .chain(expr_trailing)
            .chain(close_leading)
            .map(|t| t.single_line_len(input.input))
            .sum::<Option<usize>>()?;
        Some(const { "()".len() } + inner + trivia_len)
    }
}

impl PrintMultiLine for ParenExpr {
    /// Multi-line layout: inner expression wraps to an indented new line,
    /// closing paren aligns with the opening context.
    ///
    /// ```baml
    /// (
    ///     some_long_expression
    /// )
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_shape = Shape {
            width: shape.width.saturating_sub(printer.config.indent_width),
            indent: shape.indent + printer.config.indent_width,
            first_line_offset: 0,
        };
        printer.print_raw_token(&self.open_paren);
        printer.print_trivia_all_trailing_for(self.open_paren.token_span);
        printer.print_newline();

        let (expr_leading, expr_trailing) = printer.trivia.get_for_element(&*self.expr);
        printer.print_trivia_with_newline(expr_leading.trim_blanks(), inner_shape.indent);
        printer.print_spaces(inner_shape.indent);
        printer.print(&*self.expr, inner_shape.clone());
        printer.print_trivia_trailing(expr_trailing);
        printer.print_newline();

        let (close_leading, _) = printer.trivia.get_for_range_split(self.close_paren.span());
        printer.print_trivia_with_newline(close_leading.trim_blanks(), inner_shape.indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_paren);
        PrintInfo::default_multi_lined()
    }
}

impl ParenExpr {
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the parenthesized expression on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        printer.print_raw_token(&self.open_paren);
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_paren.span());
        printer.try_print_trivia_single_line_squished(open_trailing)?;

        let (expr_leading, expr_trailing) = printer.trivia.get_for_element(&*self.expr);
        printer.try_print_trivia_single_line_squished(expr_leading)?;
        if printer
            .print(&*self.expr, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        printer.try_print_trivia_single_line_squished(expr_trailing)?;

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

impl Printable for ParenExpr {
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

        // Get operator — handle `??` which appears as two consecutive QUESTION tokens
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

impl BinaryExpr {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let (left, right) = &*self.sides;
        let left_width = left.single_line_width(input)?;
        let right_width = right.single_line_width(input)?;
        // Must match trivia handled by try_print_single_line
        let mut trivia_len = 0usize;
        let left_trailing = input.trivia.get_trailing_for_element(left);

        let (op_leading, op_trailing) = input.trivia.get_for_range_split(self.op.span());
        trivia_len += (op_leading.try_squished_len(input.input)?
            + left_trailing.try_squished_len(input.input)?)
        .max(const { " ".len() }); // basically, if not comments then we have the space

        let right_leading = input.trivia.get_leading_for_element(right);
        trivia_len += (right_leading.try_squished_len(input.input)?
            + op_trailing.try_squished_len(input.input)?)
        .max(const { " ".len() }); // basically, if not comments then we have the space

        let len = left_width + usize::from(self.op.span().len()) + right_width + trivia_len;
        Some(len)
    }

    /// Recursively lifts binary expressions in the same chaining group to the top level.
    /// For ops that are not in any chaining groups, return will be the same as the original.
    ///
    /// The vec will never be empty.
    fn get_chaining_members(&self) -> (&Expression, Vec<(&BinaryOp, &Expression)>) {
        let mut members = Vec::new();
        let Some(chaining_group) = BinaryOpChainingGroup::group_for_op(&self.op) else {
            members.push((&self.op, &self.sides.1));
            return (&self.sides.0, members);
        };

        match &*self.sides {
            (Expression::Binary(left), Expression::Binary(right))
                if BinaryOpChainingGroup::group_for_op(&left.op) == Some(chaining_group)
                    && BinaryOpChainingGroup::group_for_op(&right.op) == Some(chaining_group) =>
            {
                let (left_first, left_rest) = left.get_chaining_members();
                let (right_first, right_rest) = right.get_chaining_members();

                members.extend(left_rest);
                members.push((&self.op, right_first));
                members.extend(right_rest);

                (left_first, members)
            }
            (Expression::Binary(left), right)
                if BinaryOpChainingGroup::group_for_op(&left.op) == Some(chaining_group) =>
            {
                let (first, left_rest) = left.get_chaining_members();

                members.extend(left_rest);
                members.push((&self.op, right));
                (first, members)
            }
            (left, Expression::Binary(right))
                if BinaryOpChainingGroup::group_for_op(&right.op) == Some(chaining_group) =>
            {
                let (right_first, right_rest) = right.get_chaining_members();

                members.push((&self.op, right_first));
                members.extend(right_rest);
                (left, members)
            }
            (left, right) => {
                members.push((&self.op, right));
                (left, members)
            }
        }
    }
}

impl PrintMultiLine for BinaryExpr {
    /// Multi-line layout: splits at the operator. The operator and right-hand
    /// side wrap to an indented new line. Trailing comments on sub-expressions
    /// are preserved.
    ///
    /// ```baml
    /// left_expression // trailing comment
    ///     + right_expression
    /// ```
    ///
    /// For chainable operators, contained binary ops (of the same group) should be printed at the same indentation.
    /// Groups:
    ///     - Add/Subtract
    ///     - Multiply/Divide/Modulo
    ///     - Bitwise And/Or/Xor
    ///     - Logical And/Or
    ///
    /// ```baml
    /// a
    ///     + b
    ///     + c
    ///     - d * e
    /// ```
    ///
    /// ```baml
    /// // precedence matters:
    /// aaaaaaaaa
    ///     + bbbbbbbbb
    ///         * cccccccc
    ///         / dddddddd
    ///     - eeeeeee
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_indent = shape.indent + printer.config.indent_width;
        let (first, chain_members) = self.get_chaining_members();
        printer.print(first, shape);
        printer.print_trivia_all_trailing_for(first.rightmost_token());
        let num_chain_members = chain_members.len();
        for (i, (op, right)) in chain_members.into_iter().enumerate() {
            printer.print_newline();
            printer.print_spaces(inner_indent);
            printer.print(op, Shape::unlimited_single_line());
            printer.print_str(" ");
            let inner_shape = Shape {
                width: printer
                    .config
                    .line_width
                    .saturating_sub(inner_indent + usize::from(op.span().len()) + 1),
                indent: inner_indent,
                first_line_offset: usize::from(op.span().len()) + 1,
            };
            printer.print(right, inner_shape.clone());
            if i + 1 < num_chain_members {
                printer.print_trivia_all_trailing_for(right.rightmost_token());
            }
        }
        PrintInfo::default_multi_lined()
    }
}

impl BinaryExpr {
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the binary expression on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        let (left, right) = &*self.sides;

        if printer
            .print(left, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        let left_trailing = printer.trivia.get_trailing_for_element(left);
        let (op_leading, op_trailing) = printer.trivia.get_for_range_split(self.op.span());
        let right_leading = printer.trivia.get_leading_for_element(right);

        let mut left_trivia_len = printer.try_print_trivia_single_line_squished(left_trailing)?;
        left_trivia_len += printer.print_trivia_squished(op_leading);
        if left_trivia_len == 0 {
            printer.print_spaces(1); // only add space if there are no block comments between
        }

        printer.print(&self.op, Shape::unlimited_single_line());

        let mut right_trivia_len = printer.print_trivia_squished(op_trailing);
        right_trivia_len += printer.print_trivia_squished(right_leading);
        if right_trivia_len == 0 {
            printer.print_spaces(1); // only add space if there are no block comments between
        }
        if printer
            .print(right, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        // right trailing is the outermost trailing — not printed here

        if printer.output.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
    }
}

impl Printable for BinaryExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
    fn leftmost_token(&self) -> TextRange {
        self.sides.0.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.sides.1.rightmost_token()
    }
}

/// Categories for grouping binary operators for nested chaining
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BinaryOpChainingGroup {
    AddSubtract,
    MultiplyDivide,
    Bitwise,
    Logical,
}
impl BinaryOpChainingGroup {
    fn group_for_op(op: &BinaryOp) -> Option<Self> {
        match op {
            BinaryOp::Plus(_) | BinaryOp::Minus(_) => Some(Self::AddSubtract),
            BinaryOp::Star(_) | BinaryOp::Slash(_) | BinaryOp::Percent(_) => {
                Some(Self::MultiplyDivide)
            }
            BinaryOp::And(_) | BinaryOp::Pipe(_) | BinaryOp::Caret(_) => Some(Self::Bitwise),
            BinaryOp::AndAnd(_) | BinaryOp::OrOr(_) => Some(Self::Logical),
            _ => None,
        }
    }
}

/// Corresponds to a [`SyntaxKind::IS_EXPR`] node.
///
/// `<expr> is <pattern>` — Rust `matches!`-style pattern test. Structure is
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

impl IsExpr {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if the LHS can never be single-lined.
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let lhs = self.lhs.single_line_width(input)?;
        // The pattern's width is hard to query precisely without
        // reimplementing MatchPattern's own width logic, so use the source
        // span between leftmost and rightmost tokens as an upper bound —
        // overestimates by leading/trailing trivia, which is fine for the
        // line-fit check.
        let pat_left = self.pattern.leftmost_token().start();
        let pat_right = self.pattern.rightmost_token().end();
        let pattern_width = usize::from(pat_right - pat_left);
        // `<lhs> is <pattern>` — lhs + " " + "is" + " " + pattern.
        Some(lhs + 1 + usize::from(self.keyword.span().len()) + 1 + pattern_width)
    }
}

impl Printable for IsExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        // Mirrors `BinaryExpr::try_print_single_line`'s trivia handling so
        // comments around the `is` keyword (e.g. `v /*hint*/ is int`) round-
        // trip instead of being silently dropped.
        let mut multi_lined = false;

        multi_lined |= printer.print(&*self.lhs, shape.clone()).multi_lined;

        let lhs_trailing = printer.trivia.get_trailing_for_element(&*self.lhs);
        let (kw_leading, kw_trailing) = printer.trivia.get_for_range_split(self.keyword.span());

        let mut left_trivia_len = printer.print_trivia_squished(lhs_trailing);
        left_trivia_len += printer.print_trivia_squished(kw_leading);
        if left_trivia_len == 0 {
            printer.print_spaces(1);
        }

        printer.print_raw_token(&self.keyword);

        let pat_leading = printer.trivia.get_leading_for_element(&self.pattern);
        let mut right_trivia_len = printer.print_trivia_squished(kw_trailing);
        right_trivia_len += printer.print_trivia_squished(pat_leading);
        if right_trivia_len == 0 {
            printer.print_spaces(1);
        }

        multi_lined |= printer.print(&self.pattern, shape).multi_lined;
        PrintInfo { multi_lined }
    }
    fn leftmost_token(&self) -> TextRange {
        self.lhs.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.pattern.rightmost_token()
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

impl UnaryExpr {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let expr = self.expr.single_line_width(input)?;
        Some(usize::from(self.op.span().len()) + expr)
    }
}

impl Printable for UnaryExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;
        multi_lined |= printer.print(&self.op, shape.clone()).multi_lined;
        multi_lined |= printer.print(&*self.expr, shape).multi_lined;

        PrintInfo { multi_lined }
    }
    fn leftmost_token(&self) -> TextRange {
        self.op.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.expr.rightmost_token()
    }
}

/// Corresponds to a [`SyntaxKind::IF_EXPR`] node.
#[derive(Debug)]
pub struct IfExpr {
    pub keyword: t::If,
    /// The condition expression. Parens are optional in Baml, so this can be
    /// any expression — `if (a == b)` and `if a == b` are both valid.
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

impl Printable for IfExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        // Always print parens around the condition. Source may omit them
        // (Baml allows `if cond { ... }`), but emitting them keeps formatter
        // output consistent with the canonical form.
        let needs_parens = !matches!(*self.condition, Expression::Paren(_));
        let cond_shape = if needs_parens {
            // Reserve room for the synthetic `( )` so a barely-fitting
            // condition doesn't push the line past the width budget once
            // we wrap parens around it.
            let mut s = shape.clone();
            s.width = s.width.saturating_sub(2);
            s
        } else {
            shape.clone()
        };
        if needs_parens {
            printer.print_str("(");
        }
        printer.print(&*self.condition, cond_shape);
        if needs_parens {
            printer.print_str(")");
        }
        printer.print_str(" ");
        printer.print(&self.block, shape.clone());

        if let Some((else_kw, else_expr)) = &self.else_branch {
            printer.print_str(" ");
            printer.print_raw_token(else_kw);
            printer.print_str(" ");
            printer.print(else_expr, shape);
        }

        PrintInfo::default_multi_lined()
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        if let Some((_, else_expr)) = &self.else_branch {
            else_expr.rightmost_token()
        } else {
            self.block.rightmost_token()
        }
    }
}

/// Used in [`IfExpr`] to represent the else/else-if branch.
#[derive(Debug)]
pub enum ElseExpr {
    /// else if
    If(Box<IfExpr>),
    /// final else block
    Block(Box<BlockExpr>),
}

impl Printable for ElseExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            ElseExpr::If(if_expr) => if_expr.print(shape, printer),
            ElseExpr::Block(block) => block.print(shape, printer),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            ElseExpr::If(if_expr) => if_expr.leftmost_token(),
            ElseExpr::Block(block) => block.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            ElseExpr::If(if_expr) => if_expr.rightmost_token(),
            ElseExpr::Block(block) => block.rightmost_token(),
        }
    }
}

/// Corresponds to a [`SyntaxKind::MATCH_EXPR`] node.
#[derive(Debug)]
pub struct MatchExpr {
    pub keyword: t::Match,
    pub open_paren: t::LParen,
    pub scrutinee: Box<Expression>,
    pub close_paren: t::RParen,
    pub open_brace: t::LBrace,
    pub arms: Vec<MatchArm>,
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
}

impl KnownKind for MatchExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::MATCH_EXPR
    }
}

impl MatchExpr {
    fn try_print_scrutinee_single_line(
        &self,
        shape: &Shape,
        printer: &mut Printer,
    ) -> Option<PrintInfo> {
        printer.print_raw_token(&self.open_paren);
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_paren.span());
        printer.try_print_trivia_single_line_squished(open_trailing)?;

        let (scrutinee_leading, scrutinee_trailing) =
            printer.trivia.get_for_element(&*self.scrutinee);
        printer.try_print_trivia_single_line_squished(scrutinee_leading)?;
        if printer
            .print(&*self.scrutinee, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        printer.try_print_trivia_single_line_squished(scrutinee_trailing)?;

        let (close_leading, _) = printer.trivia.get_for_range_split(self.close_paren.span());
        printer.try_print_trivia_single_line_squished(close_leading)?;
        printer.print_raw_token(&self.close_paren);

        if printer.output.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
    }

    fn print_scrutinee_multi_line(&self, shape: &Shape, printer: &mut Printer) {
        let paren_inner_indent = shape.indent + printer.config.indent_width;
        printer.print_raw_token(&self.open_paren);
        printer.print_trivia_all_trailing_for(self.open_paren.span());
        printer.print_newline();

        printer.print_standalone_with_trivia(&*self.scrutinee, paren_inner_indent);
        printer.print_newline();
        printer
            .print_trivia_all_leading_with_newline_for(self.close_paren.span(), paren_inner_indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_paren);
    }
}

impl Printable for MatchExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_indent = shape.indent + printer.config.indent_width;

        // Print "match" keyword
        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");

        // Print scrutinee: try single-line, fall back to multi-line
        if printer
            .try_sub_printer(|p| self.try_print_scrutinee_single_line(&shape, p))
            .is_none()
        {
            self.print_scrutinee_multi_line(&shape, printer);
        }

        // Print body with block container pattern
        printer.print_str(" ");
        printer.print_raw_token(&self.open_brace);
        printer.print_trivia_all_trailing_for(self.open_brace.span());
        printer.print_newline();

        for arm in &self.arms {
            printer.print_standalone_with_trivia(arm, inner_indent);
            printer.print_newline();
        }

        printer.print_trivia_all_leading_with_newline_for(self.close_brace.span(), inner_indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_brace);

        PrintInfo::default_multi_lined()
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.close_brace.span()
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

impl MatchArm {
    /// Prints all of the arm except the body/expression (prints up to and including the `=>`)
    fn print_condition(&self, shape: &Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;

        let mut pattern_printer = printer.sub_printer();
        let pattern_info = pattern_printer.print(&self.pattern, shape.clone());
        multi_lined |= pattern_info.multi_lined;
        let pattern_len = pattern_printer.len();
        printer.append_from_printer(pattern_printer);

        if let Some(guard) = &self.guard {
            if pattern_info.multi_lined {
                // Guard goes on new line
                printer.print_newline();
                printer.print_spaces(shape.indent + printer.config.indent_width);
                let offset = usize::from(guard.keyword.token_span.len()) + const { " ".len() };
                let guard_shape = Shape {
                    width: printer.config.line_width.saturating_sub(
                        shape.indent + printer.config.indent_width + offset + const { " =>".len() },
                    ),
                    indent: shape.indent + printer.config.indent_width,
                    first_line_offset: offset,
                };
                guard.print(guard_shape, printer);
            } else if matches!(guard.condition, Expression::Paren(_) | Expression::Block(_)) {
                // we can delegate determining whether or not to multi-line to the guard expression
                // since it will do so nicely
                printer.print_spaces(1);
                let offset = shape.first_line_offset + pattern_len + 1;
                let guard_shape = Shape {
                    width: printer
                        .config
                        .line_width
                        .saturating_sub(shape.indent + offset + const { " => ".len() }),
                    indent: shape.indent,
                    first_line_offset: offset,
                };
                let guard_info = guard.print(guard_shape, printer);
                multi_lined |= guard_info.multi_lined;
            } else {
                // try printing guard single-line
                let mut guard_single_line = printer.sub_printer();
                let guard_info =
                    guard.print(Shape::unlimited_single_line(), &mut guard_single_line);

                let single_line_len = pattern_len
                    + const { " ".len() }
                    + guard_single_line.len()
                    + const { " =>".len() };
                if guard_info.multi_lined || single_line_len > shape.width {
                    // Guard is too long to fit on a single line, so print it on the next line
                    printer.print_newline();
                    printer.print_spaces(shape.indent + printer.config.indent_width);
                    let guard_shape = Shape {
                        width: printer
                            .config
                            .line_width
                            .saturating_sub(shape.indent + const { " => {".len() }),
                        indent: shape.indent,
                        first_line_offset: 0,
                    };
                    guard.print(guard_shape, printer);
                } else {
                    // guard goes on the same line after the pattern
                    printer.print_spaces(1);
                    printer.append_from_printer(guard_single_line);
                }
            }
        }

        printer.print_str(" =>");

        PrintInfo { multi_lined }
    }
}

impl Printable for MatchArm {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let condition_info = self.print_condition(&shape, printer);
        let condition_multi_lined = condition_info.multi_lined;

        if condition_multi_lined {
            // the body goes in a block expression on a new line
            printer.print_newline();

            printer.print_spaces(shape.indent);
            if let Expression::Block(block) = &self.body {
                // body is already a block expression
                let body_shape = Shape {
                    width: printer.config.line_width.saturating_sub(shape.indent),
                    indent: shape.indent,
                    first_line_offset: 0,
                };
                printer.print(block, body_shape);
                printer.print_str(",");
            } else {
                // put the body in a block expression
                printer.print_str("{");
                printer.print_newline();
                printer.print_standalone_with_trivia(
                    &self.body,
                    shape.indent + printer.config.indent_width,
                );
                printer.print_newline();
                printer.print_spaces(shape.indent);
                printer.print_str("},");
            }
            return PrintInfo::default_multi_lined();
        }

        // condition is single line, see if we can fit the body with it
        // TODO: if the body is a block with only a tail expression, we might be able to un-nest it

        printer.print_spaces(1);
        let line_len_remaining = printer.current_line_remaining_width();
        if let Expression::Block(block) = &self.body {
            // If it is a block expression, we print it directly in front of the ` => `.
            // Since the condition was single-line, the preceding line had no extra indent
            // so we don't need to put the `{` on a new line.
            let body_shape = Shape {
                width: line_len_remaining,
                indent: shape.indent,
                first_line_offset: printer
                    .config
                    .line_width
                    .saturating_sub(shape.indent + line_len_remaining),
            };
            let info = printer.print(block, body_shape);
            printer.print_str(",");
            return info;
        } else if let Expression::Match(match_expr) = &self.body
            && let Some(match_scrutinee_len) = match_expr.scrutinee.single_line_width(printer)
            && const { "match () {".len() } + match_scrutinee_len <= line_len_remaining
        {
            // Match expressions also may go directly on the same line if
            // `match (...) {` fits. The arms can be multi-line.
            let match_shape = Shape {
                width: line_len_remaining,
                indent: shape.indent,
                first_line_offset: printer
                    .config
                    .line_width
                    .saturating_sub(shape.indent + line_len_remaining),
            };
            let info = match_expr.print(match_shape, printer);
            printer.print_str(",");
            return info;
        }

        // try and print the body single-line
        let mut try_body = printer.sub_printer();
        let try_body_info = self
            .body
            .print(Shape::unlimited_single_line(), &mut try_body);

        if try_body_info.multi_lined || try_body.len() > line_len_remaining {
            // create a block expression around it
            printer.print_str("{");
            printer.print_newline();
            printer.print_standalone_with_trivia(
                &self.body,
                shape.indent + printer.config.indent_width,
            );
            printer.print_newline();
            printer.print_spaces(shape.indent);
            printer.print_str("},");
            PrintInfo::default_multi_lined()
        } else {
            printer.append_from_printer(try_body);
            printer.print_str(",");
            PrintInfo::default_single_line()
        }
    }
    fn leftmost_token(&self) -> TextRange {
        self.pattern.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        if let Some(comma) = &self.comma {
            comma.span()
        } else {
            self.body.rightmost_token()
        }
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

impl Printable for MatchGuard {
    fn print(&self, mut shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        shape.width = shape
            .width
            .saturating_sub(usize::from(self.keyword.token_span.len()) + 1);
        shape.first_line_offset += usize::from(self.keyword.token_span.len()) + 1;
        printer.print(&self.condition, shape)
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.condition.rightmost_token()
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

impl CallExpr {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let callee = self.callee.single_line_width(input)?;
        let args = self.args.single_line_width(input)?;
        Some(callee + args)
    }
}

impl Printable for CallExpr {
    /// The main way to call this should be through [`PrintChain`]
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;
        multi_lined |= printer.print(&*self.callee, shape.clone()).multi_lined;
        multi_lined |= printer.print(&self.args, shape).multi_lined;
        PrintInfo { multi_lined }
    }
    fn leftmost_token(&self) -> TextRange {
        self.callee.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.args.rightmost_token()
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

impl CallArg {
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let mut len = 0;
        if let Some((name, equals)) = &self.label {
            let (_, name_trailing) = input.trivia.get_for_range_split(name.span());
            let (equals_leading, equals_trailing) = input.trivia.get_for_range_split(equals.span());
            let expr_leading = input.trivia.get_leading_for_element(&self.expr);
            len += usize::from(name.span().len())
                + name_trailing.try_squished_len(input.input)?
                + equals_leading.try_squished_len(input.input)?
                + " = ".len()
                + equals_trailing.try_squished_len(input.input)?
                + expr_leading.try_squished_len(input.input)?;
        }
        len += self.expr.single_line_width(input)?;
        Some(len)
    }
}

impl Printable for CallArg {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        if let Some((name, equals)) = &self.label {
            printer.print_raw_token(name);
            let (_, name_trailing) = printer.trivia.get_for_range_split(name.span());
            let (equals_leading, equals_trailing) =
                printer.trivia.get_for_range_split(equals.span());
            let expr_leading = printer.trivia.get_leading_for_element(&self.expr);
            printer.print_trivia_squished(name_trailing);
            printer.print_trivia_squished(equals_leading);
            printer.print_str(" = ");
            printer.print_trivia_squished(equals_trailing);
            printer.print_trivia_squished(expr_leading);
        }
        printer.print(&self.expr, shape)
    }

    fn leftmost_token(&self) -> TextRange {
        self.label
            .as_ref()
            .map_or_else(|| self.expr.leftmost_token(), |(name, _)| name.span())
    }

    fn rightmost_token(&self) -> TextRange {
        self.expr.rightmost_token()
    }
}

impl KnownKind for CallArgs {
    fn kind() -> SyntaxKind {
        SyntaxKind::CALL_ARGS
    }
}

impl PrintMultiLine for CallArgs {
    /// Always multi-lined, even if there are no arguments it would still be `(\n<indent>)`
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_indent = shape.indent + printer.config.indent_width;
        let inner_shape = Shape {
            width: printer.config.line_width.saturating_sub(inner_indent),
            indent: inner_indent,
            first_line_offset: 0,
        };

        printer.print_raw_token(&self.open_paren);
        printer.print_trivia_all_trailing_for(self.open_paren.span());
        printer.print_newline();

        for (arg, comma) in &self.args {
            printer.print_trivia_all_leading_with_newline_for(
                arg.leftmost_token(),
                inner_shape.indent,
            );
            printer.print_spaces(inner_shape.indent);
            printer.print(arg, inner_shape.clone());
            if let Some(comma) = comma {
                printer.print_raw_token(comma);
                printer.print_trivia_all_trailing_for(comma.span());
            } else {
                printer.print_str(",");
                printer.print_trivia_all_trailing_for(arg.rightmost_token());
            }
            printer.print_newline();
        }

        printer
            .print_trivia_all_leading_with_newline_for(self.close_paren.span(), inner_shape.indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_paren);

        PrintInfo::default_multi_lined()
    }
}

impl CallArgs {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let mut len = const { "()".len() };
        let (_, open_trailing) = input.trivia.get_for_range_split(self.open_paren.span());
        for t in open_trailing {
            len += t.single_line_len(input.input)?;
        }
        for (i, (arg, comma)) in self.args.iter().enumerate() {
            let (arg_leading, arg_trailing) = input.trivia.get_for_element(arg);
            for t in arg_leading {
                len += t.single_line_len(input.input)?;
            }
            len += arg.single_line_width(input)?;
            for t in arg_trailing {
                len += t.single_line_len(input.input)?;
            }
            if i + 1 < self.args.len() {
                if let Some(comma) = comma {
                    let (comma_leading, comma_trailing) =
                        input.trivia.get_for_range_split(comma.span());
                    for t in comma_leading {
                        len += t.single_line_len(input.input)?;
                    }
                    len += 1; // ","
                    for t in comma_trailing {
                        len += t.single_line_len(input.input)?;
                    }
                } else {
                    len += 1; // ","
                }
                len += 1; // " "
            } else if let Some(comma) = comma {
                // Trailing comma is removed in single-line mode, but check trivia
                let (comma_leading, comma_trailing) =
                    input.trivia.get_for_range_split(comma.span());
                for t in comma_leading {
                    len += t.single_line_len(input.input)?;
                }
                for t in comma_trailing {
                    len += t.single_line_len(input.input)?;
                }
            }
        }
        let (close_leading, _) = input.trivia.get_for_range_split(self.close_paren.span());
        for t in close_leading {
            len += t.single_line_len(input.input)?;
        }
        Some(len)
    }

    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the call args on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        printer.print_raw_token(&self.open_paren);
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_paren.span());
        printer.try_print_trivia_single_line_squished(open_trailing)?;

        for (i, (arg, comma)) in self.args.iter().enumerate() {
            if printer.output.len() > shape.width {
                return None;
            }
            let (arg_leading, arg_trailing) = printer.trivia.get_for_element(arg);
            printer.try_print_trivia_single_line_squished(arg_leading)?;
            if printer
                .print(arg, Shape::unlimited_single_line())
                .multi_lined
            {
                return None;
            }
            printer.try_print_trivia_single_line_squished(arg_trailing)?;
            if i + 1 < self.args.len() {
                if let Some(comma) = comma {
                    let (comma_leading, comma_trailing) =
                        printer.trivia.get_for_range_split(comma.span());
                    printer.try_print_trivia_single_line_squished(comma_leading)?;
                    printer.print_raw_token(comma);
                    printer.try_print_trivia_single_line_squished(comma_trailing)?;
                } else {
                    printer.print_str(",");
                }
                printer.print_str(" ");
            } else if let Some(comma) = comma {
                // Trailing comma is removed in single-line mode, but we still try the comments.
                let (comma_leading, comma_trailing) =
                    printer.trivia.get_for_range_split(comma.span());
                printer.try_print_trivia_single_line_squished(comma_leading)?;
                printer.try_print_trivia_single_line_squished(comma_trailing)?;
            }
        }

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

impl Printable for CallArgs {
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

/// Represents the bracket-enclosed portion of an index expression: `[expr]`.
/// Analogous to [`CallArgs`] for call expressions.
/// Used by both [`IndexExpr`] and [`PrintChain`].
#[derive(Debug)]
pub struct IndexArgs<'a> {
    pub open_bracket: &'a t::LBracket,
    pub index: &'a Expression,
    pub close_bracket: &'a t::RBracket,
}

impl IndexArgs<'_> {
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let mut len = const { "[]".len() };
        len += self.index.single_line_width(input)?;
        let (_, open_trailing) = input.trivia.get_for_range_split(self.open_bracket.span());
        len += open_trailing.try_squished_len(input.input)?;
        let (index_leading, index_trailing) = input.trivia.get_for_element(self.index);
        len += index_leading.try_squished_len(input.input)?;
        len += index_trailing.try_squished_len(input.input)?;
        let (close_leading, _) = input.trivia.get_for_range_split(self.close_bracket.span());
        len += close_leading.try_squished_len(input.input)?;
        Some(len)
    }

    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        printer.print_raw_token(self.open_bracket);
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_bracket.span());
        printer.try_print_trivia_single_line_squished(open_trailing)?;

        let (index_leading, index_trailing) = printer.trivia.get_for_element(self.index);
        printer.try_print_trivia_single_line_squished(index_leading)?;
        if printer
            .print(self.index, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        printer.try_print_trivia_single_line_squished(index_trailing)?;

        let (close_leading, _) = printer
            .trivia
            .get_for_range_split(self.close_bracket.span());
        printer.try_print_trivia_single_line_squished(close_leading)?;
        printer.print_raw_token(self.close_bracket);

        if printer.output.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
    }
}

impl PrintMultiLine for IndexArgs<'_> {
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_indent = shape.indent + printer.config.indent_width;
        printer.print_raw_token(self.open_bracket);
        printer.print_trivia_all_trailing_for(self.open_bracket.span());
        printer.print_newline();

        let (index_leading, index_trailing) = printer.trivia.get_for_element(self.index);
        printer.print_trivia_with_newline(index_leading.trim_blanks(), inner_indent);
        printer.print_spaces(inner_indent);
        let inner_shape = Shape::standalone(printer.config.line_width, inner_indent);
        printer.print(self.index, inner_shape);
        printer.print_trivia_trailing(index_trailing);
        printer.print_newline();

        let (close_leading, _) = printer
            .trivia
            .get_for_range_split(self.close_bracket.span());
        printer.print_trivia_with_newline(close_leading.trim_blanks(), inner_indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(self.close_bracket);
        PrintInfo::default_multi_lined()
    }
}

impl Printable for IndexArgs<'_> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
    fn leftmost_token(&self) -> TextRange {
        self.open_bracket.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.close_bracket.span()
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

impl IndexExpr {
    fn args(&self) -> IndexArgs<'_> {
        IndexArgs {
            open_bracket: &self.open_bracket,
            index: &self.index,
            close_bracket: &self.close_bracket,
        }
    }

    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let base = self.base.single_line_width(input)?;
        Some(base + self.args().single_line_width(input)?)
    }
}

impl PrintMultiLine for IndexExpr {
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print(&*self.base, shape.clone());
        self.args().print_multi_line(shape, printer)
    }
}

impl IndexExpr {
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        let base_len = self.base.single_line_width(printer)?;
        let args_len = self.args().single_line_width(printer)?;
        if base_len + args_len > shape.width {
            return None;
        }
        if self
            .base
            .print(Shape::unlimited_single_line(), printer)
            .multi_lined
        {
            return None;
        }
        if self
            .args()
            .print(Shape::unlimited_single_line(), printer)
            .multi_lined
        {
            return None;
        }
        Some(PrintInfo::default_single_line())
    }
}

impl Printable for IndexExpr {
    /// The main way to call this should be through [`PrintChain`]
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
    fn leftmost_token(&self) -> TextRange {
        self.base.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.close_bracket.span()
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

impl FieldAccessExpr {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let base = self.base.single_line_width(input)?;
        Some(base + usize::from(self.dot.span().len()) + usize::from(self.field.span().len()))
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

impl OptionalFieldAccessExpr {
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let base = self.base.single_line_width(input)?;
        Some(
            base + usize::from(self.question_dot.span().len())
                + usize::from(self.field.span().len()),
        )
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

impl OptionalIndexExpr {
    fn args(&self) -> IndexArgs<'_> {
        IndexArgs {
            open_bracket: &self.open_bracket,
            index: &self.index,
            close_bracket: &self.close_bracket,
        }
    }

    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let base = self.base.single_line_width(input)?;
        Some(
            base + usize::from(self.question_dot.span().len())
                + self.args().single_line_width(input)?,
        )
    }
}

impl Printable for OptionalIndexExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;
        multi_lined |= printer.print(&*self.base, shape.clone()).multi_lined;
        printer.print_raw_token(&self.question_dot);
        multi_lined |= printer.print(&self.args(), shape).multi_lined;
        PrintInfo { multi_lined }
    }
    fn leftmost_token(&self) -> TextRange {
        self.base.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.close_bracket.span()
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

impl OptionalCallExpr {
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let callee = self.callee.single_line_width(input)?;
        let args = self.args.single_line_width(input)?;
        Some(callee + usize::from(self.question_dot.span().len()) + args)
    }
}

impl Printable for OptionalCallExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;
        multi_lined |= printer.print(&*self.callee, shape.clone()).multi_lined;
        printer.print_raw_token(&self.question_dot);
        multi_lined |= printer.print(&self.args, shape).multi_lined;
        PrintInfo { multi_lined }
    }
    fn leftmost_token(&self) -> TextRange {
        self.callee.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.args.rightmost_token()
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

impl EnvAccessExpr {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    #[allow(clippy::unnecessary_wraps)]
    pub(crate) fn single_line_width(&self, _input: &Printer<'_>) -> Option<usize> {
        Some(
            usize::from(self.keyword.span().len())
                + usize::from(self.dot.span().len())
                + usize::from(self.field.span().len()),
        )
    }
}

impl Printable for EnvAccessExpr {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);
        printer.print_raw_token(&self.dot);
        printer.print_raw_token(&self.field);
        PrintInfo::default_single_line()
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.field.span()
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

impl Printable for BlockExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.open_brace);
        printer.print_trivia_all_trailing_for(self.open_brace.span());
        printer.print_newline();

        // body statements
        let inner_indent = shape.indent + printer.config.indent_width;
        if let Some((first, rest)) = self.stmts.split_first() {
            let (first_leading, first_trailing) = printer.trivia.get_for_element(first);
            printer.print_trivia_with_newline(first_leading.trim_leading_blanks(), inner_indent);
            printer.print_spaces(inner_indent);
            let inner_shape = Shape::standalone(printer.config.line_width, inner_indent);
            printer.print(first, inner_shape);
            printer.print_trivia_trailing(first_trailing);
            printer.print_newline();

            for stmt in rest {
                printer.print_standalone_with_trivia(stmt, inner_indent);
                printer.print_newline();
            }
        }

        // tail expression
        if let Some(expr) = self.expr.as_deref() {
            let (expr_leading, expr_trailing) = printer.trivia.get_for_element(expr);
            let expr_leading = if self.stmts.is_empty() {
                expr_leading.trim_leading_blanks()
            } else {
                expr_leading
            };
            printer.print_trivia_with_newline(expr_leading, inner_indent);
            printer.print_spaces(inner_indent);
            let inner_shape = Shape::standalone(printer.config.line_width, inner_indent);
            printer.print(expr, inner_shape);
            printer.print_trivia_trailing(expr_trailing);
            printer.print_newline();
        }

        let (close_brace_leading, _) = printer.trivia.get_for_range_split(self.close_brace.span());
        printer.print_trivia_with_newline(close_brace_leading.trim_trailing_blanks(), inner_indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_brace);

        PrintInfo { multi_lined: true }
    }
    fn leftmost_token(&self) -> TextRange {
        self.open_brace.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.close_brace.span()
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

impl PrintMultiLine for ArrayInitializer {
    /// Multi-line layout: each element on its own indented line with trailing comma.
    /// Closing bracket on its own line.
    ///
    /// ```baml
    /// [
    ///     element1,
    ///     element2,
    ///     element3,
    /// ]
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.open_bracket);
        printer.print_trivia_all_trailing_for(self.open_bracket.span());
        printer.print_newline();

        let inner_indent = shape.indent + printer.config.indent_width;
        for (elem, comma) in &self.elements {
            let (elem_leading, elem_trailing) = printer.trivia.get_for_element(elem);
            printer.print_trivia_with_newline(elem_leading.trim_blanks(), inner_indent);
            printer.print_spaces(inner_indent);
            let inner_shape = Shape::standalone(printer.config.line_width, inner_indent);
            printer.print(elem, inner_shape);
            if let Some(comma) = comma {
                printer.print_trivia_squished(elem_trailing);
                printer.print_raw_token(comma);
                printer.print_trivia_all_trailing_for(comma.span());
            } else {
                printer.print_str(",");
                printer.print_trivia_trailing(elem_trailing);
            }
            printer.print_newline();
        }

        let (close_bracket_leading, _) = printer
            .trivia
            .get_for_range_split(self.close_bracket.span());
        printer
            .print_trivia_with_newline(close_bracket_leading.trim_trailing_blanks(), inner_indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_bracket);
        PrintInfo::default_multi_lined()
    }
}

impl ArrayInitializer {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let mut len = const { "[".len() };
        let (_, open_trailing) = input.trivia.get_for_range_split(self.open_bracket.span());
        len += open_trailing.try_squished_len(input.input)?;

        for (i, (elem, comma)) in self.elements.iter().enumerate() {
            let (el_leading, el_trailing) = input.trivia.get_for_element(elem);

            len += el_leading.try_squished_len(input.input)?;
            len += elem.single_line_width(input)?;

            let is_last = i + 1 >= self.elements.len();
            if let Some(comma) = comma {
                let (comma_leading, comma_trailing) =
                    input.trivia.get_for_range_split(comma.span());
                len += el_trailing.squished_len(input.input); // always squished before the comma
                len += comma_leading.squished_len(input.input); // always squished before the comma
                if !is_last {
                    len += const { ", ".len() };
                }
                len += comma_trailing.try_squished_len(input.input)?;
            } else {
                len += el_trailing.try_squished_len(input.input)?; // if multilined would go after the added comma
                if !is_last {
                    len += const { ", ".len() };
                }
            }
        }

        let (close_leading, _) = input.trivia.get_for_range_split(self.close_bracket.span());
        len += close_leading.try_squished_len(input.input)?;
        len += const { "]".len() };
        Some(len)
    }

    /// Tries to print the array initializer as a single line.
    ///
    /// If successful, returns the info.
    ///
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the array initializer on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        printer.print_raw_token(&self.open_bracket);
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_bracket.span());
        printer.try_print_trivia_single_line_squished(open_trailing)?;

        for (i, (elem, comma)) in self.elements.iter().enumerate() {
            if printer.output.len() > shape.width {
                return None;
            }

            let (el_leading, el_trailing) = printer.trivia.get_for_element(elem);
            printer.try_print_trivia_single_line_squished(el_leading)?;
            if printer
                .print(elem, Shape::unlimited_single_line())
                .multi_lined
            {
                return None;
            }

            let is_last = i + 1 >= self.elements.len();
            if let Some(comma) = comma {
                let (comma_leading, comma_trailing) =
                    printer.trivia.get_for_range_split(comma.span());
                printer.print_trivia_squished(el_trailing); // always squished before the comma
                printer.print_trivia_squished(comma_leading); // always squished before the comma
                if !is_last {
                    printer.print_str(", ");
                }
                printer.try_print_trivia_single_line_squished(comma_trailing)?;
            } else {
                printer.try_print_trivia_single_line_squished(el_trailing)?; // if multilined would go after the added comma and thus would not be squished
                if !is_last {
                    printer.print_str(", ");
                }
            }
        }

        let (close_leading, _) = printer
            .trivia
            .get_for_range_split(self.close_bracket.span());
        printer.try_print_trivia_single_line_squished(close_leading)?;
        printer.print_raw_token(&self.close_bracket);

        if printer.output.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
    }
}

impl Printable for ArrayInitializer {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
    fn leftmost_token(&self) -> TextRange {
        self.open_bracket.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.close_bracket.span()
    }
}

/// Corresponds to a [`SyntaxKind::OBJECT_LITERAL`] node.
#[derive(Debug)]
pub struct ObjectInitializer {
    pub name: PathExpr,
    pub open_brace: t::LBrace,
    pub fields: Vec<(ObjectField, Option<t::Comma>)>,
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
}

impl KnownKind for ObjectInitializer {
    fn kind() -> SyntaxKind {
        SyntaxKind::OBJECT_LITERAL
    }
}

impl PrintMultiLine for ObjectInitializer {
    /// Multi-line layout: each field on its own indented line with trailing comma.
    /// Closing brace on its own line.
    ///
    ///
    /// ```baml
    /// Name {
    ///     field1: value1,
    ///     field2: value2,
    /// }
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_shape = Shape {
            width: shape.width.saturating_sub(printer.config.indent_width),
            indent: shape.indent + printer.config.indent_width,
            first_line_offset: 0,
        };

        printer.print(&self.name, Shape::unlimited_single_line());
        printer.print_str(" ");
        printer.print_raw_token(&self.open_brace);
        printer.print_trivia_all_trailing_for(self.open_brace.span());
        printer.print_newline();

        for (field, comma) in &self.fields {
            printer.print_trivia_all_leading_with_newline_for(
                field.leftmost_token(),
                inner_shape.indent,
            );
            printer.print_spaces(inner_shape.indent);
            printer.print(field, inner_shape.clone());
            if let Some(comma) = comma {
                printer.print_raw_token(comma);
                printer.print_trivia_all_trailing_for(comma.span());
            } else {
                printer.print_str(",");
                printer.print_trivia_all_trailing_for(field.rightmost_token());
            }
            printer.print_newline();
        }

        printer.print_spaces(shape.indent);
        printer.print_trivia_all_leading_with_newline_for(self.close_brace.span(), shape.indent);
        printer.print_raw_token(&self.close_brace);
        PrintInfo::default_multi_lined()
    }
}

impl ObjectInitializer {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        // Name { field1: v1, field2: v2 }
        let mut len = self.name.single_line_width(input)? + const { " {  }".len() };
        let (_, open_trailing) = input.trivia.get_for_range_split(self.open_brace.span());
        len += open_trailing.try_squished_len(input.input)?;
        for (i, (field, comma)) in self.fields.iter().enumerate() {
            let (fld_leading, fld_trailing) = input.trivia.get_for_element(field);
            len += fld_leading.try_squished_len(input.input)?;
            len += field.single_line_width(input)?;
            len += fld_trailing.try_squished_len(input.input)?;
            if i + 1 < self.fields.len() {
                if let Some(comma) = comma {
                    let (comma_leading, comma_trailing) =
                        input.trivia.get_for_range_split(comma.span());
                    len += comma_leading.try_squished_len(input.input)?;
                    len += 1; // ","
                    len += comma_trailing.try_squished_len(input.input)?;
                } else {
                    len += 1; // ","
                }
                len += 1; // " "
            } else if let Some(comma) = comma {
                // Trailing comma is removed in single-line mode, but check trivia
                let (comma_leading, comma_trailing) =
                    input.trivia.get_for_range_split(comma.span());
                len += comma_leading.try_squished_len(input.input)?;
                len += comma_trailing.try_squished_len(input.input)?;
            }
        }
        let (close_leading, _) = input.trivia.get_for_range_split(self.close_brace.span());
        len += close_leading.try_squished_len(input.input)?;
        Some(len)
    }

    /// Tries to print the object initializer as a single line.
    ///
    /// If successful, returns the info.
    ///
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the object initializer on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        printer.print(&self.name, Shape::unlimited_single_line());
        printer.print_str(" ");
        printer.print_raw_token(&self.open_brace);
        printer.print_str(" ");
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_brace.span());
        printer.try_print_trivia_single_line_squished(open_trailing)?;

        for (i, (field, comma)) in self.fields.iter().enumerate() {
            if printer.output.len() > shape.width {
                return None;
            }
            let (fld_leading, fld_trailing) = printer.trivia.get_for_element(field);
            printer.try_print_trivia_single_line_squished(fld_leading)?;
            if printer
                .print(field, Shape::unlimited_single_line())
                .multi_lined
            {
                return None;
            }
            printer.try_print_trivia_single_line_squished(fld_trailing)?;
            if i + 1 < self.fields.len() {
                if let Some(comma) = comma {
                    let (comma_leading, comma_trailing) =
                        printer.trivia.get_for_range_split(comma.span());
                    printer.try_print_trivia_single_line_squished(comma_leading)?;
                    printer.print_raw_token(comma);
                    printer.try_print_trivia_single_line_squished(comma_trailing)?;
                } else {
                    printer.print_str(",");
                }
                printer.print_str(" ");
            } else if let Some(comma) = comma {
                // Trailing comma is removed in single-line mode, but we still try the comments.
                let (comma_leading, comma_trailing) =
                    printer.trivia.get_for_range_split(comma.span());
                printer.try_print_trivia_single_line_squished(comma_leading)?;
                printer.try_print_trivia_single_line_squished(comma_trailing)?;
            }
        }
        let (close_leading, _) = printer.trivia.get_for_range_split(self.close_brace.span());
        printer.try_print_trivia_single_line_squished(close_leading)?;
        printer.print_str(" ");
        printer.print_raw_token(&self.close_brace);

        if printer.output.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
    }
}

impl Printable for ObjectInitializer {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
    fn leftmost_token(&self) -> TextRange {
        self.name.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.close_brace.span()
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

impl PrintMultiLine for MapLiteral {
    /// Multi-line layout: each entry on its own indented line with trailing comma.
    /// Closing brace on its own line.
    ///
    /// ```baml
    /// {
    ///     key1: value1,
    ///     key2: value2,
    /// }
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_shape = Shape {
            width: shape.width.saturating_sub(printer.config.indent_width),
            indent: shape.indent + printer.config.indent_width,
            first_line_offset: 0,
        };

        printer.print_raw_token(&self.open_brace);
        printer.print_trivia_all_trailing_for(self.open_brace.span());
        printer.print_newline();

        for (field, comma) in &self.fields {
            printer.print_trivia_all_leading_with_newline_for(
                field.leftmost_token(),
                inner_shape.indent,
            );
            printer.print_spaces(inner_shape.indent);
            printer.print(field, inner_shape.clone());
            if let Some(comma) = comma {
                printer.print_raw_token(comma);
                printer.print_trivia_all_trailing_for(comma.span());
            } else {
                printer.print_str(",");
                printer.print_trivia_all_trailing_for(field.rightmost_token());
            }
            printer.print_newline();
        }

        printer
            .print_trivia_all_leading_with_newline_for(self.close_brace.span(), inner_shape.indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_brace);
        PrintInfo::default_multi_lined()
    }
}

impl MapLiteral {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        // { k1: v1, k2: v2 }
        let mut len = const { "{  }".len() };
        let (_, open_trailing) = input.trivia.get_for_range_split(self.open_brace.span());
        for t in open_trailing {
            len += t.single_line_len(input.input)?;
        }
        for (i, (field, comma)) in self.fields.iter().enumerate() {
            let (fld_leading, fld_trailing) = input.trivia.get_for_element(field);
            for t in fld_leading {
                len += t.single_line_len(input.input)?;
            }
            len += field.single_line_width(input)?;
            for t in fld_trailing {
                len += t.single_line_len(input.input)?;
            }
            if i + 1 < self.fields.len() {
                if let Some(comma) = comma {
                    let (comma_leading, comma_trailing) =
                        input.trivia.get_for_range_split(comma.span());
                    for t in comma_leading {
                        len += t.single_line_len(input.input)?;
                    }
                    len += 1; // ","
                    for t in comma_trailing {
                        len += t.single_line_len(input.input)?;
                    }
                } else {
                    len += 1; // ","
                }
                len += 1; // " "
            } else if let Some(comma) = comma {
                // Trailing comma is removed in single-line mode, but check trivia
                let (comma_leading, comma_trailing) =
                    input.trivia.get_for_range_split(comma.span());
                for t in comma_leading {
                    len += t.single_line_len(input.input)?;
                }
                for t in comma_trailing {
                    len += t.single_line_len(input.input)?;
                }
            }
        }
        let (close_leading, _) = input.trivia.get_for_range_split(self.close_brace.span());
        for t in close_leading {
            len += t.single_line_len(input.input)?;
        }
        Some(len)
    }

    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the map literal on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        printer.print_raw_token(&self.open_brace);
        printer.print_str(" ");
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_brace.span());
        printer.try_print_trivia_single_line_squished(open_trailing)?;

        for (i, (field, comma)) in self.fields.iter().enumerate() {
            if printer.output.len() > shape.width {
                return None;
            }
            let (fld_leading, fld_trailing) = printer.trivia.get_for_element(field);
            printer.try_print_trivia_single_line_squished(fld_leading)?;
            if printer
                .print(field, Shape::unlimited_single_line())
                .multi_lined
            {
                return None;
            }
            printer.try_print_trivia_single_line_squished(fld_trailing)?;
            if i + 1 < self.fields.len() {
                if let Some(comma) = comma {
                    let (comma_leading, comma_trailing) =
                        printer.trivia.get_for_range_split(comma.span());
                    printer.try_print_trivia_single_line_squished(comma_leading)?;
                    printer.print_raw_token(comma);
                    printer.try_print_trivia_single_line_squished(comma_trailing)?;
                } else {
                    printer.print_str(",");
                }
                printer.print_str(" ");
            } else if let Some(comma) = comma {
                // Trailing comma is removed in single-line mode, but we still try the comments.
                let (comma_leading, comma_trailing) =
                    printer.trivia.get_for_range_split(comma.span());
                printer.try_print_trivia_single_line_squished(comma_leading)?;
                printer.try_print_trivia_single_line_squished(comma_trailing)?;
            }
        }
        let (close_leading, _) = printer.trivia.get_for_range_split(self.close_brace.span());
        printer.try_print_trivia_single_line_squished(close_leading)?;
        printer.print_str(" ");
        printer.print_raw_token(&self.close_brace);

        if printer.output.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
    }
}

impl Printable for MapLiteral {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
    fn leftmost_token(&self) -> TextRange {
        self.open_brace.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.close_brace.span()
    }
}

/// Corresponds to a [`SyntaxKind::OBJECT_FIELD`] node.
#[derive(Debug)]
pub struct ObjectField {
    pub name: ObjectFieldKey,
    pub colon: t::Colon,
    pub value: Expression,
}

impl FromCST for ObjectField {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::OBJECT_FIELD)?;

        let mut it = SyntaxNodeIter::new(&node);

        let name = it.expect_next("WORD or STRING_LITERAL")?;
        let name = ObjectFieldKey::from_cst(name)?;

        let colon = it.expect_parse()?;

        let value = it.expect_next("value")?;
        let value = Expression::from_cst(value)?;

        it.expect_end()?;

        Ok(ObjectField { name, colon, value })
    }
}

impl KnownKind for ObjectField {
    fn kind() -> SyntaxKind {
        SyntaxKind::OBJECT_FIELD
    }
}

impl ObjectField {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let name = self.name.single_line_width(input)?;
        let value = self.value.single_line_width(input)?;
        // Must match trivia handled by print: colon_trailing + value_leading
        let mut trivia_len = 0usize;
        let (_, colon_trailing) = input.trivia.get_for_range_split(self.colon.span());
        for t in colon_trailing {
            trivia_len += t.single_line_len(input.input)?;
        }
        let value_leading = input.trivia.get_leading_for_element(&self.value);
        for t in value_leading {
            trivia_len += t.single_line_len(input.input)?;
        }
        Some(name + const { ": ".len() } + value + trivia_len)
    }
}

impl Printable for ObjectField {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;
        multi_lined |= printer.print(&self.name, shape.clone()).multi_lined;
        printer.print_raw_token(&self.colon);
        let (_, colon_trailing) = printer.trivia.get_for_range_split(self.colon.span());
        printer.print_str(" ");
        printer.print_trivia_squished(colon_trailing);
        let value_leading = printer.trivia.get_leading_for_element(&self.value);
        printer.print_trivia_squished(value_leading);
        multi_lined |= printer.print(&self.value, shape).multi_lined;
        PrintInfo { multi_lined }
    }
    fn leftmost_token(&self) -> TextRange {
        self.name.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.value.rightmost_token()
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

impl ObjectFieldKey {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        match self {
            ObjectFieldKey::Word(word) => Some(usize::from(word.span().len())),
            ObjectFieldKey::String(s) => {
                if input.input[s.span()].contains('\n') {
                    None
                } else {
                    Some(usize::from(s.span().len()))
                }
            }
        }
    }
}

impl Printable for ObjectFieldKey {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            ObjectFieldKey::Word(word) => {
                printer.print_raw_token(word);
                PrintInfo::default_single_line()
            }
            ObjectFieldKey::String(string) => printer.print(string, shape),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            ObjectFieldKey::Word(word) => word.span(),
            ObjectFieldKey::String(string) => string.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            ObjectFieldKey::Word(word) => word.span(),
            ObjectFieldKey::String(string) => string.rightmost_token(),
        }
    }
}

// ─── Lambda Expression ────────────────────────────────────────────────────────

/// Corresponds to a [`SyntaxKind::GENERIC_PARAM_LIST`] node.
///
/// Contains `<T, U>` generic parameter declarations for a lambda expression.
/// Printed as `<T>` or `<K, V>` etc.
#[derive(Debug)]
pub struct GenericParamList {
    pub open_angle: t::Less,
    /// Comma-separated type parameter names: `(Word, Comma?)` pairs.
    pub params: Vec<(t::Word, Option<t::Comma>)>,
    pub close_angle: t::Greater,
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
                    // GENERIC_PARAM contains a single WORD
                    let param_node = StrongAstError::assert_is_node(elem)?;
                    let mut param_it = SyntaxNodeIter::new(&param_node);
                    let word: t::Word = param_it.expect_parse()?;
                    let comma = it
                        .next_if_kind(SyntaxKind::COMMA)
                        .map(t::Comma::from_cst)
                        .transpose()?;
                    params.push((word, comma));
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

impl KnownKind for GenericParamList {
    fn kind() -> SyntaxKind {
        SyntaxKind::GENERIC_PARAM_LIST
    }
}

impl Printable for GenericParamList {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.open_angle);
        for (i, (word, _comma)) in self.params.iter().enumerate() {
            printer.print_raw_token(word);
            if i + 1 < self.params.len() {
                printer.print_str(", ");
            }
        }
        printer.print_raw_token(&self.close_angle);
        PrintInfo::default_single_line()
    }
    fn leftmost_token(&self) -> TextRange {
        self.open_angle.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.close_angle.span()
    }
}

/// Corresponds to a [`SyntaxKind::GENERIC_ARGS`] node.
///
/// Contains `<Type1, Type2, ...>` generic arguments at a call site
/// or generic-typed path (e.g. `f<int, string>(...)`, `Box<int> { ... }`).
#[derive(Debug)]
pub struct GenericArgs {
    pub open_angle: t::Less,
    /// Comma-separated type arguments.
    pub args: Vec<(crate::ast::Type, Option<t::Comma>)>,
    pub close_angle: t::Greater,
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
                    let ty = crate::ast::Type::from_cst(elem)?;
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
}

impl GenericArgs {
    /// Width that the formatter would emit on a single line, ignoring any
    /// internal trivia in the source. Used by single-line-width estimators
    /// upstream to decide whether a host expression fits on one line.
    ///
    /// Format is `<T1, T2, T3>`: 2 chars for `<>`, plus each type argument's
    /// source-text width, plus `, ` (2 chars) between arguments. Source
    /// types may contain whitespace, but for typical cases this is a tight
    /// upper bound and tracks what the printer actually emits.
    pub(crate) fn formatted_single_line_width(&self) -> usize {
        let mut len: usize = 2; // `<` and `>`
        for (i, (ty, _)) in self.args.iter().enumerate() {
            let arg_span = ty.rightmost_token().end() - ty.leftmost_token().start();
            len += usize::from(arg_span);
            if i + 1 < self.args.len() {
                len += 2; // `, `
            }
        }
        len
    }
}

impl KnownKind for GenericArgs {
    fn kind() -> SyntaxKind {
        SyntaxKind::GENERIC_ARGS
    }
}

impl Printable for GenericArgs {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.open_angle);
        for (i, (ty, _comma)) in self.args.iter().enumerate() {
            printer.print(ty, shape.clone());
            if i + 1 < self.args.len() {
                printer.print_str(", ");
            }
        }
        printer.print_raw_token(&self.close_angle);
        PrintInfo::default_single_line()
    }
    fn leftmost_token(&self) -> TextRange {
        self.open_angle.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.close_angle.span()
    }
}

/// Corresponds to a [`SyntaxKind::THROWS_CLAUSE`] node.
///
/// Contains `throws <type>`.
#[derive(Debug)]
pub struct ThrowsClause {
    pub keyword: t::Throws,
    pub ty: crate::ast::Type,
}

impl FromCST for ThrowsClause {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::THROWS_CLAUSE)?;

        let mut it = SyntaxNodeIter::new(&node);
        let keyword: t::Throws = it.expect_parse()?;
        let ty: crate::ast::Type = it.expect_parse()?;
        it.expect_end()?;

        Ok(ThrowsClause { keyword, ty })
    }
}

impl KnownKind for ThrowsClause {
    fn kind() -> SyntaxKind {
        SyntaxKind::THROWS_CLAUSE
    }
}

impl Printable for ThrowsClause {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;
        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        multi_lined |= printer.print(&self.ty, shape).multi_lined;
        PrintInfo { multi_lined }
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.ty.rightmost_token()
    }
}

/// Arrow token in a lambda expression. Accepts either `->` (canonical) or
/// `=>` (accepted permissively for ergonomic parity with JS/TS arrow functions);
/// the formatter always emits `->`.
#[derive(Debug)]
pub enum LambdaArrow {
    Arrow(t::Arrow),
    FatArrow(t::FatArrow),
}

impl LambdaArrow {
    #[must_use]
    pub fn span(&self) -> TextRange {
        match self {
            LambdaArrow::Arrow(t) => t.span(),
            LambdaArrow::FatArrow(t) => t.span(),
        }
    }

    /// Returns true if the source used `=>` instead of the canonical `->`.
    #[must_use]
    pub fn is_fat_arrow(&self) -> bool {
        matches!(self, LambdaArrow::FatArrow(_))
    }
}

impl FromCST for LambdaArrow {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
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
}

impl KnownKind for LambdaArrow {
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
    pub arrow: LambdaArrow,
    pub return_type: Option<crate::ast::Type>,
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
        let arrow: LambdaArrow = it.expect_parse()?;

        // Optional return type: TYPE_EXPR before THROWS_CLAUSE or BLOCK_EXPR
        let return_type = if it.peek().map(|e| e.kind()) == Some(SyntaxKind::TYPE_EXPR) {
            let elem = it.next().expect("peeked");
            Some(crate::ast::Type::from_cst(elem)?)
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

impl Printable for LambdaExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        // Optional generic params: <T>
        if let Some(ref gp) = self.generic_params {
            printer.print(gp, shape.clone());
        }

        // Parameter list
        printer.print(&self.param_list, shape.clone());

        // Space + arrow (always normalize to canonical `->`)
        printer.print_str(" ->");

        // Optional return type
        if let Some(ref ret) = self.return_type {
            printer.print_str(" ");
            printer.print(ret, shape.clone());
        }

        // Optional throws clause
        if let Some(ref throws) = self.throws {
            printer.print_str(" ");
            printer.print(throws, shape.clone());
        }

        // Space + block
        printer.print_str(" ");
        printer.print(&self.block, shape);

        PrintInfo::default_multi_lined()
    }
    fn leftmost_token(&self) -> TextRange {
        if let Some(ref gp) = self.generic_params {
            gp.leftmost_token()
        } else {
            self.param_list.leftmost_token()
        }
    }
    fn rightmost_token(&self) -> TextRange {
        self.block.rightmost_token()
    }
}

// ─── PrintChain ───────────────────────────────────────────────────────────────

/// Only used for printing chained expressions.
///
/// Needed to re-organize before printing from a hierarchical structure to a flat-ish one.
pub struct PrintChain<'a> {
    /// May be a [`PathExpr`] in which case only the first item is used (the rest are included in [`PrintChain::chain_members`]).
    first: &'a Expression,
    /// Will always start with a field access (if not empty), since calls/indexes will be included in `first` if not following a field access.
    chain_members: Vec<PrintChainItem<'a>>,
}
impl<'a> PrintChain<'a> {
    #[must_use]
    pub fn new(from: &'a Expression) -> Self {
        match from {
            Expression::Path(path_expr) => {
                let mut chain_members: Vec<PrintChainItem<'a>> = path_expr
                    .rest
                    .iter()
                    .map(|(dot, word)| PrintChainItem::FieldAccess(dot, word))
                    .collect();
                if let Some(ref ga) = path_expr.generic_args {
                    chain_members.push(PrintChainItem::GenericArgs(ga));
                }
                Self {
                    first: from,
                    chain_members,
                }
            }
            Expression::Call(call_expr) => {
                let mut chain = Self::new(&call_expr.callee);
                if chain.chain_members.is_empty() {
                    // included in `first` if not following a field access
                    Self {
                        first: from,
                        chain_members: Vec::new(),
                    }
                } else {
                    chain
                        .chain_members
                        .push(PrintChainItem::Call(&call_expr.args));
                    chain
                }
            }
            Expression::Index(index_expr) => {
                let mut chain = Self::new(&index_expr.base);
                if chain.chain_members.is_empty() {
                    // included in `first` if not following a field access
                    Self {
                        first: from,
                        chain_members: Vec::new(),
                    }
                } else {
                    chain
                        .chain_members
                        .push(PrintChainItem::Index(index_expr.args()));
                    chain
                }
            }
            Expression::FieldAccess(field_access_expr) => {
                let mut chain = Self::new(&field_access_expr.base);
                chain.chain_members.push(PrintChainItem::FieldAccess(
                    &field_access_expr.dot,
                    &field_access_expr.field,
                ));
                chain
            }
            Expression::OptionalFieldAccess(ofa) => {
                let mut chain = Self::new(&ofa.base);
                chain
                    .chain_members
                    .push(PrintChainItem::OptionalFieldAccess(
                        &ofa.question_dot,
                        &ofa.field,
                    ));
                chain
            }
            Expression::OptionalIndex(oi) => {
                let mut chain = Self::new(&oi.base);
                chain
                    .chain_members
                    .push(PrintChainItem::OptionalIndex(&oi.question_dot, oi.args()));
                chain
            }
            Expression::OptionalCall(oc) => {
                let mut chain = Self::new(&oc.callee);
                chain
                    .chain_members
                    .push(PrintChainItem::OptionalCall(&oc.question_dot, &oc.args));
                chain
            }
            base => Self {
                first: base,
                chain_members: Vec::new(),
            },
        }
    }
}

impl PrintMultiLine for PrintChain<'_> {
    /// Prints the chained expression, with each field member on a new line.
    ///
    /// Uses similar rules to rustfmt
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let first_single_line = match self.first {
            Expression::Path(path_expr) => {
                printer.print_raw_token(&path_expr.first);
                true
            }
            Expression::Call(call_expr) => {
                let first_info = printer.print(call_expr, shape.clone());
                !first_info.multi_lined
            }
            Expression::Index(index_expr) => {
                let first_info = printer.print(index_expr, shape.clone());
                !first_info.multi_lined
            }
            _ => {
                let first_info = printer.print(self.first, shape.clone());
                !first_info.multi_lined
            }
        };

        let offset = printer.current_line_len().saturating_sub(shape.indent);
        // Only indent the chain when it actually has somewhere to break across
        // lines — i.e. it has multiple field-access steps, or the first chunk
        // already pushed past one indent. Single-call chains like
        // `f(longarg)` or `obj.method(longarg)` should let the trailing
        // `(args)` wrap at the *outer* indent rather than chain_indent + 4.
        let field_access_steps = self
            .chain_members
            .iter()
            .filter(|m| {
                matches!(
                    m,
                    PrintChainItem::FieldAccess(..) | PrintChainItem::OptionalFieldAccess(..)
                )
            })
            .count();
        let should_indent_chain =
            (first_single_line && field_access_steps > 1) || offset > printer.config.indent_width;
        let chain_indent = if should_indent_chain {
            shape.indent + printer.config.indent_width
        } else {
            shape.indent
        };

        let mut line_remaining_width = printer.current_line_remaining_width();
        let mut it = self.chain_members.iter();
        if first_single_line && offset <= printer.config.indent_width {
            // Try to print the second item on the same line if it's a field access and fits.
            let peeked = it.next();
            let second_len = match peeked {
                Some(&PrintChainItem::FieldAccess(dot, word)) => {
                    Some(usize::from(dot.span().len() + word.span().len()))
                }
                Some(&PrintChainItem::OptionalFieldAccess(qd, word)) => {
                    Some(usize::from(qd.span().len() + word.span().len()))
                }
                _ => None,
            };
            if let Some(second_len) = second_len {
                let item = peeked.unwrap();
                if line_remaining_width >= second_len {
                    Self::print_field_access_item(item, printer);
                    line_remaining_width = line_remaining_width.saturating_sub(second_len);
                } else {
                    printer.print_newline();
                    printer.print_spaces(chain_indent);
                    Self::print_field_access_item(item, printer);
                    line_remaining_width = printer
                        .config
                        .line_width
                        .saturating_sub(chain_indent + second_len);
                }
            } else if let Some(item) = peeked {
                Self::print_non_field_item(item, chain_indent, &mut line_remaining_width, printer);
            }
        }
        for item in it {
            match item {
                &PrintChainItem::FieldAccess(_, _) | &PrintChainItem::OptionalFieldAccess(_, _) => {
                    printer.print_newline();
                    printer.print_spaces(chain_indent);
                    Self::print_field_access_item(item, printer);
                    let item_len = match *item {
                        PrintChainItem::FieldAccess(dot, word) => {
                            usize::from(dot.span().len() + word.span().len())
                        }
                        PrintChainItem::OptionalFieldAccess(qd, word) => {
                            usize::from(qd.span().len() + word.span().len())
                        }
                        _ => unreachable!(),
                    };
                    line_remaining_width = printer
                        .config
                        .line_width
                        .saturating_sub(chain_indent + item_len);
                }
                _ => {
                    Self::print_non_field_item(
                        item,
                        chain_indent,
                        &mut line_remaining_width,
                        printer,
                    );
                }
            }
        }

        PrintInfo::default_multi_lined()
    }
}

impl PrintChain<'_> {
    fn print_field_access_item(item: &PrintChainItem<'_>, printer: &mut Printer) {
        match *item {
            PrintChainItem::FieldAccess(dot, word) => {
                printer.print_raw_token(dot);
                printer.print_raw_token(word);
            }
            PrintChainItem::OptionalFieldAccess(qd, word) => {
                printer.print_raw_token(qd);
                printer.print_raw_token(word);
            }
            _ => unreachable!("print_field_access_item called with non-field-access item"),
        }
    }

    fn print_non_field_item(
        item: &PrintChainItem<'_>,
        chain_indent: usize,
        line_remaining_width: &mut usize,
        printer: &mut Printer,
    ) {
        match item {
            PrintChainItem::Index(index_args) => {
                let index_shape = Shape {
                    width: *line_remaining_width,
                    indent: chain_indent,
                    first_line_offset: printer.current_line_len().saturating_sub(chain_indent),
                };
                printer.print(index_args, index_shape);
                *line_remaining_width = printer.current_line_remaining_width();
            }
            PrintChainItem::OptionalIndex(qd, index_args) => {
                printer.print_raw_token(*qd);
                let index_shape = Shape {
                    width: *line_remaining_width,
                    indent: chain_indent,
                    first_line_offset: printer.current_line_len().saturating_sub(chain_indent),
                };
                printer.print(index_args, index_shape);
                *line_remaining_width = printer.current_line_remaining_width();
            }
            &PrintChainItem::Call(call_args) => {
                let call_shape = Shape {
                    width: *line_remaining_width,
                    indent: chain_indent,
                    first_line_offset: printer.current_line_len().saturating_sub(chain_indent),
                };
                printer.print(call_args, call_shape);
                *line_remaining_width = printer.current_line_remaining_width();
            }
            &PrintChainItem::OptionalCall(qd, call_args) => {
                printer.print_raw_token(qd);
                let call_shape = Shape {
                    width: *line_remaining_width,
                    indent: chain_indent,
                    first_line_offset: printer.current_line_len().saturating_sub(chain_indent),
                };
                printer.print(call_args, call_shape);
                *line_remaining_width = printer.current_line_remaining_width();
            }
            &PrintChainItem::GenericArgs(generic_args) => {
                let ga_shape = Shape {
                    width: *line_remaining_width,
                    indent: chain_indent,
                    first_line_offset: printer.current_line_len().saturating_sub(chain_indent),
                };
                printer.print(generic_args, ga_shape);
                *line_remaining_width = printer.current_line_remaining_width();
            }
            _ => unreachable!("print_non_field_item called with field-access item"),
        }
    }

    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        match self.first {
            Expression::Path(path_expr) => {
                printer.print_raw_token(&path_expr.first);
            }
            Expression::FieldAccess(..)
            | Expression::OptionalFieldAccess(..)
            | Expression::OptionalIndex(..)
            | Expression::OptionalCall(..) => {
                unreachable!("Should have been unwrapped when the PrintChain was created")
            }
            Expression::Call(call_expr) => {
                if printer
                    .print(call_expr, Shape::unlimited_single_line())
                    .multi_lined
                {
                    return None;
                }
            }
            Expression::Index(index_expr) => {
                if printer
                    .print(index_expr, Shape::unlimited_single_line())
                    .multi_lined
                {
                    return None;
                }
            }
            _ => {
                if self.first.single_line_width(printer)? > shape.width {
                    return None;
                }
                if printer
                    .print(self.first, Shape::unlimited_single_line())
                    .multi_lined
                {
                    return None;
                }
            }
        }
        for item in &self.chain_members {
            if printer.output.len() > shape.width {
                return None;
            }
            match item {
                &PrintChainItem::FieldAccess(dot, word) => {
                    printer.print_raw_token(dot);
                    printer.print_raw_token(word);
                }
                &PrintChainItem::OptionalFieldAccess(qd, word) => {
                    printer.print_raw_token(qd);
                    printer.print_raw_token(word);
                }
                PrintChainItem::Index(index_args) => {
                    index_args.try_print_single_line(shape, printer)?;
                }
                PrintChainItem::OptionalIndex(qd, index_args) => {
                    printer.print_raw_token(*qd);
                    index_args.try_print_single_line(shape, printer)?;
                }
                &PrintChainItem::Call(call_args) => {
                    call_args.try_print_single_line(shape, printer)?;
                }
                &PrintChainItem::OptionalCall(qd, call_args) => {
                    printer.print_raw_token(qd);
                    call_args.try_print_single_line(shape, printer)?;
                }
                &PrintChainItem::GenericArgs(generic_args) => {
                    printer.print(generic_args, Shape::unlimited_single_line());
                }
            }
        }
        if printer.output.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
    }
}

impl Printable for PrintChain<'_> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
    fn leftmost_token(&self) -> TextRange {
        self.first.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        match self.chain_members.last() {
            Some(
                PrintChainItem::FieldAccess(_, word) | PrintChainItem::OptionalFieldAccess(_, word),
            ) => word.span(),
            Some(
                PrintChainItem::Index(index_args) | PrintChainItem::OptionalIndex(_, index_args),
            ) => index_args.close_bracket.span(),
            Some(PrintChainItem::Call(call_args) | PrintChainItem::OptionalCall(_, call_args)) => {
                call_args.rightmost_token()
            }
            Some(PrintChainItem::GenericArgs(ga)) => ga.close_angle.span(),
            None => self.first.rightmost_token(),
        }
    }
}

/// Only used for printing chained expressions. See [`PrintChain`].
enum PrintChainItem<'a> {
    FieldAccess(&'a t::Dot, &'a t::Word),
    OptionalFieldAccess(&'a t::QuestionDot, &'a t::Word),
    Index(IndexArgs<'a>),
    OptionalIndex(&'a t::QuestionDot, IndexArgs<'a>),
    Call(&'a CallArgs),
    OptionalCall(&'a t::QuestionDot, &'a CallArgs),
    GenericArgs(&'a GenericArgs),
}
