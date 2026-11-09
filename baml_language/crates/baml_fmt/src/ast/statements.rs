use baml_syntax::{
    nodes::{
        AnyForHeader, AnyForInit, AnyForStep, AnyStmt, AssignStmt, BreakExpr, CForHeader,
        ContinueExpr, ExprStmt, FileTree, ForInHeader, ForStmt, LetElse, LetStmt, ParenForInHeader,
        ReturnExpr, TypeBindingStmt, WhileLetStmt, WhileStmt,
    },
    token::{Token, TokenIdx, TokenSet as _, punct, sets::CommentToken},
    tree::{Id, TokenRange},
};

use super::{
    expressions::ExprExt as _, pattern::print_binding_keyword_with_trailing_trivia, present,
    union_printable,
};
use crate::{
    printer::{PrintInfo, PrintMultiLine, Printable, Printer, Shape, Spanned},
    trivia_classifier::{EmittableTrivia, TriviaSliceExt as _},
};

impl Printable for AnyStmt {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            AnyStmt::Expr(stmt) => printer.print(stmt, shape),
            AnyStmt::Let(stmt) => printer.print(stmt, shape),
            AnyStmt::TypeBinding(stmt) => printer.print(stmt, shape),
            AnyStmt::Assign(stmt) => printer.print(stmt, shape),
            AnyStmt::While(stmt) => printer.print(stmt, shape),
            AnyStmt::WhileLet(stmt) => printer.print(stmt, shape),
            AnyStmt::For(stmt) => printer.print(stmt, shape),
            AnyStmt::Test(test) => printer.print(test, shape),
            AnyStmt::Header(header) => {
                printer.print_token(printer.tree.get(header).token);
                PrintInfo::default_single_line()
            }
            AnyStmt::Empty(stmt) => {
                printer.print_token(printer.tree.get(stmt).semi);
                PrintInfo::default_single_line()
            }
            // Not modelled by the printer yet: printed as written.
            AnyStmt::Defer(stmt) => {
                printer.print_verbatim(stmt);
                PrintInfo::default_multi_lined()
            }
            AnyStmt::Bogus(_) => unreachable!("the formatter prints only error-free trees"),
        }
    }
}

/// `;` after a statement: the one written, with the comments before it kept
/// on the line, or an added one where the statement needs it.
fn print_statement_semicolon(
    last: impl Spanned,
    semi: Option<Token<punct::Semicolon>>,
    needed: bool,
    printer: &mut Printer,
) {
    if let Some(semicolon) = semi {
        let expr_trailing = printer.trivia.get_trailing_for_element(printer.tree, last);
        printer.print_trivia_squished(expr_trailing);
        let (semicolon_leading, _) = printer.trivia.get_for_range_split(printer.range(semicolon));
        printer.print_trivia_squished(semicolon_leading);
        printer.print_token(semicolon);
    } else if needed {
        printer.print_str(";");
    }
}

impl Printable for Id<ExprStmt> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let node = printer.tree.get(self);
        let info = printer.print(node.expr, shape);
        let needed = node.expr.statement_needs_semicolon(printer.tree);
        print_statement_semicolon(node.expr, node.semi, needed, printer);
        info
    }
}

/// `target op value`, printed like the binary expression it reads as; the
/// `;` is the statement's, and a `for` header's step has none.
/// How an assignment is closed.
#[derive(Clone, Copy)]
enum AssignSemicolon {
    /// No `;`: the step of a C-style `for` header.
    Omitted,
    /// The `;` as written: an assignment closing a block without a tail.
    AsWritten,
    /// A `;`, written or added: a statement.
    Required,
}

fn print_assignment(
    id: Id<AssignStmt>,
    semicolon: AssignSemicolon,
    shape: Shape,
    printer: &mut Printer,
) -> PrintInfo {
    let node = printer.tree.get(id);
    let value = present(node.value);
    let info = super::expressions::print_binary_like(node.target, node.op, value, shape, printer);
    match semicolon {
        AssignSemicolon::Omitted => {}
        AssignSemicolon::AsWritten => print_statement_semicolon(value, node.semi, false, printer),
        AssignSemicolon::Required => print_statement_semicolon(value, node.semi, true, printer),
    }
    info
}

