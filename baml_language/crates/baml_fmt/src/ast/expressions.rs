//! Reference: [`baml_db::baml_compiler_syntax::ast::Expr`] and [`baml_db::baml_compiler_hir::body`]

use baml_db::baml_compiler_syntax::{
    FromCST, ast as raw_ast,
    validated::{
        Validated, ValidatedExprNode, ValidatedSyntaxToken,
        nodes::{
            ArmListItem, ArrayInitializer, BinaryExpr, BlockExpr, CallArg, CallArgs, CallExpr,
            CatchArm, CatchBinding, CatchClause, CatchExpr, ElseExpr, EnvAccessExpr, Expression,
            FieldAccessExpr, FunctionArrow, GenericApplyExpr, GenericArg, GenericArgs,
            GenericParamBounds, GenericParamList, IfExpr, IfLetExpr, IndexExpr, IsExpr, LambdaExpr,
            MapLiteral, MatchArm, MatchExpr, MatchGuard, ObjectField, ObjectFieldKey,
            ObjectInitializer, ObjectMember, OptionalCallExpr, OptionalFieldAccessExpr,
            OptionalIndexExpr, ParenExpr, PathExpr, SpawnExpr, SpreadElement, UnaryExpr,
            UnreflectArg,
        },
    },
};
use rowan::TextRange;

use crate::{
    ast::{BinaryOp, Literal, Token, tokens as t},
    printer::{PrintInfo, PrintMultiLine, Printable, Printer, Shape},
    trivia_classifier::{EmittableTrivia, TriviaInfo, TriviaSliceExt},
};

trait GenericArgsFormatting {
    fn formatted_single_line_width(&self) -> usize;
}

impl GenericArgsFormatting for GenericArgs {
    fn formatted_single_line_width(&self) -> usize {
        let mut len = 2;
        for (index, (argument, _)) in self.args.iter().enumerate() {
            let (left, right) = match argument {
                GenericArg::Type(ty) => (ty.leftmost_token(), ty.rightmost_token()),
                GenericArg::Unreflect(argument) => {
                    (argument.leftmost_token(), argument.rightmost_token())
                }
            };
            len += usize::from(right.end() - left.start());
            if index + 1 < self.args.len() {
                len += 2;
            }
        }
        len
    }
}

trait ExpressionWidth {
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
}

trait LiteralFormatting {
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
}

trait PathExprWidth {
    fn single_line_width(&self, _input: &Printer<'_>) -> Option<usize>;
}

trait GenericApplyExprWidth {
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
}

trait ParenExprAnalysis {
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
    fn is_transparent(&self, trivia: &TriviaInfo) -> bool;
}

trait ParenExprSingleLine {
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo>;
}

trait ExpressionPrecedence {
    fn peel_transparent_parens(&self, trivia: &TriviaInfo) -> &Expression;
    fn binds_as_postfix_operand(&self) -> bool;
    fn has_optional_chain_link(&self) -> bool;
    fn effective_postfix_operand(&self, trivia: &TriviaInfo) -> &Expression;
    fn effective_unary_operand(&self, trivia: &TriviaInfo) -> &Expression;
    fn peel_to_needed_paren(&self, trivia: &TriviaInfo, unary: bool) -> &Expression;
}

trait BinaryExprLayout {
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
    fn effective_left(&self, trivia: &TriviaInfo) -> &Expression;
    fn get_chaining_members(
        &self,
        trivia: &TriviaInfo,
    ) -> (&Expression, Vec<(&BinaryOp, &Expression)>);
}

trait BinaryExprSingleLine {
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo>;
}

trait IsExprWidth {
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
}

trait UnaryExprWidth {
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
}

trait MatchExprScrutineeLayout {
    fn try_print_scrutinee_single_line(
        &self,
        shape: &Shape,
        printer: &mut Printer,
    ) -> Option<PrintInfo>;
    fn print_scrutinee_multi_line(&self, shape: &Shape, printer: &mut Printer);
}

trait MatchArmConditionLayout {
    fn print_condition(&self, shape: &Shape, printer: &mut Printer) -> PrintInfo;
}

trait CallExprWidth {
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
}

trait CallArgLayout {
    fn is_huggable(&self) -> bool;
    fn effective_expr(&self, trivia: &TriviaInfo) -> &Expression;
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
}

trait CallArgsLayout {
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo>;
    fn can_hug(&self) -> bool;
    fn try_print_hug(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo>;
}

trait IndexExprLayout {
    fn args(&self) -> IndexArgs<'_>;
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
}

trait IndexExprSingleLine {
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo>;
}

trait FieldAccessExprWidth {
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
}

trait OptionalFieldAccessExprWidth {
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
}

trait OptionalIndexExprLayout {
    fn args(&self) -> IndexArgs<'_>;
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
}

trait OptionalCallExprWidth {
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
}

trait EnvAccessExprWidth {
    fn single_line_width(&self, _input: &Printer<'_>) -> Option<usize>;
}

trait ArrayInitializerLayout {
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo>;
}

trait ObjectInitializerLayout {
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo>;
}

trait MapLiteralLayout {
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo>;
}

trait ObjectFieldWidth {
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
}

trait ObjectMemberWidth {
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
}

trait SpreadElementWidth {
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
}

trait ObjectFieldKeyWidth {
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
}

pub(crate) trait FunctionArrowLayout {
    fn arrow_span(&self) -> TextRange;
    fn print_separator_before(
        &self,
        next_leftmost: Option<TextRange>,
        continuation_indent: usize,
        printer: &mut Printer,
    );
}

trait SpawnExprLayout {
    fn header_range(&self) -> TextRange;
    fn header_requires_verbatim(&self, input: &Printer<'_>) -> bool;
    fn header_single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize>;
    fn print_header(&self, shape: &Shape, printer: &mut Printer) -> bool;
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo>;
}

impl Printable for Validated<'_, raw_ast::ExprNode> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self.as_variant() {
            ValidatedExprNode::LiteralExpr(literal) => {
                let token = literal
                    .direct_elements()
                    .find_map(|element| element.token())
                    .expect("validated literal token");
                printer.print_raw_token(&token);
                PrintInfo::default_single_line()
            }
            ValidatedExprNode::StringLiteral(string) => {
                let token = t::QuotedString::new_from_span(string.text_range());
                token.print(shape, printer)
            }
            ValidatedExprNode::RawStringLiteral(string) => {
                let token = t::RawString::from_cst(string.syntax().clone().into())
                    .expect("validated raw string");
                token.print(shape, printer)
            }
            ValidatedExprNode::BacktickStringLiteral(string) => {
                let token = t::BacktickString::from_cst(string.syntax().clone().into())
                    .expect("validated backtick string");
                token.print(shape, printer)
            }
            ValidatedExprNode::ByteStringLiteral(string) => {
                let token = t::ByteString::from_cst(string.syntax().clone().into())
                    .expect("validated byte string");
                token.print(shape, printer)
            }
            _ => {
                let expression = Expression::from_cst(self.syntax().clone().into())
                    .expect("validated expression must convert during migration");
                expression.print(shape, printer)
            }
        }
    }

    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

impl ExpressionWidth for Expression {
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
                // Unmodeled nodes (e.g. `await f`, `x.as<T>`,
                // `throw e`) print their source verbatim (see `print`). When that
                // text is a single line it occupies a known width and can sit
                // inline like any other fitting expression. Reporting `None` here
                // used to force every *enclosing* expression to wrap even when the
                // whole thing fit the width budget (B-231).
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
                // These are all chains of postfix expressions
                let chain = PrintChain::new(chain, printer.trivia);
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
            // Print the raw `return …` / `break` / `continue` text. The arm
            // printers add the `;` when they wrap this into a block (see
            // `CatchArm`/`MatchArm`). A braceless jump only appears as a whole
            // arm value, never nested inside another expression, so it always
            // reports multi-lined.
            Expression::Return(jump) | Expression::Break(jump) | Expression::Continue(jump) => {
                printer.print_input_range_trimmed_start(jump.content_range());
                PrintInfo::default_multi_lined()
            }
            // Unmodeled nodes print their source verbatim. Report `multi_lined`
            // honestly from whether that text spans multiple lines: a single-line
            // unknown node (`await f`, `x.as<T>`, …) must not claim to be
            // multi-line, or it force-wraps its parents even when everything fits
            // on one line (B-231).
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
            | Expression::Unknown(span) => span.first_token,
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
            | Expression::Unknown(span) => span.last_token,
        }
    }
}

