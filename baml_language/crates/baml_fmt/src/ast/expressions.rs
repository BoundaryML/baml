//! Reference: [baml_compiler_syntax::ast::Expr] and [baml_compiler_hir::body]

use baml_compiler_syntax::{SyntaxElement, SyntaxKind};
use rowan::TextRange;

use crate::{
    ast::{
        BinaryOp, FromCST, KnownKind, MatchPattern, Statement, StrongAstError, SyntaxNodeIter,
        Token, UnaryOp,
    },
    printer::{PrintInfo, PrintMultiLine, Printable, Printer, Shape},
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

impl Expression {
    pub const fn statement_needs_semicolon(&self) -> bool {
        match self {
            Expression::If(_) => false,
            Expression::Match(_) => false,
            Expression::Unknown(_) => false,
            _ => true,
        }
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
            Expression::Path(path) => path.print(shape, printer),
            Expression::Paren(paren) => paren.print(shape, printer),
            Expression::Binary(binary) => binary.print(shape, printer),
            Expression::Unary(unary) => unary.print(shape, printer),
            Expression::If(if_expr) => if_expr.print(shape, printer),
            Expression::Match(match_expr) => match_expr.print(shape, printer),
            Expression::Call(call) => call.print(shape, printer),
            Expression::Index(index) => index.print(shape, printer),
            Expression::FieldAccess(field) => field.print(shape, printer),
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
}

#[derive(Debug)]
pub enum Literal {
    String(t::QuotedString),
    Integer(t::IntegerLiteral),
    Float(t::FloatLiteral),
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

        let mut it = SyntaxNodeIter::new(node);

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

impl PrintMultiLine for PathExpr {
    /// Multi-line layout: splits at dots, each subsequent segment on
    /// an indented new line. Used for chained method calls.
    ///
    /// ```baml
    /// baml
    ///     .fs
    ///     .open("some_long_file_name.txt")
    ///     .read()
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.first);
        for (dot, word) in &self.rest {
            printer.print_newline();
            printer.print_spaces(shape.indent + printer.config.indent_width);
            printer.print_raw_token(dot);
            printer.print_raw_token(word);
        }
        PrintInfo::default_multi_lined()
    }
}

impl Printable for PathExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        if self.rest.is_empty() {
            printer.print_raw_token(&self.first);
            return PrintInfo::default_single_line();
        }
        let single_line_width = usize::from(self.first.span().len())
            + self
                .rest
                .iter()
                .map(|(dot, word)| usize::from(dot.span().len() + word.span().len()))
                .sum::<usize>();
        if single_line_width > shape.width {
            self.print_multi_line(shape, printer)
        } else {
            printer.print_raw_token(&self.first);
            for (dot, word) in &self.rest {
                printer.print_raw_token(dot);
                printer.print_raw_token(word);
            }
            PrintInfo::default_single_line()
        }
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
        printer.print_newline();
        printer.print_spaces(inner_shape.indent);
        printer.print(&*self.expr, inner_shape);
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
            width: shape.width.saturating_sub(1),
            indent: shape.indent,
            first_line_offset: shape.first_line_offset + 1,
        };
        let inner_info = inner_printer.print(&*self.expr, inner_shape_single_line);

        if inner_info.multi_lined {
            let inner_shape_multi_line = Shape {
                width: shape.width.saturating_sub(printer.config.indent_width),
                indent: shape.indent + printer.config.indent_width,
                first_line_offset: 0,
            };
            printer.print_raw_token(&self.open_paren);
            printer.print_newline();
            printer.print_spaces(inner_shape_multi_line.indent);
            printer.print(&*self.expr, inner_shape_multi_line);
            printer.print_newline();
            printer.print_spaces(shape.indent);
            printer.print_raw_token(&self.close_paren);
            PrintInfo::default_multi_lined()
        } else {
            printer.print_raw_token(&self.open_paren);
            printer.append_from_printer(inner_printer);
            printer.print_raw_token(&self.close_paren);
            PrintInfo::default_single_line()
        }
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
    fn get_chaining_members<'s>(&'s self) -> (&'s Expression, Vec<(&'s BinaryOp, &'s Expression)>) {
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
    /// side wrap to an indented new line.
    ///
    /// ```baml
    /// left_expression
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
        printer.print(first, shape.clone());
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
        }
        PrintInfo::default_multi_lined()
    }
}

