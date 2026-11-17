use baml_syntax::{
    nodes::{
        AnyElse, AnyExpr, AnyMatchScrutinee, AnyObjectCtor, AnyObjectKey, AnyObjectMember, AnyStmt,
        Arg, ArgList, ArrayElement, ArrayLiteral, BinaryExpr, Block, CallExpr, CatchArm,
        CatchClause, CatchExpr, ElseBranch, EnvAccessExpr, FieldAccessExpr, FileTree,
        GenericApplyExpr, GenericArgs, GenericParamBounds, GenericParamList, IfExpr, IfLetExpr,
        IndexExpr, IsExpr, LambdaExpr, MapEntry, MapLiteral, MatchArm, MatchExpr, MatchGuard, Name,
        ObjectField, ObjectLiteral, OptionalCallExpr, OptionalFieldAccessExpr, OptionalIndexExpr,
        ParenExpr, PathExpr, SpawnExpr, SpreadElement, TypeAscription, UnaryExpr,
    },
    ops::{BinaryOp, BinaryOpTok},
    token::{Token, punct, sets::MemberDot},
    tree::{Id, SepItem, TokenRange},
};
use text_size::TextRange;

use super::{
    pattern::print_binding_keyword_with_trailing_trivia, present, statements::ClosingAssign,
    tokens::BinaryOpExt as _, union_printable,
};
use crate::{
    printer::{PrintInfo, PrintMultiLine, Printable, Printer, Shape, Spanned},
    trivia_classifier::{EmittableTrivia, TriviaSliceExt as _},
};

/// A run of tokens inside a node: the part of a member before its comma.
/// Trivia attaches to its first and last tokens like a node's.
#[derive(Clone, Copy)]
pub(crate) struct TokenSpan(TokenRange);

impl Spanned for TokenSpan {
    fn tokens(self, _tree: &FileTree) -> TokenRange {
        self.0
    }
}

/// The tokens from `first`'s start to `last`'s end.
fn span_between(first: impl Spanned, last: impl Spanned, tree: &FileTree) -> TokenSpan {
    TokenSpan(TokenRange::new(
        first.tokens(tree).start(),
        last.tokens(tree).end(),
    ))
}

// ── Expression helpers ───────────────────────────────────────────────────────

pub(crate) trait ExprExt: Copy {
    /// Whether the expression, as a statement with no `;` written, is given one.
    fn statement_needs_semicolon(self, tree: &FileTree) -> bool;
    /// The width of the expression printed on one line with its comments,
    /// or `None` when it cannot be.
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize>;
    fn peel_transparent_parens(self, printer: &Printer<'_>) -> AnyExpr;
    fn binds_as_postfix_operand(self, tree: &FileTree) -> bool;
    fn has_optional_chain_link(self, tree: &FileTree) -> bool;
    fn effective_postfix_operand(self, printer: &Printer<'_>) -> AnyExpr;
    fn effective_call_operand(self, printer: &Printer<'_>) -> AnyExpr;
    fn effective_unary_operand(self, printer: &Printer<'_>) -> AnyExpr;
    fn peel_to_needed_paren(self, printer: &Printer<'_>, unary: bool) -> AnyExpr;
}

/// A node not modelled by the printer: the width of its text on one line.
fn verbatim_width(printer: &Printer<'_>, element: impl Spanned) -> Option<usize> {
    let text = printer.text_of(element);
    if text.contains('\n') {
        None
    } else {
        Some(text.len())
    }
}

impl ExprExt for AnyExpr {
    fn statement_needs_semicolon(self, _tree: &FileTree) -> bool {
        !matches!(
            self,
            AnyExpr::If(_)
                | AnyExpr::IfLet(_)
                | AnyExpr::Match(_)
                | AnyExpr::Lambda(_)
                | AnyExpr::Spawn(_)
                // Printed as written; the formatter adds nothing to them.
                | AnyExpr::Upcast(_)
                | AnyExpr::Spec(_)
                | AnyExpr::TaggedTemplate(_)
                | AnyExpr::Await(_)
                | AnyExpr::Throw(_)
                | AnyExpr::QualifiedPath(_)
        )
    }

    fn single_line_width(self, input: &Printer<'_>) -> Option<usize> {
        let tree = input.tree;
        match self {
            AnyExpr::Literal(lit) => Some(usize::from(lit.rightmost_token(tree).len())),
            AnyExpr::String(s) => verbatim_width(input, s),
            AnyExpr::Backtick(s) => verbatim_width(input, s),
            AnyExpr::Path(path) => path.single_line_width(input),
            AnyExpr::GenericApply(ga) => ga.single_line_width(input),
            AnyExpr::Tuple(id) => {
                let elements = tree.list(tree.get(id).elements);
                let mut width = 2 + usize::from(elements.len() == 1);
                for (i, element) in elements.iter().enumerate() {
                    width += element.item.single_line_width(input)? + if i > 0 { 2 } else { 0 };
                }
                Some(width)
            }
            AnyExpr::TupleField(id) => {
                let node = tree.get(id);
                Some(
                    node.base
                        .effective_postfix_operand(input)
                        .single_line_width(input)?
                        + usize::from(input.range(node.dot).len())
                        + usize::from(input.range(node.slot).len()),
                )
            }
            AnyExpr::Paren(paren) => paren.single_line_width(input),
            AnyExpr::Binary(binary) => binary.single_line_width(input),
            AnyExpr::Is(is) => is.single_line_width(input),
            AnyExpr::Unary(unary) => unary.single_line_width(input),
            AnyExpr::If(_) | AnyExpr::IfLet(_) | AnyExpr::Match(_) | AnyExpr::Catch(_) => None,
            AnyExpr::Call(call) => call.single_line_width(input),
            AnyExpr::Index(index) => index.single_line_width(input),
            AnyExpr::FieldAccess(fa) => fa.single_line_width(input),
            AnyExpr::OptionalFieldAccess(fa) => fa.single_line_width(input),
            AnyExpr::OptionalIndex(index) => index.single_line_width(input),
            AnyExpr::OptionalCall(call) => call.single_line_width(input),
            AnyExpr::EnvAccess(env) => env.single_line_width(input),
            AnyExpr::Block(_) => None,
            AnyExpr::Array(array) => array.single_line_width(input),
            AnyExpr::Map(map) => map.single_line_width(input),
            AnyExpr::Object(obj) => obj.single_line_width(input),
            AnyExpr::Lambda(_) => None,
            AnyExpr::Spawn(spawn) => spawn.single_line_width(input),
            AnyExpr::Return(_) | AnyExpr::Break(_) | AnyExpr::Continue(_) => None,
            AnyExpr::Upcast(id) => verbatim_width(input, id),
            AnyExpr::Spec(id) => verbatim_width(input, id),
            AnyExpr::TaggedTemplate(id) => verbatim_width(input, id),
            AnyExpr::Await(id) => verbatim_width(input, id),
            AnyExpr::Throw(id) => verbatim_width(input, id),
            AnyExpr::QualifiedPath(id) => verbatim_width(input, id),
            AnyExpr::Bogus(_) => unreachable!("the formatter prints only error-free trees"),
        }
    }

    fn peel_transparent_parens(self, printer: &Printer<'_>) -> AnyExpr {
        let mut expr = self;
        while let AnyExpr::Paren(paren) = expr {
            if !paren_is_transparent(paren, printer) {
                break;
            }
            expr = present(printer.tree.get(paren).inner);
        }
        expr
    }

    fn binds_as_postfix_operand(self, tree: &FileTree) -> bool {
        match self {
            AnyExpr::Call(_)
            | AnyExpr::Index(_)
            | AnyExpr::FieldAccess(_)
            | AnyExpr::TupleField(_) => !self.has_optional_chain_link(tree),
            AnyExpr::Path(_)
            | AnyExpr::EnvAccess(_)
            | AnyExpr::Tuple(_)
            | AnyExpr::Array(_)
            | AnyExpr::String(_)
            | AnyExpr::Backtick(_) => true,
            _ => false,
        }
    }

    fn has_optional_chain_link(self, tree: &FileTree) -> bool {
        match self {
            AnyExpr::OptionalFieldAccess(_)
            | AnyExpr::OptionalIndex(_)
            | AnyExpr::OptionalCall(_) => true,
            AnyExpr::Call(call) => tree.get(call).callee.has_optional_chain_link(tree),
            AnyExpr::Index(index) => tree.get(index).base.has_optional_chain_link(tree),
            AnyExpr::TupleField(id) => {
                let node = tree.get(id);
                tree.tokens().kind(node.dot.idx) == baml_syntax::token::TokenKind::QuestionDot
                    || node.base.has_optional_chain_link(tree)
            }
            AnyExpr::FieldAccess(fa) => tree.get(fa).base.has_optional_chain_link(tree),
            AnyExpr::Paren(paren) => present(tree.get(paren).inner).has_optional_chain_link(tree),
            _ => false,
        }
    }

    fn effective_call_operand(self, printer: &Printer<'_>) -> AnyExpr {
        if matches!(self, AnyExpr::Paren(_)) {
            let inner = self.peel_transparent_parens(printer);
            let member = match inner {
                AnyExpr::Path(id) => !printer.tree.list(printer.tree.get(id).segments).is_empty(),
                AnyExpr::FieldAccess(_)
                | AnyExpr::OptionalFieldAccess(_)
                | AnyExpr::TupleField(_) => true,
                _ => false,
            };
            if member {
                return self;
            }
        }
        self.effective_postfix_operand(printer)
    }

    fn effective_postfix_operand(self, printer: &Printer<'_>) -> AnyExpr {
        self.peel_to_needed_paren(printer, false)
    }

    fn effective_unary_operand(self, printer: &Printer<'_>) -> AnyExpr {
        self.peel_to_needed_paren(printer, true)
    }

    fn peel_to_needed_paren(self, printer: &Printer<'_>, unary: bool) -> AnyExpr {
        let tree = printer.tree;
        let mut expr = self;
        while let AnyExpr::Paren(paren) = expr {
            if !paren_is_transparent(paren, printer) {
                break;
            }
            let inner = present(tree.get(paren).inner);
            let stands_alone = inner.binds_as_postfix_operand(tree)
                || matches!(inner, AnyExpr::Paren(_))
                || (unary && matches!(inner, AnyExpr::Literal(_)));
            if !stands_alone {
                break;
            }
            expr = inner;
        }
        expr
    }
}

impl Printable for AnyExpr {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            AnyExpr::Literal(lit) => printer.print(lit, shape),
            AnyExpr::String(s) => printer.print(s, shape),
            AnyExpr::Backtick(bt) => printer.print(bt, shape),
            chain @ (AnyExpr::Path(_)
            | AnyExpr::Call(_)
            | AnyExpr::Index(_)
            | AnyExpr::FieldAccess(_)
            | AnyExpr::OptionalFieldAccess(_)
            | AnyExpr::OptionalIndex(_)
            | AnyExpr::OptionalCall(_)) => {
                // These are all chains of postfix expressions
                let chain = PrintChain::new(chain, printer);
                chain.print(shape, printer)
            }
            AnyExpr::GenericApply(ga) => printer.print(ga, shape),
            AnyExpr::Tuple(id) => printer.print(id, shape),
            AnyExpr::TupleField(id) => {
                let node = printer.tree.get(id);
                let info = printer.print(node.base.effective_postfix_operand(printer), shape);
                printer.print_token(node.dot);
                printer.print_token(node.slot);
                info
            }
            AnyExpr::Paren(paren) => printer.print(paren, shape),
            AnyExpr::Binary(binary) => printer.print(binary, shape),
            AnyExpr::Is(is) => printer.print(is, shape),
            AnyExpr::Unary(unary) => printer.print(unary, shape),
            AnyExpr::If(if_expr) => printer.print(if_expr, shape),
            AnyExpr::IfLet(if_let_expr) => printer.print(if_let_expr, shape),
            AnyExpr::Match(match_expr) => printer.print(match_expr, shape),
            AnyExpr::Catch(catch_expr) => printer.print(catch_expr, shape),
            AnyExpr::EnvAccess(env) => printer.print(env, shape),
            AnyExpr::Block(block) => printer.print(block, shape),
            AnyExpr::Array(array) => printer.print(array, shape),
            AnyExpr::Map(map) => printer.print(map, shape),
            AnyExpr::Object(obj) => printer.print(obj, shape),
            AnyExpr::Lambda(lambda) => printer.print(lambda, shape),
            AnyExpr::Spawn(spawn) => printer.print(spawn, shape),
            AnyExpr::Return(jump) => printer.print(jump, shape),
            AnyExpr::Break(jump) => printer.print(jump, shape),
            AnyExpr::Continue(jump) => printer.print(jump, shape),
            // Not modelled by the printer yet: printed as written. The
            // `multi_lined` verdict is honest about whether that text spans
            // lines, so a single-line `await f` does not force-wrap its
            // parents (B-231).
            AnyExpr::Upcast(id) => printer.print_verbatim(id),
            AnyExpr::Spec(id) => printer.print_verbatim(id),
            AnyExpr::TaggedTemplate(id) => printer.print_verbatim(id),
            AnyExpr::Await(id) => printer.print_verbatim(id),
            AnyExpr::Throw(id) => printer.print_verbatim(id),
            AnyExpr::QualifiedPath(id) => printer.print_verbatim(id),
            AnyExpr::Bogus(_) => unreachable!("the formatter prints only error-free trees"),
        }
    }
}

// ── Paths and generic application ────────────────────────────────────────────

pub(crate) trait PathExprExt: Copy {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize>;
}
impl PathExprExt for Id<PathExpr> {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize> {
        let tree = input.tree;
        let node = tree.get(self);
        let mut len = usize::from(node.head.rightmost_token(tree).len());
        for segment in tree.list(node.segments) {
            len += usize::from(input.range(segment.dot).len())
                + usize::from(present(segment.name).rightmost_token(tree).len());
        }
        Some(len)
    }
}

impl Printable for Id<PathExpr> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let segments = tree.list(node.segments);
        if segments.is_empty() {
            printer.print(node.head, shape);
            return PrintInfo::default_single_line();
        }
        let chain = PrintChain {
            first: ChainFirst::Name(node.head),
            chain_members: segments
                .iter()
                .map(|segment| {
                    PrintChainItem::FieldAccess(segment.dot.widen(), present(segment.name))
                })
                .collect(),
        };
        chain.print(shape, printer)
    }
}

pub(crate) trait GenericApplyExt: Copy {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize>;
}
impl GenericApplyExt for Id<GenericApplyExpr> {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize> {
        let node = input.tree.get(self);
        Some(node.base.single_line_width(input)? + node.args.formatted_single_line_width(input))
    }
}

impl Printable for Id<GenericApplyExpr> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let node = printer.tree.get(self);
        let info = printer.print(node.base, shape);
        printer.print(node.args, shape);
        info
    }
}

