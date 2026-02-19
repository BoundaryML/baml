//! Reference: [`baml_db::baml_compiler_syntax::ast::Expr`] and [`baml_db::baml_compiler_hir::body`]

use baml_db::baml_compiler_syntax::{SyntaxElement, SyntaxKind};
use rowan::TextRange;

use crate::{
    ast::{
        BinaryOp, FromCST, KnownKind, MatchPattern, Statement, StrongAstError, SyntaxNodeIter,
        Token, UnaryOp, tokens as t,
    },
    printer::{PrintInfo, PrintMultiLine, Printable, Printer, Shape},
};

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
    EnvAccess(EnvAccessExpr),
    Block(BlockExpr),
    ArrayInitializer(ArrayInitializer),
    MapInitializer(MapLiteral),
    ObjectInitializer(ObjectInitializer),
    RawString(t::RawString),
    Unknown(TextRange),
}

impl Expression {
    #[must_use]
    pub const fn statement_needs_semicolon(&self) -> bool {
        !matches!(
            self,
            Expression::If(_) | Expression::Match(_) | Expression::Unknown(_)
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
            SyntaxKind::UNARY_EXPR => UnaryExpr::from_cst(elem).map(Expression::Unary)?,
            SyntaxKind::IF_EXPR => IfExpr::from_cst(elem).map(Expression::If)?,
            SyntaxKind::MATCH_EXPR => MatchExpr::from_cst(elem).map(Expression::Match)?,
            SyntaxKind::CALL_EXPR => CallExpr::from_cst(elem).map(Expression::Call)?,
            SyntaxKind::INDEX_EXPR => IndexExpr::from_cst(elem).map(Expression::Index)?,
            SyntaxKind::FIELD_ACCESS_EXPR => {
                FieldAccessExpr::from_cst(elem).map(Expression::FieldAccess)?
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
            _ => Expression::Unknown(elem.text_range()),
        };
        Ok(expr)
    }
}

impl Printable for Expression {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            Expression::Literal(lit) => lit.print(shape, printer),
            chain @ (Expression::Path(_)
            | Expression::Call(_)
            | Expression::Index(_)
            | Expression::FieldAccess(_)) => {
                // These are all chains of postfix expressions
                let chain = PrintChain::new(chain);
                chain.print(shape, printer)
            }
            Expression::Paren(paren) => paren.print(shape, printer),
            Expression::Binary(binary) => binary.print(shape, printer),
            Expression::Unary(unary) => unary.print(shape, printer),
            Expression::If(if_expr) => if_expr.print(shape, printer),
            Expression::Match(match_expr) => match_expr.print(shape, printer),
            Expression::EnvAccess(env) => env.print(shape, printer),
            Expression::Block(block) => block.print(shape, printer),
            Expression::ArrayInitializer(array) => array.print(shape, printer),
            Expression::MapInitializer(map) => map.print(shape, printer),
            Expression::ObjectInitializer(obj) => obj.print(shape, printer),
            Expression::RawString(raw) => raw.print(shape, printer),
            Expression::Unknown(range) => {
                printer.print_input_range(*range);
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
            Expression::Unary(unary) => unary.leftmost_token(),
            Expression::If(if_expr) => if_expr.leftmost_token(),
            Expression::Match(match_expr) => match_expr.leftmost_token(),
            Expression::Call(call) => call.leftmost_token(),
            Expression::Index(index) => index.leftmost_token(),
            Expression::FieldAccess(fa) => fa.base.leftmost_token(),
            Expression::EnvAccess(env) => env.leftmost_token(),
            Expression::Block(block) => block.leftmost_token(),
            Expression::ArrayInitializer(array) => array.leftmost_token(),
            Expression::MapInitializer(map) => map.leftmost_token(),
            Expression::ObjectInitializer(obj) => obj.leftmost_token(),
            Expression::RawString(raw) => raw.leftmost_token(),
            Expression::Unknown(range) => *range,
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            Expression::Literal(lit) => lit.rightmost_token(),
            Expression::Path(path) => path.rightmost_token(),
            Expression::Paren(paren) => paren.rightmost_token(),
            Expression::Binary(binary) => binary.rightmost_token(),
            Expression::Unary(unary) => unary.rightmost_token(),
            Expression::If(if_expr) => if_expr.rightmost_token(),
            Expression::Match(match_expr) => match_expr.rightmost_token(),
            Expression::Call(call) => call.rightmost_token(),
            Expression::Index(index) => index.rightmost_token(),
            Expression::FieldAccess(fa) => fa.field.span(),
            Expression::EnvAccess(env) => env.rightmost_token(),
            Expression::Block(block) => block.rightmost_token(),
            Expression::ArrayInitializer(array) => array.rightmost_token(),
            Expression::MapInitializer(map) => map.rightmost_token(),
            Expression::ObjectInitializer(obj) => obj.rightmost_token(),
            Expression::RawString(raw) => raw.rightmost_token(),
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
}

impl FromCST for PathExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        if elem.kind() == SyntaxKind::WORD {
            let first = t::Word::from_cst(elem)?;
            return Ok(PathExpr {
                first,
                rest: Vec::new(),
            });
        }
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::PATH_EXPR)?;