impl Printable for BinaryExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let (left, right) = &*self.sides;
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
}

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
            BinaryOp::Plus(_) => Some(Self::AddSubtract),
            BinaryOp::Minus(_) => Some(Self::AddSubtract),
            BinaryOp::Star(_) => Some(Self::MultiplyDivide),
            BinaryOp::Slash(_) => Some(Self::MultiplyDivide),
            BinaryOp::Percent(_) => Some(Self::MultiplyDivide),
            BinaryOp::And(_) => Some(Self::Bitwise),
            BinaryOp::Pipe(_) => Some(Self::Bitwise),
            BinaryOp::Caret(_) => Some(Self::Bitwise),
            BinaryOp::AndAnd(_) => Some(Self::Logical),
            BinaryOp::OrOr(_) => Some(Self::Logical),
            _ => None,
        }
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
}

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

        let mut it = SyntaxNodeIter::new(node);

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
        let inner_shape = Shape {
            width: shape.width.saturating_sub(printer.config.indent_width),
            indent: shape.indent + printer.config.indent_width,
            first_line_offset: 0,
        };

        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        printer.print_raw_token(&self.open_paren);
        printer.print(&*self.scrutinee, shape.clone());
        printer.print_raw_token(&self.close_paren);
        printer.print_str(" ");
        printer.print_raw_token(&self.open_brace);
        printer.print_newline();

        for arm in &self.arms {
            printer.print_spaces(inner_shape.indent);
            printer.print(arm, inner_shape.clone());
            printer.print_newline();
        }

        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_brace);

        PrintInfo::default_multi_lined()
    }
}

/// Corresponds to a [`SyntaxKind::MATCH_ARM`] node.
#[derive(Debug)]
pub struct MatchArm {
    pub pattern: MatchPattern,
    pub guard: Option<(t::If, Box<Expression>)>,
    pub fat_arrow: t::FatArrow,
    pub body: Box<Expression>,
    pub comma: Option<t::Comma>,
}

impl FromCST for MatchArm {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::MATCH_ARM)?;

        let mut it = SyntaxNodeIter::new(node);

        // MATCH_PATTERN
        let pattern: MatchPattern = it.expect_parse()?;

        // Check for optional guard (if condition)
        let guard = if let Some(elem) = it.next_if_kind(SyntaxKind::KW_IF) {
            let if_token = StrongAstError::assert_is_token(elem)?;
            let guard_expr_node = it.expect_next("guard expression")?;
            let guard_expr = Expression::from_cst(guard_expr_node)?;
            Some((
                t::If::new_from_span(if_token.text_range()),
                Box::new(guard_expr),
            ))
        } else {
            None
        };

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
            body: Box::new(body),
            comma,
        })
    }
}

impl KnownKind for MatchArm {
    fn kind() -> SyntaxKind {
        SyntaxKind::MATCH_ARM
    }
}

impl PrintMultiLine for MatchArm {
    /// Multi-line layout: the body is made into a block expression
    /// (if it is not already a block expression).
    /// The if guard (if present) is on its own indented line.
    ///
    /// ```baml
    /// pattern => {
    ///     some_long_body_expression
    /// }
    /// ```
    ///
    /// ```baml
    /// binding: pattern
    ///     | more_pattern
    ///     | yet_more_pattern
    ///     if some_long_guard_expression => {
    ///     some_long_body_expression
    /// }
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_shape = Shape {
            width: shape.width.saturating_sub(printer.config.indent_width),
            indent: shape.indent + printer.config.indent_width,
            first_line_offset: 0,
        };

        printer.print(&self.pattern, shape.clone());

        if let Some((if_kw, guard)) = &self.guard {
            printer.print_newline();
            printer.print_spaces(inner_shape.indent);
            printer.print_raw_token(if_kw);
            printer.print_str(" ");
            printer.print(&**guard, inner_shape.clone());
        }