// ── Parentheses ──────────────────────────────────────────────────────────────

pub(crate) trait ParenExt: Copy {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize>;
}
impl ParenExt for Id<ParenExpr> {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize> {
        let tree = input.tree;
        let node = tree.get(self);
        let expr = present(node.inner);
        let inner = expr.single_line_width(input)?;
        let (_, open_trailing) = input.trivia.get_for_range_split(input.range(node.l));
        let (expr_leading, expr_trailing) = input.trivia.get_for_element(tree, expr);
        let (close_leading, _) = input
            .trivia
            .get_for_range_split(input.range(present(node.r)));
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

/// Whether no comment or blank line touches the parentheses or their
/// content, so dropping them changes nothing a parent context queries.
fn paren_is_transparent(id: Id<ParenExpr>, printer: &Printer<'_>) -> bool {
    let tree = printer.tree;
    let node = tree.get(id);
    let trivia = printer.trivia;
    let (open_leading, open_trailing) = trivia.get_for_range_split(printer.range(node.l));
    let (close_leading, close_trailing) =
        trivia.get_for_range_split(printer.range(present(node.r)));
    let (expr_leading, expr_trailing) = trivia.get_for_element(tree, present(node.inner));
    open_leading.is_empty()
        && open_trailing.is_empty()
        && close_leading.is_empty()
        && close_trailing.is_empty()
        && expr_leading.is_empty()
        && expr_trailing.is_empty()
}

impl PrintMultiLine for Id<ParenExpr> {
    fn print_multi_line(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let expr = present(node.inner);
        let close = present(node.r);
        let inner_shape = Shape {
            width: shape.width.saturating_sub(printer.config.indent_width),
            indent: shape.indent + printer.config.indent_width,
            first_line_offset: 0,
        };
        printer.print_token(node.l);
        printer.print_trivia_all_trailing_for(printer.range(node.l));
        printer.print_newline();
        let (expr_leading, expr_trailing) = printer.trivia.get_for_element(tree, expr);
        printer.print_trivia_with_newline(expr_leading.trim_blanks(), inner_shape.indent);
        printer.print_spaces(inner_shape.indent);
        printer.print(expr, inner_shape);
        printer.print_trivia_trailing(expr_trailing);
        printer.print_newline();
        let (close_leading, _) = printer.trivia.get_for_range_split(printer.range(close));
        printer.print_trivia_with_newline(close_leading.trim_blanks(), inner_shape.indent);
        printer.print_spaces(shape.indent);
        printer.print_token(close);
        PrintInfo::default_multi_lined()
    }
}

fn paren_single_line(id: Id<ParenExpr>, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
    let tree = printer.tree;
    let node = tree.get(id);
    let expr = present(node.inner);
    let close = present(node.r);
    printer.print_token(node.l);
    let (_, open_trailing) = printer.trivia.get_for_range_split(printer.range(node.l));
    printer.try_print_trivia_single_line_squished(open_trailing)?;
    let (expr_leading, expr_trailing) = printer.trivia.get_for_element(tree, expr);
    printer.try_print_trivia_single_line_squished(expr_leading)?;
    if printer
        .print(expr, Shape::unlimited_single_line())
        .multi_lined
    {
        return None;
    }
    printer.try_print_trivia_single_line_squished(expr_trailing)?;
    let (close_leading, _) = printer.trivia.get_for_range_split(printer.range(close));
    printer.try_print_trivia_single_line_squished(close_leading)?;
    printer.print_token(close);
    if printer.output.len() > shape.width {
        None
    } else {
        Some(PrintInfo::default_single_line())
    }
}

impl Printable for Id<ParenExpr> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.with_line_separated(false, |printer| {
            printer
                .try_sub_printer(|p| paren_single_line(self, &shape, p))
                .unwrap_or_else(|| self.print_multi_line(shape, printer))
        })
    }
}

fn print_in_added_parens_multi_line(
    inner: impl Printable,
    shape: &Shape,
    printer: &mut Printer,
) -> PrintInfo {
    let inner_shape = Shape {
        width: shape.width.saturating_sub(printer.config.indent_width),
        indent: shape.indent + printer.config.indent_width,
        first_line_offset: 0,
    };
    printer.print_str("(");
    printer.print_newline();
    printer.print_spaces(inner_shape.indent);
    printer.with_line_separated(false, |p| p.print(inner, inner_shape));
    printer.print_newline();
    printer.print_spaces(shape.indent);
    printer.print_str(")");
    PrintInfo::default_multi_lined()
}

// ── Binary expressions ───────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum BinaryOpChainingGroup {
    AddSubtract,
    MultiplyDivide,
    Bitwise,
    Logical,
}

impl BinaryOpChainingGroup {
    fn group_for_op(op: BinaryOp) -> Option<Self> {
        match op {
            BinaryOp::Add | BinaryOp::Sub => Some(Self::AddSubtract),
            BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => Some(Self::MultiplyDivide),
            BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor => Some(Self::Bitwise),
            BinaryOp::And | BinaryOp::Or => Some(Self::Logical),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BinaryOpPrecedenceRow {
    AddSubtract,
    MultiplyDivideModulo,
    BitwiseAnd,
    BitwiseOr,
    BitwiseXor,
    LogicalAnd,
    LogicalOr,
}

impl BinaryOpPrecedenceRow {
    fn row_for_op(op: BinaryOp) -> Option<Self> {
        match op {
            BinaryOp::Add | BinaryOp::Sub => Some(Self::AddSubtract),
            BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => Some(Self::MultiplyDivideModulo),
            BinaryOp::BitAnd => Some(Self::BitwiseAnd),
            BinaryOp::BitOr => Some(Self::BitwiseOr),
            BinaryOp::BitXor => Some(Self::BitwiseXor),
            BinaryOp::And => Some(Self::LogicalAnd),
            BinaryOp::Or => Some(Self::LogicalOr),
            _ => None,
        }
    }
}

/// The text range of a multi-token operator.
fn op_range(op: impl Spanned, tree: &FileTree) -> TextRange {
    TextRange::new(
        op.leftmost_token(tree).start(),
        op.rightmost_token(tree).end(),
    )
}

/// The width of `left op right` on one line, comments included.
fn binary_like_single_line_width(
    left: AnyExpr,
    op: impl Spanned,
    right: AnyExpr,
    input: &Printer<'_>,
) -> Option<usize> {
    let tree = input.tree;
    let left_width = left.single_line_width(input)?;
    let right_width = right.single_line_width(input)?;
    let mut trivia_len = 0usize;
    let left_trailing = input.trivia.get_trailing_for_element(tree, left);
    let (op_leading, _) = input.trivia.get_for_range_split(op.leftmost_token(tree));
    let (_, op_trailing) = input.trivia.get_for_range_split(op.rightmost_token(tree));
    trivia_len += (op_leading.try_squished_len(input.input)?
        + left_trailing.try_squished_len(input.input)?)
    .max(const { " ".len() }); // basically, if not comments then we have the space
    let right_leading = input.trivia.get_leading_for_element(tree, right);
    // A comment after the operator is spaced from it (`a / /* c */b`), so the
    // operator never reads as the start of a comment.
    let op_trailing_len = op_trailing.try_squished_len(input.input)?;
    trivia_len += (right_leading.try_squished_len(input.input)?
        + op_trailing_len
        + usize::from(op_trailing_len > 0))
    .max(const { " ".len() }); // basically, if not comments then we have the space
    Some(left_width + usize::from(op_range(op, tree).len()) + right_width + trivia_len)
}

/// `left op right` on one line, or `None` when a part cannot be.
fn binary_like_try_single_line(
    left: AnyExpr,
    op: impl Printable,
    right: AnyExpr,
    shape: &Shape,
    printer: &mut Printer,
) -> Option<PrintInfo> {
    let tree = printer.tree;
    if printer
        .print(left, Shape::unlimited_single_line())
        .multi_lined
    {
        return None;
    }
    let left_trailing = printer.trivia.get_trailing_for_element(tree, left);
    let (op_leading, _) = printer.trivia.get_for_range_split(op.leftmost_token(tree));
    let (_, op_trailing) = printer.trivia.get_for_range_split(op.rightmost_token(tree));
    let right_leading = printer.trivia.get_leading_for_element(tree, right);
    let mut left_trivia_len = printer.try_print_trivia_single_line_squished(left_trailing)?;
    left_trivia_len += printer.print_trivia_squished(op_leading);
    if left_trivia_len == 0 {
        printer.print_spaces(1); // only add space if there are no block comments between
    }
    printer.print(op, Shape::unlimited_single_line());
    let mut right_trivia_len = 0;
    if op_trailing.iter().any(EmittableTrivia::is_comment) {
        // Spaced from the operator, as the width above counted it.
        printer.print_spaces(1);
        right_trivia_len += 1;
    }
    right_trivia_len += printer.print_trivia_squished(op_trailing);
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
    if printer.output.len() > shape.width {
        None
    } else {
        Some(PrintInfo::default_single_line())
    }
}

/// The members of a chain `first op right op right …`, each operator with
/// the operand it precedes, one per line.
fn print_chain_members_multi_line<O: Printable>(
    first: AnyExpr,
    members: &[(O, AnyExpr)],
    shape: Shape,
    printer: &mut Printer,
) -> PrintInfo {
    let tree = printer.tree;
    let inner_indent = shape.indent + printer.config.indent_width;
    printer.print(first, shape);
    printer.print_trivia_all_trailing_for(first.rightmost_token(tree));
    let num_chain_members = members.len();
    for (i, (op, right)) in members.iter().enumerate() {
        let op_len = usize::from(op_range(*op, tree).len());
        printer.print_newline();
        printer.print_spaces(inner_indent);
        printer.print(*op, Shape::unlimited_single_line());
        printer.print_str(" ");
        let inner_shape = Shape {
            width: printer
                .config
                .line_width
                .saturating_sub(inner_indent + op_len + 1),
            indent: inner_indent,
            first_line_offset: op_len + 1,
        };
        printer.print(*right, inner_shape);
        if i + 1 < num_chain_members {
            printer.print_trivia_all_trailing_for(right.rightmost_token(tree));
        }
    }
    PrintInfo::default_multi_lined()
}

/// An assignment or other `left op right` that never chains.
pub(crate) fn print_binary_like(
    left: AnyExpr,
    op: impl Printable,
    right: AnyExpr,
    shape: Shape,
    printer: &mut Printer,
) -> PrintInfo {
    if let Some(info) =
        printer.try_sub_printer(|p| binary_like_try_single_line(left, op, right, &shape, p))
    {
        return info;
    }
    print_chain_members_multi_line(left, &[(op, right)], shape, printer)
}

pub(crate) trait BinaryExprExt: Copy {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize>;
    fn effective_left(self, printer: &Printer<'_>) -> AnyExpr;
    fn get_chaining_members(self, printer: &Printer<'_>) -> (AnyExpr, Vec<(BinaryOpTok, AnyExpr)>);
    fn has_minus_in_chain(self, printer: &Printer<'_>) -> bool;
}

impl BinaryExprExt for Id<BinaryExpr> {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize> {
        let node = input.tree.get(self);
        let left = self.effective_left(input);
        binary_like_single_line_width(left, node.op, present(node.rhs), input)
    }

    fn effective_left(self, printer: &Printer<'_>) -> AnyExpr {
        let tree = printer.tree;
        let node = tree.get(self);
        let Some(row) = BinaryOpPrecedenceRow::row_for_op(node.op.op()) else {
            return node.lhs;
        };
        let peeled = node.lhs.peel_transparent_parens(printer);
        match peeled {
            AnyExpr::Binary(inner)
                if BinaryOpPrecedenceRow::row_for_op(tree.get(inner).op.op()) == Some(row) =>
            {
                peeled
            }
            _ => node.lhs,
        }
    }

    /// Recursively lifts binary expressions in the same chaining group to the top level.
    /// For ops that are not in any chaining groups, return will be the same as the original.
    /// Redundant parens around left operands are peeled (see [`Self::effective_left`])
    /// so a fully parenthesized chain flattens like an unparenthesized one.
    ///
    /// The vec will never be empty.
    fn get_chaining_members(self, printer: &Printer<'_>) -> (AnyExpr, Vec<(BinaryOpTok, AnyExpr)>) {
        let tree = printer.tree;
        let node = tree.get(self);
        let rhs = present(node.rhs);
        let mut members = Vec::new();
        let Some(chaining_group) = BinaryOpChainingGroup::group_for_op(node.op.op()) else {
            members.push((node.op, rhs));
            return (node.lhs, members);
        };
        let in_group = |inner: Id<BinaryExpr>| {
            BinaryOpChainingGroup::group_for_op(tree.get(inner).op.op()) == Some(chaining_group)
        };
        match (self.effective_left(printer), rhs) {
            (AnyExpr::Binary(left), AnyExpr::Binary(right))
                if in_group(left) && in_group(right) =>
            {
                let (left_first, left_rest) = left.get_chaining_members(printer);
                let (right_first, right_rest) = right.get_chaining_members(printer);
                members.extend(left_rest);
                members.push((node.op, right_first));
                members.extend(right_rest);
                (left_first, members)
            }
            (AnyExpr::Binary(left), right) if in_group(left) => {
                let (first, left_rest) = left.get_chaining_members(printer);
                members.extend(left_rest);
                members.push((node.op, right));
                (first, members)
            }
            (left, AnyExpr::Binary(right)) if in_group(right) => {
                let (right_first, right_rest) = right.get_chaining_members(printer);
                members.push((node.op, right_first));
                members.extend(right_rest);
                (left, members)
            }
            (left, right) => {
                members.push((node.op, right));
                (left, members)
            }
        }
    }

    fn has_minus_in_chain(self, printer: &Printer<'_>) -> bool {
        let (_, chain_members) = self.get_chaining_members(printer);
        chain_members
            .iter()
            .any(|(op, _)| op.begins_statement_at_line_start())
    }
}

impl PrintMultiLine for Id<BinaryExpr> {
    fn print_multi_line(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let (first, chain_members) = self.get_chaining_members(printer);
        print_chain_members_multi_line(first, &chain_members, shape, printer)
    }
}

impl Printable for Id<BinaryExpr> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let node = printer.tree.get(self);
        let left = self.effective_left(printer);
        let right = present(node.rhs);
        if let Some(info) = printer
            .try_sub_printer(|p| binary_like_try_single_line(left, node.op, right, &shape, p))
        {
            return info;
        }
        if printer.line_separated && self.has_minus_in_chain(printer) {
            return print_in_added_parens_multi_line(self, &shape, printer);
        }
        self.print_multi_line(shape, printer)
    }
}

// ── `is`, unary ──────────────────────────────────────────────────────────────

pub(crate) trait IsExprExt: Copy {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize>;
}
impl IsExprExt for Id<IsExpr> {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize> {
        let tree = input.tree;
        let node = tree.get(self);
        let pattern = present(node.pattern);
        let lhs = node.expr.single_line_width(input)?;
        let pat_left = pattern.leftmost_token(tree).start();
        let pat_right = pattern.rightmost_token(tree).end();
        let pattern_width = usize::from(pat_right - pat_left);
        Some(lhs + 1 + usize::from(input.range(node.is_kw).len()) + 1 + pattern_width)
    }
}

impl Printable for Id<IsExpr> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let pattern = present(node.pattern);
        let mut multi_lined = false;
        multi_lined |= printer.print(node.expr, shape).multi_lined;
        let lhs_trailing = printer.trivia.get_trailing_for_element(tree, node.expr);
        let (kw_leading, kw_trailing) = printer
            .trivia
            .get_for_range_split(printer.range(node.is_kw));
        let mut left_trivia_len = printer.print_trivia_squished(lhs_trailing);
        left_trivia_len += printer.print_trivia_squished(kw_leading);
        if left_trivia_len == 0 {
            printer.print_spaces(1);
        }
        printer.print_token(node.is_kw);
        let pat_leading = printer.trivia.get_leading_for_element(tree, pattern);
        let mut right_trivia_len = printer.print_trivia_squished(kw_trailing);
        right_trivia_len += printer.print_trivia_squished(pat_leading);
        if right_trivia_len == 0 {
            printer.print_spaces(1);
        }
        multi_lined |= printer.print(pattern, shape).multi_lined;
        PrintInfo { multi_lined }
    }
}

pub(crate) trait UnaryExprExt: Copy {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize>;
}
impl UnaryExprExt for Id<UnaryExpr> {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize> {
        let node = input.tree.get(self);
        let expr = present(node.operand)
            .effective_unary_operand(input)
            .single_line_width(input)?;
        Some(usize::from(input.range(node.op).len()) + expr)
    }
}

impl Printable for Id<UnaryExpr> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let node = printer.tree.get(self);
        let mut multi_lined = false;
        multi_lined |= printer.print(node.op, shape).multi_lined;
        let expr = present(node.operand).effective_unary_operand(printer);
        multi_lined |= printer.print(expr, shape).multi_lined;
        PrintInfo { multi_lined }
    }
}