impl LiteralFormatting for Literal {
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        match self {
            Literal::String(s) => {
                if input.input[s.span()].contains('\n') {
                    None
                } else {
                    Some(usize::from(s.span().len()))
                }
            }
            Literal::Bigint(i) => Some(usize::from(i.span().len())),
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
            Literal::Bigint(i) => printer.print_raw_token(i),
            Literal::Integer(i) => printer.print_raw_token(i),
            Literal::Float(f) => printer.print_raw_token(f),
            Literal::Keyword(k) => printer.print_raw_token(k),
        }
        PrintInfo::default_single_line()
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            Literal::String(s) => s.leftmost_token(),
            Literal::Bigint(i) => i.span(),
            Literal::Integer(i) => i.span(),
            Literal::Float(f) => f.span(),
            Literal::Keyword(k) => k.span(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            Literal::String(s) => s.rightmost_token(),
            Literal::Bigint(i) => i.span(),
            Literal::Integer(i) => i.span(),
            Literal::Float(f) => f.span(),
            Literal::Keyword(k) => k.span(),
        }
    }
}

impl PathExprWidth for PathExpr {
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

impl GenericApplyExprWidth for GenericApplyExpr {
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

impl ParenExprAnalysis for ParenExpr {
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

    /// Whether no comments are attached to either paren token or to the inner
    /// expression's boundary. Peeling a transparent paren cannot lose trivia:
    /// every span a parent context queries around it is empty.
    fn is_transparent(&self, trivia: &TriviaInfo) -> bool {
        let (open_leading, open_trailing) = trivia.get_for_range_split(self.open_paren.span());
        let (close_leading, close_trailing) = trivia.get_for_range_split(self.close_paren.span());
        let (expr_leading, expr_trailing) = trivia.get_for_element(&*self.expr);
        open_leading.is_empty()
            && open_trailing.is_empty()
            && close_leading.is_empty()
            && close_trailing.is_empty()
            && expr_leading.is_empty()
            && expr_trailing.is_empty()
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

impl ParenExprSingleLine for ParenExpr {
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

impl ExpressionPrecedence for Expression {
    /// Strips nested [`ParenExpr`] wrappers that are transparent (no comments
    /// attached to the parens or the inner expression's boundary), returning
    /// the innermost expression. Callers decide per context whether printing
    /// the peeled expression instead of `self` is safe.
    fn peel_transparent_parens(&self, trivia: &TriviaInfo) -> &Expression {
        let mut expr = self;
        while let Expression::Paren(paren) = expr {
            if !paren.is_transparent(trivia) {
                break;
            }
            expr = &paren.expr;
        }
        expr
    }

    /// Whether this expression binds at least as tightly as a postfix
    /// operator, i.e. it can sit directly in a receiver position (`X.f`,
    /// `X(..)`, `X[i]`) or as a unary operand with no parens around it.
    ///
    /// Numeric and keyword literals are excluded: the `.` in `(1).to_string()`
    /// re-lexes as part of a float once the parens come off. Object and map
    /// literals are excluded because a bare leading `{` is ambiguous with a
    /// block, and [`Expression::GenericApply`] because its `<` is ambiguous
    /// with a comparison.
    ///
    /// An optional-chain link anywhere on the spine also disqualifies it —
    /// see [`Self::has_optional_chain_link`].
    fn binds_as_postfix_operand(&self) -> bool {
        match self {
            Expression::Call(_) | Expression::Index(_) | Expression::FieldAccess(_) => {
                !self.has_optional_chain_link()
            }
            Expression::Path(_)
            | Expression::EnvAccess(_)
            | Expression::ArrayInitializer(_)
            | Expression::RawString(_)
            | Expression::BacktickString(_)
            | Expression::ByteString(_) => true,
            Expression::Literal(lit) => matches!(lit, Literal::String(_)),
            _ => false,
        }
    }

    /// Whether this expression's postfix spine contains a `?.` link.
    ///
    /// Parens around such a receiver are load-bearing, not decoration: they
    /// **end the short-circuit region**. `(a?.b).c` evaluates `(null).c` when
    /// `a` is null — a `TypeError` — where `a?.b.c` short-circuits to null. So
    /// peeling them would silently change runtime behavior and these parens
    /// always stay.
    ///
    /// Only the spine counts. A `?.` inside a call argument or index operand
    /// (`f(a?.b).c`) belongs to a separate chain and is unaffected.
    fn has_optional_chain_link(&self) -> bool {
        match self {
            Expression::OptionalFieldAccess(_)
            | Expression::OptionalIndex(_)
            | Expression::OptionalCall(_) => true,
            Expression::Call(call) => call.callee.has_optional_chain_link(),
            Expression::Index(index) => index.base.has_optional_chain_link(),
            Expression::FieldAccess(fa) => fa.base.has_optional_chain_link(),
            Expression::Paren(paren) => paren.expr.has_optional_chain_link(),
            _ => false,
        }
    }

    /// The expression a postfix-receiver or unary-operand position actually
    /// prints: transparent parens peel while what they wrap still stands on
    /// its own here, so the parens delimit nothing.
    /// `((xs).join(` `)).includes(x)` prints as `xs.join(` `).includes(x)`.
    ///
    /// A receiver that binds looser than postfix keeps *one* paren — removing
    /// it would re-parse against a different base (`(a ?? b).length()`) — but
    /// the redundant layers around it still peel, so `((a + b)).f()` prints as
    /// `(a + b).f()` rather than keeping the whole stack.
    fn effective_postfix_operand(&self, trivia: &TriviaInfo) -> &Expression {
        self.peel_to_needed_paren(trivia, false)
    }

    /// [`Self::effective_postfix_operand`] for a unary operand.
    ///
    /// Identical except that literals peel here. The literal restriction exists
    /// only to keep `(1).to_string()` from re-lexing its `.` into a float, and
    /// no `.` follows a unary operand — so `-((1))` prints as `-1` and
    /// `!((true))` as `!true`. A literal that *is* a postfix receiver
    /// (`-(1).to_string()`) sits in the receiver position, not this one, and
    /// still keeps its parens.
    fn effective_unary_operand(&self, trivia: &TriviaInfo) -> &Expression {
        self.peel_to_needed_paren(trivia, true)
    }

    fn peel_to_needed_paren(&self, trivia: &TriviaInfo, unary: bool) -> &Expression {
        let mut expr = self;
        while let Expression::Paren(paren) = expr {
            if !paren.is_transparent(trivia) {
                break;
            }
            // Peel only down to the last paren this position still needs: an
            // inner paren is reconsidered on the next turn, so a stack around
            // a looser-binding receiver collapses to exactly one.
            let stands_alone = paren.expr.binds_as_postfix_operand()
                || matches!(&*paren.expr, Expression::Paren(_))
                || (unary && matches!(&*paren.expr, Expression::Literal(_)));
            if !stands_alone {
                break;
            }
            expr = &paren.expr;
        }
        expr
    }
}

impl BinaryExprLayout for BinaryExpr {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let left = self.effective_left(input.trivia);
        let right = &self.sides.1;
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

    /// The left operand with redundant parens peeled.
    ///
    /// `(a && b) && c` and `a && b && c` parse to different trees but mean
    /// the same thing and print identically, so a transparent paren around
    /// the left operand is dropped when the inner operator sits in the same
    /// precedence row as this one (reparsing the output yields the printed
    /// tree, keeping the formatter idempotent). Right operands are never
    /// peeled: removing those parens would re-associate, as in `a - (b - c)`.
    /// Mixed-precedence parens like `(a * b) + c` are kept: they are
    /// redundant to the parser but carry clarity for the reader.
    fn effective_left(&self, trivia: &TriviaInfo) -> &Expression {
        let Some(row) = BinaryOpPrecedenceRow::row_for_op(&self.op) else {
            return &self.sides.0;
        };
        let peeled = self.sides.0.peel_transparent_parens(trivia);
        match peeled {
            Expression::Binary(inner)
                if BinaryOpPrecedenceRow::row_for_op(&inner.op) == Some(row) =>
            {
                peeled
            }
            _ => &self.sides.0,
        }
    }

    /// Recursively lifts binary expressions in the same chaining group to the top level.
    /// For ops that are not in any chaining groups, return will be the same as the original.
    /// Redundant parens around left operands are peeled (see [`Self::effective_left`])
    /// so a fully parenthesized chain flattens like an unparenthesized one.
    ///
    /// The vec will never be empty.
    fn get_chaining_members(
        &self,
        trivia: &TriviaInfo,
    ) -> (&Expression, Vec<(&BinaryOp, &Expression)>) {
        let mut members = Vec::new();
        let Some(chaining_group) = BinaryOpChainingGroup::group_for_op(&self.op) else {
            members.push((&self.op, &self.sides.1));
            return (&self.sides.0, members);
        };

        match (self.effective_left(trivia), &self.sides.1) {
            (Expression::Binary(left), Expression::Binary(right))
                if BinaryOpChainingGroup::group_for_op(&left.op) == Some(chaining_group)
                    && BinaryOpChainingGroup::group_for_op(&right.op) == Some(chaining_group) =>
            {
                let (left_first, left_rest) = left.get_chaining_members(trivia);
                let (right_first, right_rest) = right.get_chaining_members(trivia);

                members.extend(left_rest);
                members.push((&self.op, right_first));
                members.extend(right_rest);

                (left_first, members)
            }
            (Expression::Binary(left), right)
                if BinaryOpChainingGroup::group_for_op(&left.op) == Some(chaining_group) =>
            {
                let (first, left_rest) = left.get_chaining_members(trivia);

                members.extend(left_rest);
                members.push((&self.op, right));
                (first, members)
            }
            (left, Expression::Binary(right))
                if BinaryOpChainingGroup::group_for_op(&right.op) == Some(chaining_group) =>
            {
                let (right_first, right_rest) = right.get_chaining_members(trivia);

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
        let (first, chain_members) = self.get_chaining_members(printer.trivia);
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

impl BinaryExprSingleLine for BinaryExpr {
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the binary expression on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        let left = self.effective_left(printer.trivia);
        let right = &self.sides.1;

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

/// Precedence rows whose redundant left-operand parens the formatter strips
/// (see [`BinaryExpr::effective_left`]). Ops within a row share one binding
/// power in the parser (`infix_binding_power`), so `(a OP b) OP c` reparses
/// identically without the parens. Comparisons, equality, shifts, `??`, and
/// assignments are deliberately absent: chains of those are unusual enough
/// that explicit parens read as intent.
///
/// Finer-grained than [`BinaryOpChainingGroup`], which mixes precedence
/// levels (`&&` with `||`, `&` with `|`) because it only groups layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BinaryOpPrecedenceRow {
    AddSubtract,
    MultiplyDivideModulo,
    BitwiseAnd,
    BitwiseOr,
    BitwiseXor,
    LogicalAnd,
    LogicalOr,
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

impl BinaryOpPrecedenceRow {
    fn row_for_op(op: &BinaryOp) -> Option<Self> {
        match op {
            BinaryOp::Plus(_) | BinaryOp::Minus(_) => Some(Self::AddSubtract),
            BinaryOp::Star(_) | BinaryOp::Slash(_) | BinaryOp::Percent(_) => {
                Some(Self::MultiplyDivideModulo)
            }
            BinaryOp::And(_) => Some(Self::BitwiseAnd),
            BinaryOp::Pipe(_) => Some(Self::BitwiseOr),
            BinaryOp::Caret(_) => Some(Self::BitwiseXor),
            BinaryOp::AndAnd(_) => Some(Self::LogicalAnd),
            BinaryOp::OrOr(_) => Some(Self::LogicalOr),
            _ => None,
        }
    }
}

impl IsExprWidth for IsExpr {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if the LHS can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
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

impl UnaryExprWidth for UnaryExpr {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let expr = self
            .expr
            .effective_unary_operand(input.trivia)
            .single_line_width(input)?;
        Some(usize::from(self.op.span().len()) + expr)
    }
}

impl Printable for UnaryExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;
        multi_lined |= printer.print(&self.op, shape.clone()).multi_lined;
        let expr = self.expr.effective_unary_operand(printer.trivia);
        multi_lined |= printer.print(expr, shape).multi_lined;

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
        // `if let PATTERN = SCRUTINEE { ... }` — pattern carries its own
        // leading `let`. No surrounding parens around the pattern or
        // scrutinee (unlike plain `if`, where parens are canonicalised
        // around the condition).
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

impl<A: Printable> Printable for ArmListItem<A> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            Self::Arm(arm) => arm.print(shape, printer),
            Self::Header(header) => {
                printer.print_raw_token(header);
                PrintInfo::default_single_line()
            }
        }
    }

    fn leftmost_token(&self) -> TextRange {
        match self {
            Self::Arm(arm) => arm.leftmost_token(),
            Self::Header(header) => header.span(),
        }
    }

    fn rightmost_token(&self) -> TextRange {
        match self {
            Self::Arm(arm) => arm.rightmost_token(),
            Self::Header(header) => header.span(),
        }
    }
}

impl MatchExprScrutineeLayout for MatchExpr {
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

impl MatchArmConditionLayout for MatchArm {
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

/// Print an arm body that is being wrapped into a `{ … }` block (the `{` and
/// newline are already emitted; the caller emits the closing `}`).
///
/// `arm_indent` is the arm's own indent; the body is printed one level deeper.
/// A braceless jump body (`return`/`break`/`continue`) additionally gets its
/// statement `;` — and its trailing trivia is deliberately left for the arm
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
                print_wrapped_arm_body(printer, &self.body, shape.indent);
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

impl CallExprWidth for CallExpr {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let callee = self
            .callee
            .effective_postfix_operand(input.trivia)
            .single_line_width(input)?;
        let args = self.args.single_line_width(input)?;
        Some(callee + args)
    }
}

impl Printable for CallExpr {
    /// The main way to call this should be through [`PrintChain`]
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;
        let line_len_before = printer.current_line_len();
        let callee = self.callee.effective_postfix_operand(printer.trivia);
        multi_lined |= printer.print(callee, shape.clone()).multi_lined;
        // Account for the callee on the call line so the args' hug layout
        // (see `CallArgs::try_print_hug`) budgets its first line correctly.
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

impl CallArgLayout for CallArg {
    /// A block-terminal argument (a lambda or a `spawn { … }`) that may hug
    /// the call parens instead of forcing the whole call to break: the
    /// argument's block opens on the call line and its `}` is immediately
    /// followed by `)`.
    fn is_huggable(&self) -> bool {
        matches!(self.expr, Expression::Lambda(_) | Expression::Spawn(_))
    }

    /// The argument expression with redundant parens peeled: the call's own
    /// parens already delimit the argument, so a transparent paren wrapping
    /// the whole expression carries nothing. Lambdas and `spawn` keep their
    /// parens: peeling one would flip [`Self::is_huggable`] between passes
    /// and break idempotency.
    fn effective_expr(&self, trivia: &TriviaInfo) -> &Expression {
        let peeled = self.expr.peel_transparent_parens(trivia);
        if matches!(peeled, Expression::Lambda(_) | Expression::Spawn(_)) {
            &self.expr
        } else {
            peeled
        }
    }

    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let expr = self.effective_expr(input.trivia);
        let mut len = 0;
        if let Some((name, equals)) = &self.label {
            let (_, name_trailing) = input.trivia.get_for_range_split(name.span());
            let (equals_leading, equals_trailing) = input.trivia.get_for_range_split(equals.span());
            let expr_leading = input.trivia.get_leading_for_element(expr);
            len += usize::from(name.span().len())
                + name_trailing.try_squished_len(input.input)?
                + equals_leading.try_squished_len(input.input)?
                + " = ".len()
                + equals_trailing.try_squished_len(input.input)?
                + expr_leading.try_squished_len(input.input)?;
        }
        len += expr.single_line_width(input)?;
        Some(len)
    }
}

impl Printable for CallArg {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let expr = self.effective_expr(printer.trivia);
        if let Some((name, equals)) = &self.label {
            printer.print_raw_token(name);
            let (_, name_trailing) = printer.trivia.get_for_range_split(name.span());
            let (equals_leading, equals_trailing) =
                printer.trivia.get_for_range_split(equals.span());
            let expr_leading = printer.trivia.get_leading_for_element(expr);
            printer.print_trivia_squished(name_trailing);
            printer.print_trivia_squished(equals_leading);
            printer.print_str(" = ");
            printer.print_trivia_squished(equals_trailing);
            printer.print_trivia_squished(expr_leading);
        }
        printer.print(expr, shape)
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

impl CallArgsLayout for CallArgs {
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

    /// Whether the hug layout (see [`Self::try_print_hug`]) applies: the last
    /// argument is block-terminal (a lambda or `spawn { … }`).
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
        // The hugged argument's first line continues the call line, so its
        // single-line budget is what remains after the indent, the call's own
        // offset, and everything printed since the open paren.
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
        // The trailing comma is dropped in the hug layout, but keep any
        // comments attached around it.
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
struct IndexArgs<'a> {
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

impl IndexExprLayout for IndexExpr {
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
        let base = self
            .base
            .effective_postfix_operand(input.trivia)
            .single_line_width(input)?;
        Some(base + self.args().single_line_width(input)?)
    }
}

impl PrintMultiLine for IndexExpr {
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let base = self.base.effective_postfix_operand(printer.trivia);
        printer.print(base, shape.clone());
        self.args().print_multi_line(shape, printer)
    }
}

impl IndexExprSingleLine for IndexExpr {
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        let base = self.base.effective_postfix_operand(printer.trivia);
        let base_len = base.single_line_width(printer)?;
        let args_len = self.args().single_line_width(printer)?;
        if base_len + args_len > shape.width {
            return None;
        }
        if base
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

impl FieldAccessExprWidth for FieldAccessExpr {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let base = self
            .base
            .effective_postfix_operand(input.trivia)
            .single_line_width(input)?;
        Some(base + usize::from(self.dot.span().len()) + usize::from(self.field.span().len()))
    }
}

impl OptionalFieldAccessExprWidth for OptionalFieldAccessExpr {
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let base = self
            .base
            .effective_postfix_operand(input.trivia)
            .single_line_width(input)?;
        Some(
            base + usize::from(self.question_dot.span().len())
                + usize::from(self.field.span().len()),
        )
    }
}

impl OptionalIndexExprLayout for OptionalIndexExpr {
    fn args(&self) -> IndexArgs<'_> {
        IndexArgs {
            open_bracket: &self.open_bracket,
            index: &self.index,
            close_bracket: &self.close_bracket,
        }
    }

    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let base = self
            .base
            .effective_postfix_operand(input.trivia)
            .single_line_width(input)?;
        Some(
            base + usize::from(self.question_dot.span().len())
                + self.args().single_line_width(input)?,
        )
    }
}

impl Printable for OptionalIndexExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;
        let base = self.base.effective_postfix_operand(printer.trivia);
        multi_lined |= printer.print(base, shape.clone()).multi_lined;
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

impl OptionalCallExprWidth for OptionalCallExpr {
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let callee = self
            .callee
            .effective_postfix_operand(input.trivia)
            .single_line_width(input)?;
        let args = self.args.single_line_width(input)?;
        Some(callee + usize::from(self.question_dot.span().len()) + args)
    }
}

impl Printable for OptionalCallExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;
        let callee = self.callee.effective_postfix_operand(printer.trivia);
        multi_lined |= printer.print(callee, shape.clone()).multi_lined;
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

impl EnvAccessExprWidth for EnvAccessExpr {
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
        // An empty block with no comment trapped inside collapses to `{}`
        // (e.g. an empty match arm `null => {},` or an empty `if` body).
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

impl Printable for Validated<'_, raw_ast::BlockExpr> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        BlockExpr::from_cst(self.syntax().clone().into())
            .expect("validated block expression must convert during migration")
            .print(shape, printer)
    }

    fn leftmost_token(&self) -> TextRange {
        self.l_brace_token().text_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.r_brace_token().text_range()
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

impl ArrayInitializerLayout for ArrayInitializer {
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

impl ObjectInitializerLayout for ObjectInitializer {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
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

impl MapLiteralLayout for MapLiteral {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let (_, open_trailing) = input.trivia.get_for_range_split(self.open_brace.span());
        let (close_leading, _) = input.trivia.get_for_range_split(self.close_brace.span());
        // A populated map carries two interior padding spaces (`{ k1: v1 }`);
        // an empty map is just `{}`. Keep this in sync with `try_print_single_line`.
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
        // An empty map renders as `{}` with no interior padding. The padding
        // spaces are only added when there is something to surround: fields or
        // an interior comment (the only trivia that prints on a single line).
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
                // Trailing comma is removed in single-line mode, but we still try the comments.
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

impl ObjectFieldWidth for ObjectField {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let name = self.name.single_line_width(input)?;
        let (Some(colon), Some(value)) = (&self.colon, &self.value) else {
            return Some(name);
        };
        let value_width = value.single_line_width(input)?;
        // Must match trivia handled by print: colon_trailing + value_leading
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

impl ObjectMemberWidth for ObjectMember {
    /// Returns the width of the member if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        match self {
            ObjectMember::Field(field) => field.single_line_width(input),
            ObjectMember::Spread(spread) => spread.single_line_width(input),
        }
    }
}

impl Printable for ObjectMember {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            ObjectMember::Field(field) => field.print(shape, printer),
            ObjectMember::Spread(spread) => spread.print(shape, printer),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            ObjectMember::Field(field) => field.leftmost_token(),
            ObjectMember::Spread(spread) => spread.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            ObjectMember::Field(field) => field.rightmost_token(),
            ObjectMember::Spread(spread) => spread.rightmost_token(),
        }
    }
}

impl SpreadElementWidth for SpreadElement {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        // Must match the trivia handled by `print`: dots_trailing + value_leading.
        let mut trivia_len = 0usize;
        let (_, dots_trailing) = input.trivia.get_for_range_split(self.dot_dot_dot.span());
        for t in dots_trailing {
            trivia_len += t.single_line_len(input.input)?;
        }
        let value_leading = input.trivia.get_leading_for_element(&self.value);
        for t in value_leading {
            trivia_len += t.single_line_len(input.input)?;
        }
        let value_width = self.value.single_line_width(input)?;
        Some(const { "...".len() } + value_width + trivia_len)
    }
}

impl Printable for SpreadElement {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        // No space after `...` — it binds tightly to its operand.
        printer.print_raw_token(&self.dot_dot_dot);
        let (_, dots_trailing) = printer.trivia.get_for_range_split(self.dot_dot_dot.span());
        printer.print_trivia_squished(dots_trailing);
        let value_leading = printer.trivia.get_leading_for_element(&self.value);
        printer.print_trivia_squished(value_leading);
        printer.print(&self.value, shape)
    }
    fn leftmost_token(&self) -> TextRange {
        self.dot_dot_dot.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.value.rightmost_token()
    }
}

impl ObjectFieldKeyWidth for ObjectFieldKey {
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

// ─── Lambda Expression ────────────────────────────────────────────────────────

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

impl Printable for UnreflectArg {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);
        printer.print_raw_token(&self.open_paren);
        printer.print(self.expr.as_ref(), shape);
        printer.print_raw_token(&self.close_paren);
        PrintInfo::default_single_line()
    }

    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.close_paren.span()
    }
}

impl Printable for GenericArgs {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.open_angle);
        for (i, (arg, _comma)) in self.args.iter().enumerate() {
            match arg {
                GenericArg::Type(ty) => printer.print(ty, shape.clone()),
                GenericArg::Unreflect(arg) => printer.print(arg, shape.clone()),
            };
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

impl Printable for raw_ast::ThrowsClause {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let keyword = self.throws_token().expect("validated throws token");
        let ty = self.type_expr().expect("validated throws type");
        printer.print_input_range(keyword.text_range());
        printer.print_str(" ");
        printer.print(&ty, shape)
    }

    fn leftmost_token(&self) -> TextRange {
        self.throws_token()
            .expect("validated throws token")
            .text_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.type_expr()
            .expect("validated throws type")
            .rightmost_token()
    }
}

impl Printable for Validated<'_, raw_ast::ThrowsClause> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.throws_token());
        printer.print_str(" ");
        printer.print(&self.type_expr(), shape)
    }

    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