        let mut it = SyntaxNodeIter::new(&node);

        // First WORD
        let first = it.expect_parse()?;

        let mut rest = Vec::new();

        // Collect DOT WORD pairs
        while let Some(elem) = it.next() {
            if elem.kind() == SyntaxKind::DOT {
                let dot = t::Dot::from_cst(elem)?;
                let word = it.expect_parse()?;

                rest.push((dot, word));
            } else {
                return Err(StrongAstError::UnexpectedAdditionalElement {
                    parent: it.parent,
                    at: elem.text_range(),
                });
            }
        }

        Ok(PathExpr { first, rest })
    }
}

impl KnownKind for PathExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::PATH_EXPR
    }
}

impl Printable for PathExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        if self.rest.is_empty() {
            printer.print_raw_token(&self.first);
            return PrintInfo::default_single_line();
        }
        let first = Expression::Path(PathExpr {
            first: self.first.clone(),
            rest: Vec::new(),
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
        chain.print(shape, printer)
    }
    fn leftmost_token(&self) -> TextRange {
        self.first.span()
    }
    fn rightmost_token(&self) -> TextRange {
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

        printer.print_standalone_with_trivia(&*self.expr, inner_shape.indent);

        printer
            .print_trivia_all_leading_with_newline_for(self.close_paren.span(), inner_shape.indent);
        printer.print_newline();
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_paren);
        PrintInfo::default_multi_lined()
    }
}

impl Printable for ParenExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut inner_printer = Printer::new_empty(printer.input, printer.config, printer.trivia);
        let inner_shape_single_line = Shape {
            width: shape.width.saturating_sub(2),
            indent: shape.indent,
            first_line_offset: shape.first_line_offset + 1,
        };
        let inner_info = inner_printer.print(&*self.expr, inner_shape_single_line);

        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_paren.span());
        let (expr_leading, expr_trailing) = printer.trivia.get_for_element(&*self.expr);
        let (close_leading, _) = printer.trivia.get_for_range_split(self.close_paren.span());
        let single_line_len: usize = open_trailing
            .iter()
            .chain(expr_leading)
            .chain(expr_trailing)
            .chain(close_leading)
            .map(|t| t.single_line_len(printer.input))
            .sum::<Option<usize>>()
            .map_or(usize::MAX, |sum| {
                sum + inner_printer.len() + const { "()".len() }
            });

        if inner_info.multi_lined || single_line_len > shape.width {
            self.print_multi_line(shape, printer)
        } else {
            printer.print_raw_token(&self.open_paren);
            for t in open_trailing {
                printer.print_trivia(t);
            }
            for t in expr_leading {
                if t.is_comment() {
                    printer.print_trivia(t);
                }
            }
            printer.append_from_printer(inner_printer);
            for t in expr_trailing {
                printer.print_trivia(t);
            }
            for t in close_leading {
                if t.is_comment() {
                    printer.print_trivia(t);
                }
            }
            printer.print_raw_token(&self.close_paren);
            PrintInfo::default_single_line()
        }
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

        // Get operator
        let op = it.expect_next("binary operator")?;
        let op = BinaryOp::from_cst(op)?;

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
        for (op, right) in chain_members {
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
            printer.print_trivia_all_trailing_for(right.rightmost_token());
        }
        PrintInfo::default_multi_lined()
    }
}