/// `target op value;` — an assignment is a statement.
impl Printable for Id<AssignStmt> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        print_assignment(self, AssignSemicolon::Required, shape, printer)
    }
}

/// An assignment closing a block that has no tail. It keeps the `;` as
/// written, the way a block's final expression does.
#[derive(Clone, Copy)]
pub(crate) struct ClosingAssign(pub(crate) Id<AssignStmt>);
impl Spanned for ClosingAssign {
    fn tokens(self, tree: &FileTree) -> TokenRange {
        self.0.tokens(tree)
    }
}
impl Printable for ClosingAssign {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        print_assignment(self.0, AssignSemicolon::AsWritten, shape, printer)
    }
}

/// ` else { … }` after a `let`'s initializer.
fn print_let_else(else_branch: LetElse, shape: Shape, printer: &mut Printer) -> bool {
    let block = present(else_branch.block);
    let (else_leading, else_trailing) = printer
        .trivia
        .get_for_range_split(printer.range(else_branch.kw));
    printer.print_str(" ");
    printer.print_trivia_squished(else_leading);
    printer.print_token(else_branch.kw);
    printer.print_str(" ");
    printer.print_trivia_squished(else_trailing);
    printer.print(block, shape).multi_lined
}

/// The `;` closing a `let`: the one written (comments before it kept) or
/// an added one.
fn print_let_semicolon(semi: Option<Token<punct::Semicolon>>, printer: &mut Printer) {
    if let Some(semicolon) = semi {
        let (semicolon_leading, _) = printer.trivia.get_for_range_split(printer.range(semicolon));
        printer.print_trivia_squished(semicolon_leading);
        printer.print_token(semicolon);
    } else {
        printer.print_str(";");
    }
}

/// `let pattern = init[ else { … }];` closed by `semi`: the statement's own
/// `;`, or the `semi1` of the C-style `for` header the `let` initialises.
fn print_let(
    id: Id<LetStmt>,
    semi: Option<Token<punct::Semicolon>>,
    shape: Shape,
    printer: &mut Printer,
) -> PrintInfo {
    let tree = printer.tree;
    let node = tree.get(id);
    let pattern = present(node.pattern);
    let eq = present(node.eq);
    let init = present(node.init);
    let mut multi_lined = false;
    print_binding_keyword_with_trailing_trivia(printer, node.introducer);
    multi_lined |= printer.print(pattern, shape).multi_lined;
    let (_, equals_trailing) = printer.trivia.get_for_range_split(printer.range(eq));
    printer.print_str(" ");
    printer.print_token(eq);
    printer.print_str(" ");
    printer.print_trivia_squished(equals_trailing);
    let expr_leading = printer.trivia.get_leading_for_element(tree, init);
    printer.print_trivia_squished(expr_leading);
    multi_lined |= printer.print(init, shape).multi_lined;
    if semi.is_some() || node.else_branch.is_some() {
        let expr_trailing = printer.trivia.get_trailing_for_element(tree, init);
        printer.print_trivia_squished(expr_trailing);
    }
    if let Some(else_branch) = node.else_branch {
        multi_lined |= print_let_else(else_branch, shape, printer);
    }
    print_let_semicolon(semi, printer);
    PrintInfo { multi_lined }
}

impl Printable for Id<LetStmt> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        print_let(self, printer.tree.get(self).semi, shape, printer)
    }
}