impl FunctionArrowLayout for FunctionArrow {
    fn arrow_span(&self) -> TextRange {
        match self {
            FunctionArrow::Arrow(t) => t.span(),
            FunctionArrow::FatArrow(t) => t.span(),
        }
    }

    /// Print trivia between the source arrow and the next canonical element.
    /// The arrow spelling may be synthesized, but its source span still owns
    /// comments that must survive normalization.
    fn print_separator_before(
        &self,
        next_leftmost: Option<TextRange>,
        continuation_indent: usize,
        printer: &mut Printer,
    ) {
        let (_, arrow_trailing) = printer.trivia.get_for_range_split(self.arrow_span());
        let next_leading = next_leftmost
            .map(|range| printer.trivia.get_for_range_split(range).0)
            .unwrap_or(&[]);
        let mut printed_comment = false;
        let mut continued_on_newline = false;

        for trivia in arrow_trailing.iter().chain(next_leading) {
            if !trivia.is_comment() {
                continue;
            }
            if !continued_on_newline {
                printer.print_spaces(1);
            }
            printer.print_trivia(trivia);
            printed_comment = true;
            continued_on_newline = trivia.single_line_len(printer.input).is_none();
            if continued_on_newline {
                printer.print_newline();
                printer.print_spaces(continuation_indent);
            }
        }

        if !printed_comment || !continued_on_newline {
            printer.print_spaces(1);
        }
    }
}