impl Printable for BinaryExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let (left, right) = &*self.sides;

        // Check trailing trivia on sub-expressions for single-line compatibility
        let (_, left_trailing) = printer.trivia.get_for_range_split(left.rightmost_token());
        let (_, right_trailing) = printer.trivia.get_for_range_split(right.rightmost_token());

        let trivia_single_line_len = left_trailing
            .iter()
            .chain(right_trailing.iter())
            .map(|t| t.single_line_len(printer.input))
            .sum::<Option<usize>>();

        if trivia_single_line_len.is_none() {
            return Self::print_multi_line(self, shape, printer);
        }

        let mut single_line_printer =
            Printer::new_empty(printer.input, printer.config, printer.trivia);
        let mut multi_lined = false;
        multi_lined |= single_line_printer
            .print(left, Shape::unlimited_single_line())
            .multi_lined;
        single_line_printer.print_str(" ");
        multi_lined |= single_line_printer
            .print(&self.op, Shape::unlimited_single_line())
            .multi_lined;
        single_line_printer.print_str(" ");
        multi_lined |= single_line_printer
            .print(right, Shape::unlimited_single_line())
            .multi_lined;

        if multi_lined || single_line_printer.output.len() > shape.width {
            Self::print_multi_line(self, shape, printer)
        } else {
            printer.append_from_printer(single_line_printer);
            PrintInfo::default_single_line()
        }
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
    pub condition: ParenExpr,
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

        // PAREN_EXPR
        let condition: ParenExpr = it.expect_parse()?;

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
        printer.print(&self.condition, shape.clone());
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