// ── if / if let ──────────────────────────────────────────────────────────────

union_printable!(AnyElse { Block, If, IfLet });

/// ` else …` after a branch.
fn print_else_branch(else_branch: ElseBranch, shape: Shape, printer: &mut Printer) {
    printer.print_str(" ");
    printer.print_token(else_branch.kw);
    printer.print_str(" ");
    printer.print(present(else_branch.body), shape);
}

impl Printable for Id<IfExpr> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let node = printer.tree.get(self);
        let condition = present(node.cond);
        let block = present(node.then_branch);
        printer.print_token(node.kw);
        printer.print_str(" ");
        if matches!(condition, AnyExpr::Paren(_)) {
            printer.print(condition, shape);
        } else if printer
            .try_sub_printer(|p| {
                p.print_str("(");
                if p.with_line_separated(false, |p| {
                    p.print(condition, Shape::unlimited_single_line())
                })
                .multi_lined
                {
                    return None;
                }
                p.print_str(")");
                (p.output.len() <= shape.width).then(PrintInfo::default_single_line)
            })
            .is_none()
        {
            print_in_added_parens_multi_line(condition, &shape, printer);
        }
        printer.print_str(" ");
        printer.print(block, shape);
        if let Some(else_branch) = node.else_branch {
            print_else_branch(else_branch, shape, printer);
        }
        PrintInfo::default_multi_lined()
    }
}

impl Printable for Id<IfLetExpr> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let node = printer.tree.get(self);
        let pattern = present(node.pattern);
        let eq = present(node.eq);
        let scrutinee = present(node.scrutinee);
        let block = present(node.then_branch);
        printer.print_token(node.kw);
        printer.print_str(" ");
        print_binding_keyword_with_trailing_trivia(printer, node.introducer);
        printer.print(pattern, shape);
        printer.print_str(" ");
        printer.print_token(eq);
        printer.print_str(" ");
        printer.print(scrutinee, shape);
        printer.print_str(" ");
        printer.print(block, shape);
        if let Some(else_branch) = node.else_branch {
            print_else_branch(else_branch, shape, printer);
        }
        PrintInfo::default_multi_lined()
    }
}

// ── match ────────────────────────────────────────────────────────────────────

type ScrutineeParens = Option<(Token<punct::LParen>, Token<punct::RParen>)>;

/// The scrutinee's expression and, when written, its parentheses and type.
fn scrutinee_parts(
    scrutinee: AnyMatchScrutinee,
    tree: &FileTree,
) -> (AnyExpr, ScrutineeParens, Option<Id<TypeAscription>>) {
    match scrutinee {
        AnyMatchScrutinee::Paren(paren) => {
            let node = tree.get(paren);
            (
                present(node.expr),
                Some((node.l, present(node.r))),
                node.ascription,
            )
        }
        AnyMatchScrutinee::Bare(expr) => (expr, None, None),
        AnyMatchScrutinee::Bogus(_) => unreachable!("the formatter prints only error-free trees"),
    }
}

fn print_scrutinee_ascription(ascription: Id<TypeAscription>, printer: &mut Printer) {
    let node = printer.tree.get(ascription);
    if let Some(colon) = node.colon {
        printer.print_token(colon);
    } else {
        printer.print_str(":");
    }
    printer.print_str(" ");
    printer.print(present(node.ty), Shape::unlimited_single_line());
}

fn match_scrutinee_single_line(
    id: Id<MatchExpr>,
    shape: &Shape,
    printer: &mut Printer,
) -> Option<PrintInfo> {
    let tree = printer.tree;
    let node = tree.get(id);
    let (scrutinee, parens, ascription) = scrutinee_parts(present(node.scrutinee), tree);
    if matches!(scrutinee, AnyExpr::Tuple(_)) && ascription.is_none() {
        let info = printer.print(scrutinee, *shape);
        return (!info.multi_lined && printer.output.len() <= shape.width).then_some(info);
    }
    match parens {
        Some((open, _)) => {
            printer.print_token(open);
            let (_, open_trailing) = printer.trivia.get_for_range_split(printer.range(open));
            printer.try_print_trivia_single_line_squished(open_trailing)?;
        }
        None => printer.print_str("("),
    }
    let (scrutinee_leading, scrutinee_trailing) = printer.trivia.get_for_element(tree, scrutinee);
    printer.try_print_trivia_single_line_squished(scrutinee_leading)?;
    if printer
        .print(scrutinee, Shape::unlimited_single_line())
        .multi_lined
    {
        return None;
    }
    if let Some(ascription) = ascription {
        print_scrutinee_ascription(ascription, printer);
    }
    printer.try_print_trivia_single_line_squished(scrutinee_trailing)?;
    match parens {
        Some((_, close)) => {
            let (close_leading, _) = printer.trivia.get_for_range_split(printer.range(close));
            printer.try_print_trivia_single_line_squished(close_leading)?;
            printer.print_token(close);
        }
        None => printer.print_str(")"),
    }
    if printer.output.len() > shape.width {
        None
    } else {
        Some(PrintInfo::default_single_line())
    }
}

fn match_scrutinee_multi_line(id: Id<MatchExpr>, shape: &Shape, printer: &mut Printer) {
    let tree = printer.tree;
    let node = tree.get(id);
    let (scrutinee, parens, ascription) = scrutinee_parts(present(node.scrutinee), tree);
    if matches!(scrutinee, AnyExpr::Tuple(_)) && ascription.is_none() {
        printer.print(scrutinee, *shape);
        return;
    }
    let paren_inner_indent = shape.indent + printer.config.indent_width;
    match parens {
        Some((open, _)) => {
            printer.print_token(open);
            printer.print_trivia_all_trailing_for(printer.range(open));
        }
        None => printer.print_str("("),
    }
    printer.print_newline();
    printer.print_standalone_with_trivia(scrutinee, paren_inner_indent);
    if let Some(ascription) = ascription {
        print_scrutinee_ascription(ascription, printer);
    }
    printer.print_newline();
    match parens {
        Some((_, close)) => {
            printer.print_trivia_all_leading_with_newline_for(
                printer.range(close),
                paren_inner_indent,
            );
            printer.print_spaces(shape.indent);
            printer.print_token(close);
        }
        None => {
            printer.print_spaces(shape.indent);
            printer.print_str(")");
        }
    }
}

impl Printable for Id<MatchExpr> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let open = present(node.l);
        let close = present(node.r);
        let inner_indent = shape.indent + printer.config.indent_width;
        printer.print_token(node.kw);
        printer.print_str(" ");
        printer.with_line_separated(false, |printer| {
            if printer
                .try_sub_printer(|p| match_scrutinee_single_line(self, &shape, p))
                .is_none()
            {
                match_scrutinee_multi_line(self, &shape, printer);
            }
        });
        printer.print_str(" ");
        printer.print_token(open);
        printer.print_trivia_all_trailing_for(printer.range(open));
        printer.print_newline();
        for arm in tree.list(node.arms) {
            printer.print_standalone_with_trivia(*arm, inner_indent);
            printer.print_newline();
        }
        printer.print_trivia_all_leading_with_newline_for(printer.range(close), inner_indent);
        printer.print_spaces(shape.indent);
        printer.print_token(close);
        PrintInfo::default_multi_lined()
    }
}

/// `pattern [if guard] =>`.
fn match_arm_print_condition(id: Id<MatchArm>, shape: &Shape, printer: &mut Printer) -> PrintInfo {
    let tree = printer.tree;
    let node = tree.get(id);
    let pattern = present(node.pattern);
    let mut multi_lined = false;
    let mut pattern_printer = printer.sub_printer();
    let pattern_info = pattern_printer.print(pattern, *shape);
    multi_lined |= pattern_info.multi_lined;
    let pattern_len = pattern_printer.len();
    printer.append_from_printer(pattern_printer);
    if let Some(guard) = node.guard {
        let condition = present(guard.cond);
        if pattern_info.multi_lined {
            printer.print_newline();
            printer.print_spaces(shape.indent + printer.config.indent_width);
            let offset = usize::from(printer.range(guard.kw).len()) + const { " ".len() };
            let guard_shape = Shape {
                width: printer.config.line_width.saturating_sub(
                    shape.indent + printer.config.indent_width + offset + const { " =>".len() },
                ),
                indent: shape.indent + printer.config.indent_width,
                first_line_offset: offset,
            };
            printer.print(guard, guard_shape);
        } else if matches!(condition, AnyExpr::Paren(_) | AnyExpr::Block(_)) {
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
            let guard_info = printer.print(guard, guard_shape);
            multi_lined |= guard_info.multi_lined;
        } else {
            let mut guard_single_line = printer.sub_printer();
            let guard_info = guard_single_line.print(guard, Shape::unlimited_single_line());
            let single_line_len =
                pattern_len + const { " ".len() } + guard_single_line.len() + const { " =>".len() };
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
                printer.print(guard, guard_shape);
            } else {
                printer.print_spaces(1);
                printer.append_from_printer(guard_single_line);
            }
        }
    }
    printer.print_str(" =>");
    PrintInfo { multi_lined }
}

fn print_wrapped_arm_body(printer: &mut Printer, body: AnyExpr, arm_indent: usize) {
    let inner_indent = arm_indent + printer.config.indent_width;
    if matches!(
        body,
        AnyExpr::Return(_) | AnyExpr::Break(_) | AnyExpr::Continue(_)
    ) {
        printer.print_standalone_leading_and_body(body, inner_indent);
        printer.print_str(";");
    } else {
        printer.print_standalone_with_trivia(body, inner_indent);
    }
}

impl Printable for Id<MatchArm> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let body = present(node.body);
        printer.with_line_separated(true, |printer| {
            let condition_info = match_arm_print_condition(self, &shape, printer);
            let condition_multi_lined = condition_info.multi_lined;
            if condition_multi_lined {
                printer.print_newline();
                printer.print_spaces(shape.indent);
                if let AnyExpr::Block(block) = body {
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
                    print_wrapped_arm_body(printer, body, shape.indent);
                    printer.print_newline();
                    printer.print_spaces(shape.indent);
                    printer.print_str("},");
                }
                return PrintInfo::default_multi_lined();
            }
            printer.print_spaces(1);
            let line_len_remaining = printer.current_line_remaining_width();
            if let AnyExpr::Block(block) = body {
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
            } else if let AnyExpr::Match(match_expr) = body
                && let (scrutinee, _, _) =
                    scrutinee_parts(present(tree.get(match_expr).scrutinee), tree)
                && let Some(match_scrutinee_len) = scrutinee.single_line_width(printer)
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
                let info = printer.print(match_expr, match_shape);
                printer.print_str(",");
                return info;
            }
            let mut try_body = printer.sub_printer();
            let try_body_info = try_body.print(body, Shape::unlimited_single_line());
            if try_body_info.multi_lined || try_body.len() > line_len_remaining {
                printer.print_str("{");
                printer.print_newline();
                print_wrapped_arm_body(printer, body, shape.indent);
                printer.print_newline();
                printer.print_spaces(shape.indent);
                printer.print_str("},");
                PrintInfo::default_multi_lined()
            } else {
                printer.append_from_printer(try_body);
                printer.print_str(",");
                PrintInfo::default_single_line()
            }
        })
    }
}