        printer.print_str(" ");
        printer.print_raw_token(&self.fat_arrow);
        printer.print_str(" ");

        // If body is already a block, print it directly; otherwise wrap it in a block
        match &*self.body {
            Expression::Block(block) => {
                printer.print(block, shape.clone());
            }
            _ => {
                printer.print_str("{");
                printer.print_newline();
                printer.print_spaces(inner_shape.indent);
                printer.print(&*self.body, inner_shape);
                printer.print_newline();
                printer.print_spaces(shape.indent);
                printer.print_str("}");
            }
        }

        if let Some(comma) = &self.comma {
            printer.print_raw_token(comma);
        } else {
            printer.print_str(",");
        }

        PrintInfo::default_multi_lined()
    }
}

impl Printable for MatchArm {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut single_line_printer =
            Printer::new_empty(printer.input, printer.config, printer.trivia);
        let mut multi_lined = false;
        multi_lined |= single_line_printer
            .print(&self.pattern, Shape::unlimited_single_line())
            .multi_lined;

        if let Some((if_kw, guard)) = &self.guard {
            single_line_printer.print_str(" ");
            single_line_printer.print_raw_token(if_kw);
            single_line_printer.print_str(" ");
            multi_lined |= single_line_printer
                .print(&**guard, Shape::unlimited_single_line())
                .multi_lined;
        }

        single_line_printer.print_str(" ");
        single_line_printer.print_raw_token(&self.fat_arrow);
        single_line_printer.print_str(" ");
        multi_lined |= single_line_printer
            .print(&*self.body, Shape::unlimited_single_line())
            .multi_lined;

        if let Some(comma) = &self.comma {
            single_line_printer.print_raw_token(comma);
        } else {
            single_line_printer.print_str(",");
        }

        if multi_lined || single_line_printer.output.len() > shape.width {
            Self::print_multi_line(self, shape, printer)
        } else {
            printer.append_from_printer(single_line_printer);
            PrintInfo::default_single_line()
        }
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

        let mut it = SyntaxNodeIter::new(node);

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
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;
        multi_lined |= printer.print(&*self.callee, shape.clone()).multi_lined;
        multi_lined |= printer.print(&self.args, shape).multi_lined;
        PrintInfo { multi_lined }
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

        let open_paren = it.expect_parse()?;

        let mut args = Vec::new();
        let close_paren = loop {
            let Some(elem) = it.next() else {
                return Err(StrongAstError::missing(SyntaxKind::R_PAREN, it.parent));
            };
            match elem.kind() {
                SyntaxKind::R_PAREN => {
                    break t::RParen::from_cst(elem)?;
                }
                _ => {
                    let expr = Expression::from_cst(elem)?;
                    let comma = it
                        .next_if_kind(SyntaxKind::COMMA)
                        .map(t::Comma::from_cst)
                        .transpose()?;
                    args.push((expr, comma));
                }
            }
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
        printer.print_raw_token(&self.open_paren);
        printer.print_newline();

        let inner_shape = Shape {
            width: shape.width.saturating_sub(printer.config.indent_width),
            indent: shape.indent + printer.config.indent_width,
            first_line_offset: 0,
        };
        for (arg, comma) in self.args.iter() {
            printer.print_spaces(inner_shape.indent);
            printer.print(arg, inner_shape.clone());
            if let Some(comma) = comma {
                printer.print_raw_token(comma);
            } else {
                printer.print_str(",");
            }
            printer.print_newline();
        }
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_paren);

        PrintInfo::default_multi_lined()
    }
}

impl Printable for CallArgs {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;
        let mut single_line_printer =
            Printer::new_empty(printer.input, printer.config, printer.trivia);
        single_line_printer.print_raw_token(&self.open_paren);
        for (i, (arg, comma)) in self.args.iter().enumerate() {
            multi_lined |= single_line_printer
                .print(arg, Shape::unlimited_single_line())
                .multi_lined;
            if let Some(comma) = comma {
                single_line_printer.print_raw_token(comma);
                single_line_printer.print_spaces(1);
            } else if i + 1 < self.args.len() {
                single_line_printer.print_str(", ");
            }

            if multi_lined || single_line_printer.output.len() > shape.width {
                return Self::print_multi_line(self, shape, printer);
            }
        }
        single_line_printer.print_raw_token(&self.close_paren);