impl FunctionArrowLayout for ValidatedSyntaxToken {
    fn arrow_span(&self) -> TextRange {
        Token::span(self)
    }

    fn print_separator_before(
        &self,
        next_leftmost: Option<TextRange>,
        continuation_indent: usize,
        printer: &mut Printer,
    ) {
        let (_, arrow_trailing) = printer.trivia.get_for_range_split(self.arrow_span());
        let next_leading = next_leftmost
            .map(|range| printer.trivia.get_for_range_split(range).0)
            .unwrap_or(&[]);
        let mut printed_comment = false;
        let mut continued_on_newline = false;

        for trivia in arrow_trailing.iter().chain(next_leading) {
            if !trivia.is_comment() {
                continue;
            }
            if !continued_on_newline {
                printer.print_spaces(1);
            }
            printer.print_trivia(trivia);
            printed_comment = true;
            continued_on_newline = trivia.single_line_len(printer.input).is_none();
            if continued_on_newline {
                printer.print_newline();
                printer.print_spaces(continuation_indent);
            }
        }

        if !printed_comment || !continued_on_newline {
            printer.print_spaces(1);
        }
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
            self.arrow.print_separator_before(
                Some(ret.leftmost_token()),
                shape.indent + printer.config.indent_width,
                printer,
            );
            printer.print(ret, shape.clone());
            if let Some(ref throws) = self.throws {
                printer.print_str(" ");
                printer.print(throws, shape.clone());
            }
            printer.print_str(" ");
        } else if let Some(ref throws) = self.throws {
            self.arrow.print_separator_before(
                Some(throws.leftmost_token()),
                shape.indent + printer.config.indent_width,
                printer,
            );
            printer.print(throws, shape.clone());
            printer.print_str(" ");
        } else {
            self.arrow.print_separator_before(
                Some(self.block.leftmost_token()),
                shape.indent + printer.config.indent_width,
                printer,
            );
        }

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

/// The `with` options clause of a [`SpawnExpr`]: the keyword and its
/// comma-separated expressions (in v1 a single `baml.spawn.options(...)`
/// call).
impl SpawnExprLayout for SpawnExpr {
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