impl Spanned for MatchGuard {
    fn tokens(self, tree: &FileTree) -> TokenRange {
        span_between(self.kw, present(self.cond), tree).0
    }
}

impl Printable for MatchGuard {
    fn print(self, mut shape: Shape, printer: &mut Printer) -> PrintInfo {
        let condition = present(self.cond);
        printer.print_token(self.kw);
        printer.print_str(" ");
        let kw_len = usize::from(printer.range(self.kw).len());
        shape.width = shape.width.saturating_sub(kw_len + 1);
        shape.first_line_offset += kw_len + 1;
        printer.print(condition, shape)
    }
}

// ── catch ────────────────────────────────────────────────────────────────────

impl Printable for Id<CatchExpr> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let clauses = tree.list(node.clauses.as_list());
        let base_info = printer.print(node.body, shape);
        for clause in clauses {
            printer.print_str(" ");
            printer.print(*clause, shape);
        }
        PrintInfo {
            multi_lined: base_info.multi_lined || !clauses.is_empty(),
        }
    }
}

impl Printable for Id<CatchClause> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let open_brace = present(node.lbrace);
        let close_brace = present(node.rbrace);
        let inner_indent = shape.indent + printer.config.indent_width;
        printer.print_token(node.kw);
        printer.print_str(" ");
        printer.print_token(present(node.l));
        printer.print(present(node.binding), Shape::unlimited_single_line());
        if let Some(trace) = node.trace {
            printer.print_token(trace.comma);
            printer.print_str(" ");
            printer.print(present(trace.name), Shape::unlimited_single_line());
        }
        printer.print_token(present(node.r));
        printer.print_str(" ");
        printer.print_token(open_brace);
        printer.print_trivia_all_trailing_for(printer.range(open_brace));
        printer.print_newline();
        for arm in tree.list(node.arms) {
            printer.print_standalone_with_trivia(*arm, inner_indent);
            printer.print_newline();
        }
        printer.print_trivia_all_leading_with_newline_for(printer.range(close_brace), inner_indent);
        printer.print_spaces(shape.indent);
        printer.print_token(close_brace);
        PrintInfo::default_multi_lined()
    }
}

impl Printable for Id<CatchArm> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let pattern = present(node.pattern);
        let arrow = present(node.arrow);
        let body = present(node.body);
        printer.with_line_separated(true, |printer| {
            printer.print(pattern, shape);
            printer.print_str(" ");
            printer.print_token(arrow);
            printer.print_str(" ");
            let line_len_remaining = printer.current_line_remaining_width();
            if let AnyExpr::Block(block) = body {
                let body_shape = Shape {
                    width: line_len_remaining,
                    indent: shape.indent,
                    first_line_offset: printer
                        .config
                        .line_width
                        .saturating_sub(shape.indent + line_len_remaining),
                };
                let info = printer.print(block, body_shape);
                if node.comma.is_some() {
                    printer.print_str(",");
                }
                return info;
            }
            let mut try_body = printer.sub_printer();
            let try_body_info = try_body.print(body, Shape::unlimited_single_line());
            if try_body_info.multi_lined || try_body.len() > line_len_remaining {
                printer.print_str("{");
                printer.print_newline();
                print_wrapped_arm_body(printer, body, shape.indent);
                printer.print_newline();
                printer.print_spaces(shape.indent);
                printer.print_str("}");
                if node.comma.is_some() {
                    printer.print_str(",");
                }
                PrintInfo::default_multi_lined()
            } else {
                printer.append_from_printer(try_body);
                if node.comma.is_some() {
                    printer.print_str(",");
                }
                PrintInfo::default_single_line()
            }
        })
    }
}

// ── calls ────────────────────────────────────────────────────────────────────

pub(crate) trait CallExprExt: Copy {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize>;
}
impl CallExprExt for Id<CallExpr> {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize> {
        let node = input.tree.get(self);
        let callee = node
            .callee
            .effective_call_operand(input)
            .single_line_width(input)?;
        let args = node.args.single_line_width(input)?;
        Some(callee + args)
    }
}

impl Printable for Id<CallExpr> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let node = printer.tree.get(self);
        let mut multi_lined = false;
        let line_len_before = printer.current_line_len();
        let callee = node.callee.effective_call_operand(printer);
        multi_lined |= printer.print(callee, shape).multi_lined;
        let args_shape = Shape {
            first_line_offset: shape.first_line_offset
                + printer.current_line_len().saturating_sub(line_len_before),
            ..shape
        };
        multi_lined |= printer.print(node.args, args_shape).multi_lined;
        PrintInfo { multi_lined }
    }
}

pub(crate) trait ArgExt: Copy {
    fn is_huggable(self, tree: &FileTree) -> bool;
    fn effective_expr(self, printer: &Printer<'_>) -> AnyExpr;
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize>;
}
impl ArgExt for Id<Arg> {
    fn is_huggable(self, tree: &FileTree) -> bool {
        matches!(
            present(tree.get(self).value),
            AnyExpr::Lambda(_) | AnyExpr::Spawn(_)
        )
    }
    fn effective_expr(self, printer: &Printer<'_>) -> AnyExpr {
        let value = present(printer.tree.get(self).value);
        let peeled = value.peel_transparent_parens(printer);
        if matches!(peeled, AnyExpr::Lambda(_) | AnyExpr::Spawn(_)) {
            value
        } else {
            peeled
        }
    }
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize> {
        let tree = input.tree;
        let node = tree.get(self);
        let expr = self.effective_expr(input);
        let mut len = 0;
        if let Some(label) = node.label {
            let name_range = label.name.rightmost_token(tree);
            let (_, name_trailing) = input.trivia.get_for_range_split(name_range);
            let (equals_leading, equals_trailing) =
                input.trivia.get_for_range_split(input.range(label.eq));
            let expr_leading = input.trivia.get_leading_for_element(tree, expr);
            len += usize::from(name_range.len())
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

impl Printable for Id<Arg> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let expr = self.effective_expr(printer);
        if let Some(label) = node.label {
            printer.print(label.name, shape);
            let (_, name_trailing) = printer
                .trivia
                .get_for_range_split(label.name.rightmost_token(tree));
            let (equals_leading, equals_trailing) =
                printer.trivia.get_for_range_split(printer.range(label.eq));
            let expr_leading = printer.trivia.get_leading_for_element(tree, expr);
            printer.print_trivia_squished(name_trailing);
            printer.print_trivia_squished(equals_leading);
            printer.print_str(" = ");
            printer.print_trivia_squished(equals_trailing);
            printer.print_trivia_squished(expr_leading);
        }
        printer.print(expr, shape)
    }
}

impl PrintMultiLine for Id<ArgList> {
    fn print_multi_line(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let close = present(node.r);
        let inner_indent = shape.indent + printer.config.indent_width;
        let inner_shape = Shape {
            width: printer.config.line_width.saturating_sub(inner_indent),
            indent: inner_indent,
            first_line_offset: 0,
        };
        printer.print_token(node.l);
        printer.print_trivia_all_trailing_for(printer.range(node.l));
        printer.print_newline();
        for SepItem { item: arg, sep } in tree.list(node.args) {
            printer.print_trivia_all_leading_with_newline_for(
                arg.leftmost_token(tree),
                inner_shape.indent,
            );
            printer.print_spaces(inner_shape.indent);
            printer.print(*arg, inner_shape);
            if let Some(comma) = *sep {
                printer.print_token(comma);
                printer.print_trivia_all_trailing_for(printer.range(comma));
            } else {
                printer.print_str(",");
                printer.print_trivia_all_trailing_for(arg.rightmost_token(tree));
            }
            printer.print_newline();
        }
        printer.print_trivia_all_leading_with_newline_for(printer.range(close), inner_shape.indent);
        printer.print_spaces(shape.indent);
        printer.print_token(close);
        PrintInfo::default_multi_lined()
    }
}

pub(crate) trait ArgListExt: Copy {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize>;
    fn try_print_single_line(self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo>;
    fn can_hug(self, tree: &FileTree) -> bool;
    fn try_print_hug(self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo>;
}

impl ArgListExt for Id<ArgList> {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize> {
        let tree = input.tree;
        let node = tree.get(self);
        let args = tree.list(node.args);
        let mut len = const { "()".len() };
        let (_, open_trailing) = input.trivia.get_for_range_split(input.range(node.l));
        for t in open_trailing {
            len += t.single_line_len(input.input)?;
        }
        for (i, SepItem { item: arg, sep }) in args.iter().enumerate() {
            let (arg_leading, arg_trailing) = input.trivia.get_for_element(tree, *arg);
            for t in arg_leading {
                len += t.single_line_len(input.input)?;
            }
            len += arg.single_line_width(input)?;
            for t in arg_trailing {
                len += t.single_line_len(input.input)?;
            }
            if i + 1 < args.len() {
                if let Some(comma) = *sep {
                    let (comma_leading, comma_trailing) =
                        input.trivia.get_for_range_split(input.range(comma));
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
            } else if let Some(comma) = *sep {
                let (comma_leading, comma_trailing) =
                    input.trivia.get_for_range_split(input.range(comma));
                for t in comma_leading {
                    len += t.single_line_len(input.input)?;
                }
                for t in comma_trailing {
                    len += t.single_line_len(input.input)?;
                }
            }
        }
        let (close_leading, _) = input
            .trivia
            .get_for_range_split(input.range(present(node.r)));
        for t in close_leading {
            len += t.single_line_len(input.input)?;
        }
        Some(len)
    }

    fn try_print_single_line(self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        let tree = printer.tree;
        let node = tree.get(self);
        let close = present(node.r);
        let args = tree.list(node.args);
        printer.print_token(node.l);
        let (_, open_trailing) = printer.trivia.get_for_range_split(printer.range(node.l));
        printer.try_print_trivia_single_line_squished(open_trailing)?;
        for (i, SepItem { item: arg, sep }) in args.iter().enumerate() {
            if printer.output.len() > shape.width {
                return None;
            }
            let (arg_leading, arg_trailing) = printer.trivia.get_for_element(tree, *arg);
            printer.try_print_trivia_single_line_squished(arg_leading)?;
            if printer
                .print(*arg, Shape::unlimited_single_line())
                .multi_lined
            {
                return None;
            }
            printer.try_print_trivia_single_line_squished(arg_trailing)?;
            if i + 1 < args.len() {
                if let Some(comma) = *sep {
                    let (comma_leading, comma_trailing) =
                        printer.trivia.get_for_range_split(printer.range(comma));
                    printer.try_print_trivia_single_line_squished(comma_leading)?;
                    printer.print_token(comma);
                    printer.try_print_trivia_single_line_squished(comma_trailing)?;
                } else {
                    printer.print_str(",");
                }
                printer.print_str(" ");
            } else if let Some(comma) = *sep {
                let (comma_leading, comma_trailing) =
                    printer.trivia.get_for_range_split(printer.range(comma));
                printer.try_print_trivia_single_line_squished(comma_leading)?;
                printer.try_print_trivia_single_line_squished(comma_trailing)?;
            }
        }
        let (close_leading, _) = printer.trivia.get_for_range_split(printer.range(close));
        printer.try_print_trivia_single_line_squished(close_leading)?;
        printer.print_token(close);
        if printer.output.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
    }

    fn can_hug(self, tree: &FileTree) -> bool {
        tree.list(tree.get(self).args)
            .split_last()
            .is_some_and(|(SepItem { item: arg, .. }, _)| arg.is_huggable(tree))
    }

    fn try_print_hug(self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        let tree = printer.tree;
        let node = tree.get(self);
        let close = present(node.r);
        let (
            SepItem {
                item: last_arg,
                sep: last_comma,
            },
            init,
        ) = tree.list(node.args).split_last()?;
        if !last_arg.is_huggable(tree) {
            return None;
        }
        printer.print_token(node.l);
        let (_, open_trailing) = printer.trivia.get_for_range_split(printer.range(node.l));
        printer.try_print_trivia_single_line_squished(open_trailing)?;
        for SepItem { item: arg, sep } in init {
            let (arg_leading, arg_trailing) = printer.trivia.get_for_element(tree, *arg);
            printer.try_print_trivia_single_line_squished(arg_leading)?;
            if printer
                .print(*arg, Shape::unlimited_single_line())
                .multi_lined
            {
                return None;
            }
            printer.try_print_trivia_single_line_squished(arg_trailing)?;
            if let Some(comma) = *sep {
                let (comma_leading, comma_trailing) =
                    printer.trivia.get_for_range_split(printer.range(comma));
                printer.try_print_trivia_single_line_squished(comma_leading)?;
                printer.print_token(comma);
                printer.try_print_trivia_single_line_squished(comma_trailing)?;
            } else {
                printer.print_str(",");
            }
            printer.print_str(" ");
        }
        let (last_leading, last_trailing) = printer.trivia.get_for_element(tree, *last_arg);
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
        printer.print(*last_arg, arg_shape);
        printer.try_print_trivia_single_line_squished(last_trailing)?;
        if let Some(comma) = *last_comma {
            let (comma_leading, comma_trailing) =
                printer.trivia.get_for_range_split(printer.range(comma));
            printer.try_print_trivia_single_line_squished(comma_leading)?;
            printer.try_print_trivia_single_line_squished(comma_trailing)?;
        }
        let (close_leading, _) = printer.trivia.get_for_range_split(printer.range(close));
        printer.try_print_trivia_single_line_squished(close_leading)?;
        printer.print_token(close);
        Some(PrintInfo::default_multi_lined())
    }
}

impl Printable for Id<ArgList> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.with_line_separated(false, |printer| {
            printer
                .try_sub_printer(|p| self.try_print_single_line(&shape, p))
                .or_else(|| printer.try_sub_printer(|p| self.try_print_hug(&shape, p)))
                .unwrap_or_else(|| self.print_multi_line(shape, printer))
        })
    }
}

// ── indexing ─────────────────────────────────────────────────────────────────

/// `[index]` of an index expression, optional or not.
#[derive(Clone, Copy)]
pub(crate) struct IndexArgs {
    open_bracket: Token<punct::LBracket>,
    index: AnyExpr,
    close_bracket: Token<punct::RBracket>,
}

impl Spanned for IndexArgs {
    fn tokens(self, tree: &FileTree) -> TokenRange {
        span_between(self.open_bracket, self.close_bracket, tree).0
    }
}

impl IndexArgs {
    fn of_index(id: Id<IndexExpr>, tree: &FileTree) -> Self {
        let node = tree.get(id);
        Self {
            open_bracket: node.l,
            index: present(node.index),
            close_bracket: present(node.r),
        }
    }