        if multi_lined || single_line_printer.output.len() > shape.width {
            Self::print_multi_line(self, shape, printer)
        } else {
            printer.append_from_printer(single_line_printer);
            PrintInfo::default_single_line()
        }
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

impl PrintMultiLine for FieldAccessExpr {
    /// Multi-line layout:
    ///
    /// ```baml
    /// some_multi_line_expression(
    ///     long_arg,
    /// ).field
    /// ```
    ///
    /// ```baml
    /// not_a_chain
    ///     .because_it_is_an_expression()
    ///     .field
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print(&*self.base, shape);
        printer.print_raw_token(&self.dot);
        printer.print_raw_token(&self.field);
        PrintInfo::default_multi_lined()
    }
}

impl Printable for FieldAccessExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut single_line_printer =
            Printer::new_empty(printer.input, printer.config, printer.trivia);
        let multi_lined = single_line_printer
            .print(&*self.base, Shape::unlimited_single_line())
            .multi_lined;
        single_line_printer.print_raw_token(&self.dot);
        single_line_printer.print_raw_token(&self.field);

        if multi_lined || single_line_printer.output.len() > shape.width {
            Self::print_multi_line(self, shape, printer)
        } else {
            printer.append_from_printer(single_line_printer);
            PrintInfo::default_single_line()
        }
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

        let mut it = SyntaxNodeIter::new(node);

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
        printer.print_newline();
        for stmt in &self.stmts {
            printer.print_spaces(inner_shape.indent);
            printer.print(stmt, inner_shape.clone());
            printer.print_newline();
        }
        if let Some(expr) = self.expr.as_deref() {
            printer.print_spaces(inner_shape.indent);
            printer.print(expr, inner_shape.clone());
            printer.print_newline();
        }
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_brace);

        PrintInfo { multi_lined: true }
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

        let open_bracket = it.expect_parse()?;

        let mut elements: Vec<(Expression, Option<t::Comma>)> = Vec::new();

        let close_bracket = loop {
            let Some(elem) = it.next() else {
                return Err(StrongAstError::missing(SyntaxKind::R_BRACKET, it.parent));
            };
            match elem.kind() {
                SyntaxKind::R_BRACKET => {
                    break t::RBracket::from_cst(elem)?;
                }
                _ => {
                    let expr = Expression::from_cst(elem)?;
                    let comma = it
                        .next_if_kind(SyntaxKind::COMMA)
                        .map(t::Comma::from_cst)
                        .transpose()?;

                    elements.push((expr, comma));
                }
            }
        };

        return Ok(ArrayInitializer {
            open_bracket,
            elements,
            close_bracket,
        });
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
        printer.print_newline();

        for (elem, comma) in &self.elements {
            printer.print_spaces(inner_shape.indent);
            printer.print(elem, inner_shape.clone());
            if let Some(comma) = comma {
                printer.print_raw_token(comma);
            } else {
                printer.print_str(",");
            }
            printer.print_newline();
        }

        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_bracket);
        PrintInfo::default_multi_lined()
    }
}

impl Printable for ArrayInitializer {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;
        let mut single_line_printer =
            Printer::new_empty(printer.input, printer.config, printer.trivia);
        single_line_printer.print_raw_token(&self.open_bracket);
        for (i, (elem, comma)) in self.elements.iter().enumerate() {
            multi_lined |= single_line_printer
                .print(elem, Shape::unlimited_single_line())
                .multi_lined;
            if i + 1 < self.elements.len() {
                if let Some(comma) = comma {
                    single_line_printer.print_raw_token(comma);
                } else {
                    single_line_printer.print_str(",");
                }
                single_line_printer.print_str(" ");
            }
            if multi_lined || single_line_printer.output.len() > shape.width {
                return Self::print_multi_line(self, shape, printer);
            }
        }
        single_line_printer.print_raw_token(&self.close_bracket);