impl Printable for Id<TypeBindingStmt> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let name = present(node.name);
        let eq = present(node.eq);
        let operand = present(node.init);
        let mut multi_lined = false;
        printer.print_token(node.let_kw);
        printer.print_str(" ");
        printer.print_token(node.type_kw);
        printer.print_str(" ");
        printer.print(name, shape);
        if let Some(bounds) = node.bounds {
            printer.print_str(" ");
            printer.print(bounds, Shape::unlimited_single_line());
        }
        let (_, equals_trailing) = printer.trivia.get_for_range_split(printer.range(eq));
        printer.print_str(" ");
        printer.print_token(eq);
        printer.print_str(" ");
        printer.print_trivia_squished(equals_trailing);
        let operand_leading = printer.trivia.get_leading_for_element(tree, operand);
        printer.print_trivia_squished(operand_leading);
        multi_lined |= printer.print(operand, shape).multi_lined;
        if node.semi.is_some() || node.else_branch.is_some() {
            let operand_trailing = printer.trivia.get_trailing_for_element(tree, operand);
            printer.print_trivia_squished(operand_trailing);
        }
        if let Some(else_branch) = node.else_branch {
            multi_lined |= print_let_else(else_branch, shape, printer);
        }
        print_let_semicolon(node.semi, printer);
        PrintInfo { multi_lined }
    }
}

impl Printable for Id<WhileStmt> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let node = printer.tree.get(self);
        let cond = present(node.cond);
        let body = present(node.body);
        printer.print_token(node.kw);
        printer.print_str(" ");
        let condition_shape = Shape {
            width: shape.width,
            indent: shape.indent,
            first_line_offset: shape.first_line_offset + const { "while ".len() },
        };
        printer.print(cond, condition_shape);
        printer.print_str(" ");
        let body_shape = Shape {
            width: shape.width,
            indent: shape.indent,
            first_line_offset: 0, // irrelevant since body new-lines immediately after `{`
        };
        printer.print(body, body_shape);
        PrintInfo::default_multi_lined()
    }
}

impl Printable for Id<WhileLetStmt> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let node = printer.tree.get(self);
        let pattern = present(node.pattern);
        let eq = present(node.eq);
        let scrutinee = present(node.scrutinee);
        let body = present(node.body);
        printer.print_token(node.kw);
        printer.print_str(" ");
        print_binding_keyword_with_trailing_trivia(printer, node.introducer);
        printer.print(pattern, shape);
        printer.print_str(" ");
        printer.print_token(eq);
        printer.print_str(" ");
        printer.print(scrutinee, shape);
        printer.print_str(" ");
        let body_shape = Shape {
            width: shape.width,
            indent: shape.indent,
            first_line_offset: 0, // irrelevant since body new-lines immediately after `{`
        };
        printer.print(body, body_shape);
        PrintInfo::default_multi_lined()
    }
}

impl Printable for Id<ForStmt> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let node = printer.tree.get(self);
        let header = present(node.header);
        let body = present(node.body);
        printer.print_token(node.kw);
        printer.print_str(" ");
        printer.print(header, shape);
        printer.print_str(" ");
        printer.print(body, shape);
        PrintInfo::default_multi_lined()
    }
}

union_printable!(AnyForHeader {
    In,
    ParenIn,
    CStyle
});
union_printable!(AnyForInit { Let, Expr });
/// The step of a C-style `for`: an assignment there has no `;`.
impl Printable for AnyForStep {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            AnyForStep::Assign(stmt) => {
                print_assignment(stmt, AssignSemicolon::Omitted, shape, printer)
            }
            AnyForStep::Expr(expr) => printer.print(expr, shape),
            AnyForStep::Bogus(_) => unreachable!("the formatter prints only error-free trees"),
        }
    }
}

/// The initializer of a C-style `for`: a `let` carries its own `;`, an
/// expression is followed by the header's first `;`.
fn print_for_init(
    init: Option<AnyForInit>,
    semi1: Option<Token<punct::Semicolon>>,
    shape: Shape,
    printer: &mut Printer,
) -> PrintInfo {
    match init {
        Some(AnyForInit::Let(stmt)) => print_let(stmt, semi1, shape, printer),
        Some(AnyForInit::Expr(expr)) => {
            let info = printer.print(expr, shape);
            print_statement_semicolon(expr, semi1, true, printer);
            info
        }
        None => {
            if let Some(semi) = semi1 {
                printer.print_token(semi);
            } else {
                printer.print_str(";");
            }
            PrintInfo::default_single_line()
        }
        Some(AnyForInit::Bogus(_)) => unreachable!("the formatter prints only error-free trees"),
    }
}