    fn of_optional_index(id: Id<OptionalIndexExpr>, tree: &FileTree) -> Self {
        let node = tree.get(id);
        Self {
            open_bracket: node.l,
            index: present(node.index),
            close_bracket: present(node.r),
        }
    }

    fn single_line_width(self, input: &Printer<'_>) -> Option<usize> {
        let tree = input.tree;
        let mut len = const { "[]".len() };
        len += self.index.single_line_width(input)?;
        let (_, open_trailing) = input
            .trivia
            .get_for_range_split(input.range(self.open_bracket));
        len += open_trailing.try_squished_len(input.input)?;
        let (index_leading, index_trailing) = input.trivia.get_for_element(tree, self.index);
        len += index_leading.try_squished_len(input.input)?;
        len += index_trailing.try_squished_len(input.input)?;
        let (close_leading, _) = input
            .trivia
            .get_for_range_split(input.range(self.close_bracket));
        len += close_leading.try_squished_len(input.input)?;
        Some(len)
    }

    fn try_print_single_line(self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        let tree = printer.tree;
        printer.print_token(self.open_bracket);
        let (_, open_trailing) = printer
            .trivia
            .get_for_range_split(printer.range(self.open_bracket));
        printer.try_print_trivia_single_line_squished(open_trailing)?;
        let (index_leading, index_trailing) = printer.trivia.get_for_element(tree, self.index);
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
            .get_for_range_split(printer.range(self.close_bracket));
        printer.try_print_trivia_single_line_squished(close_leading)?;
        printer.print_token(self.close_bracket);
        if printer.output.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
    }
}

impl PrintMultiLine for IndexArgs {
    fn print_multi_line(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let inner_indent = shape.indent + printer.config.indent_width;
        printer.print_token(self.open_bracket);
        printer.print_trivia_all_trailing_for(printer.range(self.open_bracket));
        printer.print_newline();
        let (index_leading, index_trailing) = printer.trivia.get_for_element(tree, self.index);
        printer.print_trivia_with_newline(index_leading.trim_blanks(), inner_indent);
        printer.print_spaces(inner_indent);
        let inner_shape = Shape::standalone(printer.config.line_width, inner_indent);
        printer.print(self.index, inner_shape);
        printer.print_trivia_trailing(index_trailing);
        printer.print_newline();
        let (close_leading, _) = printer
            .trivia
            .get_for_range_split(printer.range(self.close_bracket));
        printer.print_trivia_with_newline(close_leading.trim_blanks(), inner_indent);
        printer.print_spaces(shape.indent);
        printer.print_token(self.close_bracket);
        PrintInfo::default_multi_lined()
    }
}

impl Printable for IndexArgs {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.with_line_separated(false, |printer| {
            printer
                .try_sub_printer(|p| self.try_print_single_line(&shape, p))
                .unwrap_or_else(|| self.print_multi_line(shape, printer))
        })
    }
}

pub(crate) trait IndexExprExt: Copy {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize>;
}
impl IndexExprExt for Id<IndexExpr> {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize> {
        let node = input.tree.get(self);
        let base = node
            .base
            .effective_postfix_operand(input)
            .single_line_width(input)?;
        Some(base + IndexArgs::of_index(self, input.tree).single_line_width(input)?)
    }
}

impl PrintMultiLine for Id<IndexExpr> {
    fn print_multi_line(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let node = printer.tree.get(self);
        let base = node.base.effective_postfix_operand(printer);
        printer.print(base, shape);
        IndexArgs::of_index(self, printer.tree).print_multi_line(shape, printer)
    }
}

fn index_single_line(id: Id<IndexExpr>, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
    let node = printer.tree.get(id);
    let base = node.base.effective_postfix_operand(printer);
    let args = IndexArgs::of_index(id, printer.tree);
    let base_len = base.single_line_width(printer)?;
    let args_len = args.single_line_width(printer)?;
    if base_len + args_len > shape.width {
        return None;
    }
    if printer
        .print(base, Shape::unlimited_single_line())
        .multi_lined
    {
        return None;
    }
    if printer
        .print(args, Shape::unlimited_single_line())
        .multi_lined
    {
        return None;
    }
    Some(PrintInfo::default_single_line())
}

impl Printable for Id<IndexExpr> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| index_single_line(self, &shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
}

pub(crate) trait FieldAccessExt: Copy {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize>;
}
impl FieldAccessExt for Id<FieldAccessExpr> {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize> {
        let tree = input.tree;
        let node = tree.get(self);
        let base = node
            .base
            .effective_postfix_operand(input)
            .single_line_width(input)?;
        Some(
            base + usize::from(input.range(node.dot).len())
                + usize::from(present(node.name).rightmost_token(tree).len()),
        )
    }
}
impl FieldAccessExt for Id<OptionalFieldAccessExpr> {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize> {
        let tree = input.tree;
        let node = tree.get(self);
        let base = node
            .base
            .effective_postfix_operand(input)
            .single_line_width(input)?;
        Some(
            base + usize::from(input.range(node.qdot).len())
                + usize::from(present(node.name).rightmost_token(tree).len()),
        )
    }
}

pub(crate) trait OptionalIndexExt: Copy {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize>;
}
impl OptionalIndexExt for Id<OptionalIndexExpr> {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize> {
        let node = input.tree.get(self);
        let base = node
            .base
            .effective_postfix_operand(input)
            .single_line_width(input)?;
        Some(
            base + usize::from(input.range(node.qdot).len())
                + IndexArgs::of_optional_index(self, input.tree).single_line_width(input)?,
        )
    }
}

impl Printable for Id<OptionalIndexExpr> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let node = printer.tree.get(self);
        let mut multi_lined = false;
        let base = node.base.effective_postfix_operand(printer);
        multi_lined |= printer.print(base, shape).multi_lined;
        printer.print_token(node.qdot);
        multi_lined |= printer
            .print(IndexArgs::of_optional_index(self, printer.tree), shape)
            .multi_lined;
        PrintInfo { multi_lined }
    }
}

pub(crate) trait OptionalCallExt: Copy {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize>;
}
impl OptionalCallExt for Id<OptionalCallExpr> {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize> {
        let node = input.tree.get(self);
        let callee = node
            .callee
            .effective_call_operand(input)
            .single_line_width(input)?;
        let args = node.args.single_line_width(input)?;
        Some(callee + usize::from(input.range(node.qdot).len()) + args)
    }
}

impl Printable for Id<OptionalCallExpr> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let node = printer.tree.get(self);
        let mut multi_lined = false;
        let callee = node.callee.effective_call_operand(printer);
        multi_lined |= printer.print(callee, shape).multi_lined;
        printer.print_token(node.qdot);
        multi_lined |= printer.print(node.args, shape).multi_lined;
        PrintInfo { multi_lined }
    }
}

pub(crate) trait EnvAccessExt: Copy {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize>;
}
impl EnvAccessExt for Id<EnvAccessExpr> {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize> {
        let tree = input.tree;
        let node = tree.get(self);
        Some(
            usize::from(input.range(node.env).len())
                + usize::from(input.range(node.dot).len())
                + usize::from(present(node.name).rightmost_token(tree).len()),
        )
    }
}

impl Printable for Id<EnvAccessExpr> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let node = printer.tree.get(self);
        printer.print_token(node.env);
        printer.print_token(node.dot);
        printer.print(present(node.name), shape);
        PrintInfo::default_single_line()
    }
}

// ── blocks ───────────────────────────────────────────────────────────────────

impl Printable for Id<Block> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let close = present(node.r);
        let stmts = tree.list(node.stmts);
        printer.with_line_separated(true, |printer| {
            if stmts.is_empty()
                && node.tail.is_none()
                && printer.try_print_empty_braces(node.l, close)
            {
                return PrintInfo::default_single_line();
            }
            printer.print_token(node.l);
            printer.print_trivia_all_trailing_for(printer.range(node.l));
            printer.print_newline();
            let inner_indent = shape.indent + printer.config.indent_width;
            if let Some((first, rest)) = stmts.split_first() {
                let closing = |index: usize| index + 1 == stmts.len() && node.tail.is_none();
                let (first_leading, first_trailing) = printer.trivia.get_for_element(tree, *first);
                printer
                    .print_trivia_with_newline(first_leading.trim_leading_blanks(), inner_indent);
                printer.print_spaces(inner_indent);
                let inner_shape = Shape::standalone(printer.config.line_width, inner_indent);
                match *first {
                    AnyStmt::Assign(assign) if closing(0) => {
                        printer.print(ClosingAssign(assign), inner_shape);
                    }
                    _ => {
                        printer.print(*first, inner_shape);
                    }
                }
                printer.print_trivia_trailing(first_trailing);
                printer.print_newline();
                for (index, stmt) in rest.iter().enumerate() {
                    match *stmt {
                        AnyStmt::Assign(assign) if closing(index + 1) => {
                            printer
                                .print_standalone_with_trivia(ClosingAssign(assign), inner_indent);
                        }
                        _ => printer.print_standalone_with_trivia(*stmt, inner_indent),
                    }
                    printer.print_newline();
                }
            }
            if let Some(expr) = node.tail {
                let (expr_leading, expr_trailing) = printer.trivia.get_for_element(tree, expr);
                let expr_leading = if stmts.is_empty() {
                    expr_leading.trim_leading_blanks()
                } else {
                    expr_leading
                };
                printer.print_trivia_with_newline(expr_leading, inner_indent);
                printer.print_spaces(inner_indent);
                let inner_shape = Shape::standalone(printer.config.line_width, inner_indent);
                printer.print(expr, inner_shape);
                // A jump ends the block like a statement, `;` included.
                if matches!(
                    expr,
                    AnyExpr::Return(_) | AnyExpr::Break(_) | AnyExpr::Continue(_)
                ) {
                    printer.print_str(";");
                }
                printer.print_trivia_trailing(expr_trailing);
                printer.print_newline();
            }
            let (close_brace_leading, _) = printer.trivia.get_for_range_split(printer.range(close));
            printer.print_trivia_with_newline(
                close_brace_leading.trim_trailing_blanks(),
                inner_indent,
            );
            printer.print_spaces(shape.indent);
            printer.print_token(close);
            PrintInfo { multi_lined: true }
        })
    }
}

// ── array literals ───────────────────────────────────────────────────────────

impl PrintMultiLine for Id<ArrayLiteral> {
    fn print_multi_line(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let close = present(node.r);
        printer.print_token(node.l);
        printer.print_trivia_all_trailing_for(printer.range(node.l));
        printer.print_newline();
        let inner_indent = shape.indent + printer.config.indent_width;
        for ArrayElement { value: elem, comma } in tree.list(node.elements) {
            let (elem_leading, elem_trailing) = printer.trivia.get_for_element(tree, *elem);
            printer.print_trivia_with_newline(elem_leading.trim_blanks(), inner_indent);
            printer.print_spaces(inner_indent);
            let inner_shape = Shape::standalone(printer.config.line_width, inner_indent);
            printer.print(*elem, inner_shape);
            if let Some(comma) = *comma {
                printer.print_trivia_squished(elem_trailing);
                printer.print_token(comma);
                printer.print_trivia_all_trailing_for(printer.range(comma));
            } else {
                printer.print_str(",");
                printer.print_trivia_trailing(elem_trailing);
            }
            printer.print_newline();
        }
        let (close_bracket_leading, _) = printer.trivia.get_for_range_split(printer.range(close));
        printer
            .print_trivia_with_newline(close_bracket_leading.trim_trailing_blanks(), inner_indent);
        printer.print_spaces(shape.indent);
        printer.print_token(close);
        PrintInfo::default_multi_lined()
    }
}