impl Printable for MatchExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_indent = shape.indent + printer.config.indent_width;

        // Print "match" keyword
        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");

        // Handle scrutinee with ParenExpr-style trivia
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_paren.span());
        let (scrutinee_leading, _) = printer
            .trivia
            .get_for_range_split(self.scrutinee.leftmost_token());
        let (_, scrutinee_trailing) = printer
            .trivia
            .get_for_range_split(self.scrutinee.rightmost_token());
        let (close_paren_leading, _) = printer.trivia.get_for_range_split(self.close_paren.span());

        let scrutinee_trivia_single_line_len: Option<usize> = open_trailing
            .iter()
            .chain(scrutinee_leading)
            .chain(scrutinee_trailing)
            .chain(close_paren_leading)
            .map(|t| t.single_line_len(printer.input))
            .sum::<Option<usize>>();

        // Try printing scrutinee on a single line
        let mut scrutinee_printer =
            Printer::new_empty(printer.input, printer.config, printer.trivia);
        let scrutinee_info = scrutinee_printer.print(
            &*self.scrutinee,
            Shape {
                width: shape.width.saturating_sub(2),
                indent: shape.indent,
                first_line_offset: shape.first_line_offset + 1,
            },
        );

        let scrutinee_multi_line =
            scrutinee_info.multi_lined || scrutinee_trivia_single_line_len.is_none();

        if scrutinee_multi_line {
            // Multi-line scrutinee: like ParenExpr::print_multi_line
            let paren_inner_indent = shape.indent + printer.config.indent_width;
            printer.print_raw_token(&self.open_paren);
            printer.print_trivia_all_trailing_for(self.open_paren.span());
            printer.print_newline();

            printer.print_standalone_with_trivia(&*self.scrutinee, paren_inner_indent);

            printer.print_trivia_all_leading_with_newline_for(
                self.close_paren.span(),
                paren_inner_indent,
            );
            printer.print_newline();
            printer.print_spaces(shape.indent);
            printer.print_raw_token(&self.close_paren);
        } else {
            // Single-line scrutinee
            printer.print_raw_token(&self.open_paren);
            for t in open_trailing {
                printer.print_trivia(t);
            }
            for t in scrutinee_leading {
                if t.is_comment() {
                    printer.print_trivia(t);
                }
            }
            printer.append_from_printer(scrutinee_printer);
            for t in scrutinee_trailing {
                printer.print_trivia(t);
            }
            for t in close_paren_leading {
                if t.is_comment() {
                    printer.print_trivia(t);
                }
            }
            printer.print_raw_token(&self.close_paren);
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

impl Printable for MatchArm {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut pattern_printer = printer.sub_printer();
        let pattern_info = pattern_printer.print(&self.pattern, shape.clone());

        let condition_multi_lined;
        printer.append_from_printer(pattern_printer);

        if let Some(guard) = &self.guard {
            let guard_indent = shape.indent + printer.config.indent_width;
            if pattern_info.multi_lined {
                // guard goes on new line
                printer.print_newline();
                printer.print_spaces(guard_indent);
                let offset = usize::from(guard.keyword.token_span.len()) + const { " ".len() };
                let guard_shape = Shape {
                    width: printer
                        .config
                        .line_width
                        .saturating_sub(guard_indent + offset + const { " => ".len() }),
                    indent: guard_indent,
                    first_line_offset: offset,
                };
                printer.print(guard, guard_shape);
                printer.print_str(" => ");
                condition_multi_lined = true;
            } else {
                let mut single_line_guard_printer = printer.sub_printer();
                single_line_guard_printer.print_str(" ");
                single_line_guard_printer.print_raw_token(&guard.keyword);
                single_line_guard_printer.print_str(" ");
                let guard_info = single_line_guard_printer
                    .print(&guard.condition, Shape::unlimited_single_line());

                if guard_info.multi_lined
                    || printer.current_line_len()
                        + single_line_guard_printer.len()
                        + const { " =>".len() }
                        > printer.config.line_width
                {
                    // Guard is too long to fit on a single line, so print it on the next line
                    printer.print_newline();
                    printer.print_spaces(guard_indent);
                    printer.print_raw_token(&guard.keyword);
                    printer.print_str(" ");
                    let guard_shape = Shape {
                        width: printer.config.line_width.saturating_sub(
                            guard_indent + usize::from(guard.keyword.span().len()) + 1,
                        ),
                        indent: guard_indent,
                        first_line_offset: usize::from(guard.keyword.span().len()) + 1,
                    };
                    printer.print(&guard.condition, guard_shape);
                    printer.print_str(" ");
                    printer.print_raw_token(&self.fat_arrow);
                    condition_multi_lined = true;
                } else {
                    printer.append_from_printer(single_line_guard_printer);
                    printer.print_str(" ");
                    printer.print_raw_token(&self.fat_arrow);
                    printer.print_str(" ");
                    condition_multi_lined = false;
                }
            }
        } else {
            condition_multi_lined = pattern_info.multi_lined;
            printer.print_str(" ");
            printer.print_raw_token(&self.fat_arrow);
            printer.print_str(" ");
        }

        let body_info = if condition_multi_lined {
            printer.print_newline();
            let body_shape = Shape {
                width: printer.config.line_width.saturating_sub(shape.indent),
                indent: shape.indent,
                first_line_offset: 0,
            };
            printer.print_spaces(shape.indent);
            printer.print(&self.body, body_shape)
        } else {
            let remaining = printer.current_line_remaining_width();
            let body_shape = Shape {
                width: remaining,
                indent: shape.indent,
                first_line_offset: printer
                    .config
                    .line_width
                    .saturating_sub(remaining + shape.indent),
            };
            printer.print(&self.body, body_shape)
        };

        let multi_lined = condition_multi_lined || body_info.multi_lined;
        if let Some(comma) = &self.comma {
            printer.print_raw_token(comma);
        } else {
            printer.print_str(",");
        }
        PrintInfo { multi_lined }
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
    pub args: Vec<(Expression, Option<t::Comma>)>,
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

            let expr = Expression::from_cst(elem)?;
            let comma = it
                .next_if_kind(SyntaxKind::COMMA)
                .map(t::Comma::from_cst)
                .transpose()?;
            args.push((expr, comma));
        };

        it.expect_end()?;

        Ok(CallArgs {
            open_paren,
            args,
            close_paren,
        })
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
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the call args on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        printer.print_raw_token(&self.open_paren);
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_paren.span());
        printer.print_trivia_single_line_squished(open_trailing)?;

        for (i, (arg, comma)) in self.args.iter().enumerate() {
            if printer.output.len() > shape.width {
                return None;
            }
            let (arg_leading, arg_trailing) = printer.trivia.get_for_element(arg);
            printer.print_trivia_single_line_squished(arg_leading)?;
            if printer
                .print(arg, Shape::unlimited_single_line())
                .multi_lined
            {
                return None;
            }
            printer.print_trivia_single_line_squished(arg_trailing)?;
            if i + 1 < self.args.len() {
                if let Some(comma) = comma {
                    let (comma_leading, comma_trailing) =
                        printer.trivia.get_for_range_split(comma.span());
                    printer.print_trivia_single_line_squished(comma_leading)?;
                    printer.print_raw_token(comma);
                    printer.print_trivia_single_line_squished(comma_trailing)?;
                } else {
                    printer.print_str(",");
                }
                printer.print_str(" ");
            } else if let Some(comma) = comma {
                // Trailing comma is removed in single-line mode, but we still try the comments.
                let (comma_leading, comma_trailing) =
                    printer.trivia.get_for_range_split(comma.span());
                printer.print_trivia_single_line_squished(comma_leading)?;
                printer.print_trivia_single_line_squished(comma_trailing)?;
            }
        }

        let (close_leading, _) = printer.trivia.get_for_range_split(self.close_paren.span());
        printer.print_trivia_single_line_squished(close_leading)?;
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