/// The trivia after a C-style `for` initialiser: after its `;` when written,
/// since the initialiser prints the comments before the `;` itself.
fn c_for_init_trailing<'t>(
    init: AnyForInit,
    semi1: Option<Token<punct::Semicolon>>,
    printer: &Printer<'t>,
) -> &'t [EmittableTrivia] {
    match semi1 {
        Some(semi) => printer.trivia.get_for_range_split(printer.range(semi)).1,
        None => printer.trivia.get_trailing_for_element(printer.tree, init),
    }
}

impl PrintMultiLine for Id<CForHeader> {
    fn print_multi_line(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let open = present(node.l);
        let close = present(node.r);
        let inner_shape = Shape::standalone(
            printer.config.line_width,
            shape.indent + printer.config.indent_width,
        );
        printer.print_token(open);
        printer.print_trivia_all_trailing_for(printer.range(open));
        printer.print_newline();
        if let Some(init) = node.init {
            let init_leading = printer.trivia.get_leading_for_element(tree, init);
            let init_trailing = c_for_init_trailing(init, node.semi1, printer);
            printer.print_trivia_with_newline(init_leading.trim_blanks(), inner_shape.indent);
            printer.print_spaces(inner_shape.indent);
            print_for_init(Some(init), node.semi1, inner_shape, printer);
            printer.print_trivia_trailing(init_trailing);
        } else {
            printer.print_spaces(inner_shape.indent);
            print_for_init(None, node.semi1, inner_shape, printer);
        }
        printer.print_newline();
        if let Some(condition) = node.cond {
            let (cond_leading, cond_trailing) = printer.trivia.get_for_element(tree, condition);
            printer.print_trivia_with_newline(cond_leading.trim_blanks(), inner_shape.indent);
            printer.print_spaces(inner_shape.indent);
            printer.print(condition, inner_shape);
            printer.print_trivia_squished(cond_trailing); // always squished before `;`
        } else {
            printer.print_spaces(inner_shape.indent);
        }
        if let Some(semi) = node.semi2 {
            let (semi_leading, semi_trailing) =
                printer.trivia.get_for_range_split(printer.range(semi));
            printer.print_trivia_squished(semi_leading); // always squished before `;`
            printer.print_token(semi);
            printer.print_trivia_trailing(semi_trailing);
        } else {
            printer.print_str(";");
        }
        printer.print_newline();
        if let Some(step) = node.step {
            let (update_leading, update_trailing) = printer.trivia.get_for_element(tree, step);
            printer.print_trivia_with_newline(update_leading.trim_blanks(), inner_shape.indent);
            printer.print_spaces(inner_shape.indent);
            printer.print(step, inner_shape);
            printer.print_trivia_trailing(update_trailing);
            printer.print_newline();
        }
        let (close_paren_leading, _) = printer.trivia.get_for_range_split(printer.range(close));
        printer.print_trivia_with_newline(close_paren_leading.trim_blanks(), inner_shape.indent);
        printer.print_spaces(shape.indent);
        printer.print_token(close);
        PrintInfo::default_multi_lined()
    }
}