pub(crate) trait ArrayLiteralExt: Copy {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize>;
}
impl ArrayLiteralExt for Id<ArrayLiteral> {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize> {
        let tree = input.tree;
        let node = tree.get(self);
        let elements = tree.list(node.elements);
        let mut len = const { "[".len() };
        let (_, open_trailing) = input.trivia.get_for_range_split(input.range(node.l));
        len += open_trailing.try_squished_len(input.input)?;
        for (i, ArrayElement { value: elem, comma }) in elements.iter().enumerate() {
            let (el_leading, el_trailing) = input.trivia.get_for_element(tree, *elem);
            len += el_leading.try_squished_len(input.input)?;
            len += elem.single_line_width(input)?;
            let is_last = i + 1 >= elements.len();
            if let Some(comma) = *comma {
                let (comma_leading, comma_trailing) =
                    input.trivia.get_for_range_split(input.range(comma));
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
        let (close_leading, _) = input
            .trivia
            .get_for_range_split(input.range(present(node.r)));
        len += close_leading.try_squished_len(input.input)?;
        len += const { "]".len() };
        Some(len)
    }
}

fn array_single_line(
    id: Id<ArrayLiteral>,
    shape: &Shape,
    printer: &mut Printer,
) -> Option<PrintInfo> {
    let tree = printer.tree;
    let node = tree.get(id);
    let close = present(node.r);
    let elements = tree.list(node.elements);
    printer.print_token(node.l);
    let (_, open_trailing) = printer.trivia.get_for_range_split(printer.range(node.l));
    printer.try_print_trivia_single_line_squished(open_trailing)?;
    for (i, ArrayElement { value: elem, comma }) in elements.iter().enumerate() {
        if printer.output.len() > shape.width {
            return None;
        }
        let (el_leading, el_trailing) = printer.trivia.get_for_element(tree, *elem);
        printer.try_print_trivia_single_line_squished(el_leading)?;
        if printer
            .print(*elem, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        let is_last = i + 1 >= elements.len();
        if let Some(comma) = *comma {
            let (comma_leading, comma_trailing) =
                printer.trivia.get_for_range_split(printer.range(comma));
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
    let (close_leading, _) = printer.trivia.get_for_range_split(printer.range(close));
    printer.try_print_trivia_single_line_squished(close_leading)?;
    printer.print_token(close);
    if printer.output.len() > shape.width {
        None
    } else {
        Some(PrintInfo::default_single_line())
    }
}

impl Printable for Id<ArrayLiteral> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.with_line_separated(false, |printer| {
            printer
                .try_sub_printer(|p| array_single_line(self, &shape, p))
                .unwrap_or_else(|| self.print_multi_line(shape, printer))
        })
    }
}

// ── object literals ──────────────────────────────────────────────────────────

union_printable!(AnyObjectCtor { Path, Generic });
union_printable!(AnyObjectMember { Field, Spread });
union_printable!(AnyObjectKey {
    Name,
    String,
    Backtick
});

pub(crate) trait ObjectMemberExt: Copy {
    /// The member without its comma: what trivia attaches around.
    fn core(self, tree: &FileTree) -> TokenSpan;
    fn comma(self, tree: &FileTree) -> Option<Token<punct::Comma>>;
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize>;
}

impl ObjectMemberExt for AnyObjectMember {
    fn core(self, tree: &FileTree) -> TokenSpan {
        match self {
            AnyObjectMember::Field(field) => {
                let node = tree.get(field);
                let last: TokenRange = match node.value {
                    Some(value) => present(value.value).tokens(tree),
                    None => node.key.tokens(tree),
                };
                TokenSpan(TokenRange::new(node.key.tokens(tree).start(), last.end()))
            }
            AnyObjectMember::Spread(spread) => {
                let node = tree.get(spread);
                span_between(node.dots, present(node.value), tree)
            }
            AnyObjectMember::Bogus(_) => unreachable!("the formatter prints only error-free trees"),
        }
    }
    fn comma(self, tree: &FileTree) -> Option<Token<punct::Comma>> {
        match self {
            AnyObjectMember::Field(field) => tree.get(field).comma,
            AnyObjectMember::Spread(spread) => tree.get(spread).comma,
            AnyObjectMember::Bogus(_) => unreachable!("the formatter prints only error-free trees"),
        }
    }
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize> {
        match self {
            AnyObjectMember::Field(field) => field.single_line_width(input),
            AnyObjectMember::Spread(spread) => spread.single_line_width(input),
            AnyObjectMember::Bogus(_) => unreachable!("the formatter prints only error-free trees"),
        }
    }
}

pub(crate) trait ObjectFieldExt: Copy {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize>;
}
impl ObjectFieldExt for Id<ObjectField> {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize> {
        let tree = input.tree;
        let node = tree.get(self);
        let name = node.key.single_line_width(input)?;
        let Some(field_value) = node.value else {
            return Some(name);
        };
        let value = present(field_value.value);
        let value_width = value.single_line_width(input)?;
        let mut trivia_len = 0usize;
        let (_, colon_trailing) = input
            .trivia
            .get_for_range_split(input.range(field_value.colon));
        for t in colon_trailing {
            trivia_len += t.single_line_len(input.input)?;
        }
        let value_leading = input.trivia.get_leading_for_element(tree, value);
        for t in value_leading {
            trivia_len += t.single_line_len(input.input)?;
        }
        Some(name + const { ": ".len() } + value_width + trivia_len)
    }
}

/// An object field prints without its comma; the literal prints that.
impl Printable for Id<ObjectField> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let mut multi_lined = false;
        multi_lined |= printer.print(node.key, shape).multi_lined;
        let Some(field_value) = node.value else {
            return PrintInfo { multi_lined };
        };
        let value = present(field_value.value);
        printer.print_token(field_value.colon);
        let (_, colon_trailing) = printer
            .trivia
            .get_for_range_split(printer.range(field_value.colon));
        printer.print_str(" ");
        printer.print_trivia_squished(colon_trailing);
        let value_leading = printer.trivia.get_leading_for_element(tree, value);
        printer.print_trivia_squished(value_leading);
        multi_lined |= printer.print(value, shape).multi_lined;
        PrintInfo { multi_lined }
    }
}

pub(crate) trait ObjectKeyExt: Copy {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize>;
}
impl ObjectKeyExt for AnyObjectKey {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize> {
        match self {
            AnyObjectKey::Name(name) => Some(input.text_of(name).len()),
            AnyObjectKey::String(s) => verbatim_width(input, s),
            AnyObjectKey::Backtick(s) => verbatim_width(input, s),
            AnyObjectKey::Bogus(_) => unreachable!("the formatter prints only error-free trees"),
        }
    }
}

pub(crate) trait SpreadExt: Copy {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize>;
}
impl SpreadExt for Id<SpreadElement> {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize> {
        let tree = input.tree;
        let node = tree.get(self);
        let value = present(node.value);
        let mut trivia_len = 0usize;
        let (_, dots_trailing) = input.trivia.get_for_range_split(input.range(node.dots));
        for t in dots_trailing {
            trivia_len += t.single_line_len(input.input)?;
        }
        let value_leading = input.trivia.get_leading_for_element(tree, value);
        for t in value_leading {
            trivia_len += t.single_line_len(input.input)?;
        }
        let value_width = value.single_line_width(input)?;
        Some(const { "...".len() } + value_width + trivia_len)
    }
}

impl Printable for Id<SpreadElement> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let value = present(node.value);
        printer.print_token(node.dots);
        let (_, dots_trailing) = printer.trivia.get_for_range_split(printer.range(node.dots));
        printer.print_trivia_squished(dots_trailing);
        let value_leading = printer.trivia.get_leading_for_element(tree, value);
        printer.print_trivia_squished(value_leading);
        printer.print(value, shape)
    }
}

/// Whether the braces hold members or comments.
fn braces_have_content(
    open: Token<punct::LBrace>,
    close: Token<punct::RBrace>,
    members: usize,
    input: &Printer<'_>,
) -> bool {
    let (_, open_trailing) = input.trivia.get_for_range_split(input.range(open));
    let (close_leading, _) = input.trivia.get_for_range_split(input.range(close));
    members > 0
        || open_trailing.iter().any(EmittableTrivia::is_comment)
        || close_leading.iter().any(EmittableTrivia::is_comment)
}

impl PrintMultiLine for Id<ObjectLiteral> {
    fn print_multi_line(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let close = present(node.r);
        let inner_shape = Shape {
            width: shape.width.saturating_sub(printer.config.indent_width),
            indent: shape.indent + printer.config.indent_width,
            first_line_offset: 0,
        };
        printer.print(node.ctor, Shape::unlimited_single_line());
        printer.print_str(" ");
        printer.print_token(node.l);
        printer.print_trivia_all_trailing_for(printer.range(node.l));
        printer.print_newline();
        for member in tree.list(node.members) {
            let core = member.core(tree);
            printer.print_trivia_all_leading_with_newline_for(
                core.leftmost_token(tree),
                inner_shape.indent,
            );
            printer.print_spaces(inner_shape.indent);
            printer.print(*member, inner_shape);
            if let Some(comma) = member.comma(tree) {
                printer.print_token(comma);
                printer.print_trivia_all_trailing_for(printer.range(comma));
            } else {
                printer.print_str(",");
                printer.print_trivia_all_trailing_for(core.rightmost_token(tree));
            }
            printer.print_newline();
        }
        printer.print_spaces(shape.indent);
        printer.print_trivia_all_leading_with_newline_for(printer.range(close), shape.indent);
        printer.print_token(close);
        PrintInfo::default_multi_lined()
    }
}

pub(crate) trait ObjectLiteralExt: Copy {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize>;
}
impl ObjectLiteralExt for Id<ObjectLiteral> {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize> {
        let tree = input.tree;
        let node = tree.get(self);
        let close = present(node.r);
        let members = tree.list(node.members);
        let (_, open_trailing) = input.trivia.get_for_range_split(input.range(node.l));
        let ctor_width = match node.ctor {
            AnyObjectCtor::Path(path) => path.single_line_width(input)?,
            AnyObjectCtor::Generic(ga) => ga.single_line_width(input)?,
            AnyObjectCtor::Bogus(_) => unreachable!("the formatter prints only error-free trees"),
        };
        let mut len = ctor_width
            + if braces_have_content(node.l, close, members.len(), input) {
                const { " {  }".len() }
            } else {
                const { " {}".len() }
            };
        len += open_trailing.try_squished_len(input.input)?;
        for (i, member) in members.iter().enumerate() {
            let core = member.core(tree);
            let (fld_leading, fld_trailing) = input.trivia.get_for_element(tree, core);
            len += fld_leading.try_squished_len(input.input)?;
            len += member.single_line_width(input)?;
            len += fld_trailing.try_squished_len(input.input)?;
            if i + 1 < members.len() {
                if let Some(comma) = member.comma(tree) {
                    let (comma_leading, comma_trailing) =
                        input.trivia.get_for_range_split(input.range(comma));
                    len += comma_leading.try_squished_len(input.input)?;
                    len += 1; // ","
                    len += comma_trailing.try_squished_len(input.input)?;
                } else {
                    len += 1; // ","
                }
                len += 1; // " "
            } else if let Some(comma) = member.comma(tree) {
                let (comma_leading, comma_trailing) =
                    input.trivia.get_for_range_split(input.range(comma));
                len += comma_leading.try_squished_len(input.input)?;
                len += comma_trailing.try_squished_len(input.input)?;
            }
        }
        let (close_leading, _) = input.trivia.get_for_range_split(input.range(close));
        len += close_leading.try_squished_len(input.input)?;
        Some(len)
    }
}

fn object_single_line(
    id: Id<ObjectLiteral>,
    shape: &Shape,
    printer: &mut Printer,
) -> Option<PrintInfo> {
    let tree = printer.tree;
    let node = tree.get(id);
    let close = present(node.r);
    let members = tree.list(node.members);
    let has_content = braces_have_content(node.l, close, members.len(), printer);
    printer.print(node.ctor, Shape::unlimited_single_line());
    printer.print_str(" ");
    printer.print_token(node.l);
    if has_content {
        printer.print_str(" ");
    }
    let (_, open_trailing) = printer.trivia.get_for_range_split(printer.range(node.l));
    printer.try_print_trivia_single_line_squished(open_trailing)?;
    for (i, member) in members.iter().enumerate() {
        if printer.output.len() > shape.width {
            return None;
        }
        let core = member.core(tree);
        let (fld_leading, fld_trailing) = printer.trivia.get_for_element(tree, core);
        printer.try_print_trivia_single_line_squished(fld_leading)?;
        if printer
            .print(*member, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        printer.try_print_trivia_single_line_squished(fld_trailing)?;
        if i + 1 < members.len() {
            if let Some(comma) = member.comma(tree) {
                let (comma_leading, comma_trailing) =
                    printer.trivia.get_for_range_split(printer.range(comma));
                printer.try_print_trivia_single_line_squished(comma_leading)?;
                printer.print_token(comma);
                printer.try_print_trivia_single_line_squished(comma_trailing)?;
            } else {
                printer.print_str(",");
            }
            printer.print_str(" ");
        } else if let Some(comma) = member.comma(tree) {
            let (comma_leading, comma_trailing) =
                printer.trivia.get_for_range_split(printer.range(comma));
            printer.try_print_trivia_single_line_squished(comma_leading)?;
            printer.try_print_trivia_single_line_squished(comma_trailing)?;
        }
    }
    let (close_leading, _) = printer.trivia.get_for_range_split(printer.range(close));
    printer.try_print_trivia_single_line_squished(close_leading)?;
    if has_content {
        printer.print_str(" ");
    }
    printer.print_token(close);
    if printer.output.len() > shape.width {
        None
    } else {
        Some(PrintInfo::default_single_line())
    }
}

impl Printable for Id<ObjectLiteral> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.with_line_separated(false, |printer| {
            printer
                .try_sub_printer(|p| object_single_line(self, &shape, p))
                .unwrap_or_else(|| self.print_multi_line(shape, printer))
        })
    }
}

// ── map literals ─────────────────────────────────────────────────────────────

pub(crate) trait MapEntryExt: Copy {
    /// The entry without its comma: what trivia attaches around.
    fn core(self, tree: &FileTree) -> TokenSpan;
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize>;
}
impl MapEntryExt for Id<MapEntry> {
    fn core(self, tree: &FileTree) -> TokenSpan {
        let node = tree.get(self);
        span_between(node.key, present(node.value), tree)
    }
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize> {
        let tree = input.tree;
        let node = tree.get(self);
        let value = present(node.value);
        let colon = present(node.colon);
        let name = node.key.single_line_width(input)?;
        let value_width = value.single_line_width(input)?;
        let mut trivia_len = 0usize;
        let (_, colon_trailing) = input.trivia.get_for_range_split(input.range(colon));
        for t in colon_trailing {
            trivia_len += t.single_line_len(input.input)?;
        }
        let value_leading = input.trivia.get_leading_for_element(tree, value);
        for t in value_leading {
            trivia_len += t.single_line_len(input.input)?;
        }
        Some(name + const { ": ".len() } + value_width + trivia_len)
    }
}

/// A map entry prints without its comma; the literal prints that.
impl Printable for Id<MapEntry> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let value = present(node.value);
        let colon = present(node.colon);
        let mut multi_lined = false;
        multi_lined |= printer.print(node.key, shape).multi_lined;
        printer.print_token(colon);
        let (_, colon_trailing) = printer.trivia.get_for_range_split(printer.range(colon));
        printer.print_str(" ");
        printer.print_trivia_squished(colon_trailing);
        let value_leading = printer.trivia.get_leading_for_element(tree, value);
        printer.print_trivia_squished(value_leading);
        multi_lined |= printer.print(value, shape).multi_lined;
        PrintInfo { multi_lined }
    }
}