impl Printable for IndexExpr {
    /// The main way to call this should be through [`PrintChain`]
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let multi_lined = printer.print(&*self.base, shape.clone()).multi_lined;

        let mut index_printer = Printer::new_empty(printer.input, printer.config, printer.trivia);
        let index_info = index_printer.print(&*self.index, Shape::unlimited_single_line());

        if index_info.multi_lined
            || index_printer.output.len() + 2 > printer.current_line_remaining_width()
        {
            // We do not fit, switch to multi-line
            printer.print_raw_token(&self.open_bracket);
            printer.print_newline();
            let inner_indent = shape.indent + printer.config.indent_width;
            let inner_shape = Shape {
                width: shape.width.saturating_sub(inner_indent),
                indent: inner_indent,
                first_line_offset: 0,
            };
            printer.print_spaces(inner_shape.indent);
            printer.print(&*self.index, inner_shape);
            printer.print_newline();
            printer.print_spaces(shape.indent);
            printer.print_raw_token(&self.close_bracket);
            PrintInfo::default_multi_lined()
        } else {
            printer.print_raw_token(&self.open_bracket);
            printer.append_from_printer(index_printer);
            printer.print_raw_token(&self.close_bracket);
            PrintInfo { multi_lined }
        }
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

/// Corresponds to a [`SyntaxKind::ENV_ACCESS_EXPR`] node.
#[derive(Debug)]
pub struct EnvAccessExpr {
    pub keyword: t::Env,
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
        let inner_shape = Shape {
            width: shape.width.saturating_sub(printer.config.indent_width),
            indent: shape.indent + printer.config.indent_width,
            first_line_offset: 0,
        };

        printer.print_raw_token(&self.open_brace);
        printer.print_trivia_all_trailing_for(self.open_brace.span());
        printer.print_newline();
        for stmt in &self.stmts {
            printer.print_standalone_with_trivia(stmt, inner_shape.indent);
            printer.print_newline();
        }
        if let Some(expr) = self.expr.as_deref() {
            printer.print_standalone_with_trivia(expr, inner_shape.indent);
            printer.print_newline();
        }

        printer
            .print_trivia_all_leading_with_newline_for(self.close_brace.span(), inner_shape.indent);
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
        let inner_shape = Shape {
            width: shape.width.saturating_sub(printer.config.indent_width),
            indent: shape.indent + printer.config.indent_width,
            first_line_offset: 0,
        };

        printer.print_raw_token(&self.open_bracket);
        printer.print_trivia_all_trailing_for(self.open_bracket.span());
        printer.print_newline();

        for (elem, comma) in &self.elements {
            printer.print_trivia_all_leading_with_newline_for(
                elem.leftmost_token(),
                inner_shape.indent,
            );
            printer.print_spaces(inner_shape.indent);
            printer.print(elem, inner_shape.clone());
            if let Some(comma) = comma {
                printer.print_raw_token(comma);
                printer.print_trivia_all_trailing_for(comma.span());
            } else {
                printer.print_str(",");
                printer.print_trivia_all_trailing_for(elem.rightmost_token());
            }
            printer.print_newline();
        }