fn c_for_header_single_line(
    id: Id<CForHeader>,
    shape: &Shape,
    printer: &mut Printer,
) -> Option<PrintInfo> {
    let tree = printer.tree;
    let node = tree.get(id);
    let open = present(node.l);
    let close = present(node.r);
    printer.print_token(open);
    let (_, open_trailing) = printer.trivia.get_for_range_split(printer.range(open));
    printer.try_print_trivia_single_line_squished(open_trailing)?;
    if let Some(init) = node.init {
        let init_leading = printer.trivia.get_leading_for_element(tree, init);
        let init_trailing = c_for_init_trailing(init, node.semi1, printer);
        printer.try_print_trivia_single_line_squished(init_leading)?;
        if print_for_init(
            Some(init),
            node.semi1,
            Shape::unlimited_single_line(),
            printer,
        )
        .multi_lined
        {
            return None;
        }
        printer.try_print_trivia_single_line_squished(init_trailing)?;
    } else {
        print_for_init(None, node.semi1, Shape::unlimited_single_line(), printer);
    }
    printer.print_str(" ");
    if let Some(condition) = node.cond {
        let (cond_leading, cond_trailing) = printer.trivia.get_for_element(tree, condition);
        printer.try_print_trivia_single_line_squished(cond_leading)?;
        if printer
            .print(condition, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        printer.print_trivia_squished(cond_trailing); // always squished before `;`
    }
    if let Some(semi) = node.semi2 {
        let (semi_leading, semi_trailing) = printer.trivia.get_for_range_split(printer.range(semi));
        printer.print_trivia_squished(semi_leading); // always squished before `;`
        printer.print_token(semi);
        printer.try_print_trivia_single_line_squished(semi_trailing)?;
    } else {
        printer.print_str(";");
    }
    if let Some(step) = node.step {
        printer.print_str(" ");
        let (update_leading, update_trailing) = printer.trivia.get_for_element(tree, step);
        printer.try_print_trivia_single_line_squished(update_leading)?;
        if printer
            .print(step, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        printer.try_print_trivia_single_line_squished(update_trailing)?;
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

impl Printable for Id<CForHeader> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| c_for_header_single_line(self, &shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
}

/// `let pattern in iter`, with or without the parentheses around it.
fn for_in_header_multi_line(
    header: Id<ForInHeader>,
    parens: Option<(Token<punct::LParen>, Token<punct::RParen>)>,
    shape: Shape,
    printer: &mut Printer,
) -> PrintInfo {
    let tree = printer.tree;
    let node = tree.get(header);
    let pattern = present(node.pattern);
    let in_kw = present(node.in_kw);
    let expression = present(node.iter);
    let inner_shape = Shape::standalone(shape.width, shape.indent + printer.config.indent_width);
    if let Some((open_paren, _)) = parens {
        printer.print_token(open_paren);
        printer.print_trivia_all_trailing_for(printer.range(open_paren));
        printer.print_newline();
        let (binding_leading, _) = printer
            .trivia
            .get_for_range_split(printer.range(node.introducer));
        printer.print_trivia_with_newline(binding_leading, inner_shape.indent);
        printer.print_spaces(inner_shape.indent);
    }
    printer.print_token(node.introducer);
    printer.print_spaces(1);
    let (_, let_trailing) = printer
        .trivia
        .get_for_range_split(printer.range(node.introducer));
    if printer.print_trivia_squished(let_trailing) > 0 {
        printer.print_spaces(1);
    }
    printer.print(pattern, inner_shape);
    printer.print_str(" ");
    printer.print_token(in_kw);
    printer.print_spaces(1);
    let (_, in_trailing) = printer.trivia.get_for_range_split(printer.range(in_kw));
    let (expr_leading, expr_trailing) = printer.trivia.get_for_element(tree, expression);
    printer.print_trivia_squished(in_trailing);
    printer.print_trivia_squished(expr_leading);
    let curr_line_len = printer.current_line_len();
    let offset = curr_line_len.saturating_sub(inner_shape.indent);
    let expr_shape = Shape {
        width: printer.config.line_width.saturating_sub(curr_line_len),
        indent: inner_shape.indent,
        first_line_offset: offset,
    };
    printer.print(expression, expr_shape);
    printer.print_trivia_trailing(expr_trailing);
    if let Some((_, close_paren)) = parens {
        printer.print_newline();
        let (close_paren_leading, _) = printer
            .trivia
            .get_for_range_split(printer.range(close_paren));
        printer.print_trivia_with_newline(close_paren_leading, inner_shape.indent);
        printer.print_spaces(shape.indent);
        printer.print_token(close_paren);
    }
    PrintInfo::default_multi_lined()
}

fn for_in_header_single_line(
    header: Id<ForInHeader>,
    parens: Option<(Token<punct::LParen>, Token<punct::RParen>)>,
    shape: &Shape,
    printer: &mut Printer,
) -> Option<PrintInfo> {
    let tree = printer.tree;
    let node = tree.get(header);
    let pattern = present(node.pattern);
    let in_kw = present(node.in_kw);
    let expression = present(node.iter);
    if let Some((open_paren, _)) = parens {
        printer.print_token(open_paren);
        let (_, open_trailing) = printer
            .trivia
            .get_for_range_split(printer.range(open_paren));
        printer.try_print_trivia_single_line_squished(open_trailing)?;
    }
    printer.print_token(node.introducer);
    printer.print_spaces(1);
    let (_, let_trailing) = printer
        .trivia
        .get_for_range_split(printer.range(node.introducer));
    if printer.print_trivia_squished(let_trailing) > 0 {
        printer.print_spaces(1);
    }
    if printer
        .print(pattern, Shape::unlimited_single_line())
        .multi_lined
    {
        return None;
    }
    printer.print_str(" ");
    printer.print_token(in_kw);
    printer.print_str(" ");
    let (_, in_trailing) = printer.trivia.get_for_range_split(printer.range(in_kw));
    let (expr_leading, expr_trailing) = printer.trivia.get_for_element(tree, expression);
    printer.print_trivia_squished(in_trailing);
    printer.print_trivia_squished(expr_leading);
    if printer
        .print(expression, Shape::unlimited_single_line())
        .multi_lined
    {
        return None;
    }
    printer.try_print_trivia_single_line_squished(expr_trailing)?;
    if let Some((_, close_paren)) = parens {
        let (close_leading, _) = printer
            .trivia
            .get_for_range_split(printer.range(close_paren));
        printer.try_print_trivia_single_line_squished(close_leading)?;
        printer.print_token(close_paren);
    }
    if printer.output.len() > shape.width {
        None
    } else {
        Some(PrintInfo::default_single_line())
    }
}

impl Printable for Id<ForInHeader> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| for_in_header_single_line(self, None, &shape, p))
            .unwrap_or_else(|| for_in_header_multi_line(self, None, shape, printer))
    }
}

impl Printable for Id<ParenForInHeader> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let node = printer.tree.get(self);
        let parens = Some((node.l, present(node.r)));
        printer
            .try_sub_printer(|p| for_in_header_single_line(node.header, parens, &shape, p))
            .unwrap_or_else(|| for_in_header_multi_line(node.header, parens, shape, printer))
    }
}