        if multi_lined || single_line_printer.output.len() > shape.width {
            Self::print_multi_line(self, shape, printer)
        } else {
            printer.append_from_printer(single_line_printer);
            PrintInfo::default_single_line()
        }
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

        let mut it = SyntaxNodeIter::new(node);

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
        printer.print_newline();

        for (field, comma) in &self.fields {
            printer.print_spaces(inner_shape.indent);
            printer.print(field, inner_shape.clone());
            if let Some(comma) = comma {
                printer.print_raw_token(comma);
            } else {
                printer.print_str(",");
            }
            printer.print_newline();
        }

        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_brace);
        PrintInfo::default_multi_lined()
    }
}

impl Printable for ObjectInitializer {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;
        let mut single_line_printer =
            Printer::new_empty(printer.input, printer.config, printer.trivia);
        single_line_printer.print_raw_token(&self.name);
        single_line_printer.print_str(" ");
        single_line_printer.print_raw_token(&self.open_brace);
        single_line_printer.print_str(" ");
        for (i, (field, comma)) in self.fields.iter().enumerate() {
            multi_lined |= single_line_printer
                .print(field, Shape::unlimited_single_line())
                .multi_lined;
            if i + 1 < self.fields.len() {
                if let Some(comma) = comma {
                    single_line_printer.print_raw_token(comma);
                } else {
                    single_line_printer.print_str(",");
                }
                single_line_printer.print_str(" ");
            }
            if multi_lined || single_line_printer.output.len() > shape.width {
                return Self::print_multi_line(self, shape, printer);
            }
        }
        single_line_printer.print_str(" ");
        single_line_printer.print_raw_token(&self.close_brace);

        if multi_lined || single_line_printer.output.len() > shape.width {
            Self::print_multi_line(self, shape, printer)
        } else {
            printer.append_from_printer(single_line_printer);
            PrintInfo::default_single_line()
        }
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

        let mut it = SyntaxNodeIter::new(node);

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
        printer.print_newline();

        for (field, comma) in &self.fields {
            printer.print_spaces(inner_shape.indent);
            printer.print(field, inner_shape.clone());
            if let Some(comma) = comma {
                printer.print_raw_token(comma);
            } else {
                printer.print_str(",");
            }
            printer.print_newline();
        }

        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_brace);
        PrintInfo::default_multi_lined()
    }
}

impl Printable for MapLiteral {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;
        let mut single_line_printer =
            Printer::new_empty(printer.input, printer.config, printer.trivia);
        single_line_printer.print_raw_token(&self.open_brace);
        single_line_printer.print_str(" ");
        for (i, (field, comma)) in self.fields.iter().enumerate() {
            multi_lined |= single_line_printer
                .print(field, Shape::unlimited_single_line())
                .multi_lined;
            if i + 1 < self.fields.len() {
                if let Some(comma) = comma {
                    single_line_printer.print_raw_token(comma);
                } else {
                    single_line_printer.print_str(",");
                }
                single_line_printer.print_str(" ");
            }
            if multi_lined || single_line_printer.output.len() > shape.width {
                return Self::print_multi_line(self, shape, printer);
            }
        }
        single_line_printer.print_str(" ");
        single_line_printer.print_raw_token(&self.close_brace);

        if multi_lined || single_line_printer.output.len() > shape.width {
            Self::print_multi_line(self, shape, printer)
        } else {
            printer.append_from_printer(single_line_printer);
            PrintInfo::default_single_line()
        }
    }
}

/// Corresponds to a [`SyntaxKind::OBJECT_FIELD`] node.
#[derive(Debug)]
pub struct ObjectField {
    pub name: t::Word,
    pub colon: t::Colon,
    pub value: Expression,
}

impl FromCST for ObjectField {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::OBJECT_FIELD)?;

        let mut it = SyntaxNodeIter::new(node);

        let name = it.expect_parse()?;

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
        printer.print_raw_token(&self.name);
        printer.print_raw_token(&self.colon);
        printer.print_str(" ");
        printer.print(&self.value, shape)
    }
}