        printer.print_trivia_all_leading_with_newline_for(
            self.close_bracket.span(),
            inner_shape.indent,
        );
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_bracket);
        PrintInfo::default_multi_lined()
    }
}

impl ArrayInitializer {
    /// Tries to print the array initializer as a single line.
    ///
    /// If successful, returns the info.
    ///
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the array initializer on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        printer.print_raw_token(&self.open_bracket);
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_bracket.span());
        printer.print_trivia_single_line_squished(open_trailing)?;

        for (i, (elem, comma)) in self.elements.iter().enumerate() {
            if printer.output.len() > shape.width {
                return None;
            }

            let (el_leading, el_trailing) = printer.trivia.get_for_element(elem);
            printer.print_trivia_single_line_squished(el_leading)?;
            if printer
                .print(elem, Shape::unlimited_single_line())
                .multi_lined
            {
                return None;
            }
            printer.print_trivia_single_line_squished(el_trailing)?;
            if i + 1 < self.elements.len() {
                if let Some(comma) = comma {
                    let (comma_leading, comma_trailing) =
                        printer.trivia.get_for_range_split(comma.span());
                    printer.print_trivia_single_line_squished(comma_leading)?;
                    printer.print_raw_token(comma);
                    printer.print_trivia_single_line_squished(comma_trailing)?;
                } else {
                    printer.print_str(",");
                }
                printer.print_str(" ");
            } else if let Some(comma) = comma {
                // Trailing comma is removed in single-line mode, but we still try the comments.
                let (comma_leading, comma_trailing) =
                    printer.trivia.get_for_range_split(comma.span());
                printer.print_trivia_single_line_squished(comma_leading)?;
                printer.print_trivia_single_line_squished(comma_trailing)?;
            }
        }

        let (close_leading, _) = printer
            .trivia
            .get_for_range_split(self.close_bracket.span());
        printer.print_trivia_single_line_squished(close_leading)?;
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
    pub name: t::Word,
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
        let name = it.expect_parse()?;

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

        printer.print_raw_token(&self.name);
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
    /// Tries to print the object initializer as a single line.
    ///
    /// If successful, returns the info.
    ///
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the object initializer on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        printer.print_raw_token(&self.name);
        printer.print_str(" ");
        printer.print_raw_token(&self.open_brace);
        printer.print_str(" ");
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_brace.span());
        printer.print_trivia_single_line_squished(open_trailing)?;

        for (i, (field, comma)) in self.fields.iter().enumerate() {
            if printer.output.len() > shape.width {
                return None;
            }
            let (fld_leading, fld_trailing) = printer.trivia.get_for_element(field);
            printer.print_trivia_single_line_squished(fld_leading)?;
            if printer
                .print(field, Shape::unlimited_single_line())
                .multi_lined
            {
                return None;
            }
            printer.print_trivia_single_line_squished(fld_trailing)?;
            if i + 1 < self.fields.len() {
                if let Some(comma) = comma {
                    let (comma_leading, comma_trailing) =
                        printer.trivia.get_for_range_split(comma.span());
                    printer.print_trivia_single_line_squished(comma_leading)?;
                    printer.print_raw_token(comma);
                    printer.print_trivia_single_line_squished(comma_trailing)?;
                } else {
                    printer.print_str(",");
                }
                printer.print_str(" ");
            } else if let Some(comma) = comma {
                // Trailing comma is removed in single-line mode, but we still try the comments.
                let (comma_leading, comma_trailing) =
                    printer.trivia.get_for_range_split(comma.span());
                printer.print_trivia_single_line_squished(comma_leading)?;
                printer.print_trivia_single_line_squished(comma_trailing)?;
            }
        }
        let (close_leading, _) = printer.trivia.get_for_range_split(self.close_brace.span());
        printer.print_trivia_single_line_squished(close_leading)?;
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
        self.name.span()
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
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the map literal on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        printer.print_raw_token(&self.open_brace);
        printer.print_str(" ");
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_brace.span());
        printer.print_trivia_single_line_squished(open_trailing)?;

        for (i, (field, comma)) in self.fields.iter().enumerate() {
            if printer.output.len() > shape.width {
                return None;
            }
            let (fld_leading, fld_trailing) = printer.trivia.get_for_element(field);
            printer.print_trivia_single_line_squished(fld_leading)?;
            if printer
                .print(field, Shape::unlimited_single_line())
                .multi_lined
            {
                return None;
            }
            printer.print_trivia_single_line_squished(fld_trailing)?;
            if i + 1 < self.fields.len() {
                if let Some(comma) = comma {
                    let (comma_leading, comma_trailing) =
                        printer.trivia.get_for_range_split(comma.span());
                    printer.print_trivia_single_line_squished(comma_leading)?;
                    printer.print_raw_token(comma);
                    printer.print_trivia_single_line_squished(comma_trailing)?;
                } else {
                    printer.print_str(",");
                }
                printer.print_str(" ");
            } else if let Some(comma) = comma {
                // Trailing comma is removed in single-line mode, but we still try the comments.
                let (comma_leading, comma_trailing) =
                    printer.trivia.get_for_range_split(comma.span());
                printer.print_trivia_single_line_squished(comma_leading)?;
                printer.print_trivia_single_line_squished(comma_trailing)?;
            }
        }
        let (close_leading, _) = printer.trivia.get_for_range_split(self.close_brace.span());
        printer.print_trivia_single_line_squished(close_leading)?;
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