impl PrintMultiLine for Id<MapLiteral> {
    fn print_multi_line(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let close = present(node.r);
        let inner_shape = Shape {
            width: shape.width.saturating_sub(printer.config.indent_width),
            indent: shape.indent + printer.config.indent_width,
            first_line_offset: 0,
        };
        if let Some(prefix) = node.map_kw {
            printer.print_token(prefix);
            let (_, line_comment) = printer.print_trivia_all_trailing_for(printer.range(prefix));
            let (leading, _) = printer.trivia.get_for_range_split(printer.range(node.l));
            if line_comment || !leading.is_empty() {
                printer.print_newline();
                printer.print_trivia_with_newline(leading, shape.indent);
                printer.print_spaces(shape.indent);
            } else {
                printer.print_str(" ");
            }
        }
        printer.print_token(node.l);
        printer.print_trivia_all_trailing_for(printer.range(node.l));
        printer.print_newline();
        for entry in tree.list(node.entries) {
            let core = entry.core(tree);
            printer.print_trivia_all_leading_with_newline_for(
                core.leftmost_token(tree),
                inner_shape.indent,
            );
            printer.print_spaces(inner_shape.indent);
            printer.print(*entry, inner_shape);
            if let Some(comma) = tree.get(*entry).comma {
                printer.print_token(comma);
                printer.print_trivia_all_trailing_for(printer.range(comma));
            } else {
                printer.print_str(",");
                printer.print_trivia_all_trailing_for(core.rightmost_token(tree));
            }
            printer.print_newline();
        }
        printer.print_trivia_all_leading_with_newline_for(printer.range(close), inner_shape.indent);
        printer.print_spaces(shape.indent);
        printer.print_token(close);
        PrintInfo::default_multi_lined()
    }
}

pub(crate) trait MapLiteralExt: Copy {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize>;
}
impl MapLiteralExt for Id<MapLiteral> {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize> {
        let tree = input.tree;
        let node = tree.get(self);
        let close = present(node.r);
        let entries = tree.list(node.entries);
        let (_, open_trailing) = input.trivia.get_for_range_split(input.range(node.l));
        let (close_leading, _) = input.trivia.get_for_range_split(input.range(close));
        let has_content = braces_have_content(node.l, close, entries.len(), input);
        let mut len = if has_content {
            const { "{  }".len() }
        } else {
            const { "{}".len() }
        };
        if let Some(prefix) = node.map_kw {
            len += const { "map ".len() };
            let (_, trailing) = input.trivia.get_for_range_split(input.range(prefix));
            let (leading, _) = input.trivia.get_for_range_split(input.range(node.l));
            len += trailing.try_squished_len(input.input)?;
            len += leading.try_squished_len(input.input)?;
            if trailing
                .iter()
                .chain(leading)
                .any(EmittableTrivia::is_comment)
            {
                len += 1;
            }
        }
        for t in open_trailing {
            len += t.single_line_len(input.input)?;
        }
        for (i, entry) in entries.iter().enumerate() {
            let core = entry.core(tree);
            let comma = tree.get(*entry).comma;
            let (fld_leading, fld_trailing) = input.trivia.get_for_element(tree, core);
            for t in fld_leading {
                len += t.single_line_len(input.input)?;
            }
            len += entry.single_line_width(input)?;
            for t in fld_trailing {
                len += t.single_line_len(input.input)?;
            }
            if i + 1 < entries.len() {
                if let Some(comma) = comma {
                    let (comma_leading, comma_trailing) =
                        input.trivia.get_for_range_split(input.range(comma));
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
                let (comma_leading, comma_trailing) =
                    input.trivia.get_for_range_split(input.range(comma));
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
}

fn map_single_line(id: Id<MapLiteral>, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
    let tree = printer.tree;
    let node = tree.get(id);
    let close = present(node.r);
    let entries = tree.list(node.entries);
    let (_, open_trailing) = printer.trivia.get_for_range_split(printer.range(node.l));
    let (close_leading, _) = printer.trivia.get_for_range_split(printer.range(close));
    let has_content = braces_have_content(node.l, close, entries.len(), printer);
    if let Some(prefix) = node.map_kw {
        printer.print_token(prefix);
        let (_, trailing) = printer.trivia.get_for_range_split(printer.range(prefix));
        let (leading, _) = printer.trivia.get_for_range_split(printer.range(node.l));
        printer.print_str(" ");
        printer.try_print_trivia_single_line_squished(trailing)?;
        printer.try_print_trivia_single_line_squished(leading)?;
        if trailing
            .iter()
            .chain(leading)
            .any(EmittableTrivia::is_comment)
        {
            printer.print_str(" ");
        }
    }
    printer.print_token(node.l);
    if has_content {
        printer.print_str(" ");
    }
    printer.try_print_trivia_single_line_squished(open_trailing)?;
    for (i, entry) in entries.iter().enumerate() {
        if printer.output.len() > shape.width {
            return None;
        }
        let core = entry.core(tree);
        let comma = tree.get(*entry).comma;
        let (fld_leading, fld_trailing) = printer.trivia.get_for_element(tree, core);
        printer.try_print_trivia_single_line_squished(fld_leading)?;
        if printer
            .print(*entry, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        printer.try_print_trivia_single_line_squished(fld_trailing)?;
        if i + 1 < entries.len() {
            if let Some(comma) = comma {
                let (comma_leading, comma_trailing) =
                    printer.trivia.get_for_range_split(printer.range(comma));
                printer.try_print_trivia_single_line_squished(comma_leading)?;
                printer.print_token(comma);
                printer.try_print_trivia_single_line_squished(comma_trailing)?;
            } else {
                printer.print_str(",");
            }
            printer.print_str(" ");
        } else if let Some(comma) = comma {
            let (comma_leading, comma_trailing) =
                printer.trivia.get_for_range_split(printer.range(comma));
            printer.try_print_trivia_single_line_squished(comma_leading)?;
            printer.try_print_trivia_single_line_squished(comma_trailing)?;
        }
    }
    printer.try_print_trivia_single_line_squished(close_leading)?;
    if has_content {
        printer.print_str(" ");
    }
    printer.print_token(close);
    if printer.output.len() > shape.width {
        None
    } else {
        Some(PrintInfo::default_single_line())
    }
}

impl Printable for Id<MapLiteral> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.with_line_separated(false, |printer| {
            printer
                .try_sub_printer(|p| map_single_line(self, &shape, p))
                .unwrap_or_else(|| self.print_multi_line(shape, printer))
        })
    }
}

// ── generics ─────────────────────────────────────────────────────────────────

impl Printable for Id<GenericParamList> {
    fn print(self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let params = tree.list(node.params);
        printer.print_token(node.l);
        for (i, SepItem { item: param, .. }) in params.iter().enumerate() {
            let param = tree.get(*param);
            printer.print(param.name, Shape::unlimited_single_line());
            if let Some(bounds) = param.bounds {
                printer.print_str(" ");
                printer.print(bounds, Shape::unlimited_single_line());
            }
            if i + 1 < params.len() {
                printer.print_str(", ");
            }
        }
        printer.print_token(present(node.r));
        PrintInfo::default_single_line()
    }
}

impl Printable for Id<GenericParamBounds> {
    fn print(self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        printer.print_token(node.kw);
        printer.print_str(" ");
        printer.print(present(node.first), Shape::unlimited_single_line());
        for arm in tree.list(node.rest) {
            printer.print_str(" & ");
            printer.print(present(arm.ty), Shape::unlimited_single_line());
        }
        PrintInfo::default_single_line()
    }
}

pub(crate) trait GenericArgsExt: Copy {
    /// The width of the arguments as printed: `<a, b>`, which tracks what
    /// the printer emits rather than the source spacing.
    fn formatted_single_line_width(self, printer: &Printer<'_>) -> usize;
}
impl GenericArgsExt for Id<GenericArgs> {
    fn formatted_single_line_width(self, printer: &Printer<'_>) -> usize {
        let tree = printer.tree;
        let node = tree.get(self);
        let args = tree.list(node.args);
        let mut len: usize = 2; // `<` and `>`
        for (i, SepItem { item: ty, .. }) in args.iter().enumerate() {
            let left = ty.leftmost_token(tree);
            let right = ty.rightmost_token(tree);
            len += usize::from(right.end() - left.start());
            if i + 1 < args.len() {
                len += 2; // `, `
            }
        }
        len
    }
}

impl Printable for Id<GenericArgs> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let args = tree.list(node.args);
        printer.print_token(node.l);
        for (i, SepItem { item: ty, .. }) in args.iter().enumerate() {
            printer.print(*ty, shape);
            if i + 1 < args.len() {
                printer.print_str(", ");
            }
        }
        printer.print_token(present(node.r));
        PrintInfo::default_single_line()
    }
}

// ── lambdas and spawns ───────────────────────────────────────────────────────

impl Printable for Id<LambdaExpr> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let arrow = present(node.arrow);
        let block = present(node.body);
        let continuation = shape.indent + printer.config.indent_width;
        printer.print(node.params, shape);
        // `=>` is accepted in place of `->` and printed as the latter.
        printer.print_str(" ->");
        if let Some(ret) = node.ret {
            printer.print_separator(
                printer.range(arrow),
                Some(ret.leftmost_token(tree)),
                continuation,
                " ",
            );
            printer.print(ret, shape);
            if let Some(throws) = node.throws {
                printer.print_str(" ");
                printer.print(throws, shape);
            }
            printer.print_str(" ");
        } else if let Some(throws) = node.throws {
            printer.print_separator(
                printer.range(arrow),
                Some(throws.leftmost_token(tree)),
                continuation,
                " ",
            );
            printer.print(throws, shape);
            printer.print_str(" ");
        } else {
            printer.print_separator(
                printer.range(arrow),
                Some(block.leftmost_token(tree)),
                continuation,
                " ",
            );
        }
        printer.print(block, shape);
        PrintInfo::default_multi_lined()
    }
}

/// `spawn [name] [with a, b]`: the part before the block.
fn spawn_header_range(id: Id<SpawnExpr>, printer: &Printer<'_>) -> TextRange {
    let tree = printer.tree;
    let node = tree.get(id);
    TextRange::new(
        printer.range(node.kw).start(),
        printer.range(tree.get(present(node.body)).l).start(),
    )
}

fn spawn_header_requires_verbatim(id: Id<SpawnExpr>, input: &Printer<'_>) -> bool {
    let range = spawn_header_range(id, input);
    input.trivia.all_trivia().iter().any(|trivia| {
        let attached_at = trivia.attached_to().start();
        trivia.is_comment() && attached_at >= range.start() && attached_at <= range.end()
    })
}