    /// Returns the width of the expression if it fits on a single line —
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

    /// Single-line layout: `spawn name? (with opts)? {}` or `… { tail }`.
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

// ─── PrintChain ───────────────────────────────────────────────────────────────

/// Only used for printing chained expressions.
///
/// Needed to re-organize before printing from a hierarchical structure to a flat-ish one.
struct PrintChain<'a> {
    /// May be a [`PathExpr`] in which case only the first item is used (the rest are included in [`PrintChain::chain_members`]).
    first: &'a Expression,
    /// Will always start with a field access (if not empty), since calls/indexes will be included in `first` if not following a field access.
    chain_members: Vec<PrintChainItem<'a>>,
}
impl<'a> PrintChain<'a> {
    /// Builds the flat chain for a postfix spine.
    ///
    /// Every receiver is taken through `Expression::effective_postfix_operand`
    /// so redundant parens around it peel and the walk continues through them.
    /// A paren that survives (looser-binding receiver, or one carrying a
    /// comment) still terminates the walk and becomes `first`, which is what
    /// puts it on its own indent level.
    #[must_use]
    fn new(from: &'a Expression, trivia: &TriviaInfo) -> Self {
        let from = from.effective_postfix_operand(trivia);
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
                let mut chain = Self::new(&call_expr.callee, trivia);
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
                let mut chain = Self::new(&index_expr.base, trivia);
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
                let mut chain = Self::new(&field_access_expr.base, trivia);
                chain.chain_members.push(PrintChainItem::FieldAccess(
                    &field_access_expr.dot,
                    &field_access_expr.field,
                ));
                chain
            }
            Expression::OptionalFieldAccess(ofa) => {
                let mut chain = Self::new(&ofa.base, trivia);
                chain
                    .chain_members
                    .push(PrintChainItem::OptionalFieldAccess(
                        &ofa.question_dot,
                        &ofa.field,
                    ));
                chain
            }
            Expression::OptionalIndex(oi) => {
                let mut chain = Self::new(&oi.base, trivia);
                chain
                    .chain_members
                    .push(PrintChainItem::OptionalIndex(&oi.question_dot, oi.args()));
                chain
            }
            Expression::OptionalCall(oc) => {
                let mut chain = Self::new(&oc.callee, trivia);
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
    /// Prints the chained expression broken at method-call boundaries,
    /// prettier/rustfmt style.
    ///
    /// Plain member accesses (namespace segments, field accesses, generic
    /// type segments) are atomic with their receiver and never split, no
    /// matter how long the path is. Break points sit before the `.name` of
    /// each call group; the first call group stays glued to the receiver
    /// line when it fits:
    ///
    /// ```baml
    /// root.ai.Agent<Itinerary>.new()
    ///     .with_client(client)
    ///     .run(spec)
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let first_single_line = match self.first {
            Expression::Path(path_expr) => {
                printer.print_raw_token(&path_expr.first);
                true
            }
            // Call/Index print directly: routing them through
            // `Expression::print` would rebuild this same chain and recurse.
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
        let mut multi_lined = !first_single_line;

        let chain_indent = shape.indent + printer.config.indent_width;
        let mut line_remaining_width = printer.current_line_remaining_width();
        let mut rest: &[PrintChainItem<'_>] = &self.chain_members;

        // A call/index applied directly to the receiver (`base?.(x).field`)
        // cannot break away from it: glue it to the receiver's line.
        while let Some((item, tail)) = rest.split_first() {
            if Self::is_plain_access(item) {
                break;
            }
            multi_lined |=
                Self::print_non_field_item(item, chain_indent, &mut line_remaining_width, printer);
            rest = tail;
        }

        // The leading run of plain accesses is the namespace path; it is
        // atomic with the receiver and always stays on its line. When a call
        // follows, the final access of the run is that call's method name and
        // belongs to the call's group instead (`.new` stays glued to `()`).
        let plain_run_len = rest
            .iter()
            .take_while(|item| Self::is_plain_access(item))
            .count();
        let path_len = if plain_run_len == rest.len() {
            plain_run_len
        } else {
            rest[..plain_run_len]
                .iter()
                .rposition(|item| {
                    matches!(
                        item,
                        PrintChainItem::FieldAccess(..) | PrintChainItem::OptionalFieldAccess(..)
                    )
                })
                .unwrap_or(plain_run_len)
        };
        for item in &rest[..path_len] {
            Self::print_plain_item(item, printer);
        }
        rest = &rest[path_len..];
        line_remaining_width = printer.current_line_remaining_width();

        // Split the remaining items into groups: each group is a run of
        // plain accesses (the method name) followed by its calls/indexes.
        let mut is_first_group = true;
        while !rest.is_empty() {
            let group_plain = rest
                .iter()
                .take_while(|item| Self::is_plain_access(item))
                .count();
            let group_callish = rest[group_plain..]
                .iter()
                .take_while(|item| !Self::is_plain_access(item))
                .count();
            let (group, tail) = rest.split_at(group_plain + group_callish);
            rest = tail;

            // A group can only start with a call/index when the path had no
            // field access to serve as its name; such a group cannot move to
            // its own line. Otherwise, the first call group stays glued to
            // the receiver line when it fits; later groups always break.
            let glue = if group_plain == 0 {
                true
            } else if is_first_group && first_single_line {
                Self::group_single_line_width(group, printer)
                    .is_some_and(|width| width <= line_remaining_width)
            } else {
                false
            };
            if !glue {
                printer.print_newline();
                printer.print_spaces(chain_indent);
                line_remaining_width = printer.config.line_width.saturating_sub(chain_indent);
                multi_lined = true;
            }
            for item in group {
                if Self::is_plain_access(item) {
                    Self::print_plain_item(item, printer);
                    line_remaining_width = printer.current_line_remaining_width();
                } else {
                    multi_lined |= Self::print_non_field_item(
                        item,
                        chain_indent,
                        &mut line_remaining_width,
                        printer,
                    );
                }
            }
            is_first_group = false;
        }

        PrintInfo { multi_lined }
    }
}

impl PrintChain<'_> {
    /// Plain (non-call) chain items: member accesses and generic type
    /// segments. These are atomic with their receiver and never move to
    /// their own line.
    const fn is_plain_access(item: &PrintChainItem<'_>) -> bool {
        matches!(
            item,
            PrintChainItem::FieldAccess(..)
                | PrintChainItem::OptionalFieldAccess(..)
                | PrintChainItem::GenericArgs(..)
        )
    }

    /// Prints a plain access glued to whatever precedes it on the line.
    fn print_plain_item(item: &PrintChainItem<'_>, printer: &mut Printer) {
        match *item {
            PrintChainItem::FieldAccess(dot, word) => {
                printer.print_raw_token(dot);
                printer.print_raw_token(word);
            }
            PrintChainItem::OptionalFieldAccess(qd, word) => {
                printer.print_raw_token(qd);
                printer.print_raw_token(word);
            }
            PrintChainItem::GenericArgs(generic_args) => {
                printer.print(generic_args, Shape::unlimited_single_line());
            }
            _ => unreachable!("print_plain_item called with a call/index item"),
        }
    }

    /// Returns the single-line width of one chain item, or `None` if it can
    /// never be single-lined.
    fn item_single_line_width(item: &PrintChainItem<'_>, printer: &Printer<'_>) -> Option<usize> {
        match item {
            PrintChainItem::FieldAccess(dot, word) => {
                Some(usize::from(dot.span().len() + word.span().len()))
            }
            PrintChainItem::OptionalFieldAccess(qd, word) => {
                Some(usize::from(qd.span().len() + word.span().len()))
            }
            PrintChainItem::Index(index_args) => index_args.single_line_width(printer),
            PrintChainItem::OptionalIndex(qd, index_args) => {
                Some(usize::from(qd.span().len()) + index_args.single_line_width(printer)?)
            }
            PrintChainItem::Call(call_args) => call_args.single_line_width(printer),
            PrintChainItem::OptionalCall(qd, call_args) => {
                Some(usize::from(qd.span().len()) + call_args.single_line_width(printer)?)
            }
            PrintChainItem::GenericArgs(generic_args) => {
                Some(generic_args.formatted_single_line_width())
            }
        }
    }

    /// Returns the single-line width of a group of chain items, or `None` if
    /// any of them can never be single-lined.
    fn group_single_line_width(
        group: &[PrintChainItem<'_>],
        printer: &Printer<'_>,
    ) -> Option<usize> {
        group
            .iter()
            .map(|item| Self::item_single_line_width(item, printer))
            .sum()
    }

    /// Prints a call/index item on the current line. Its arguments may wrap.
    ///
    /// Returns whether the printed item spanned multiple lines.
    fn print_non_field_item(
        item: &PrintChainItem<'_>,
        chain_indent: usize,
        line_remaining_width: &mut usize,
        printer: &mut Printer,
    ) -> bool {
        let multi_lined = match item {
            PrintChainItem::Index(index_args) => {
                let index_shape = Shape {
                    width: *line_remaining_width,
                    indent: chain_indent,
                    first_line_offset: printer.current_line_len().saturating_sub(chain_indent),
                };
                printer.print(index_args, index_shape).multi_lined
            }
            PrintChainItem::OptionalIndex(qd, index_args) => {
                printer.print_raw_token(*qd);
                let index_shape = Shape {
                    width: *line_remaining_width,
                    indent: chain_indent,
                    first_line_offset: printer.current_line_len().saturating_sub(chain_indent),
                };
                printer.print(index_args, index_shape).multi_lined
            }
            &PrintChainItem::Call(call_args) => {
                let call_shape = Shape {
                    width: *line_remaining_width,
                    indent: chain_indent,
                    first_line_offset: printer.current_line_len().saturating_sub(chain_indent),
                };
                printer.print(call_args, call_shape).multi_lined
            }
            &PrintChainItem::OptionalCall(qd, call_args) => {
                printer.print_raw_token(qd);
                let call_shape = Shape {
                    width: *line_remaining_width,
                    indent: chain_indent,
                    first_line_offset: printer.current_line_len().saturating_sub(chain_indent),
                };
                printer.print(call_args, call_shape).multi_lined
            }
            _ => unreachable!("print_non_field_item called with a plain access item"),
        };
        *line_remaining_width = printer.current_line_remaining_width();
        multi_lined
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
            // `try_print_hug` runs in this same sub-printer, so its
            // `current_line_len()` already includes the printed chain prefix.
            // Keep only the offset that existed before this chain began.
            first_line_offset: shape.first_line_offset,
        };
        call_args.try_print_hug(&hug_shape, printer)
    }

    /// Tail-broken layout: the receiver, the namespace path, and every
    /// intermediate call stay on one line, and only the final call/index
    /// wraps its arguments:
    ///
    /// ```baml
    /// root.ai.Agent<Itinerary>.new().run(
    ///     plan_trip_spec(...),
    /// );
    /// ```
    ///
    /// Applies when the whole prefix up to the final call fits the line.
    /// Should be passed a sub-printer to avoid printing partial output in
    /// the event that the layout does not apply.
    fn try_print_tail_call_broken(
        &self,
        shape: &Shape,
        printer: &mut Printer,
    ) -> Option<PrintInfo> {
        let (last, prefix) = self.chain_members.split_last()?;
        let question_dot = match last {
            PrintChainItem::Call(_) | PrintChainItem::Index(_) => None,
            PrintChainItem::OptionalCall(qd, _) | PrintChainItem::OptionalIndex(qd, _) => Some(*qd),
            PrintChainItem::FieldAccess(..)
            | PrintChainItem::OptionalFieldAccess(..)
            | PrintChainItem::GenericArgs(..) => return None,
        };
        self.try_print_members_single_line(prefix, shape, printer)?;
        if let Some(qd) = question_dot {
            printer.print_raw_token(qd);
        }
        if printer.output.len() > shape.width {
            return None;
        }
        // `shape.width` is the remaining line budget measured from the
        // chain's start column (`width + indent + first_line_offset ==
        // line_width`), and this sub-printer's output also starts at that
        // column, so the args' budget is what the prefix left over.
        let args_shape = Shape {
            width: shape.width.saturating_sub(printer.output.len()),
            indent: shape.indent,
            first_line_offset: shape.first_line_offset + printer.output.len(),
        };
        let info = match last {
            PrintChainItem::Call(call_args) | PrintChainItem::OptionalCall(_, call_args) => {
                printer.print(*call_args, args_shape)
            }
            PrintChainItem::Index(index_args) | PrintChainItem::OptionalIndex(_, index_args) => {
                printer.print(index_args, args_shape)
            }
            _ => unreachable!("checked above"),
        };
        // The final call/index may still overflow the prefix line: its
        // multi-line layout keeps the opening bracket (plus any squished
        // trivia) on that line without re-checking the budget. Reject the
        // layout in that case so the chain breaks at call boundaries instead.
        let first_line_len = printer.output.find('\n').unwrap_or(printer.output.len());
        if first_line_len > shape.width {
            return None;
        }
        Some(info)
    }
}

impl Printable for PrintChain<'_> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .or_else(|| printer.try_sub_printer(|p| self.try_print_hug(&shape, p)))
            .or_else(|| printer.try_sub_printer(|p| self.try_print_tail_call_broken(&shape, p)))
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