impl Printable for ObjectField {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;
        multi_lined |= printer.print(&self.name, shape.clone()).multi_lined;
        printer.print_raw_token(&self.colon);
        printer.print_str(" ");
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
            Expression::Path(path_expr) => Self {
                first: from,
                chain_members: path_expr
                    .rest
                    .iter()
                    .map(|(dot, word)| PrintChainItem::FieldAccess(dot, word))
                    .collect(),
            },
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
                    chain.chain_members.push(PrintChainItem::Index(
                        &index_expr.open_bracket,
                        &index_expr.index,
                        &index_expr.close_bracket,
                    ));
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
        let should_indent_chain = first_single_line || offset > printer.config.indent_width;
        let chain_indent = if should_indent_chain {
            shape.indent + printer.config.indent_width
        } else {
            shape.indent
        };

        let mut line_remaining_width = printer.current_line_remaining_width();
        let mut it = self.chain_members.iter();
        if first_single_line
            && offset <= printer.config.indent_width
            && let Some(&PrintChainItem::FieldAccess(dot, word)) = it.next()
        {
            // We can try to print the second item on the same line as the first item
            // if it fits, since the first item is very short.
            let second_len = usize::from(dot.span().len() + word.span().len());
            if line_remaining_width >= second_len {
                printer.print_raw_token(dot);
                printer.print_raw_token(word);
                line_remaining_width = line_remaining_width.saturating_sub(second_len);
            } else {
                // Otherwise, we need to print the first item on its own line.
                printer.print_newline();
                printer.print_spaces(chain_indent);
                printer.print_raw_token(dot);
                printer.print_raw_token(word);
                line_remaining_width = printer
                    .config
                    .line_width
                    .saturating_sub(chain_indent + second_len);
            }
        }
        for item in it {
            match *item {
                PrintChainItem::FieldAccess(dot, word) => {
                    printer.print_newline();
                    printer.print_spaces(chain_indent);
                    printer.print_raw_token(dot);
                    printer.print_raw_token(word);
                    line_remaining_width = printer.config.line_width.saturating_sub(
                        chain_indent + usize::from(dot.span().len() + word.span().len()),
                    );
                }
                PrintChainItem::Index(lbracket, expression, rbracket) => {
                    let mut single_line_printer =
                        Printer::new_empty(printer.input, printer.config, printer.trivia);
                    let single_line_info =
                        single_line_printer.print(expression, Shape::unlimited_single_line());
                    let single_line_len = single_line_printer.output.len()
                        + usize::from(lbracket.span().len() + rbracket.span().len());
                    if single_line_info.multi_lined || single_line_len > line_remaining_width {
                        // Print multi-line
                        printer.print_raw_token(lbracket);
                        printer.print_newline();
                        let inner_expr_indent = chain_indent + printer.config.indent_width;
                        let inner_expr_shape = Shape {
                            width: printer.config.line_width.saturating_sub(inner_expr_indent),
                            indent: inner_expr_indent,
                            first_line_offset: 0,
                        };
                        printer.print_spaces(inner_expr_indent);
                        printer.print(expression, inner_expr_shape);
                        printer.print_newline();
                        printer.print_spaces(chain_indent);
                        printer.print_raw_token(rbracket);
                        line_remaining_width = printer
                            .config
                            .line_width
                            .saturating_sub(chain_indent + usize::from(rbracket.span().len()));
                    } else {
                        // Print on end of line
                        line_remaining_width = line_remaining_width.saturating_sub(single_line_len);
                        printer.print_raw_token(lbracket);
                        printer.append_from_printer(single_line_printer);
                        printer.print_raw_token(rbracket);
                    }
                }
                PrintChainItem::Call(call_args) => {
                    let mut single_line_printer =
                        Printer::new_empty(printer.input, printer.config, printer.trivia);
                    let single_line_info =
                        single_line_printer.print(call_args, Shape::unlimited_single_line());
                    if single_line_info.multi_lined
                        || single_line_printer.output.len() > line_remaining_width
                    {
                        // Print multi-line
                        let call_args_shape = Shape {
                            width: 0, // not single-lined
                            indent: chain_indent,
                            first_line_offset: line_remaining_width.saturating_sub(chain_indent),
                        };
                        call_args.print_multi_line(call_args_shape, printer);
                    } else {
                        // Print on end of line
                        line_remaining_width =
                            line_remaining_width.saturating_sub(single_line_printer.output.len());
                        printer.append_from_printer(single_line_printer);
                    }
                }
            }
        }