/// `return`, `break`, `continue`: the keyword and, for `return`, its value.
/// Whether a comment token sits inside `element`'s own tokens.
fn has_interior_comment(tree: &FileTree, element: impl Spanned) -> bool {
    let tokens = tree.tokens();
    let range = element.tokens(tree);
    (range.start().0..range.end().0)
        .map(TokenIdx)
        .any(|idx| CommentToken::KINDS.contains(tokens.kind(idx)))
}

/// A braceless jump only appears as a whole statement or arm value, never
/// nested inside another expression, so it reports multi-lined and the arm
/// printers wrap it.
impl Printable for Id<ReturnExpr> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let node = printer.tree.get(self);
        // A value with comments inside it (`return e /* c */ . m`) is printed
        // as written: the single-line layouts of its parts drop them.
        if node
            .value
            .is_some_and(|value| has_interior_comment(printer.tree, value))
        {
            return printer.print_verbatim(self);
        }
        printer.print_token(node.kw);
        if let Some(value) = node.value {
            let (_, kw_trailing) = printer.trivia.get_for_range_split(printer.range(node.kw));
            printer.print_trivia_squished(kw_trailing);
            let value_leading = printer.trivia.get_leading_for_element(printer.tree, value);
            printer.print_str(" ");
            printer.print_trivia_squished(value_leading);
            printer.print(value, shape);
        }
        PrintInfo::default_multi_lined()
    }
}

impl Printable for Id<BreakExpr> {
    fn print(self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_token(printer.tree.get(self).kw);
        PrintInfo::default_multi_lined()
    }
}

impl Printable for Id<ContinueExpr> {
    fn print(self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_token(printer.tree.get(self).kw);
        PrintInfo::default_multi_lined()
    }
}