fn spawn_header_single_line_width(id: Id<SpawnExpr>, input: &Printer<'_>) -> Option<usize> {
    let tree = input.tree;
    let node = tree.get(id);
    if spawn_header_requires_verbatim(id, input) {
        return None;
    }
    let mut len = usize::from(input.range(node.kw).len());
    if let Some(name) = node.name {
        len += const { " ".len() } + name.single_line_width(input)?;
    }
    if let Some(with) = node.with {
        let with = tree.get(with);
        len += const { " ".len() } + usize::from(input.range(with.kw).len());
        for (i, SepItem { item: expr, .. }) in tree.list(with.items).iter().enumerate() {
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

pub(crate) trait SpawnExt: Copy {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize>;
}
impl SpawnExt for Id<SpawnExpr> {
    fn single_line_width(self, input: &Printer<'_>) -> Option<usize> {
        let tree = input.tree;
        let node = tree.get(self);
        let block = tree.get(present(node.body));
        if !tree.list(block.stmts).is_empty() {
            return None;
        }
        let header = spawn_header_single_line_width(self, input)?;
        let close = present(block.r);
        let (_, open_trailing) = input.trivia.get_for_range_split(input.range(block.l));
        let (close_leading, _) = input.trivia.get_for_range_split(input.range(close));
        let body = match block.tail {
            Some(tail) => {
                let (tail_leading, tail_trailing) = input.trivia.get_for_element(tree, tail);
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
}

fn spawn_print_header(id: Id<SpawnExpr>, shape: &Shape, printer: &mut Printer) -> bool {
    let tree = printer.tree;
    let node = tree.get(id);
    let mut multi_lined = false;
    printer.print_token(node.kw);
    if let Some(name) = node.name {
        printer.print_str(" ");
        multi_lined |= printer.print(name, *shape).multi_lined;
    }
    if let Some(with) = node.with {
        let with = tree.get(with);
        printer.print_str(" ");
        printer.print_token(with.kw);
        for (i, SepItem { item: expr, .. }) in tree.list(with.items).iter().enumerate() {
            printer.print_str(if i == 0 { " " } else { ", " });
            multi_lined |= printer.print(*expr, *shape).multi_lined;
        }
    }
    multi_lined
}

fn spawn_single_line(id: Id<SpawnExpr>, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
    let tree = printer.tree;
    let node = tree.get(id);
    let block = tree.get(present(node.body));
    let close = present(block.r);
    if !tree.list(block.stmts).is_empty() || spawn_header_requires_verbatim(id, printer) {
        return None;
    }
    if spawn_print_header(id, &Shape::unlimited_single_line(), printer) {
        return None;
    }
    printer.print_str(" ");
    let (_, open_trailing) = printer.trivia.get_for_range_split(printer.range(block.l));
    let (close_leading, _) = printer.trivia.get_for_range_split(printer.range(close));
    match block.tail {
        Some(tail) => {
            printer.print_token(block.l);
            printer.print_str(" ");
            printer.try_print_trivia_single_line_squished(open_trailing)?;
            let (tail_leading, tail_trailing) = printer.trivia.get_for_element(tree, tail);
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
            printer.print_token(close);
        }
        None => {
            if open_trailing.iter().any(EmittableTrivia::is_comment)
                || close_leading.iter().any(EmittableTrivia::is_comment)
            {
                return None;
            }
            printer.print_token(block.l);
            printer.print_token(close);
        }
    }
    if printer.output.len() > shape.width {
        None
    } else {
        Some(PrintInfo::default_single_line())
    }
}

impl PrintMultiLine for Id<SpawnExpr> {
    fn print_multi_line(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let block = present(printer.tree.get(self).body);
        if spawn_header_requires_verbatim(self, printer) {
            let range = spawn_header_range(self, printer);
            printer.print_input_range(range);
        } else {
            spawn_print_header(self, &shape, printer);
            printer.print_str(" ");
        }
        printer.print(block, shape);
        PrintInfo::default_multi_lined()
    }
}

impl Printable for Id<SpawnExpr> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| spawn_single_line(self, &shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
}

// ── postfix chains ───────────────────────────────────────────────────────────

/// The head of a postfix chain.
#[derive(Clone, Copy)]
enum ChainFirst {
    /// A bare name, from a path.
    Name(Id<Name>),
    Expr(AnyExpr),
}

/// Only used for printing chained expressions.
///
/// Needed to re-organize before printing from a hierarchical structure to a flat-ish one.
pub(crate) struct PrintChain {
    first: ChainFirst,
    /// Will always start with a field access (if not empty), since calls/indexes will be included in `first` if not following a field access.
    chain_members: Vec<PrintChainItem>,
}

/// Only used for printing chained expressions. See [`PrintChain`].
#[derive(Clone, Copy)]
enum PrintChainItem {
    FieldAccess(Token<MemberDot>, Id<Name>),
    OptionalFieldAccess(Token<punct::QuestionDot>, Id<Name>),
    Index(IndexArgs),
    OptionalIndex(Token<punct::QuestionDot>, IndexArgs),
    Call(Id<ArgList>),
    OptionalCall(Token<punct::QuestionDot>, Id<ArgList>),
    GenericArgs(Id<GenericArgs>),
}

impl PrintChain {
    /// Builds the flat chain for a postfix spine.
    ///
    /// Every receiver is taken through `effective_postfix_operand` so
    /// redundant parens around it peel and the walk continues through them.
    /// A paren that survives (looser-binding receiver, or one carrying a
    /// comment) still terminates the walk and becomes `first`, which is what
    /// puts it on its own indent level.
    pub(crate) fn new(from: AnyExpr, printer: &Printer<'_>) -> Self {
        let tree = printer.tree;
        let from = from.effective_postfix_operand(printer);
        match from {
            AnyExpr::Path(path_expr) => {
                let node = tree.get(path_expr);
                let chain_members: Vec<PrintChainItem> = tree
                    .list(node.segments)
                    .iter()
                    .map(|segment| {
                        PrintChainItem::FieldAccess(segment.dot.widen(), present(segment.name))
                    })
                    .collect();
                Self {
                    first: ChainFirst::Name(node.head),
                    chain_members,
                }
            }
            AnyExpr::GenericApply(ga) => {
                let node = tree.get(ga);
                let mut chain = Self::new(node.base, printer);
                chain
                    .chain_members
                    .push(PrintChainItem::GenericArgs(node.args));
                chain
            }
            AnyExpr::Call(call_expr) => {
                let node = tree.get(call_expr);
                if matches!(
                    node.callee.effective_call_operand(printer),
                    AnyExpr::Paren(_)
                ) {
                    return Self {
                        first: ChainFirst::Expr(from),
                        chain_members: Vec::new(),
                    };
                }
                let mut chain = Self::new(node.callee, printer);
                if chain.chain_members.is_empty() {
                    Self {
                        first: ChainFirst::Expr(from),
                        chain_members: Vec::new(),
                    }
                } else {
                    chain.chain_members.push(PrintChainItem::Call(node.args));
                    chain
                }
            }
            AnyExpr::Index(index_expr) => {
                let node = tree.get(index_expr);
                let mut chain = Self::new(node.base, printer);
                if chain.chain_members.is_empty() {
                    Self {
                        first: ChainFirst::Expr(from),
                        chain_members: Vec::new(),
                    }
                } else {
                    chain
                        .chain_members
                        .push(PrintChainItem::Index(IndexArgs::of_index(index_expr, tree)));
                    chain
                }
            }
            AnyExpr::FieldAccess(field_access_expr) => {
                let node = tree.get(field_access_expr);
                let mut chain = Self::new(node.base, printer);
                chain
                    .chain_members
                    .push(PrintChainItem::FieldAccess(node.dot, present(node.name)));
                chain
            }
            AnyExpr::OptionalFieldAccess(ofa) => {
                let node = tree.get(ofa);
                let mut chain = Self::new(node.base, printer);
                chain
                    .chain_members
                    .push(PrintChainItem::OptionalFieldAccess(
                        node.qdot,
                        present(node.name),
                    ));
                chain
            }
            AnyExpr::OptionalIndex(oi) => {
                let node = tree.get(oi);
                let mut chain = Self::new(node.base, printer);
                chain.chain_members.push(PrintChainItem::OptionalIndex(
                    node.qdot,
                    IndexArgs::of_optional_index(oi, tree),
                ));
                chain
            }
            AnyExpr::OptionalCall(oc) => {
                let node = tree.get(oc);
                let mut chain = Self::new(node.callee, printer);
                chain
                    .chain_members
                    .push(PrintChainItem::OptionalCall(node.qdot, node.args));
                chain
            }
            base => Self {
                first: ChainFirst::Expr(base),
                chain_members: Vec::new(),
            },
        }
    }

    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let first_single_line = match self.first {
            ChainFirst::Name(name) => {
                printer.print(name, shape);
                true
            }
            ChainFirst::Expr(AnyExpr::Call(call_expr)) => {
                let first_info = printer.print(call_expr, shape);
                !first_info.multi_lined
            }
            ChainFirst::Expr(AnyExpr::Index(index_expr)) => {
                let first_info = printer.print(index_expr, shape);
                !first_info.multi_lined
            }
            ChainFirst::Expr(expr) => {
                let first_info = printer.print(expr, shape);
                !first_info.multi_lined
            }
        };
        let mut multi_lined = !first_single_line;
        let chain_indent = shape.indent + printer.config.indent_width;
        let mut line_remaining_width = printer.current_line_remaining_width();
        let mut rest: &[PrintChainItem] = &self.chain_members;
        while let Some((item, tail)) = rest.split_first() {
            if Self::is_plain_access(item) {
                break;
            }
            multi_lined |=
                Self::print_non_field_item(*item, chain_indent, &mut line_remaining_width, printer);
            rest = tail;
        }
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
            Self::print_plain_item(*item, printer);
        }
        rest = &rest[path_len..];
        line_remaining_width = printer.current_line_remaining_width();
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
                    Self::print_plain_item(*item, printer);
                    line_remaining_width = printer.current_line_remaining_width();
                } else {
                    multi_lined |= Self::print_non_field_item(
                        *item,
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

    const fn is_plain_access(item: &PrintChainItem) -> bool {
        matches!(
            item,
            PrintChainItem::FieldAccess(..)
                | PrintChainItem::OptionalFieldAccess(..)
                | PrintChainItem::GenericArgs(..)
        )
    }

    fn print_plain_item(item: PrintChainItem, printer: &mut Printer) {
        match item {
            PrintChainItem::FieldAccess(dot, word) => {
                printer.print_token(dot);
                printer.print(word, Shape::unlimited_single_line());
            }
            PrintChainItem::OptionalFieldAccess(qd, word) => {
                printer.print_token(qd);
                printer.print(word, Shape::unlimited_single_line());
            }
            PrintChainItem::GenericArgs(generic_args) => {
                printer.print(generic_args, Shape::unlimited_single_line());
            }
            PrintChainItem::Index(_)
            | PrintChainItem::OptionalIndex(..)
            | PrintChainItem::Call(_)
            | PrintChainItem::OptionalCall(..) => {
                unreachable!("print_plain_item called with a call/index item")
            }
        }
    }

    fn item_single_line_width(item: PrintChainItem, printer: &Printer<'_>) -> Option<usize> {
        let tree = printer.tree;
        match item {
            PrintChainItem::FieldAccess(dot, word) => Some(usize::from(
                printer.range(dot).len() + word.rightmost_token(tree).len(),
            )),
            PrintChainItem::OptionalFieldAccess(qd, word) => Some(usize::from(
                printer.range(qd).len() + word.rightmost_token(tree).len(),
            )),
            PrintChainItem::Index(index_args) => index_args.single_line_width(printer),
            PrintChainItem::OptionalIndex(qd, index_args) => {
                Some(usize::from(printer.range(qd).len()) + index_args.single_line_width(printer)?)
            }
            PrintChainItem::Call(call_args) => call_args.single_line_width(printer),
            PrintChainItem::OptionalCall(qd, call_args) => {
                Some(usize::from(printer.range(qd).len()) + call_args.single_line_width(printer)?)
            }
            PrintChainItem::GenericArgs(generic_args) => {
                Some(generic_args.formatted_single_line_width(printer))
            }
        }
    }

    fn group_single_line_width(group: &[PrintChainItem], printer: &Printer<'_>) -> Option<usize> {
        group
            .iter()
            .map(|item| Self::item_single_line_width(*item, printer))
            .sum()
    }

    fn print_non_field_item(
        item: PrintChainItem,
        chain_indent: usize,
        line_remaining_width: &mut usize,
        printer: &mut Printer,
    ) -> bool {
        let shape_now = |printer: &Printer| Shape {
            width: *line_remaining_width,
            indent: chain_indent,
            first_line_offset: printer.current_line_len().saturating_sub(chain_indent),
        };
        let multi_lined = match item {
            PrintChainItem::Index(index_args) => {
                let index_shape = shape_now(printer);
                printer.print(index_args, index_shape).multi_lined
            }
            PrintChainItem::OptionalIndex(qd, index_args) => {
                printer.print_token(qd);
                let index_shape = shape_now(printer);
                printer.print(index_args, index_shape).multi_lined
            }
            PrintChainItem::Call(call_args) => {
                let call_shape = shape_now(printer);
                printer.print(call_args, call_shape).multi_lined
            }
            PrintChainItem::OptionalCall(qd, call_args) => {
                printer.print_token(qd);
                let call_shape = shape_now(printer);
                printer.print(call_args, call_shape).multi_lined
            }
            PrintChainItem::FieldAccess(..)
            | PrintChainItem::OptionalFieldAccess(..)
            | PrintChainItem::GenericArgs(..) => {
                unreachable!("print_non_field_item called with a plain access item")
            }
        };
        *line_remaining_width = printer.current_line_remaining_width();
        multi_lined
    }

    fn try_print_members_single_line(
        &self,
        members: &[PrintChainItem],
        shape: &Shape,
        printer: &mut Printer,
    ) -> Option<()> {
        match self.first {
            ChainFirst::Name(name) => {
                printer.print(name, Shape::unlimited_single_line());
            }
            ChainFirst::Expr(
                AnyExpr::FieldAccess(..)
                | AnyExpr::OptionalFieldAccess(..)
                | AnyExpr::OptionalIndex(..)
                | AnyExpr::OptionalCall(..),
            ) => {
                unreachable!("Should have been unwrapped when the PrintChain was created")
            }
            ChainFirst::Expr(AnyExpr::Call(call_expr)) => {
                if printer
                    .print(call_expr, Shape::unlimited_single_line())
                    .multi_lined
                {
                    return None;
                }
            }
            ChainFirst::Expr(AnyExpr::Index(index_expr)) => {
                if printer
                    .print(index_expr, Shape::unlimited_single_line())
                    .multi_lined
                {
                    return None;
                }
            }
            ChainFirst::Expr(first) => {
                if first.single_line_width(printer)? > shape.width {
                    return None;
                }
                if printer
                    .print(first, Shape::unlimited_single_line())
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
            match *item {
                PrintChainItem::FieldAccess(dot, word) => {
                    printer.print_token(dot);
                    printer.print(word, Shape::unlimited_single_line());
                }
                PrintChainItem::OptionalFieldAccess(qd, word) => {
                    printer.print_token(qd);
                    printer.print(word, Shape::unlimited_single_line());
                }
                PrintChainItem::Index(index_args) => {
                    index_args.try_print_single_line(shape, printer)?;
                }
                PrintChainItem::OptionalIndex(qd, index_args) => {
                    printer.print_token(qd);
                    index_args.try_print_single_line(shape, printer)?;
                }
                PrintChainItem::Call(call_args) => {
                    call_args.try_print_single_line(shape, printer)?;
                }
                PrintChainItem::OptionalCall(qd, call_args) => {
                    printer.print_token(qd);
                    call_args.try_print_single_line(shape, printer)?;
                }
                PrintChainItem::GenericArgs(generic_args) => {
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

    fn try_print_hug(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        let (last, prefix) = self.chain_members.split_last()?;
        let (question_dot, call_args) = match *last {
            PrintChainItem::Call(args) => (None, args),
            PrintChainItem::OptionalCall(qd, args) => (Some(qd), args),
            _ => return None,
        };
        if !call_args.can_hug(printer.tree) {
            return None;
        }
        self.try_print_members_single_line(prefix, shape, printer)?;
        if printer.output.len() > shape.width {
            return None;
        }
        if let Some(qd) = question_dot {
            printer.print_token(qd);
        }
        let hug_shape = Shape {
            width: shape.width,
            indent: shape.indent,
            first_line_offset: shape.first_line_offset,
        };
        call_args.try_print_hug(&hug_shape, printer)
    }

    fn try_print_tail_call_broken(
        &self,
        shape: &Shape,
        printer: &mut Printer,
    ) -> Option<PrintInfo> {
        let (last, prefix) = self.chain_members.split_last()?;
        let question_dot = match *last {
            PrintChainItem::Call(_) | PrintChainItem::Index(_) => None,
            PrintChainItem::OptionalCall(qd, _) | PrintChainItem::OptionalIndex(qd, _) => Some(qd),
            PrintChainItem::FieldAccess(..)
            | PrintChainItem::OptionalFieldAccess(..)
            | PrintChainItem::GenericArgs(..) => return None,
        };
        self.try_print_members_single_line(prefix, shape, printer)?;
        if let Some(qd) = question_dot {
            printer.print_token(qd);
        }
        if printer.output.len() > shape.width {
            return None;
        }
        let args_shape = Shape {
            width: shape.width.saturating_sub(printer.output.len()),
            indent: shape.indent,
            first_line_offset: shape.first_line_offset + printer.output.len(),
        };
        let info = match *last {
            PrintChainItem::Call(call_args) | PrintChainItem::OptionalCall(_, call_args) => {
                printer.print(call_args, args_shape)
            }
            PrintChainItem::Index(index_args) | PrintChainItem::OptionalIndex(_, index_args) => {
                printer.print(index_args, args_shape)
            }
            PrintChainItem::FieldAccess(..)
            | PrintChainItem::OptionalFieldAccess(..)
            | PrintChainItem::GenericArgs(..) => unreachable!("checked above"),
        };
        let first_line_len = printer.output.find('\n').unwrap_or(printer.output.len());
        if first_line_len > shape.width {
            return None;
        }
        Some(info)
    }

    pub(crate) fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .or_else(|| printer.try_sub_printer(|p| self.try_print_hug(&shape, p)))
            .or_else(|| printer.try_sub_printer(|p| self.try_print_tail_call_broken(&shape, p)))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
}
