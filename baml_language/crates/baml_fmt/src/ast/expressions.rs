pub use baml_db::baml_compiler_syntax::Literal;
pub(super) use baml_db::baml_compiler_syntax::validated::*;
use rowan::TextRange;

use crate::{
    ast::{BinaryOp, Token, tokens as t},
    printer::{PrintInfo, PrintMultiLine, Printable, Printer, Shape},
    trivia_classifier::{EmittableTrivia, TriviaSliceExt},
};
pub(crate) trait ExpressionFormatExt {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
}
impl ExpressionFormatExt for Expression {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        match self {
            Expression::Literal(lit) => lit.single_line_width(input),
            Expression::Path(path) => path.single_line_width(input),
            Expression::GenericApply(ga) => ga.single_line_width(input),
            Expression::Paren(paren) => paren.single_line_width(input),
            Expression::Binary(binary) => binary.single_line_width(input),
            Expression::Is(is) => is.single_line_width(input),
            Expression::Unary(unary) => unary.single_line_width(input),
            Expression::If(_) => None,
            Expression::IfLet(_) => None,
            Expression::Match(_) => None,
            Expression::Catch(_) => None,
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
            Expression::BacktickString(bt) => {
                if input.input[bt.span()].contains('\n') {
                    None
                } else {
                    Some(usize::from(bt.span().len()))
                }
            }
            Expression::ByteString(bs) => Some(usize::from(bs.span().len())),
            Expression::Lambda(_) => None,
            Expression::Spawn(spawn) => spawn.single_line_width(input),
            Expression::Return(_) | Expression::Break(_) | Expression::Continue(_) => None,
            Expression::Unknown(unknown) => {
                let text = &input.input[unknown.content_range()];
                if text.contains('\n') {
                    None
                } else {
                    Some(text.trim_start().len())
                }
            }
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
                let chain = PrintChain::new(chain);
                chain.print(shape, printer)
            }
            Expression::GenericApply(ga) => ga.print(shape, printer),
            Expression::Paren(paren) => paren.print(shape, printer),
            Expression::Binary(binary) => binary.print(shape, printer),
            Expression::Is(is) => is.print(shape, printer),
            Expression::Unary(unary) => unary.print(shape, printer),
            Expression::If(if_expr) => if_expr.print(shape, printer),
            Expression::IfLet(if_let_expr) => if_let_expr.print(shape, printer),
            Expression::Match(match_expr) => match_expr.print(shape, printer),
            Expression::Catch(catch_expr) => catch_expr.print(shape, printer),
            Expression::EnvAccess(env) => env.print(shape, printer),
            Expression::Block(block) => block.print(shape, printer),
            Expression::ArrayInitializer(array) => array.print(shape, printer),
            Expression::MapInitializer(map) => map.print(shape, printer),
            Expression::ObjectInitializer(obj) => obj.print(shape, printer),
            Expression::RawString(raw) => raw.print(shape, printer),
            Expression::BacktickString(bt) => bt.print(shape, printer),
            Expression::ByteString(bs) => bs.print(shape, printer),
            Expression::Lambda(lambda) => lambda.print(shape, printer),
            Expression::Spawn(spawn) => spawn.print(shape, printer),
            Expression::Return(jump) | Expression::Break(jump) | Expression::Continue(jump) => {
                printer.print_input_range_trimmed_start(jump.content_range());
                PrintInfo::default_multi_lined()
            }
            Expression::Unknown(unknown) => {
                let range = unknown.content_range();
                printer.print_input_range_trimmed_start(range);
                PrintInfo {
                    multi_lined: printer.input[range].contains('\n'),
                }
            }
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            Expression::Literal(lit) => lit.leftmost_token(),
            Expression::Path(path) => path.leftmost_token(),
            Expression::GenericApply(ga) => ga.leftmost_token(),
            Expression::Paren(paren) => paren.leftmost_token(),
            Expression::Binary(binary) => binary.leftmost_token(),
            Expression::Is(is) => is.leftmost_token(),
            Expression::Unary(unary) => unary.leftmost_token(),
            Expression::If(if_expr) => if_expr.leftmost_token(),
            Expression::IfLet(if_let_expr) => if_let_expr.leftmost_token(),
            Expression::Match(match_expr) => match_expr.leftmost_token(),
            Expression::Catch(catch_expr) => catch_expr.leftmost_token(),
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
            Expression::BacktickString(bt) => bt.leftmost_token(),
            Expression::ByteString(bs) => bs.leftmost_token(),
            Expression::Lambda(lambda) => lambda.leftmost_token(),
            Expression::Spawn(spawn) => spawn.leftmost_token(),
            Expression::Return(span)
            | Expression::Break(span)
            | Expression::Continue(span)
            | Expression::Unknown(span) => span.first_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            Expression::Literal(lit) => lit.rightmost_token(),
            Expression::Path(path) => path.rightmost_token(),
            Expression::GenericApply(ga) => ga.rightmost_token(),
            Expression::Paren(paren) => paren.rightmost_token(),
            Expression::Binary(binary) => binary.rightmost_token(),
            Expression::Is(is) => is.rightmost_token(),
            Expression::Unary(unary) => unary.rightmost_token(),
            Expression::If(if_expr) => if_expr.rightmost_token(),
            Expression::IfLet(if_let_expr) => if_let_expr.rightmost_token(),
            Expression::Match(match_expr) => match_expr.rightmost_token(),
            Expression::Catch(catch_expr) => catch_expr.rightmost_token(),
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
            Expression::BacktickString(bt) => bt.rightmost_token(),
            Expression::ByteString(bs) => bs.rightmost_token(),
            Expression::Lambda(lambda) => lambda.rightmost_token(),
            Expression::Spawn(spawn) => spawn.rightmost_token(),
            Expression::Return(span)
            | Expression::Break(span)
            | Expression::Continue(span)
            | Expression::Unknown(span) => span.last_token(),
        }
    }
}
trait LiteralPrintExt {
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
}
impl LiteralPrintExt for Literal {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
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
            Literal::Keyword(k) => Some(usize::from(k.span().len())),
        }
    }
}
impl Printable for Literal {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            Literal::String(s) => printer.print_raw_token(s),
            Literal::Integer(i) => printer.print_raw_token(i),
            Literal::Float(f) => printer.print_raw_token(f),
            Literal::Keyword(k) => printer.print_raw_token(k),
        }
        PrintInfo::default_single_line()
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            Literal::String(s) => s.leftmost_token(),
            Literal::Integer(i) => i.span(),
            Literal::Float(f) => f.span(),
            Literal::Keyword(k) => k.span(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            Literal::String(s) => s.rightmost_token(),
            Literal::Integer(i) => i.span(),
            Literal::Float(f) => f.span(),
            Literal::Keyword(k) => k.span(),
        }
    }
}
pub(crate) trait PathExprFormatExt {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    #[allow(clippy::unnecessary_wraps)]
    fn single_line_width(&self, _input: &Printer<'_>) -> Option<usize>;
}
impl PathExprFormatExt for PathExpr {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    #[allow(clippy::unnecessary_wraps)]
    fn single_line_width(&self, _input: &Printer<'_>) -> Option<usize> {
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
pub(crate) trait GenericApplyExprFormatExt {
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
}
impl GenericApplyExprFormatExt for GenericApplyExpr {
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        Some(self.base.single_line_width(input)? + self.generic_args.formatted_single_line_width())
    }
}
impl Printable for GenericApplyExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let info = self.base.print(shape.clone(), printer);
        printer.print(&self.generic_args, shape);
        info
    }
    fn leftmost_token(&self) -> TextRange {
        self.base.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.generic_args.close_angle.span()
    }
}
pub(crate) trait ParenExprMeasureExt {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
}
impl ParenExprMeasureExt for ParenExpr {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
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
pub(crate) trait ParenExprPrintExt {
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the parenthesized expression on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo>;
}
impl ParenExprPrintExt for ParenExpr {
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
pub(crate) trait BinaryExprMeasureExt {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
    /// Recursively lifts binary expressions in the same chaining group to the top level.
    /// For ops that are not in any chaining groups, return will be the same as the original.
    ///
    /// The vec will never be empty.
    fn get_chaining_members(&self) -> (&Expression, Vec<(&BinaryOp, &Expression)>);
}
impl BinaryExprMeasureExt for BinaryExpr {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let (left, right) = &*self.sides;
        let left_width = left.single_line_width(input)?;
        let right_width = right.single_line_width(input)?;
        let mut trivia_len = 0usize;
        let left_trailing = input.trivia.get_trailing_for_element(left);
        let (op_leading, op_trailing) = input.trivia.get_for_range_split(self.op.span());
        trivia_len += (op_leading.try_squished_len(input.input)?
            + left_trailing.try_squished_len(input.input)?)
        .max(const { " ".len() });
        let right_leading = input.trivia.get_leading_for_element(right);
        trivia_len += (right_leading.try_squished_len(input.input)?
            + op_trailing.try_squished_len(input.input)?)
        .max(const { " ".len() });
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
pub(crate) trait BinaryExprPrintExt {
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the binary expression on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo>;
}
impl BinaryExprPrintExt for BinaryExpr {
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
            printer.print_spaces(1);
        }
        printer.print(&self.op, Shape::unlimited_single_line());
        let mut right_trivia_len = printer.print_trivia_squished(op_trailing);
        right_trivia_len += printer.print_trivia_squished(right_leading);
        if right_trivia_len == 0 {
            printer.print_spaces(1);
        }
        if printer
            .print(right, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
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
pub(crate) trait IsExprFormatExt {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if the LHS can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
}
impl IsExprFormatExt for IsExpr {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if the LHS can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let lhs = self.lhs.single_line_width(input)?;
        let pat_left = self.pattern.leftmost_token().start();
        let pat_right = self.pattern.rightmost_token().end();
        let pattern_width = usize::from(pat_right - pat_left);
        Some(lhs + 1 + usize::from(self.keyword.span().len()) + 1 + pattern_width)
    }
}
impl Printable for IsExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
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
pub(crate) trait UnaryExprFormatExt {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
}
impl UnaryExprFormatExt for UnaryExpr {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
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
impl Printable for IfExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        let needs_parens = !matches!(*self.condition, Expression::Paren(_));
        let cond_shape = if needs_parens {
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
impl Printable for ElseExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            ElseExpr::If(if_expr) => if_expr.print(shape, printer),
            ElseExpr::IfLet(if_let_expr) => if_let_expr.print(shape, printer),
            ElseExpr::Block(block) => block.print(shape, printer),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            ElseExpr::If(if_expr) => if_expr.leftmost_token(),
            ElseExpr::IfLet(if_let_expr) => if_let_expr.leftmost_token(),
            ElseExpr::Block(block) => block.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            ElseExpr::If(if_expr) => if_expr.rightmost_token(),
            ElseExpr::IfLet(if_let_expr) => if_let_expr.rightmost_token(),
            ElseExpr::Block(block) => block.rightmost_token(),
        }
    }
}
impl Printable for IfLetExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        printer.print(&self.pattern, shape.clone());
        printer.print_str(" ");
        printer.print_raw_token(&self.equals);
        printer.print_str(" ");
        printer.print(&*self.scrutinee, shape.clone());
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
pub(crate) trait MatchExprFormatExt {
    fn try_print_scrutinee_single_line(
        &self,
        shape: &Shape,
        printer: &mut Printer,
    ) -> Option<PrintInfo>;
    fn print_scrutinee_multi_line(&self, shape: &Shape, printer: &mut Printer);
}
impl MatchExprFormatExt for MatchExpr {
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
        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        if printer
            .try_sub_printer(|p| self.try_print_scrutinee_single_line(&shape, p))
            .is_none()
        {
            self.print_scrutinee_multi_line(&shape, printer);
        }
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
pub(crate) trait MatchArmFormatExt {
    /// Prints all of the arm except the body/expression (prints up to and including the `=>`)
    fn print_condition(&self, shape: &Shape, printer: &mut Printer) -> PrintInfo;
}
impl MatchArmFormatExt for MatchArm {
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
                let mut guard_single_line = printer.sub_printer();
                let guard_info =
                    guard.print(Shape::unlimited_single_line(), &mut guard_single_line);
                let single_line_len = pattern_len
                    + const { " ".len() }
                    + guard_single_line.len()
                    + const { " =>".len() };
                if guard_info.multi_lined || single_line_len > shape.width {
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
                    printer.print_spaces(1);
                    printer.append_from_printer(guard_single_line);
                }
            }
        }
        printer.print_str(" =>");
        PrintInfo { multi_lined }
    }
}
/// Print an arm body that is being wrapped into a `{ ... }` block (the `{` and
/// newline are already emitted; the caller emits the closing `}`).
///
/// `arm_indent` is the arm's own indent; the body is printed one level deeper.
/// A braceless jump body (`return`/`break`/`continue`) additionally gets its
/// statement `;` - and its trailing trivia is deliberately left for the arm
/// level so a same-line comment stays attached to the arm (emitted after the
/// wrapped `},`) instead of being split from the `;` or dropped/duplicated when
/// the arm has no comma (B-629).
fn print_wrapped_arm_body(printer: &mut Printer, body: &Expression, arm_indent: usize) {
    let inner_indent = arm_indent + printer.config.indent_width;
    if matches!(
        body,
        Expression::Return(_) | Expression::Break(_) | Expression::Continue(_)
    ) {
        printer.print_standalone_leading_and_body(body, inner_indent);
        printer.print_str(";");
    } else {
        printer.print_standalone_with_trivia(body, inner_indent);
    }
}
impl Printable for MatchArm {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let condition_info = self.print_condition(&shape, printer);
        let condition_multi_lined = condition_info.multi_lined;
        if condition_multi_lined {
            printer.print_newline();
            printer.print_spaces(shape.indent);
            if let Expression::Block(block) = &self.body {
                let body_shape = Shape {
                    width: printer.config.line_width.saturating_sub(shape.indent),
                    indent: shape.indent,
                    first_line_offset: 0,
                };
                printer.print(block, body_shape);
                printer.print_str(",");
            } else {
                printer.print_str("{");
                printer.print_newline();
                print_wrapped_arm_body(printer, &self.body, shape.indent);
                printer.print_newline();
                printer.print_spaces(shape.indent);
                printer.print_str("},");
            }
            return PrintInfo::default_multi_lined();
        }
        printer.print_spaces(1);
        let line_len_remaining = printer.current_line_remaining_width();
        if let Expression::Block(block) = &self.body {
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
        let mut try_body = printer.sub_printer();
        let try_body_info = self
            .body
            .print(Shape::unlimited_single_line(), &mut try_body);
        if try_body_info.multi_lined || try_body.len() > line_len_remaining {
            printer.print_str("{");
            printer.print_newline();
            print_wrapped_arm_body(printer, &self.body, shape.indent);
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
impl Printable for CatchExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let base_info = printer.print(&*self.base, shape.clone());
        for clause in &self.clauses {
            printer.print_str(" ");
            printer.print(clause, shape.clone());
        }
        PrintInfo {
            multi_lined: base_info.multi_lined || !self.clauses.is_empty(),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        self.base.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.clauses
            .last()
            .map_or_else(|| self.base.rightmost_token(), CatchClause::rightmost_token)
    }
}
impl Printable for CatchBinding {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.name);
        PrintInfo::default_single_line()
    }
    fn leftmost_token(&self) -> TextRange {
        self.name.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.name.span()
    }
}
impl Printable for CatchClause {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_indent = shape.indent + printer.config.indent_width;
        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        printer.print_raw_token(&self.open_paren);
        printer.print(&self.binding, Shape::unlimited_single_line());
        if let Some((comma, stack_trace_binding)) = &self.stack_trace_binding {
            printer.print_raw_token(comma);
            printer.print_str(" ");
            printer.print(stack_trace_binding, Shape::unlimited_single_line());
        }
        printer.print_raw_token(&self.close_paren);
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
impl Printable for CatchArm {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print(&self.pattern, shape.clone());
        printer.print_str(" ");
        printer.print_raw_token(&self.fat_arrow);
        printer.print_str(" ");
        let line_len_remaining = printer.current_line_remaining_width();
        if let Expression::Block(block) = &self.body {
            let body_shape = Shape {
                width: line_len_remaining,
                indent: shape.indent,
                first_line_offset: printer
                    .config
                    .line_width
                    .saturating_sub(shape.indent + line_len_remaining),
            };
            let info = printer.print(block, body_shape);
            if self.comma.is_some() {
                printer.print_str(",");
            }
            return info;
        }
        let mut try_body = printer.sub_printer();
        let try_body_info = self
            .body
            .print(Shape::unlimited_single_line(), &mut try_body);
        if try_body_info.multi_lined || try_body.len() > line_len_remaining {
            printer.print_str("{");
            printer.print_newline();
            print_wrapped_arm_body(printer, &self.body, shape.indent);
            printer.print_newline();
            printer.print_spaces(shape.indent);
            printer.print_str("}");
            if self.comma.is_some() {
                printer.print_str(",");
            }
            PrintInfo::default_multi_lined()
        } else {
            printer.append_from_printer(try_body);
            if self.comma.is_some() {
                printer.print_str(",");
            }
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
pub(crate) trait CallExprFormatExt {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
}
impl CallExprFormatExt for CallExpr {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let callee = self.callee.single_line_width(input)?;
        let args = self.args.single_line_width(input)?;
        Some(callee + args)
    }
}
impl Printable for CallExpr {
    /// The main way to call this should be through [`PrintChain`]
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;
        let line_len_before = printer.current_line_len();
        multi_lined |= printer.print(&*self.callee, shape.clone()).multi_lined;
        let args_shape = Shape {
            first_line_offset: shape.first_line_offset
                + printer.current_line_len().saturating_sub(line_len_before),
            ..shape
        };
        multi_lined |= printer.print(&self.args, args_shape).multi_lined;
        PrintInfo { multi_lined }
    }
    fn leftmost_token(&self) -> TextRange {
        self.callee.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.args.rightmost_token()
    }
}
pub(crate) trait CallArgFormatExt {
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
}
impl CallArgFormatExt for CallArg {
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
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
pub(crate) trait CallArgsFormatExt {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the call args on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo>;
    /// Whether the hug layout (see [`Self::try_print_hug`]) applies: the last
    /// argument is block-terminal (a lambda or `spawn { ... }`).
    fn can_hug(&self) -> bool;
    /// Hug layout for a trailing block-terminal argument: everything up to
    /// the last argument prints on one line, the last argument's block opens
    /// on that same line and closes at the outer indent, immediately followed
    /// by the closing paren (no trailing comma).
    ///
    /// ```baml
    /// futures.push(spawn {
    ///     work(c)
    /// })
    /// ```
    ///
    /// Should be passed a sub-printer to avoid printing trivia in the outer
    /// printer in the event that the hug layout does not apply.
    fn try_print_hug(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo>;
}
impl CallArgsFormatExt for CallArgs {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
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
                    len += 1;
                    for t in comma_trailing {
                        len += t.single_line_len(input.input)?;
                    }
                } else {
                    len += 1;
                }
                len += 1;
            } else if let Some(comma) = comma {
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
    /// Whether the hug layout (see [`Self::try_print_hug`]) applies: the last
    /// argument is block-terminal (a lambda or `spawn { ... }`).
    fn can_hug(&self) -> bool {
        self.args
            .split_last()
            .is_some_and(|((arg, _), _)| arg.is_huggable())
    }
    /// Hug layout for a trailing block-terminal argument: everything up to
    /// the last argument prints on one line, the last argument's block opens
    /// on that same line and closes at the outer indent, immediately followed
    /// by the closing paren (no trailing comma).
    ///
    /// ```baml
    /// futures.push(spawn {
    ///     work(c)
    /// })
    /// ```
    ///
    /// Should be passed a sub-printer to avoid printing trivia in the outer
    /// printer in the event that the hug layout does not apply.
    fn try_print_hug(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        let ((last_arg, last_comma), init) = self.args.split_last()?;
        if !last_arg.is_huggable() {
            return None;
        }
        printer.print_raw_token(&self.open_paren);
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_paren.span());
        printer.try_print_trivia_single_line_squished(open_trailing)?;
        for (arg, comma) in init {
            let (arg_leading, arg_trailing) = printer.trivia.get_for_element(arg);
            printer.try_print_trivia_single_line_squished(arg_leading)?;
            if printer
                .print(arg, Shape::unlimited_single_line())
                .multi_lined
            {
                return None;
            }
            printer.try_print_trivia_single_line_squished(arg_trailing)?;
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
        }
        let (last_leading, last_trailing) = printer.trivia.get_for_element(last_arg);
        printer.try_print_trivia_single_line_squished(last_leading)?;
        let first_line_offset = shape.first_line_offset + printer.current_line_len();
        let arg_shape = Shape {
            width: printer
                .config
                .line_width
                .saturating_sub(shape.indent + first_line_offset),
            indent: shape.indent,
            first_line_offset,
        };
        printer.print(last_arg, arg_shape);
        printer.try_print_trivia_single_line_squished(last_trailing)?;
        if let Some(comma) = last_comma {
            let (comma_leading, comma_trailing) = printer.trivia.get_for_range_split(comma.span());
            printer.try_print_trivia_single_line_squished(comma_leading)?;
            printer.try_print_trivia_single_line_squished(comma_trailing)?;
        }
        let (close_leading, _) = printer.trivia.get_for_range_split(self.close_paren.span());
        printer.try_print_trivia_single_line_squished(close_leading)?;
        printer.print_raw_token(&self.close_paren);
        Some(PrintInfo::default_multi_lined())
    }
}
impl Printable for CallArgs {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .or_else(|| printer.try_sub_printer(|p| self.try_print_hug(&shape, p)))
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
pub(crate) trait IndexExprMeasureExt {
    fn args(&self) -> IndexArgs<'_>;
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
}
impl IndexExprMeasureExt for IndexExpr {
    fn args(&self) -> IndexArgs<'_> {
        IndexArgs {
            open_bracket: &self.open_bracket,
            index: &self.index,
            close_bracket: &self.close_bracket,
        }
    }
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
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
pub(crate) trait IndexExprPrintExt {
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo>;
}
impl IndexExprPrintExt for IndexExpr {
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
pub(crate) trait FieldAccessExprFormatExt {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
}
impl FieldAccessExprFormatExt for FieldAccessExpr {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let base = self.base.single_line_width(input)?;
        Some(base + usize::from(self.dot.span().len()) + usize::from(self.field.span().len()))
    }
}
pub(crate) trait OptionalFieldAccessExprFormatExt {
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
}
impl OptionalFieldAccessExprFormatExt for OptionalFieldAccessExpr {
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let base = self.base.single_line_width(input)?;
        Some(
            base + usize::from(self.question_dot.span().len())
                + usize::from(self.field.span().len()),
        )
    }
}
pub(crate) trait OptionalIndexExprFormatExt {
    fn args(&self) -> IndexArgs<'_>;
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
}
impl OptionalIndexExprFormatExt for OptionalIndexExpr {
    fn args(&self) -> IndexArgs<'_> {
        IndexArgs {
            open_bracket: &self.open_bracket,
            index: &self.index,
            close_bracket: &self.close_bracket,
        }
    }
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
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
pub(crate) trait OptionalCallExprFormatExt {
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
}
impl OptionalCallExprFormatExt for OptionalCallExpr {
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
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
pub(crate) trait EnvAccessExprFormatExt {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    #[allow(clippy::unnecessary_wraps)]
    fn single_line_width(&self, _input: &Printer<'_>) -> Option<usize>;
}
impl EnvAccessExprFormatExt for EnvAccessExpr {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    #[allow(clippy::unnecessary_wraps)]
    fn single_line_width(&self, _input: &Printer<'_>) -> Option<usize> {
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
impl Printable for BlockExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        if self.stmts.is_empty() && self.expr.is_none() {
            let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_brace.span());
            let (close_leading, _) = printer.trivia.get_for_range_split(self.close_brace.span());
            if !open_trailing.iter().any(EmittableTrivia::is_comment)
                && !close_leading.iter().any(EmittableTrivia::is_comment)
            {
                printer.print_raw_token(&self.open_brace);
                printer.print_raw_token(&self.close_brace);
                return PrintInfo::default_single_line();
            }
        }
        printer.print_raw_token(&self.open_brace);
        printer.print_trivia_all_trailing_for(self.open_brace.span());
        printer.print_newline();
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
pub(crate) trait ArrayInitializerFormatExt {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
    /// Tries to print the array initializer as a single line.
    ///
    /// If successful, returns the info.
    ///
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the array initializer on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo>;
}
impl ArrayInitializerFormatExt for ArrayInitializer {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
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
                len += el_trailing.squished_len(input.input);
                len += comma_leading.squished_len(input.input);
                if !is_last {
                    len += const { ", ".len() };
                }
                len += comma_trailing.try_squished_len(input.input)?;
            } else {
                len += el_trailing.try_squished_len(input.input)?;
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
                printer.print_trivia_squished(el_trailing);
                printer.print_trivia_squished(comma_leading);
                if !is_last {
                    printer.print_str(", ");
                }
                printer.try_print_trivia_single_line_squished(comma_trailing)?;
            } else {
                printer.try_print_trivia_single_line_squished(el_trailing)?;
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
pub(crate) trait ObjectInitializerFormatExt {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
    /// Tries to print the object initializer as a single line.
    ///
    /// If successful, returns the info.
    ///
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the object initializer on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo>;
}
impl ObjectInitializerFormatExt for ObjectInitializer {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
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
                    len += 1;
                    len += comma_trailing.try_squished_len(input.input)?;
                } else {
                    len += 1;
                }
                len += 1;
            } else if let Some(comma) = comma {
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
pub(crate) trait MapLiteralFormatExt {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the map literal on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo>;
}
impl MapLiteralFormatExt for MapLiteral {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let (_, open_trailing) = input.trivia.get_for_range_split(self.open_brace.span());
        let (close_leading, _) = input.trivia.get_for_range_split(self.close_brace.span());
        let has_content = !self.fields.is_empty()
            || open_trailing.iter().any(EmittableTrivia::is_comment)
            || close_leading.iter().any(EmittableTrivia::is_comment);
        let mut len = if has_content {
            const { "{  }".len() }
        } else {
            const { "{}".len() }
        };
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
                    len += 1;
                    for t in comma_trailing {
                        len += t.single_line_len(input.input)?;
                    }
                } else {
                    len += 1;
                }
                len += 1;
            } else if let Some(comma) = comma {
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
        for t in close_leading {
            len += t.single_line_len(input.input)?;
        }
        Some(len)
    }
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the map literal on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_brace.span());
        let (close_leading, _) = printer.trivia.get_for_range_split(self.close_brace.span());
        let has_content = !self.fields.is_empty()
            || open_trailing.iter().any(EmittableTrivia::is_comment)
            || close_leading.iter().any(EmittableTrivia::is_comment);
        printer.print_raw_token(&self.open_brace);
        if has_content {
            printer.print_str(" ");
        }
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
                let (comma_leading, comma_trailing) =
                    printer.trivia.get_for_range_split(comma.span());
                printer.try_print_trivia_single_line_squished(comma_leading)?;
                printer.try_print_trivia_single_line_squished(comma_trailing)?;
            }
        }
        printer.try_print_trivia_single_line_squished(close_leading)?;
        if has_content {
            printer.print_str(" ");
        }
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
pub(crate) trait ObjectFieldFormatExt {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
}
impl ObjectFieldFormatExt for ObjectField {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let name = self.name.single_line_width(input)?;
        let (Some(colon), Some(value)) = (&self.colon, &self.value) else {
            return Some(name);
        };
        let value_width = value.single_line_width(input)?;
        let mut trivia_len = 0usize;
        let (_, colon_trailing) = input.trivia.get_for_range_split(colon.span());
        for t in colon_trailing {
            trivia_len += t.single_line_len(input.input)?;
        }
        let value_leading = input.trivia.get_leading_for_element(value);
        for t in value_leading {
            trivia_len += t.single_line_len(input.input)?;
        }
        Some(name + const { ": ".len() } + value_width + trivia_len)
    }
}
impl Printable for ObjectField {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;
        multi_lined |= printer.print(&self.name, shape.clone()).multi_lined;
        let (Some(colon), Some(value)) = (&self.colon, &self.value) else {
            return PrintInfo { multi_lined };
        };
        printer.print_raw_token(colon);
        let (_, colon_trailing) = printer.trivia.get_for_range_split(colon.span());
        printer.print_str(" ");
        printer.print_trivia_squished(colon_trailing);
        let value_leading = printer.trivia.get_leading_for_element(value);
        printer.print_trivia_squished(value_leading);
        multi_lined |= printer.print(value, shape).multi_lined;
        PrintInfo { multi_lined }
    }
    fn leftmost_token(&self) -> TextRange {
        self.name.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.value
            .as_ref()
            .map(Printable::rightmost_token)
            .unwrap_or_else(|| self.name.rightmost_token())
    }
}
pub(crate) trait ObjectFieldKeyFormatExt {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
}
impl ObjectFieldKeyFormatExt for ObjectFieldKey {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
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
impl Printable for GenericParamList {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.open_angle);
        for (i, param) in self.params.iter().enumerate() {
            printer.print_raw_token(&param.name);
            if let Some(bounds) = &param.bounds {
                printer.print_str(" ");
                printer.print(bounds, Shape::unlimited_single_line());
            }
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
impl Printable for GenericParamBounds {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.extends);
        for (idx, (bound, _and)) in self.bounds.iter().enumerate() {
            if idx == 0 {
                printer.print_str(" ");
            } else {
                printer.print_str(" & ");
            }
            printer.print(bound, Shape::unlimited_single_line());
        }
        PrintInfo::default_single_line()
    }
    fn leftmost_token(&self) -> TextRange {
        self.extends.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.bounds
            .last()
            .map(|(bound, _)| bound.rightmost_token())
            .unwrap_or_else(|| self.extends.span())
    }
}
pub(crate) trait GenericArgsFormatExt {
    /// Width that the formatter would emit on a single line, ignoring any
    /// internal trivia in the source. Used by single-line-width estimators
    /// upstream to decide whether a host expression fits on one line.
    ///
    /// Format is `<T1, T2, T3>`: 2 chars for `<>`, plus each type argument's
    /// source-text width, plus `, ` (2 chars) between arguments. Source
    /// types may contain whitespace, but for typical cases this is a tight
    /// upper bound and tracks what the printer actually emits.
    fn formatted_single_line_width(&self) -> usize;
}
impl GenericArgsFormatExt for GenericArgs {
    /// Width that the formatter would emit on a single line, ignoring any
    /// internal trivia in the source. Used by single-line-width estimators
    /// upstream to decide whether a host expression fits on one line.
    ///
    /// Format is `<T1, T2, T3>`: 2 chars for `<>`, plus each type argument's
    /// source-text width, plus `, ` (2 chars) between arguments. Source
    /// types may contain whitespace, but for typical cases this is a tight
    /// upper bound and tracks what the printer actually emits.
    fn formatted_single_line_width(&self) -> usize {
        let mut len: usize = 2;
        for (i, (ty, _)) in self.args.iter().enumerate() {
            let arg_span = ty.rightmost_token().end() - ty.leftmost_token().start();
            len += usize::from(arg_span);
            if i + 1 < self.args.len() {
                len += 2;
            }
        }
        len
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
impl Printable for LambdaExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        if let Some(ref gp) = self.generic_params {
            printer.print(gp, shape.clone());
        }
        printer.print(&self.param_list, shape.clone());
        printer.print_str(" ->");
        if let Some(ref ret) = self.return_type {
            printer.print_str(" ");
            printer.print(ret, shape.clone());
        }
        if let Some(ref throws) = self.throws {
            printer.print_str(" ");
            printer.print(throws, shape.clone());
        }
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
pub(crate) trait SpawnExprFormatExt {
    /// Source range for the spawn header, excluding the body block's opening
    /// brace. Keeping a commented header verbatim avoids dropping trivia that
    /// sits between the keyword, optional name, `with` options, and commas.
    fn header_range(&self) -> TextRange;
    /// Header comments are deliberately kept verbatim. The structured header
    /// layout canonicalizes whitespace and commas, but does not otherwise have
    /// enough information to place a line comment without changing its line.
    /// The trivia classifier catches both line and block comments here.
    fn header_requires_verbatim(&self, input: &Printer<'_>) -> bool;
    /// Width of the header (`spawn`, optional name, optional `with` clause)
    /// without the body block. `None` if any part can never be single-lined.
    fn header_single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
    /// Returns the width of the expression if it fits on a single line -
    /// a simple body (`{}` or `{ tail }`) and a single-lineable header.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
    /// Prints the header: `spawn`, then the optional name and `with` clause.
    /// Returns whether any part spilled onto multiple lines.
    fn print_header(&self, shape: &Shape, printer: &mut Printer) -> bool;
    /// Single-line layout: `spawn name? (with opts)? {}` or `... { tail }`.
    /// Only possible when the body has no statements.
    ///
    /// Should be passed a sub-printer to avoid printing trivia in the outer
    /// printer in the event that the expression cannot fit on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo>;
}
impl SpawnExprFormatExt for SpawnExpr {
    /// Source range for the spawn header, excluding the body block's opening
    /// brace. Keeping a commented header verbatim avoids dropping trivia that
    /// sits between the keyword, optional name, `with` options, and commas.
    fn header_range(&self) -> TextRange {
        TextRange::new(
            self.keyword.span().start(),
            self.block.open_brace.span().start(),
        )
    }
    /// Header comments are deliberately kept verbatim. The structured header
    /// layout canonicalizes whitespace and commas, but does not otherwise have
    /// enough information to place a line comment without changing its line.
    /// The trivia classifier catches both line and block comments here.
    fn header_requires_verbatim(&self, input: &Printer<'_>) -> bool {
        let header_start = self.keyword.span().start();
        let block_start = self.block.open_brace.span().start();
        input.trivia.all_trivia().iter().any(|trivia| {
            let attached_at = trivia.attached_to().start();
            trivia.is_comment() && attached_at >= header_start && attached_at <= block_start
        })
    }
    /// Width of the header (`spawn`, optional name, optional `with` clause)
    /// without the body block. `None` if any part can never be single-lined.
    fn header_single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        if self.header_requires_verbatim(input) {
            return None;
        }
        let mut len = usize::from(self.keyword.span().len());
        if let Some(name) = &self.name {
            len += const { " ".len() } + name.single_line_width(input)?;
        }
        if let Some((with_kw, options)) = &self.with_clause {
            len += const { " ".len() } + usize::from(with_kw.span().len());
            for (i, (expr, _)) in options.iter().enumerate() {
                len += if i == 0 {
                    const { " ".len() }
                } else {
                    const { ", ".len() }
                };
                len += expr.single_line_width(input)?;
            }
        }
        Some(len)
    }
    /// Returns the width of the expression if it fits on a single line -
    /// a simple body (`{}` or `{ tail }`) and a single-lineable header.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        if !self.block.stmts.is_empty() {
            return None;
        }
        let header = self.header_single_line_width(input)?;
        let (_, open_trailing) = input
            .trivia
            .get_for_range_split(self.block.open_brace.span());
        let (close_leading, _) = input
            .trivia
            .get_for_range_split(self.block.close_brace.span());
        let body = match self.block.expr.as_deref() {
            Some(tail) => {
                let (tail_leading, tail_trailing) = input.trivia.get_for_element(tail);
                (const { " {  }".len() })
                    + open_trailing.try_squished_len(input.input)?
                    + tail_leading.try_squished_len(input.input)?
                    + tail.single_line_width(input)?
                    + tail_trailing.try_squished_len(input.input)?
                    + close_leading.try_squished_len(input.input)?
            }
            None => {
                if open_trailing.iter().any(EmittableTrivia::is_comment)
                    || close_leading.iter().any(EmittableTrivia::is_comment)
                {
                    return None;
                }
                const { " {}".len() }
            }
        };
        Some(header + body)
    }
    /// Prints the header: `spawn`, then the optional name and `with` clause.
    /// Returns whether any part spilled onto multiple lines.
    fn print_header(&self, shape: &Shape, printer: &mut Printer) -> bool {
        let mut multi_lined = false;
        printer.print_raw_token(&self.keyword);
        if let Some(name) = &self.name {
            printer.print_str(" ");
            multi_lined |= printer.print(name, shape.clone()).multi_lined;
        }
        if let Some((with_kw, options)) = &self.with_clause {
            printer.print_str(" ");
            printer.print_raw_token(with_kw);
            for (i, (expr, _)) in options.iter().enumerate() {
                printer.print_str(if i == 0 { " " } else { ", " });
                multi_lined |= printer.print(expr, shape.clone()).multi_lined;
            }
        }
        multi_lined
    }
    /// Single-line layout: `spawn name? (with opts)? {}` or `... { tail }`.
    /// Only possible when the body has no statements.
    ///
    /// Should be passed a sub-printer to avoid printing trivia in the outer
    /// printer in the event that the expression cannot fit on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        if !self.block.stmts.is_empty() || self.header_requires_verbatim(printer) {
            return None;
        }
        if self.print_header(&Shape::unlimited_single_line(), printer) {
            return None;
        }
        printer.print_str(" ");
        let (_, open_trailing) = printer
            .trivia
            .get_for_range_split(self.block.open_brace.span());
        let (close_leading, _) = printer
            .trivia
            .get_for_range_split(self.block.close_brace.span());
        match self.block.expr.as_deref() {
            Some(tail) => {
                printer.print_raw_token(&self.block.open_brace);
                printer.print_str(" ");
                printer.try_print_trivia_single_line_squished(open_trailing)?;
                let (tail_leading, tail_trailing) = printer.trivia.get_for_element(tail);
                printer.try_print_trivia_single_line_squished(tail_leading)?;
                if printer
                    .print(tail, Shape::unlimited_single_line())
                    .multi_lined
                {
                    return None;
                }
                printer.try_print_trivia_single_line_squished(tail_trailing)?;
                printer.try_print_trivia_single_line_squished(close_leading)?;
                printer.print_str(" ");
                printer.print_raw_token(&self.block.close_brace);
            }
            None => {
                if open_trailing.iter().any(EmittableTrivia::is_comment)
                    || close_leading.iter().any(EmittableTrivia::is_comment)
                {
                    return None;
                }
                printer.print_raw_token(&self.block.open_brace);
                printer.print_raw_token(&self.block.close_brace);
            }
        }
        if printer.output.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
    }
}
impl PrintMultiLine for SpawnExpr {
    /// Multi-line layout: the header stays on the current line and the block
    /// opens right after it, closing at the outer indent.
    ///
    /// ```baml
    /// spawn with baml.spawn.options(group = g) {
    ///     compute()
    /// }
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        if self.header_requires_verbatim(printer) {
            printer.print_input_range(self.header_range());
        } else {
            self.print_header(&shape, printer);
            printer.print_str(" ");
        }
        printer.print(&self.block, shape);
        PrintInfo::default_multi_lined()
    }
}
impl Printable for SpawnExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.block.rightmost_token()
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
    /// Prints `first` followed by `members` in single-line form. Returns
    /// `None` if any element refuses to single-line. The final total-width
    /// check is left to the caller.
    fn try_print_members_single_line(
        &self,
        members: &[PrintChainItem<'_>],
        shape: &Shape,
        printer: &mut Printer,
    ) -> Option<()> {
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
        for item in members {
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
        Some(())
    }
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        self.try_print_members_single_line(&self.chain_members, shape, printer)?;
        if printer.output.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
    }
    /// Hug layout: the whole chain prints on one line except the final call,
    /// whose trailing block-terminal argument hugs the parens (see
    /// [`CallArgs::try_print_hug`]).
    ///
    /// ```baml
    /// futures.push(spawn {
    ///     work(c)
    /// });
    /// ```
    ///
    /// Should be passed a sub-printer to avoid printing partial output in the
    /// event that the hug layout does not apply.
    fn try_print_hug(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        let (last, prefix) = self.chain_members.split_last()?;
        let (question_dot, call_args) = match last {
            PrintChainItem::Call(args) => (None, *args),
            PrintChainItem::OptionalCall(qd, args) => (Some(*qd), *args),
            _ => return None,
        };
        if !call_args.can_hug() {
            return None;
        }
        self.try_print_members_single_line(prefix, shape, printer)?;
        if printer.output.len() > shape.width {
            return None;
        }
        if let Some(qd) = question_dot {
            printer.print_raw_token(qd);
        }
        let hug_shape = Shape {
            width: shape.width,
            indent: shape.indent,
            first_line_offset: shape.first_line_offset,
        };
        call_args.try_print_hug(&hug_shape, printer)
    }
}
impl Printable for PrintChain<'_> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .or_else(|| printer.try_sub_printer(|p| self.try_print_hug(&shape, p)))
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