        PrintInfo::default_multi_lined()
    }
}

impl Printable for PrintChain<'_> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut single_line_printer =
            Printer::new_empty(printer.input, printer.config, printer.trivia);
        let mut multi_lined = false;

        match self.first {
            Expression::Path(path_expr) => {
                single_line_printer.print_raw_token(&path_expr.first);
            }
            Expression::Call(call_expr) => {
                multi_lined |= single_line_printer
                    .print(call_expr, Shape::unlimited_single_line())
                    .multi_lined;
            }
            Expression::Index(index_expr) => {
                multi_lined |= single_line_printer
                    .print(index_expr, Shape::unlimited_single_line())
                    .multi_lined;
            }
            _ => {
                multi_lined |= single_line_printer
                    .print(self.first, Shape::unlimited_single_line())
                    .multi_lined;
            }
        }
        for item in &self.chain_members {
            if multi_lined || single_line_printer.output.len() > shape.width {
                return Self::print_multi_line(self, shape, printer);
            }
            match *item {
                PrintChainItem::FieldAccess(dot, word) => {
                    single_line_printer.print_raw_token(dot);
                    single_line_printer.print_raw_token(word);
                }
                PrintChainItem::Index(open_bracket, index, close_bracket) => {
                    single_line_printer.print_raw_token(open_bracket);
                    multi_lined |= single_line_printer
                        .print(index, Shape::unlimited_single_line())
                        .multi_lined;
                    single_line_printer.print_raw_token(close_bracket);
                }
                PrintChainItem::Call(call_args) => {
                    multi_lined |= single_line_printer
                        .print(call_args, Shape::unlimited_single_line())
                        .multi_lined;
                }
            }
        }
        if multi_lined || single_line_printer.output.len() > shape.width {
            Self::print_multi_line(self, shape, printer)
        } else {
            printer.append_from_printer(single_line_printer);
            PrintInfo::default_single_line()
        }
    }
    fn leftmost_token(&self) -> TextRange {
        self.first.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        match self.chain_members.last() {
            Some(PrintChainItem::FieldAccess(_, word)) => word.span(),
            Some(PrintChainItem::Index(_, _, close_bracket)) => close_bracket.span(),
            Some(PrintChainItem::Call(call_args)) => call_args.rightmost_token(),
            None => self.first.rightmost_token(),
        }
    }
}

/// Only used for printing chained expressions. See [`PrintChain`].
enum PrintChainItem<'a> {
    FieldAccess(&'a t::Dot, &'a t::Word),
    Index(&'a t::LBracket, &'a Expression, &'a t::RBracket),
    Call(&'a CallArgs),
}
